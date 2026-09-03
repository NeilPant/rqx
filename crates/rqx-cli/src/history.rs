use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Churn comes from one `git log` rather than a git library: a single process gives
/// every file's commit count in one pass, and a repository is the only place this
/// data exists anyway. No repository simply means no churn signal.
pub fn commits_per_file(root: &Path, days: u32) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["log", "--name-only", "--pretty=format:", "--no-merges"])
        .arg(format!("--since={days} days ago"))
        .output()
    else {
        return counts;
    };
    if !output.status.success() {
        return counts;
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        *counts.entry(path.to_owned()).or_insert(0) += 1;
    }
    counts
}

/// `git log` reports paths from the repository root while findings carry paths
/// relative to the scan root, so one of the two has to be translated before they
/// can be matched at all.
/// The branch name, for the header. A detached head has none, and saying so is
/// better than showing a hash nobody asked for.
pub fn branch(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!name.is_empty() && name != "HEAD").then_some(name)
}

pub fn repository_prefix(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-prefix"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Files grouped by the commit that touched them. Shotgun Surgery is a claim about
/// what changes *together*, which per-file counts cannot express.
pub fn commits_by_file_set(root: &Path, days: u32) -> Vec<Vec<String>> {
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["log", "--name-only", "--pretty=format:%x00", "--no-merges"])
        .arg(format!("--since={days} days ago"))
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .map(|commit| {
            commit
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect::<Vec<String>>()
        })
        // A commit touching half the tree is a rename or a format run, and says
        // nothing about which files share a responsibility.
        .filter(|files| (2..=20).contains(&files.len()))
        .collect()
}

/// Files that differ from a base revision. Uses the merge base so a stale branch is
/// judged on what it actually changed rather than on everything that landed on the
/// base since it forked.
pub fn changed_since(root: &Path, base: &str) -> Option<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["diff", "--name-only", "--diff-filter=d"])
        .arg(format!("{base}...HEAD"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

/// Uncommitted work counts as changed too, so `rqx check` is useful before a commit
/// exists rather than only in CI.
pub fn uncommitted(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for args in [["diff", "--name-only", "HEAD"], ["ls-files", "--others", "--exclude-standard"]] {
        let Ok(output) = Command::new("git").arg("-C").arg(root).args(args).output() else {
            continue;
        };
        if output.status.success() {
            files.extend(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_with_no_repository_yields_no_churn() {
        let counts = commits_per_file(Path::new("/"), 90);
        assert!(counts.is_empty());
    }

    #[test]
    fn this_repository_reports_churn_for_its_own_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        if repository_prefix(root).is_none() {
            return;
        }
        let counts = commits_per_file(root, 3650);
        assert!(counts.values().all(|count| *count > 0));
    }
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Everything the scan needs from git, read once. The per-file counts and the
/// co-change sets came from two `git log` invocations differing only in their format
/// string, and the repository prefix from two more — four processes for two answers,
/// all of them on the critical path after the walk.
///
/// Nothing here depends on what the walk found, so it is read on its own thread while
/// the files are being parsed and is almost always waiting by the time it is asked for.
#[derive(Default)]
pub struct History {
    commits: Vec<Vec<String>>,
    prefix: String,
}

impl History {
    /// A repository is the only place this data exists, so no repository simply means
    /// no churn signal rather than an error.
    pub fn read(root: &Path, days: u32) -> Self {
        let prefix = repository_prefix(root).unwrap_or_default();
        if days == 0 {
            return Self {
                commits: Vec::new(),
                prefix,
            };
        }
        // `%x00` delimits commits, so one pass yields both the per-file totals and
        // which files moved together.
        let Ok(output) = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["log", "--name-only", "--pretty=format:%x00", "--no-merges"])
            .arg(format!("--since={days} days ago"))
            .output()
        else {
            return Self {
                commits: Vec::new(),
                prefix,
            };
        };
        if !output.status.success() {
            return Self {
                commits: Vec::new(),
                prefix,
            };
        }
        let commits = String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .map(|commit| {
                commit
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<String>>()
            })
            .filter(|files| !files.is_empty())
            .collect();
        Self { commits, prefix }
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn commits_per_file(&self) -> HashMap<&str, u32> {
        let mut counts = HashMap::new();
        for path in self.commits.iter().flatten() {
            *counts.entry(path.as_str()).or_insert(0) += 1;
        }
        counts
    }

    /// A commit touching half the tree is a rename or a format run, and says nothing
    /// about which files share a responsibility.
    pub fn co_changes(&self) -> impl Iterator<Item = &Vec<String>> {
        self.commits
            .iter()
            .filter(|files| (2..=20).contains(&files.len()))
    }
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

/// A checkout of another revision, in a temporary directory, removed when it drops.
/// Scanning it is how rqx sees what the project looked like before: reading each
/// changed file's old text would answer the per-file rules and leave every cross-file
/// one — the graph, the clones, the churn — with nothing to read.
pub struct Worktree {
    path: PathBuf,
    root: PathBuf,
}

impl Worktree {
    pub fn add(root: &Path, revision: &str) -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "rqx-base-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos())
        ));
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["worktree", "add", "--detach", "--quiet"])
            .arg(&path)
            .arg(revision)
            .output()
            .map_err(|error| format!("git could not run: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        Ok(Self {
            path,
            root: root.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .output();
        let _ = std::fs::remove_dir_all(&self.path);
    }
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
    for args in [
        ["diff", "--name-only", "HEAD"],
        ["ls-files", "--others", "--exclude-standard"],
    ] {
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
        let history = History::read(Path::new("/"), 90);
        let counts = history.commits_per_file();
        assert!(counts.is_empty());
    }

    /// One `git log` now answers both the per-file counts and the co-change sets, so
    /// the two have to stay consistent with each other.
    #[test]
    fn one_read_answers_both_questions() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        if repository_prefix(root).is_none() {
            return;
        }
        let history = History::read(root, 3650);
        let counts = history.commits_per_file();
        for files in history.co_changes() {
            assert!(
                (2..=20).contains(&files.len()),
                "a co-change set escaped the size filter: {} files",
                files.len()
            );
            for file in files {
                assert!(
                    counts.contains_key(file.as_str()),
                    "`{file}` moved together with something but was never counted"
                );
            }
        }
    }

    #[test]
    fn this_repository_reports_churn_for_its_own_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        if repository_prefix(root).is_none() {
            return;
        }
        let history = History::read(root, 3650);
        let counts = history.commits_per_file();
        assert!(counts.values().all(|count| *count > 0));
    }
}

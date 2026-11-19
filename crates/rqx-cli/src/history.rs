use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

/// Everything the scan needs from git, read once. The per-file counts and the
/// co-change sets came from two `git log` invocations differing only in their format
/// string, and the repository prefix from two more — four processes for two answers,
/// all of them on the critical path after the walk.
///
/// Nothing here depends on what the walk found, so it is read on its own thread while
/// the files are being parsed and is almost always waiting by the time it is asked for.
#[derive(Default)]
pub struct History {
    /// Whether git answered at all. An empty history in a repository means a quiet
    /// month; outside one it means every churn rule silently did nothing, which is
    /// worth one line to whoever is reading a report that has none.
    versioned: bool,
    commits: Vec<Vec<String>>,
    /// Who wrote each commit, in step with `commits`. Kept beside rather than inside
    /// so every existing reader of the file lists is unchanged.
    authors: Vec<String>,
    prefix: String,
    git_processes: u32,
    /// What was run, as it would be typed, for a reader who asked to see it.
    commands: Vec<String>,
}

const PREFIX_ARGS: [&str; 2] = ["rev-parse", "--show-prefix"];
const LOG_ARGS: [&str; 5] = [
    "log",
    "--name-only",
    "-z",
    "--pretty=format:%an",
    "--no-merges",
];
const BLAME_ARGS: [&str; 3] = ["blame", "--porcelain", "--"];

fn since(days: u32) -> String {
    format!("--since={days} days ago")
}

/// A command as it would be typed, which is what `-vv` prints for each one the scan ran.
fn typed(arguments: &[&str], last: Option<&str>) -> String {
    let mut words = vec!["git"];
    words.extend(arguments);
    words.extend(last);
    words.join(" ")
}

pub fn blame_command(file: &str) -> String {
    typed(&BLAME_ARGS, Some(file))
}

impl History {
    /// A repository is the only place this data exists, so no repository simply means
    /// no churn signal rather than an error.
    pub fn read(root: &Path, days: u32) -> Self {
        let found = repository_prefix(root);
        let mut history = Self {
            versioned: found.is_some(),
            prefix: found.unwrap_or_default(),
            git_processes: 1,
            commands: vec![typed(&PREFIX_ARGS, None)],
            ..Self::default()
        };
        if days > 0 {
            history.git_processes += 1;
            history.commands.push(typed(&LOG_ARGS, Some(&since(days))));
            if let Some((commits, authors)) = recent_commits(root, days) {
                history.commits = commits;
                history.authors = authors;
            }
        }
        history
    }

    /// How many `git` processes reading this took, so a scan can say what it cost.
    pub const fn git_processes(&self) -> u32 {
        self.git_processes
    }

    pub fn commands(&self) -> &[String] {
        &self.commands
    }

    pub const fn versioned(&self) -> bool {
        self.versioned
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

    /// Who has touched each file, so a file only one person has ever edited can be
    /// told from one the team shares.
    pub fn authors_per_file(&self) -> HashMap<&str, (std::collections::BTreeSet<&str>, u32)> {
        let mut seen: HashMap<&str, (std::collections::BTreeSet<&str>, u32)> = HashMap::new();
        for (author, files) in self.authors.iter().zip(&self.commits) {
            for file in files {
                let entry = seen.entry(file.as_str()).or_default();
                entry.0.insert(author.as_str());
                entry.1 += 1;
            }
        }
        seen
    }

    /// The files each commit changed together. A commit touching half the tree is a
    /// rename or a format run, and says nothing about which files share a
    /// responsibility, so only commits of two to twenty files count.
    pub fn co_changes(&self) -> impl Iterator<Item = &Vec<String>> {
        self.commits
            .iter()
            .filter(|files| (2..=20).contains(&files.len()))
    }
}

/// `%x00` delimits commits, so one pass yields both the per-file totals and which
/// files moved together.
fn recent_commits(root: &Path, days: u32) -> Option<(Vec<Vec<String>>, Vec<String>)> {
    let output = git(root).args(LOG_ARGS).arg(since(days)).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(commits_from(&output.stdout))
}

/// The files and the author of each commit, from `git log -z --name-only`: each
/// commit is its author, a newline and its first path, then its other paths, every
/// path ended by a NUL and the commits parted by an empty field. NUL-separated,
/// because git quotes a path with a non-ASCII character, a tab or a quote in it, and
/// a quoted path matches no file.
fn commits_from(output: &[u8]) -> (Vec<Vec<String>>, Vec<String>) {
    let mut commits = Vec::new();
    let mut authors = Vec::new();
    let fields: Vec<String> = output
        .split(|byte| *byte == 0)
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect();
    for record in fields.split(String::is_empty) {
        let Some((head, rest)) = record.split_first() else {
            continue;
        };
        let Some((author, first)) = head.split_once('\n') else {
            continue;
        };
        let files: Vec<String> = std::iter::once(first.to_owned())
            .chain(rest.iter().cloned())
            .filter(|file| !file.is_empty())
            .collect();
        if files.is_empty() {
            continue;
        }
        authors.push(author.trim().to_owned());
        commits.push(files);
    }
    (commits, authors)
}

/// The path from the repository root to `root`. `git log` reports paths from the
/// repository root while findings carry paths relative to the scan root, so one of
/// the two has to be translated before they can be matched at all.
pub fn repository_prefix(root: &Path) -> Option<String> {
    let output = git(root).args(PREFIX_ARGS).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// A `git` in `directory`. A hook run by `git commit -a` or `git commit <path>` is
/// handed a temporary index through a relative `GIT_INDEX_FILE`, and `-C` would
/// otherwise make that path point somewhere else.
pub fn git(directory: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(directory);
    if let Some(index) = std::env::var_os("GIT_INDEX_FILE")
        && let Ok(index) = std::path::absolute(index)
    {
        command.env("GIT_INDEX_FILE", index);
    }
    command
}

/// `revision`, when git can be handed it as one. git reads any argument that begins
/// with `-` as an option wherever it stands, so a base of `--output=x` would have had
/// `git diff` write a file. A branch or tag name cannot begin with `-`, so nothing
/// that names a revision is refused. Checked here rather than with `--end-of-options`,
/// which git before 2.24 rejects.
pub fn revision(revision: &str) -> Result<&str, String> {
    if revision.is_empty() {
        return Err("an empty revision names no commit".to_owned());
    }
    if revision.starts_with('-') {
        return Err(format!(
            "`{revision}` is not a revision: git would read anything that begins with `-` \
             as an option"
        ));
    }
    Ok(revision)
}

/// What a failed git command said, for an error message: the command as it would be
/// typed and the first line git wrote on stderr.
fn failure(arguments: &[&str], stderr: &[u8]) -> String {
    let said = String::from_utf8_lossy(stderr);
    let first = said.lines().map(str::trim).find(|line| !line.is_empty());
    match first {
        Some(line) => format!("`{}` failed: {line}", typed(arguments, None)),
        None => format!("`{}` failed", typed(arguments, None)),
    }
}

/// A checkout of another revision, in a temporary directory, removed when it drops.
/// Scanning it is how rqx sees what the project looked like before: reading each
/// changed file's old text would answer the per-file rules and leave every cross-file
/// one — the graph, the clones, the churn — with nothing to read.
///
/// A process that is killed never drops it, so each checkout holds a lock on a file
/// beside its directory for as long as it lives. The next checkout of the same
/// repository removes every one whose lock nobody holds.
pub struct Worktree {
    path: PathBuf,
    root: PathBuf,
    /// Held, not read: the lock lasts as long as the file stays open.
    _held: Option<std::fs::File>,
}

/// What every temporary checkout's directory name begins with.
const CHECKOUT_PREFIX: &str = "rqx-base-";

/// How old a checkout with no lock beside it has to be before it counts as left
/// behind. Versions before the lock left none, and one of them may still be running.
const UNLOCKED_GRACE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

impl Worktree {
    pub fn add(root: &Path, revision: &str) -> Result<Self, String> {
        let revision = self::revision(revision)?;
        remove_abandoned_checkouts(root);
        let path = std::env::temp_dir().join(format!(
            "{CHECKOUT_PREFIX}{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos())
        ));
        let held = std::fs::File::create(lock_beside(&path))
            .ok()
            .filter(|file| file.try_lock().is_ok());
        let output = git(root)
            .args(["worktree", "add", "--detach", "--quiet"])
            .arg(&path)
            .arg(revision)
            .output()
            .map_err(|error| format!("git could not run: {error}"))?;
        let worktree = Self {
            path,
            root: root.to_path_buf(),
            _held: held,
        };
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        Ok(worktree)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        remove_checkout(&self.root, &self.path);
    }
}

fn lock_beside(checkout: &Path) -> PathBuf {
    checkout.with_extension("lock")
}

fn remove_checkout(root: &Path, checkout: &Path) {
    let _ = git(root)
        .args(["worktree", "remove", "--force"])
        .arg(checkout)
        .output();
    let _ = std::fs::remove_dir_all(checkout);
    let _ = std::fs::remove_file(lock_beside(checkout));
}

/// Removes the temporary checkouts of this repository that a process left behind when
/// it was killed: the ones git still lists whose lock nobody holds, or with no lock
/// and an hour old. A checkout another run is still using keeps its lock.
fn remove_abandoned_checkouts(root: &Path) {
    let Ok(output) = git(root)
        .args(["worktree", "list", "--porcelain", "-z"])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let temporary = std::env::temp_dir();
    let temporary = rqx_core::paths::canonicalize(&temporary).unwrap_or(temporary);
    for field in output.stdout.split(|byte| *byte == 0) {
        let Some(listed) = String::from_utf8_lossy(field)
            .strip_prefix("worktree ")
            .map(PathBuf::from)
        else {
            continue;
        };
        let ours = listed
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(CHECKOUT_PREFIX));
        let in_temporary = listed.parent().is_some_and(|parent| {
            parent == temporary
                || rqx_core::paths::canonicalize(parent).is_ok_and(|parent| parent == temporary)
        });
        if ours && in_temporary && is_abandoned(&listed) {
            remove_checkout(root, &listed);
        }
    }
}

fn is_abandoned(checkout: &Path) -> bool {
    match std::fs::File::open(lock_beside(checkout)) {
        Ok(lock) => lock.try_lock().is_ok(),
        Err(_) => std::fs::metadata(checkout)
            .and_then(|metadata| metadata.modified())
            .map_or(true, |modified| {
                modified.elapsed().unwrap_or_default() > UNLOCKED_GRACE
            }),
    }
}

/// The revision `base` and the working tree last had in common, which is what a
/// change is measured from.
pub fn merge_base(root: &Path, base: &str) -> Option<String> {
    let base = revision(base).ok()?;
    let output = git(root).args(["merge-base", base, "HEAD"]).output().ok()?;
    let revision = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (output.status.success() && !revision.is_empty()).then_some(revision)
}

/// A file as a revision held it, or `None` when the revision has no such file.
/// `relative` is relative to `root`, as findings name files.
pub fn file_at(root: &Path, revision: &str, relative: &str) -> Option<String> {
    let revision = self::revision(revision).ok()?;
    let output = git(root)
        .arg("show")
        .arg(format!("{revision}:./{relative}"))
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Files that differ from a base revision, or why they could not be listed: the
/// revision refused, or what git said. Uses the merge base so a stale branch is
/// judged on what it actually changed rather than on everything that landed on the
/// base since it forked.
pub fn changed_files(root: &Path, base: &str) -> Result<Vec<String>, String> {
    names_from(
        root,
        &["diff", "--name-only", "--diff-filter=d"],
        Some(base),
    )
}

/// Files a revision had that the working tree no longer has: deleted in a commit since
/// the merge base, or removed from the working tree and not yet committed. Named from
/// the repository root, as `changed_files` names them.
pub fn deleted_since(root: &Path, base: &str) -> Vec<String> {
    let committed = names_from(
        root,
        &["diff", "--name-only", "--diff-filter=D"],
        Some(base),
    );
    let uncommitted = names_from(root, &["ls-files", "--deleted", "--full-name"], None);
    committed.into_iter().chain(uncommitted).flatten().collect()
}

/// The paths `git` lists for a command, read NUL-separated: git quotes a path with a
/// non-ASCII character, a tab or a quote in it, and a quoted path matches no file.
fn names_from(root: &Path, args: &[&str], base: Option<&str>) -> Result<Vec<String>, String> {
    let range = base
        .map(|base| revision(base).map(|base| format!("{base}...HEAD")))
        .transpose()?;
    let mut arguments: Vec<&str> = args.to_vec();
    arguments.push("-z");
    arguments.extend(range.as_deref());
    let output = git(root)
        .args(&arguments)
        .output()
        .map_err(|error| format!("git could not run: {error}"))?;
    if !output.status.success() {
        return Err(failure(&arguments, &output.stderr));
    }
    Ok(nul_separated(&output.stdout))
}

/// Paths from git's `-z` output, in the order git listed them.
fn nul_separated(output: &[u8]) -> Vec<String> {
    output
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

/// Whether this clone was cut short. A CI runner fetches one commit by default, and
/// a diff against a base branch then has no ancestor to diff from.
pub fn is_shallow(root: &Path) -> bool {
    git(root)
        .args(["rev-parse", "--is-shallow-repository"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
}

/// The commit a baseline was taken at, recorded so whoever opens the file can tell
/// how old it is.
pub fn head(root: &Path) -> Option<String> {
    let output = git(root).args(["rev-parse", "HEAD"]).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|sha| !sha.is_empty())
}

/// When each line of each file was last written, for a list of files at once. Every
/// blame is its own process, so they wait on each other rather than on a core: two
/// hundred of them serially is eight seconds with the machine idle.
pub fn blame_dates_for(root: &Path, files: &[Arc<str>]) -> Vec<HashMap<u32, String>> {
    at_once(files, |file| blame_dates(root, file))
}

/// The same, carrying the author beside the date.
pub fn blame_files(root: &Path, files: &[Arc<str>]) -> Vec<HashMap<u32, (String, String)>> {
    at_once(files, |file| blame_file(root, file))
}

/// One answer per file, in the order the files came. A blame that panicked answers
/// with an empty `T`: its callers pair the answers back up with the files, and a
/// missing one would shift every later file onto the wrong dates.
fn at_once<T: Send + Sync + Default>(
    files: &[Arc<str>],
    blame: impl Fn(&str) -> T + Sync,
) -> Vec<T> {
    /// More processes than this and git's own start-up cost dominates; fewer and
    /// the wait is not covered.
    const AT_ONCE: usize = 8;

    // Workers take the next file as they finish rather than waiting for a batch to
    // drain, so one slow blame holds up one worker and not the seven beside it.
    let answers: Vec<OnceLock<T>> = files.iter().map(|_| OnceLock::new()).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..AT_ONCE.min(files.len()) {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    let (Some(file), Some(slot)) = (files.get(at), answers.get(at)) else {
                        break;
                    };
                    let answer = std::panic::catch_unwind(AssertUnwindSafe(|| blame(file)));
                    if let Ok(answer) = answer {
                        let _ = slot.set(answer);
                    }
                }
            });
        }
    });
    answers
        .into_iter()
        .map(|answer| answer.into_inner().unwrap_or_default())
        .collect()
}

/// Who last touched each line, and when. One process for the whole file: the
/// per-line form costs a process each, and the porcelain output already carries
/// every line.
pub fn blame_file(root: &Path, file: &str) -> HashMap<u32, (String, String)> {
    let Ok(output) = git(root).args(BLAME_ARGS).arg(file).output() else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // Porcelain names the commit on every line but describes it only once, so the
    // author and the date are gathered by sha and joined to the lines afterwards.
    let mut who: HashMap<&str, (String, String)> = HashMap::new();
    let mut lines: Vec<(u32, &str)> = Vec::new();
    let mut sha = "";
    for entry in text.lines() {
        if let Some((head, rest)) = entry.split_once(' ')
            && head.len() == 40
            && head.chars().all(|c| c.is_ascii_hexdigit())
        {
            sha = head;
            if let Some(number) = rest
                .split_whitespace()
                .nth(1)
                .and_then(|number| number.parse().ok())
            {
                lines.push((number, head));
            }
            continue;
        }
        if let Some(name) = entry.strip_prefix("author ") {
            who.entry(sha).or_default().0 = name.trim().to_owned();
        }
        if let Some(stamp) = entry.strip_prefix("author-time ")
            && let Ok(seconds) = stamp.trim().parse::<i64>()
        {
            who.entry(sha).or_default().1 = rqx_core::suppress::civil_date(seconds / 86_400);
        }
    }
    lines
        .into_iter()
        .filter_map(|(line, sha)| Some((line, who.get(sha)?.clone())))
        .collect()
}

/// When each line of one file was last written, as a civil date. One `git blame`
/// answers for the whole file, which is what makes this affordable on a scan: the
/// per-line form costs a process each.
pub fn blame_dates(root: &Path, file: &str) -> HashMap<u32, String> {
    let Ok(output) = git(root).args(BLAME_ARGS).arg(file).output() else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // Porcelain names the commit on every line but describes it only once, so the
    // dates are gathered by sha and joined to the lines afterwards.
    let mut when: HashMap<&str, String> = HashMap::new();
    let mut lines: Vec<(u32, &str)> = Vec::new();
    let mut sha = "";
    for entry in text.lines() {
        if let Some((head, rest)) = entry.split_once(' ')
            && head.len() == 40
            && head.chars().all(|c| c.is_ascii_hexdigit())
        {
            sha = head;
            if let Some(number) = rest
                .split_whitespace()
                .nth(1)
                .and_then(|number| number.parse().ok())
            {
                lines.push((number, head));
            }
            continue;
        }
        if let Some(stamp) = entry.strip_prefix("author-time ")
            && let Ok(seconds) = stamp.trim().parse::<i64>()
        {
            when.insert(sha, rqx_core::suppress::civil_date(seconds / 86_400));
        }
    }
    let dates: HashMap<u32, String> = lines
        .into_iter()
        .filter_map(|(line, sha)| Some((line, when.get(sha)?.clone())))
        .collect();
    dates
}

/// Uncommitted work counts as changed too, so `rqx check` is useful before a commit
/// exists rather than only in CI.
pub fn uncommitted(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for args in [
        ["diff", "--name-only", "-z", "HEAD"],
        ["ls-files", "--others", "--exclude-standard", "-z"],
    ] {
        let Ok(output) = git(root).args(args).output() else {
            continue;
        };
        if output.status.success() {
            files.extend(nul_separated(&output.stdout));
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
    fn a_command_is_described_as_it_would_be_typed() {
        assert_eq!(
            blame_command("src/a.ts"),
            "git blame --porcelain -- src/a.ts"
        );
        assert_eq!(typed(&PREFIX_ARGS, None), "git rev-parse --show-prefix");
    }

    #[test]
    fn reading_history_records_each_command_it_ran() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));

        let history = History::read(root, 30);

        assert_eq!(history.commands().len() as u32, history.git_processes());
        assert_eq!(history.commands()[0], "git rev-parse --show-prefix");
    }

    #[test]
    fn a_blame_that_panicked_keeps_its_place() {
        let files: Vec<Arc<str>> = (0..20).map(|n| Arc::from(n.to_string())).collect();

        let answers = at_once(&files, |file| {
            assert_ne!(file, "3", "git blame blew up");
            vec![file.to_owned()]
        });

        assert_eq!(answers.len(), files.len());
        for (file, answer) in files.iter().zip(&answers) {
            let expected = if &**file == "3" {
                vec![]
            } else {
                vec![file.to_string()]
            };
            assert_eq!(answer, &expected);
        }
    }

    /// With batches of eight, the first blame would wait on the ninth, which could
    /// only start once the whole first batch was done, and neither would ever finish.
    #[test]
    fn a_blame_that_waits_on_a_later_one_does_not_hold_up_the_queue() {
        let files: Vec<Arc<str>> = (0..9).map(|n| Arc::from(n.to_string())).collect();
        let last_ran = std::sync::atomic::AtomicBool::new(false);

        let answers = at_once(&files, |file| {
            if file == "0" {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !last_ran.load(Ordering::Relaxed) && std::time::Instant::now() < deadline {
                    std::thread::yield_now();
                }
            }
            if file == "8" {
                last_ran.store(true, Ordering::Relaxed);
            }
            last_ran.load(Ordering::Relaxed)
        });

        assert!(answers[0], "the first blame never saw the last one run");
    }

    #[test]
    fn no_more_blames_run_at_once_than_the_limit() {
        let files: Vec<Arc<str>> = (0..40).map(|n| Arc::from(n.to_string())).collect();
        let running = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);

        let answers = at_once(&files, |file| {
            let now = running.fetch_add(1, Ordering::Relaxed) + 1;
            most.fetch_max(now, Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(5));
            running.fetch_sub(1, Ordering::Relaxed);
            file.to_owned()
        });

        assert!(most.load(Ordering::Relaxed) <= 8);
        assert_eq!(
            answers,
            files
                .iter()
                .map(|file| file.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_log_read_with_nul_separators_keeps_every_name() {
        let output = "ana\ncafé.ts\0ta\tb.ts\0new\nline.ts\0q\"uote.ts\0\0bo\nbase.ts\0";
        let (commits, authors) = commits_from(output.as_bytes());
        assert_eq!(authors, ["ana", "bo"]);
        assert_eq!(
            commits,
            [
                vec!["café.ts", "ta\tb.ts", "new\nline.ts", "q\"uote.ts"],
                vec!["base.ts"]
            ]
        );
    }

    #[test]
    fn a_revision_that_begins_with_a_dash_is_refused() {
        assert_eq!(revision("HEAD~1"), Ok("HEAD~1"));
        assert_eq!(revision("origin/main"), Ok("origin/main"));
        assert!(revision("--output=x").is_err());
        assert!(revision("-p").is_err());
        assert!(revision("").is_err());
    }

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

    /// Running the blames at once has to answer what running them one at a time
    /// answers; the only thing that may differ is how long it took.
    #[test]
    fn blaming_together_agrees_with_blaming_one_at_a_time() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the workspace root");
        if !root.join(".git").exists() {
            return;
        }
        let files: Vec<Arc<str>> = ["Cargo.toml", "README.md", "crates/rqx-cli/src/history.rs"]
            .into_iter()
            .filter(|file| root.join(file).exists())
            .map(Arc::from)
            .collect();
        if files.is_empty() {
            return;
        }

        let together = blame_dates_for(root, &files);
        assert_eq!(together.len(), files.len(), "one answer per file, in order");
        for (file, batched) in files.iter().zip(together) {
            assert_eq!(batched, blame_dates(root, file), "{file} disagreed");
        }
    }
}

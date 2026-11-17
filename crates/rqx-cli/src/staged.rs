//! `check --staged`: review what `git commit` would record, not what the working tree
//! holds.
//!
//! The scan reads files from a directory, so the index is turned into one: a copy of
//! the repository in a temporary directory where every staged file has its staged
//! content and every other file is the working tree's. An unstaged edit to a staged
//! file therefore cannot change the verdict, and neither can an unstaged edit that
//! would have fixed it.
//!
//! The rest of the project is read from the working tree, not the index. A staged
//! file is judged against the files around it as they are on disk, which is what the
//! cross-file rules need to be right; reviewing a second, fully staged snapshot would
//! make the verdict depend on what else happens to be staged.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::history;

/// What the index differs from `HEAD` by, named from the top of the repository.
struct Changes {
    written: BTreeSet<String>,
    removed: BTreeSet<String>,
}

impl Changes {
    fn read(toplevel: &Path) -> Result<Self> {
        Ok(Self {
            written: named(toplevel, &["--diff-filter=d"])?,
            removed: named(toplevel, &["--diff-filter=D"])?,
        })
    }

    fn touches(&self, file: &str) -> bool {
        self.written.contains(file) || self.removed.contains(file)
    }
}

/// The repository as the next commit would hold the staged files, in a directory that
/// is removed when this is dropped.
pub struct Overlay {
    directory: PathBuf,
    root: PathBuf,
    /// The staged files that exist under the scan root, named the way a finding names
    /// them.
    pub files: BTreeSet<String>,
}

impl Overlay {
    pub fn build(root: &Path) -> Result<Self> {
        if root.is_file() {
            bail!(
                "`--staged` reviews a repository, and `{}` is a file; name its directory",
                root.display()
            );
        }
        let toplevel = toplevel_of(root)?;
        let prefix = history::repository_prefix(root).unwrap_or_default();
        let changes = Changes::read(&toplevel)?;

        let directory = temporary_directory();
        std::fs::create_dir_all(&directory)
            .with_context(|| format!("creating {}", directory.display()))?;
        let overlay = Self {
            root: directory.join(&prefix),
            files: changes
                .written
                .iter()
                .filter_map(|file| file.strip_prefix(&prefix).map(str::to_owned))
                .collect(),
            directory,
        };
        overlay.populate(&toplevel, &changes)?;
        Ok(overlay)
    }

    /// The directory to scan: the same place in the overlay as the scanned directory
    /// is in the repository.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn populate(&self, toplevel: &Path, changes: &Changes) -> Result<()> {
        for file in working_tree_files(toplevel)? {
            if !changes.touches(&file) {
                place(&toplevel.join(&file), &self.directory.join(&file))?;
            }
        }
        for file in &changes.written {
            write_staged(toplevel, file, &self.directory.join(file))?;
        }
        // History rules and the search for a repository boundary both look for it.
        // Failing to link it only costs the history-based findings.
        let _ = link(&toplevel.join(".git"), &self.directory.join(".git"));
        Ok(())
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn temporary_directory() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    std::env::temp_dir().join(format!("rqx-staged-{}-{nanos}", std::process::id()))
}

/// A `git` in `directory`. A hook run by `git commit -a` or `git commit <path>` is
/// handed a temporary index through a relative `GIT_INDEX_FILE`, and `-C` would
/// otherwise make that path point somewhere else.
fn git(directory: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(directory);
    if let Some(index) = std::env::var_os("GIT_INDEX_FILE")
        && let Ok(index) = std::path::absolute(index)
    {
        command.env("GIT_INDEX_FILE", index);
    }
    command
}

fn run(mut command: Command, description: &str) -> Result<Vec<u8>> {
    let output = command
        .output()
        .with_context(|| format!("running `{description}`"))?;
    if !output.status.success() {
        bail!(
            "`{description}` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

fn toplevel_of(root: &Path) -> Result<PathBuf> {
    let mut command = git(root);
    command.args(["rev-parse", "--show-toplevel"]);
    let output = run(command, "git rev-parse --show-toplevel")
        .context("`--staged` needs a git repository to read the index from")?;
    Ok(PathBuf::from(String::from_utf8_lossy(&output).trim()))
}

/// NUL-separated, because it is the only form that survives a file name with a
/// newline in it.
fn names(output: &[u8]) -> BTreeSet<String> {
    output
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

fn named(toplevel: &Path, filter: &[&str]) -> Result<BTreeSet<String>> {
    let mut command = git(toplevel);
    command
        .args(["diff", "--cached", "--name-only", "-z", "--no-renames"])
        .args(filter);
    run(command, "git diff --cached --name-only").map(|output| names(&output))
}

/// Every file the project has: tracked, or untracked and not ignored.
fn working_tree_files(toplevel: &Path) -> Result<BTreeSet<String>> {
    let mut command = git(toplevel);
    command.args([
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
    ]);
    run(command, "git ls-files").map(|output| names(&output))
}

/// A hard link where the filesystem allows one, so a large project is not copied for
/// every commit. The overlay is only read, and the staged files are written as new
/// files rather than through a link, so the working tree is never touched.
fn place(from: &Path, to: &Path) -> Result<()> {
    // A tracked file deleted from the working tree, a symbolic link and a submodule
    // have no content to put here.
    if !from.symlink_metadata().is_ok_and(|found| found.is_file()) {
        return Ok(());
    }
    create_parent(to)?;
    if std::fs::hard_link(from, to).is_err() {
        std::fs::copy(from, to).with_context(|| format!("copying {}", from.display()))?;
    }
    Ok(())
}

fn write_staged(toplevel: &Path, file: &str, to: &Path) -> Result<()> {
    let mut command = git(toplevel);
    command.arg("show").arg(format!(":{file}"));
    let contents = run(command, &format!("git show :{file}"))?;
    create_parent(to)?;
    std::fs::write(to, contents).with_context(|| format!("writing {}", to.display()))
}

fn create_parent(file: &Path) -> Result<()> {
    let Some(parent) = file.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))
}

#[cfg(unix)]
fn link(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn link(source: &Path, target: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(source, target)
    } else {
        std::os::windows::fs::symlink_file(source, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_split_on_nul_so_a_newline_stays_in_its_name() {
        let found = names(b"src/a.ts\0odd\nname.ts\0");
        assert_eq!(
            found.iter().map(String::as_str).collect::<Vec<_>>(),
            ["odd\nname.ts", "src/a.ts"]
        );
    }

    #[test]
    fn no_names_is_an_empty_set() {
        assert!(names(b"").is_empty());
    }
}

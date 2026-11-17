//! What was typed in earlier sessions, kept per user.
//!
//! One line per entry in the platform's state directory, never in the repository:
//! commands name files and rules of whatever project was open, and none of that belongs
//! in a checkout. Everything here is best effort. A history that cannot be read or
//! written costs the arrow keys a memory and nothing else.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The most entries read back, which is also what the prompt keeps.
const KEPT: usize = 1000;

pub struct Recall {
    file: Option<PathBuf>,
}

impl Recall {
    pub fn open() -> Self {
        Self {
            file: history_file(
                std::env::var_os("XDG_STATE_HOME"),
                std::env::var_os("HOME"),
                std::env::var_os("LOCALAPPDATA"),
            ),
        }
    }

    #[cfg(test)]
    fn at(file: &Path) -> Self {
        Self {
            file: Some(file.to_path_buf()),
        }
    }

    /// The newest entries, oldest first.
    pub fn load(&self) -> Vec<String> {
        let Some(text) = self
            .file
            .as_deref()
            .and_then(|file| std::fs::read_to_string(file).ok())
        else {
            return Vec::new();
        };
        let mut entries: Vec<String> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect();
        let excess = entries.len().saturating_sub(KEPT);
        entries.drain(..excess);
        entries
    }

    pub fn append(&self, entry: &str) {
        let entry = entry.trim();
        let Some(file) = self.file.as_deref() else {
            return;
        };
        if entry.is_empty() || entry.contains('\n') {
            return;
        }
        let _ = write_line(file, entry);
    }
}

fn write_line(file: &Path, entry: &str) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?;
    writeln!(handle, "{entry}")
}

/// `$XDG_STATE_HOME`, else `~/.local/state`, else the Windows local data directory.
fn history_file(
    xdg_state: Option<OsString>,
    home: Option<OsString>,
    local_app_data: Option<OsString>,
) -> Option<PathBuf> {
    let directory = match (xdg_state, home, local_app_data) {
        (Some(state), _, _) if !state.is_empty() => PathBuf::from(state),
        (_, Some(home), _) if !home.is_empty() => PathBuf::from(home).join(".local").join("state"),
        (_, _, Some(local)) if !local.is_empty() => PathBuf::from(local),
        _ => return None,
    };
    Some(directory.join("rqx").join("session-history"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("rqx-recall-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    #[test]
    fn the_state_directory_wins_over_the_home_directory() {
        let file = history_file(Some("/state".into()), Some("/home/jad".into()), None);
        assert_eq!(file, Some(PathBuf::from("/state/rqx/session-history")));
    }

    #[test]
    fn without_one_the_history_lives_under_local_state_in_the_home_directory() {
        let file = history_file(None, Some("/home/jad".into()), None);
        assert_eq!(
            file,
            Some(PathBuf::from("/home/jad/.local/state/rqx/session-history"))
        );
    }

    #[test]
    fn with_nowhere_to_put_it_there_is_no_history_and_no_error() {
        assert_eq!(history_file(None, None, None), None);
        assert_eq!(history_file(Some("".into()), Some("".into()), None), None);
        let recall = Recall { file: None };
        recall.append("/plan");
        assert!(recall.load().is_empty());
    }

    #[test]
    fn entries_come_back_in_the_order_they_were_typed() {
        let directory = scratch("order");
        let recall = Recall::at(&directory.join("nested").join("history"));
        recall.append("/plan --budget 20");
        recall.append("  /health  ");
        recall.append("");
        assert_eq!(recall.load(), ["/plan --budget 20", "/health"]);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn only_the_newest_entries_are_read_back() {
        let directory = scratch("tail");
        let file = directory.join("history");
        std::fs::create_dir_all(&directory).unwrap();
        let lines: Vec<String> = (0..KEPT + 5)
            .map(|number| format!("/cmd {number}"))
            .collect();
        std::fs::write(&file, lines.join("\n")).unwrap();
        let loaded = Recall::at(&file).load();
        assert_eq!(loaded.len(), KEPT);
        assert_eq!(loaded[0], "/cmd 5");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_missing_file_is_an_empty_history() {
        let recall = Recall::at(&scratch("missing").join("history"));
        assert!(recall.load().is_empty());
    }
}

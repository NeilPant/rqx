//! `rqx scan --watch`, and the file stamps it shares with the interface's poll.
//! Both ask the same question of the tree, so both get the same answer from here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;

use crate::config::Resolved;
use crate::scan;
use crate::term::{self, Stream};

pub(crate) const BETWEEN_WALKS: Duration = Duration::from_millis(400);

/// What a file looked like when it was last analysed. Size joins the timestamp
/// because a filesystem with one-second timestamps would otherwise miss a save
/// inside the same second.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub modified: Option<SystemTime>,
    pub size: u64,
}

/// Every file the scan would read, with enough of its metadata to tell a save from a
/// re-read. The walk opens nothing: one `stat` per file is what makes polling
/// affordable.
pub fn stamps(root: &Path, settings: &Resolved) -> Result<BTreeMap<PathBuf, Stamp>> {
    let walker = scan::build_walker(root, &settings.config)?;
    let found = std::sync::Mutex::new(BTreeMap::new());
    walker.run(|| {
        Box::new(|entry| {
            if let Ok(entry) = entry
                && entry.file_type().is_some_and(|kind| kind.is_file())
            {
                let data = entry.metadata().ok();
                found.lock().expect("the walk map").insert(
                    entry.path().to_path_buf(),
                    Stamp {
                        modified: data.as_ref().and_then(|meta| meta.modified().ok()),
                        size: data.as_ref().map_or(0, std::fs::Metadata::len),
                    },
                );
            }
            ignore::WalkState::Continue
        })
    });
    Ok(found.into_inner().expect("the walk map"))
}

/// Scan, print, and scan again whenever a file changes. Polling rather than an OS
/// watcher: the scan already walks the tree to find its files, so the walk that
/// answers "has anything changed" is one it knows how to do.
pub fn watch<T>(
    path: &Path,
    settings: &Resolved,
    run: impl Fn(&Resolved) -> Result<T>,
) -> Result<T> {
    let started = Instant::now();
    let mut seen = stamps(path, settings)?;
    let mut first = true;
    loop {
        separate_runs(first, started)?;
        first = false;
        run(settings)?;
        eprintln!("\n  watching {} \u{b7} ctrl-c to stop", path.display());
        loop {
            std::thread::sleep(BETWEEN_WALKS);
            let now = stamps(path, settings)?;
            if now != seen {
                seen = now;
                break;
            }
        }
    }
}

/// A terminal is cleared before each run so it holds one report rather than a scroll
/// of them. Anywhere else the escape sequence would land in a file or a pipe as
/// garbage, so the runs are told apart by a line instead.
fn separate_runs(first: bool, started: Instant) -> Result<()> {
    if term::is_terminal(Stream::Stdout) {
        print!("\u{1b}[2J\u{1b}[H");
        std::io::Write::flush(&mut std::io::stdout())?;
    } else if !first {
        println!("{}", separator(started.elapsed()));
    }
    Ok(())
}

/// Elapsed time since the watch began rather than the clock: rqx carries no time
/// library, and the offset still orders the runs in a log.
fn separator(since_start: Duration) -> String {
    let seconds = since_start.as_secs();
    format!(
        "\u{2500}\u{2500} rescanned after {:02}:{:02}:{:02} \u{2500}\u{2500}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_separator_counts_hours_minutes_and_seconds_since_the_start() {
        assert_eq!(
            separator(Duration::from_secs(3_723)),
            "\u{2500}\u{2500} rescanned after 01:02:03 \u{2500}\u{2500}"
        );
    }
}

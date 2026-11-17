//! `/watch on`: the project held in memory and re-analysed when a file's stamp moves.
//!
//! The polling is the one `rqx scan --watch` does, and the re-analysis is the one the
//! old interface did: the changed files alone, with the cross-file stage run over
//! everything. It happens off the input thread, so typing stays live while it runs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::project::Project;
use crate::scan::ScanOutcome;
use crate::watch::BETWEEN_WALKS;

use super::job::{Finished, Job};

/// How often to look at a refresh that is running.
const CHECK: Duration = Duration::from_millis(25);

/// The outcome before and after a refresh that found something to re-read.
pub struct Change {
    pub before: Arc<ScanOutcome>,
    pub after: Arc<ScanOutcome>,
}

pub struct Refreshed {
    project: Box<Project>,
    change: Option<Change>,
}

pub enum Watcher {
    Idle {
        project: Box<Project>,
        due: Instant,
    },
    Refreshing(Job<Refreshed>),
    /// Between states, and what a failed refresh leaves behind.
    Stopped,
}

impl Watcher {
    pub fn new(project: Box<Project>) -> Self {
        Self::Idle {
            project,
            due: Instant::now() + BETWEEN_WALKS,
        }
    }

    /// How long the loop may sleep before this wants attention.
    pub fn wakeup(&self) -> Duration {
        match self {
            Self::Idle { due, .. } => due.saturating_duration_since(Instant::now()),
            Self::Refreshing(_) => CHECK,
            Self::Stopped => Duration::MAX,
        }
    }

    /// Starts a refresh when one is due and collects the one that finished. An `Err`
    /// is a refresh that failed, after which there is nothing left to watch with.
    pub fn advance(&mut self) -> Result<Option<Change>, String> {
        match std::mem::replace(self, Self::Stopped) {
            Self::Idle { project, due } if Instant::now() >= due => {
                *self = Self::Refreshing(Job::start(move |_| refresh(project)));
                Ok(None)
            }
            Self::Refreshing(job) if job.finished() => match job.join() {
                Finished::Done(done) => {
                    *self = Self::new(done.project);
                    Ok(done.change)
                }
                Finished::Cancelled => Ok(None),
                Finished::Failed(message) => Err(message),
            },
            other => {
                *self = other;
                Ok(None)
            }
        }
    }
}

fn refresh(mut project: Box<Project>) -> Result<Refreshed> {
    let before = project.outcome();
    let seen = project.walk()?;
    let moved = project.refresh(&seen);
    let change = moved.then(|| Change {
        before,
        after: project.outcome(),
    });
    Ok(Refreshed { project, change })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_file_comes_back_as_a_change_in_what_was_found() {
        let root = std::env::temp_dir().join(format!("rqx-watching-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temp project");
        std::fs::write(root.join("a.ts"), "export const a = 1\n").expect("a file");
        let settings = crate::config::resolve(&root, None, None).expect("settings");
        let project = Project::read(&root, settings).expect("a project");
        let mut watcher = Watcher::new(Box::new(project));

        std::fs::write(
            root.join("a.ts"),
            "export function f(): number {\n  return 1\n  console.log('dead')\n}\n",
        )
        .expect("the save");

        let deadline = Instant::now() + Duration::from_secs(20);
        let change = loop {
            assert!(Instant::now() < deadline, "the save was never noticed");
            if let Some(change) = watcher.advance().expect("a refresh") {
                break change;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(change.after.counts.total > change.before.counts.total);
        let _ = std::fs::remove_dir_all(&root);
    }
}

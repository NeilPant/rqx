use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Instant, SystemTime};

use anyhow::Result;

use crate::config::Resolved;
use crate::history::History;
use crate::progress::Bar;
use crate::scan::{self, ScanMessage, ScanOutcome};

/// What a file looked like when it was last analysed. Size joins the timestamp
/// because a filesystem with one-second timestamps would otherwise miss a save
/// inside the same second.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub modified: Option<SystemTime>,
    pub size: u64,
}

struct Cached {
    stamp: Stamp,
    message: ScanMessage,
}

#[derive(Clone, Copy, serde::Serialize)]
pub struct Tick {
    pub at: u64,
    pub health: u32,
    pub findings: u32,
}

/// One file's turn through the analyser, for the activity feed.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub file: String,
    pub resolved: u32,
    pub introduced: u32,
    pub findings: u32,
}

/// The project as it currently stands, held in memory so a save re-analyses one file
/// rather than twelve thousand. The cross-file stage still runs over everything,
/// because a deleted import changes what the whole graph says.
pub struct Project {
    root: PathBuf,
    settings: Resolved,
    files: HashMap<PathBuf, Cached>,
    history: Arc<History>,
    read_history_at: Instant,
    outcome: Arc<ScanOutcome>,
    trend: VecDeque<Tick>,
}

/// Long enough that a poll does not shell out to git every second, short enough that
/// a commit shows up in the impact weighting while you are still looking at it.
const HISTORY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
const TREND_POINTS: usize = 500;

impl Project {
    pub fn read(root: &Path, settings: Resolved) -> Result<Self> {
        let history = Arc::new(History::read(root, settings.config.scan.churn_days));
        let outcome = Arc::new(scan::scan(root, &settings)?);
        let mut project = Self {
            root: root.to_path_buf(),
            settings,
            files: HashMap::new(),
            history,
            read_history_at: Instant::now(),
            outcome,
            trend: VecDeque::new(),
        };
        let seen = project.walk()?;
        project.analyse(&seen);
        project.rebuild();
        Ok(project)
    }

    pub fn outcome(&self) -> Arc<ScanOutcome> {
        Arc::clone(&self.outcome)
    }

    pub fn trend(&self) -> Vec<Tick> {
        self.trend.iter().copied().collect()
    }

    /// Every file the walker reaches, with what it would take to notice a change.
    /// The walk opens nothing: one `stat` per file is what makes polling affordable.
    pub fn walk(&self) -> Result<BTreeMap<PathBuf, Stamp>> {
        let walker = scan::build_walker(&self.root, &self.settings.config)?;
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

    /// Re-analyse what changed, drop what is gone, and rebuild the cross-file answer.
    /// Returns what moved, or `None` when the walk found nothing new.
    pub fn refresh(&mut self, seen: &BTreeMap<PathBuf, Stamp>) -> Option<Vec<Change>> {
        let changed: Vec<PathBuf> = seen
            .iter()
            .filter(|(path, stamp)| {
                self.files
                    .get(*path)
                    .is_none_or(|cached| cached.stamp != **stamp)
            })
            .map(|(path, _)| path.clone())
            .collect();
        let removed: Vec<PathBuf> = self
            .files
            .keys()
            .filter(|path| !seen.contains_key(*path))
            .cloned()
            .collect();
        if changed.is_empty() && removed.is_empty() {
            return None;
        }

        let before = self.per_file_counts();
        for path in &removed {
            self.files.remove(path);
        }
        let subset: BTreeMap<PathBuf, Stamp> = changed
            .iter()
            .filter_map(|path| seen.get(path).map(|stamp| (path.clone(), *stamp)))
            .collect();
        self.analyse(&subset);

        if self.read_history_at.elapsed() >= HISTORY_INTERVAL {
            self.history = Arc::new(History::read(
                &self.root,
                self.settings.config.scan.churn_days,
            ));
            self.read_history_at = Instant::now();
        }
        self.rebuild();

        let after = self.per_file_counts();
        let mut moved: Vec<Change> = changed
            .iter()
            .map(|path| {
                let file = scan::relative_to(&self.root, path);
                let was = before.get(&file).copied().unwrap_or(0);
                let is = after.get(&file).copied().unwrap_or(0);
                Change {
                    file,
                    resolved: was.saturating_sub(is),
                    introduced: is.saturating_sub(was),
                    findings: is,
                }
            })
            .collect();
        moved.extend(removed.iter().map(|path| {
            Change {
                file: scan::relative_to(&self.root, path),
                resolved: before
                    .get(&scan::relative_to(&self.root, path))
                    .copied()
                    .unwrap_or(0),
                introduced: 0,
                findings: 0,
            }
        }));
        Some(moved)
    }

    fn per_file_counts(&self) -> BTreeMap<String, u32> {
        self.outcome
            .findings_by_file
            .iter()
            .map(|(file, count)| (file.to_string(), *count))
            .collect()
    }

    fn analyse(&mut self, files: &BTreeMap<PathBuf, Stamp>) {
        for (path, stamp) in files {
            let message =
                scan::analyze_file(path, &self.root, Some(stamp.size), &self.settings, None);
            self.files.insert(
                path.clone(),
                Cached {
                    stamp: *stamp,
                    message,
                },
            );
        }
    }

    /// The same collector the command line runs, fed from the map instead of from a
    /// walk. Feeding it everything each time is what keeps a served report identical
    /// to a scanned one: the graph, the clone index and the churn rules all read the
    /// whole project or they read it wrong.
    fn rebuild(&mut self) {
        let (sender, receiver) = channel();
        for cached in self.files.values() {
            let _ = sender.send(cached.message.clone());
        }
        drop(sender);

        let outcome = scan::collect(
            receiver,
            &self.root,
            &self.settings,
            Bar::hidden(),
            None,
            None,
            || Arc::clone(&self.history),
        );
        let health = crate::report::Report::everything(&outcome).health();
        let findings = outcome.counts.total;
        self.outcome = Arc::new(outcome);

        if self.trend.len() == TREND_POINTS {
            self.trend.pop_front();
        }
        self.trend.push_back(Tick {
            at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            health,
            findings,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rqx-serve-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temp project");
        for (path, text) in files {
            std::fs::write(root.join(path), text).expect("a file");
        }
        root
    }

    fn slugs(outcome: &ScanOutcome) -> Vec<String> {
        let mut found: Vec<String> = outcome
            .findings
            .iter()
            .map(|finding| format!("{} {}", finding.slug(), finding.location.file))
            .collect();
        found.sort();
        found
    }

    /// A served report and a scanned one have to be the same report, or the dashboard
    /// is showing something the command line would not agree with.
    #[test]
    fn refreshing_one_file_answers_what_a_whole_scan_would() {
        let root = fixture(
            "equivalence",
            &[
                ("a.ts", "export function keep(): number {\n  return 1\n}\n"),
                (
                    "b.ts",
                    "import { keep } from './a'\nexport const used = keep()\n",
                ),
            ],
        );
        let settings = crate::config::resolve(&root, None, None).expect("settings");
        let mut project = Project::read(&root, settings.clone()).expect("a project");

        std::fs::write(
            root.join("a.ts"),
            "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n",
        )
        .expect("the edit");

        let seen = project.walk().expect("a walk");
        let moved = project.refresh(&seen).expect("the edit was noticed");
        assert!(
            moved.iter().any(|change| change.file == "a.ts"),
            "{:?}",
            moved.iter().map(|c| &c.file).collect::<Vec<_>>()
        );

        let fresh = scan::scan(&root, &settings).expect("a scan");
        assert_eq!(slugs(&project.outcome()), slugs(&fresh));
        assert_eq!(project.outcome().counts.total, fresh.counts.total);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unchanged_project_reports_nothing_moved() {
        let root = fixture("quiet", &[("a.ts", "export const a = 1\n")]);
        let settings = crate::config::resolve(&root, None, None).expect("settings");
        let mut project = Project::read(&root, settings).expect("a project");
        let seen = project.walk().expect("a walk");
        assert!(project.refresh(&seen).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_deleted_file_leaves_the_report() {
        let root = fixture(
            "deleted",
            &[
                ("a.ts", "export const a = 1\n"),
                (
                    "gone.ts",
                    "export function f(){\n  return 1\n  console.log(1)\n}\n",
                ),
            ],
        );
        let settings = crate::config::resolve(&root, None, None).expect("settings");
        let mut project = Project::read(&root, settings).expect("a project");
        std::fs::remove_file(root.join("gone.ts")).expect("removed");

        let seen = project.walk().expect("a walk");
        project.refresh(&seen).expect("the deletion was noticed");
        assert!(
            !project
                .outcome()
                .findings
                .iter()
                .any(|finding| &*finding.location.file == "gone.ts")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

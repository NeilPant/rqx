use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::Instant;

use anyhow::Result;

use crate::config::Resolved;
use crate::history::History;
use crate::progress::Bar;
use crate::scan::{self, ScanMessage, ScanOutcome};
use crate::watch::{Stamp, stamps};

struct Cached {
    stamp: Stamp,
    message: ScanMessage,
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
}

/// Long enough that a poll does not shell out to git every second, short enough that
/// a commit shows up in the impact weighting while you are still looking at it.
const HISTORY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

impl Project {
    pub fn read(root: &Path, settings: Resolved) -> Result<Self> {
        let history = Arc::new(History::read(root, settings.config.scan.churn_days));
        let mut files = HashMap::new();
        analyse_into(&mut files, root, &settings, &stamps(root, &settings)?);
        let outcome = Arc::new(rebuild_from(root, &settings, &history, &files));
        Ok(Self {
            root: root.to_path_buf(),
            settings,
            files,
            history,
            read_history_at: Instant::now(),
            outcome,
        })
    }

    pub fn outcome(&self) -> Arc<ScanOutcome> {
        Arc::clone(&self.outcome)
    }

    /// Every file the walker reaches, with what it would take to notice a change.
    /// The walk opens nothing: one `stat` per file is what makes polling affordable.
    pub fn walk(&self) -> Result<BTreeMap<PathBuf, Stamp>> {
        stamps(&self.root, &self.settings)
    }
}

impl Project {
    /// Re-analyse what changed, drop what is gone, and rebuild the cross-file answer.
    /// Says whether anything moved, so a caller can skip a repaint that would show
    /// the same thing.
    pub fn refresh(&mut self, seen: &BTreeMap<PathBuf, Stamp>) -> bool {
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
            return false;
        }

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
        true
    }

    fn analyse(&mut self, files: &BTreeMap<PathBuf, Stamp>) {
        analyse_into(&mut self.files, &self.root, &self.settings, files);
    }

    /// The same collector the command line runs, fed from the map instead of from a
    /// walk. Feeding it everything each time is what keeps a refreshed report identical
    /// to a scanned one: the graph, the clone index and the churn rules all read the
    /// whole project or they read it wrong.
    fn rebuild(&mut self) {
        self.outcome = Arc::new(rebuild_from(
            &self.root,
            &self.settings,
            &self.history,
            &self.files,
        ));
    }
}

fn analyse_into(
    into: &mut HashMap<PathBuf, Cached>,
    root: &Path,
    settings: &Resolved,
    files: &BTreeMap<PathBuf, Stamp>,
) {
    for (path, stamp) in files {
        let message = scan::analyze_file(path, root, Some(stamp.size), settings, None);
        into.insert(
            path.clone(),
            Cached {
                stamp: *stamp,
                message,
            },
        );
    }
}

fn rebuild_from(
    root: &Path,
    settings: &Resolved,
    history: &Arc<History>,
    files: &HashMap<PathBuf, Cached>,
) -> ScanOutcome {
    let (sender, receiver) = channel();
    for cached in files.values() {
        let _ = sender.send(cached.message.clone());
    }
    drop(sender);

    scan::collect(receiver, root, settings, Bar::hidden(), None, None, || {
        Arc::clone(history)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rqx-project-{name}-{}", std::process::id()));
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

    /// A refreshed report and a scanned one have to be the same report, or the
    /// interface is showing something the command line would not agree with.
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
        assert!(project.refresh(&seen), "the edit was noticed");

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
        assert!(!project.refresh(&seen));
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
        assert!(project.refresh(&seen), "the deletion was noticed");
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

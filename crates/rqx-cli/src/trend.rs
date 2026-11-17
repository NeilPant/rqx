//! The health of a project over time. A scan can append one snapshot of itself to an
//! NDJSON file; this module writes those lines and reads them back. The file is plain
//! text that a CI cache can carry from run to run and `git` can diff.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rqx_core::Cleanliness;
use rqx_core::finding::Severity;
use serde::{Deserialize, Serialize};

use crate::scan::ScanOutcome;

/// Where `--record` writes when it is given no path.
pub const DEFAULT_FILE: &str = ".rqx/trend.ndjson";

/// The shape of a snapshot line. Increases only if a key has to change meaning, so a
/// reader can skip a line newer than it understands.
const SCHEMA_VERSION: u32 = 1;

const KIND: &str = "snapshot";

/// One scan, reduced to what a trend needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default = "kind")]
    pub kind: String,
    pub schema_version: u32,
    /// The commit that was scanned, when the project is a git repository.
    pub commit: Option<String>,
    /// UTC, as `YYYY-MM-DDTHH:MM:SSZ`.
    pub time: String,
    pub rqx_version: String,
    pub health: u32,
    pub grade: String,
    pub files: u32,
    pub counts: Counts,
    /// Findings per rule, so a trend can say which rule moved.
    pub rules: BTreeMap<String, u32>,
    /// Health per owner, present only when the project has a CODEOWNERS file.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub owners: BTreeMap<String, u32>,
}

fn kind() -> String {
    KIND.to_owned()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub error: u32,
    pub warning: u32,
    pub info: u32,
    pub total: u32,
}

impl Snapshot {
    /// `now` is seconds since the Unix epoch, passed in so a snapshot is a function of
    /// its arguments.
    pub fn of(outcome: &ScanOutcome, now: u64) -> Self {
        let health = outcome
            .counts
            .weighted_health_score(outcome.code_lines, &outcome.impact);
        let by_severity = |severity: Severity| {
            outcome
                .counts
                .by_severity
                .get(&severity)
                .copied()
                .unwrap_or(0)
        };
        Self {
            kind: kind(),
            schema_version: SCHEMA_VERSION,
            commit: crate::history::head(&outcome.root),
            time: timestamp(now),
            rqx_version: env!("CARGO_PKG_VERSION").to_owned(),
            health,
            grade: Cleanliness::of(health, outcome.counts.total)
                .label()
                .to_owned(),
            files: outcome.files_scanned,
            counts: Counts {
                error: by_severity(Severity::Error),
                warning: by_severity(Severity::Warning),
                info: by_severity(Severity::Info),
                total: outcome.counts.total,
            },
            rules: outcome
                .counts
                .by_rule
                .iter()
                .map(|(rule, count)| (rule.slug().to_owned(), *count))
                .collect(),
            owners: owner_health(outcome),
        }
    }
}

fn owner_health(outcome: &ScanOutcome) -> BTreeMap<String, u32> {
    crate::ownership::table(outcome, &outcome.analyzed)
        .into_iter()
        .filter(|row| row.owner != crate::ownership::UNOWNED)
        .map(|row| (row.owner, row.health))
        .collect()
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn timestamp(seconds: u64) -> String {
    let seconds = i64::try_from(seconds).unwrap_or(0);
    let date = rqx_core::suppress::civil_date(seconds.div_euclid(86_400));
    let time = seconds.rem_euclid(86_400);
    format!(
        "{date}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

/// Appends one line, creating the file and its folder when they are not there. Never
/// rewrites what is already in the file.
pub fn record(path: &Path, snapshot: &Snapshot) -> Result<()> {
    if let Some(folder) = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("creating {}", folder.display()))?;
    }
    let mut line = serde_json::to_string(snapshot)?;
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(line.as_bytes()))
        .with_context(|| format!("recording a snapshot in {}", path.display()))
}

/// `--record` on the commands that scan.
#[derive(Debug, Default, clap::Args)]
pub struct Options {
    /// Append a snapshot of this scan's health to a trend file. Without a path it
    /// writes to `.rqx/trend.ndjson`
    #[arg(
        long,
        value_name = "PATH",
        num_args = 0..=1,
        default_missing_value = DEFAULT_FILE,
        value_hint = clap::ValueHint::FilePath,
        help_heading = "Trend"
    )]
    pub record: Option<PathBuf>,
}

impl Options {
    /// Takes the snapshot when one was asked for.
    pub fn apply(&self, outcome: &ScanOutcome) -> Result<()> {
        match &self.record {
            Some(path) => record(path, &Snapshot::of(outcome, now())),
            None => Ok(()),
        }
    }

    pub const fn is_set(&self) -> bool {
        self.record.is_some()
    }
}

#[cfg(test)]
mod tests {
    use rqx_core::finding::Severity;

    use super::*;
    use crate::report::tests::outcome_with;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("rqx-trend-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn the_time_is_utc_to_the_second() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp(1_794_930_205), "2026-11-17T15:43:25Z");
    }

    #[test]
    fn a_snapshot_carries_the_scan_in_the_documented_keys() {
        let outcome = outcome_with(
            &[
                ("a.ts", Severity::Error),
                ("a.ts", Severity::Warning),
                ("b.ts", Severity::Warning),
            ],
            4,
        );
        let snapshot = Snapshot::of(&outcome, 0);
        let line: serde_json::Value = serde_json::to_value(&snapshot).expect("json");

        assert_eq!(line["kind"], "snapshot");
        assert_eq!(line["schema_version"], 1);
        assert_eq!(line["time"], "1970-01-01T00:00:00Z");
        assert_eq!(line["files"], 4);
        assert_eq!(line["counts"]["error"], 1);
        assert_eq!(line["counts"]["warning"], 2);
        assert_eq!(line["counts"]["total"], 3);
        assert_eq!(line["rules"]["long-function"], 3);
        assert!(line["health"].is_u64() && line["grade"].is_string());
        assert!(line.get("owners").is_none(), "no CODEOWNERS file");
    }

    #[test]
    fn owners_are_recorded_with_their_health_when_a_codeowners_file_exists() {
        let mut outcome = outcome_with(&[("web/a.ts", Severity::Error)], 2);
        outcome.owners = Some(rqx_core::ownership::CodeOwners::parse(
            "web/ @web\n",
            rqx_core::ownership::Dialect::GitHub,
        ));
        let snapshot = Snapshot::of(&outcome, 0);
        assert_eq!(snapshot.owners.keys().collect::<Vec<_>>(), ["@web"]);
    }

    #[test]
    fn recording_appends_and_never_rewrites() {
        let folder = scratch("append");
        let path = folder.join("nested/trend.ndjson");
        let outcome = outcome_with(&[], 1);

        record(&path, &Snapshot::of(&outcome, 0)).expect("the first line");
        record(&path, &Snapshot::of(&outcome, 86_400)).expect("the second line");

        let text = std::fs::read_to_string(&path).expect("the file");
        let _ = std::fs::remove_dir_all(&folder);
        let times: Vec<String> = text
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).expect("json")["time"]
                    .as_str()
                    .expect("a time")
                    .to_owned()
            })
            .collect();
        assert_eq!(times, ["1970-01-01T00:00:00Z", "1970-01-02T00:00:00Z"]);
    }
}

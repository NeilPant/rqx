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

impl Snapshot {
    /// The date part of the time, which is what a reader compares snapshots by.
    pub fn date(&self) -> &str {
        self.time
            .split_once('T')
            .map_or(&self.time, |(date, _)| date)
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
/// rewrites what is already in the file. A file inside `.rqx/` also gets the
/// directory's `.gitignore`, as the cache does.
pub fn record(path: &Path, snapshot: &Snapshot) -> Result<()> {
    if let Some(folder) = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("creating {}", folder.display()))?;
        if folder.file_name() == Some(std::ffi::OsStr::new(crate::cache::CACHE_DIRECTORY)) {
            crate::cache::ignore_own_files(folder);
        }
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

/// The snapshots in a trend file, oldest first. A line that is not a snapshot, is
/// not JSON, or was written under a newer schema is skipped, so a file another tool
/// has added to still reads.
pub fn parse(text: &str) -> Vec<Snapshot> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Snapshot>(line).ok())
        .filter(|snapshot| snapshot.kind == KIND && snapshot.schema_version <= SCHEMA_VERSION)
        .collect()
}

pub fn read(path: &Path) -> Result<Vec<Snapshot>> {
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "reading the trend file {}; `rqx scan --record` writes one",
            path.display()
        )
    })?;
    Ok(parse(&text))
}

/// Which breakdown of the snapshots to list the movers of.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum By {
    Rule,
    Owner,
}

impl By {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Owner => "owner",
        }
    }
}

/// How far a series has come from where it began, for a reader who wants the verdict
/// before the numbers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    Improving,
    Worsening,
    Steady,
}

impl Direction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Improving => "improving",
            Self::Worsening => "worsening",
            Self::Steady => "steady",
        }
    }
}

/// One number across the snapshots, oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Series {
    pub name: String,
    pub points: Vec<u32>,
    /// Health goes up when things get better and a finding count goes down, so the
    /// direction cannot be read from the sign of the change alone.
    pub higher_is_better: bool,
}

impl Series {
    pub fn first(&self) -> u32 {
        self.points.first().copied().unwrap_or(0)
    }

    pub fn last(&self) -> u32 {
        self.points.last().copied().unwrap_or(0)
    }

    pub fn change(&self) -> i64 {
        i64::from(self.last()) - i64::from(self.first())
    }

    pub fn direction(&self) -> Direction {
        match (self.change(), self.higher_is_better) {
            (0, _) => Direction::Steady,
            (change, true) if change > 0 => Direction::Improving,
            (change, false) if change < 0 => Direction::Improving,
            _ => Direction::Worsening,
        }
    }

    /// One block character per point, scaled between this series' own lowest and
    /// highest value. A series that never moved sits at mid height.
    pub fn sparkline(&self) -> String {
        const BLOCKS: [char; 8] = [
            '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
            '\u{2588}',
        ];
        let low = self.points.iter().copied().min().unwrap_or(0);
        let high = self.points.iter().copied().max().unwrap_or(0);
        self.points
            .iter()
            .map(|point| match high - low {
                0 => BLOCKS[3],
                span => {
                    BLOCKS[((point - low) as usize * (BLOCKS.len() - 1) + span as usize / 2)
                        / span as usize]
                }
            })
            .collect()
    }
}

/// The last `count` snapshots, or all of them when no count is given.
pub fn last(snapshots: &[Snapshot], count: Option<usize>) -> &[Snapshot] {
    match count {
        Some(count) => &snapshots[snapshots.len().saturating_sub(count)..],
        None => snapshots,
    }
}

/// The two numbers every snapshot has: the health score and the number of findings.
pub fn overall(snapshots: &[Snapshot]) -> Vec<Series> {
    vec![
        Series {
            name: "health".to_owned(),
            points: snapshots.iter().map(|snapshot| snapshot.health).collect(),
            higher_is_better: true,
        },
        Series {
            name: "findings".to_owned(),
            points: snapshots
                .iter()
                .map(|snapshot| snapshot.counts.total)
                .collect(),
            higher_is_better: false,
        },
    ]
}

/// The rules or owners that changed most between the first snapshot and the last,
/// biggest change first. A rule that stopped firing counts as zero in the snapshots it
/// is missing from; an owner that was not yet in the file has no point there at all.
pub fn movers(snapshots: &[Snapshot], by: By, limit: usize) -> Vec<Series> {
    let names: std::collections::BTreeSet<&String> = snapshots
        .iter()
        .flat_map(|snapshot| match by {
            By::Rule => snapshot.rules.keys(),
            By::Owner => snapshot.owners.keys(),
        })
        .collect();
    let mut moved: Vec<Series> = names
        .into_iter()
        .map(|name| Series {
            name: name.clone(),
            points: snapshots
                .iter()
                .filter_map(|snapshot| match by {
                    By::Rule => Some(snapshot.rules.get(name).copied().unwrap_or(0)),
                    By::Owner => snapshot.owners.get(name).copied(),
                })
                .collect(),
            higher_is_better: by == By::Owner,
        })
        .filter(|series| series.change() != 0)
        .collect();
    moved.sort_by(|left, right| {
        right
            .change()
            .abs()
            .cmp(&left.change().abs())
            .then_with(|| left.name.cmp(&right.name))
    });
    moved.truncate(limit);
    moved
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
    /// Draw the snapshots in this file as a chart in the HTML report. A file that does
    /// not exist yet draws nothing
    #[arg(
        long,
        value_name = "PATH",
        value_hint = clap::ValueHint::FilePath,
        help_heading = "Trend"
    )]
    pub trend: Option<PathBuf>,
}

impl Options {
    /// Takes the snapshot when one was asked for, then reads the history a chart is
    /// drawn from. In that order, so a run that records and charts shows itself. A
    /// file that is not there yet is no history rather than an error: the first run of
    /// a pipeline has nothing to read.
    pub fn apply(&self, outcome: &ScanOutcome) -> Result<Option<Vec<Snapshot>>> {
        if let Some(path) = &self.record {
            record(path, &Snapshot::of(outcome, now()))?;
        }
        match self.trend.as_deref() {
            Some(path) if path.exists() => read(path).map(Some),
            Some(_) => Ok(Some(Vec::new())),
            None => Ok(None),
        }
    }

    pub const fn is_set(&self) -> bool {
        self.record.is_some() || self.trend.is_some()
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

    #[test]
    fn recording_into_rqx_keeps_the_trend_out_of_git_and_elsewhere_writes_nothing_more() {
        let folder = scratch("ignored");
        let outcome = outcome_with(&[], 1);

        record(&folder.join(DEFAULT_FILE), &Snapshot::of(&outcome, 0)).expect("a line");
        record(
            &folder.join("charts/trend.ndjson"),
            &Snapshot::of(&outcome, 0),
        )
        .expect("a line");

        let ignore = std::fs::read_to_string(folder.join(".rqx/.gitignore"));
        let elsewhere = folder.join("charts/.gitignore").exists();
        let _ = std::fs::remove_dir_all(&folder);
        assert!(
            ignore
                .expect("written")
                .lines()
                .any(|line| line == "trend.ndjson")
        );
        assert!(!elsewhere);
    }

    #[test]
    fn a_line_that_is_not_a_snapshot_is_skipped_rather_than_failing_the_read() {
        let outcome = outcome_with(&[], 1);
        let good = serde_json::to_string(&Snapshot::of(&outcome, 0)).expect("json");
        let newer = good.replace("\"schema_version\":1", "\"schema_version\":2");
        let other = r#"{"kind":"note","text":"hello"}"#;
        let text = format!("{good}\nnot json\n\n{other}\n{newer}\n{good}\n");
        assert_eq!(parse(&text).len(), 2);
    }

    #[test]
    fn a_snapshot_with_keys_a_later_version_added_still_reads() {
        let outcome = outcome_with(&[], 1);
        let mut line: serde_json::Value =
            serde_json::to_value(Snapshot::of(&outcome, 0)).expect("json");
        line["branch"] = "main".into();
        assert_eq!(parse(&line.to_string()).len(), 1);
    }

    #[test]
    fn a_trend_file_that_is_not_there_yet_is_no_history() {
        let options = Options {
            record: None,
            trend: Some(scratch("absent").join("trend.ndjson")),
        };
        let history = options.apply(&outcome_with(&[], 1)).expect("no error");
        assert_eq!(history, Some(Vec::new()));
        assert_eq!(
            Options::default()
                .apply(&outcome_with(&[], 1))
                .expect("no error"),
            None
        );
    }

    #[test]
    fn recording_and_charting_in_one_run_charts_the_new_snapshot() {
        let folder = scratch("both");
        let file = folder.join("trend.ndjson");
        let options = Options {
            record: Some(file.clone()),
            trend: Some(file),
        };
        let history = options.apply(&outcome_with(&[], 1)).expect("no error");
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(history.map(|snapshots| snapshots.len()), Some(1));
    }

    #[test]
    fn a_missing_file_is_an_error_that_says_how_to_make_one() {
        let path = scratch("missing").join("trend.ndjson");
        let error = read(&path).expect_err("no file");
        assert!(format!("{error:#}").contains("--record"), "{error:#}");
    }

    fn snapshot(day: u64, health: u32, rules: &[(&str, u32)], owners: &[(&str, u32)]) -> Snapshot {
        let mut snapshot = Snapshot::of(&outcome_with(&[], 1), day * 86_400);
        snapshot.health = health;
        snapshot.counts.total = rules.iter().map(|(_, count)| count).sum();
        snapshot.rules = rules
            .iter()
            .map(|(rule, n)| ((*rule).to_owned(), *n))
            .collect();
        snapshot.owners = owners
            .iter()
            .map(|(who, n)| ((*who).to_owned(), *n))
            .collect();
        snapshot
    }

    #[test]
    fn direction_depends_on_which_way_is_better() {
        let series = |points: &[u32], higher_is_better| Series {
            name: "x".into(),
            points: points.to_vec(),
            higher_is_better,
        };
        assert_eq!(series(&[50, 60], true).direction(), Direction::Improving);
        assert_eq!(series(&[60, 50], true).direction(), Direction::Worsening);
        assert_eq!(series(&[9, 3], false).direction(), Direction::Improving);
        assert_eq!(series(&[3, 9], false).direction(), Direction::Worsening);
        assert_eq!(series(&[4, 9, 4], false).direction(), Direction::Steady);
    }

    #[test]
    fn a_sparkline_spans_the_series_own_range() {
        let line = |points: &[u32]| {
            Series {
                name: "x".into(),
                points: points.to_vec(),
                higher_is_better: true,
            }
            .sparkline()
        };
        assert_eq!(line(&[0, 7]), "\u{2581}\u{2588}");
        assert_eq!(line(&[10, 20, 30]).chars().count(), 3);
        assert_eq!(line(&[5, 5, 5]), "\u{2584}\u{2584}\u{2584}");
        assert_eq!(line(&[]), "");
    }

    #[test]
    fn last_keeps_the_newest_snapshots() {
        let all: Vec<Snapshot> = (0..5).map(|day| snapshot(day, 50, &[], &[])).collect();
        assert_eq!(last(&all, Some(2)).len(), 2);
        assert_eq!(last(&all, Some(2))[0].date(), "1970-01-04");
        assert_eq!(last(&all, Some(99)).len(), 5);
        assert_eq!(last(&all, None).len(), 5);
    }

    #[test]
    fn movers_are_ordered_by_how_far_they_moved_and_leave_out_the_still() {
        let all = [
            snapshot(0, 50, &[("a", 10), ("b", 2), ("c", 5)], &[]),
            snapshot(1, 55, &[("a", 4), ("b", 3), ("c", 5), ("d", 7)], &[]),
        ];
        let moved = movers(&all, By::Rule, 10);
        let named: Vec<(&str, i64)> = moved
            .iter()
            .map(|series| (series.name.as_str(), series.change()))
            .collect();
        assert_eq!(named, [("d", 7), ("a", -6), ("b", 1)]);
        assert_eq!(movers(&all, By::Rule, 1).len(), 1);
    }

    #[test]
    fn an_owner_with_no_point_in_an_early_snapshot_is_charted_from_its_first() {
        let all = [
            snapshot(0, 50, &[], &[("@old", 80)]),
            snapshot(1, 50, &[], &[("@old", 70), ("@new", 40)]),
            snapshot(2, 50, &[], &[("@old", 90), ("@new", 60)]),
        ];
        let moved = movers(&all, By::Owner, 10);
        let new = moved
            .iter()
            .find(|series| series.name == "@new")
            .expect("moved");
        assert_eq!(new.points, [40, 60]);
        assert!(new.higher_is_better);
    }
}

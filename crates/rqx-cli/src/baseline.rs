//! The debt a project already has, recorded once so a gate can ask about the debt a
//! change adds.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use rqx_core::config::BaselineMatch;
use rqx_core::finding::Finding;
use rqx_core::identity::Identity;
use serde::{Deserialize, Serialize};

use crate::config::Resolved;
use crate::progress::Mode;
use crate::scan::ScanOutcome;

pub const DEFAULT_PATH: &str = ".rqx/baseline.json";

/// Bumped when an entry stops meaning what it meant. A file from an older version is
/// refused rather than half-read, because a baseline that silently matches nothing
/// turns a gate off without saying so.
const VERSION: u32 = 1;

#[derive(Debug, Deserialize, Serialize)]
pub struct Baseline {
    version: u32,
    /// The commit it was taken at and the version that took it. Neither is read back;
    /// they are here for whoever opens the file asking where it came from.
    created: u64,
    rqx: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    /// How many findings there were when it was taken. Two findings can resolve to
    /// one identity, so the entry count is not the finding count, and the ratchet
    /// has to compare like with like.
    findings: u32,
    entries: Vec<Entry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Entry {
    identity: String,
    rule: String,
    file: String,
    /// How many findings had this identity. Left out when it is one, which is also
    /// how a file written before counts existed reads.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    count: u32,
}

const fn one() -> u32 {
    1
}

const fn is_one(count: &u32) -> bool {
    *count == 1
}

impl Baseline {
    pub fn of(outcome: &ScanOutcome) -> Self {
        let mut counted: BTreeMap<(String, String, String), u32> = BTreeMap::new();
        for (identity, finding) in identities(outcome) {
            let key = (
                finding.location.file.to_string(),
                finding.slug().to_owned(),
                identity.to_string(),
            );
            *counted.entry(key).or_default() += 1;
        }
        let entries = counted
            .into_iter()
            .map(|((file, rule, identity), count)| Entry {
                identity,
                rule,
                file,
                count,
            })
            .collect();
        Self {
            version: VERSION,
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            rqx: env!("CARGO_PKG_VERSION").to_owned(),
            commit: crate::history::head(&outcome.root),
            findings: outcome.counts.total,
            entries,
        }
    }

    pub fn read(path: &Path) -> Result<Self> {
        let body = std::fs::read_to_string(path)
            .with_context(|| format!("reading the baseline at {}", path.display()))?;
        let baseline: Self = serde_json::from_str(&body)
            .with_context(|| format!("reading the baseline at {}", path.display()))?;
        anyhow::ensure!(
            baseline.version == VERSION,
            "{} was written by a different version of rqx (format {} against {VERSION}). \
             Run `rqx baseline --update` to take it again.",
            path.display(),
            baseline.version
        );
        Ok(baseline)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)
                .with_context(|| format!("creating {}", directory.display()))?;
        }
        crate::fix::write_atomically(path, &format!("{}\n", serde_json::to_string_pretty(self)?))
    }

    /// How many identities it holds. Fewer than `findings` when some repeat.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// What the ratchet compares against, and what `--update` refuses to exceed. It
    /// counts every finding, repeats included, in a file of any version.
    pub const fn findings(&self) -> u32 {
        self.findings
    }

    /// Each rule whose count is higher in `taken` than here, with both counts. A file
    /// written before entries carried counts recorded each identity once, so against
    /// one of those both sides count identities, which is all such a file can answer.
    pub fn risen_in<'a>(&self, taken: &'a Self) -> Vec<(&'a str, u32, u32)> {
        let repeats = self.records_repeats();
        let before = self.by_rule(repeats);
        taken
            .by_rule(repeats)
            .into_iter()
            .filter_map(|(rule, now)| {
                let was = before.get(rule).copied().unwrap_or(0);
                (now > was).then_some((rule, was, now))
            })
            .collect()
    }

    /// How many findings, or with `repeats` off how many identities, of each rule.
    fn by_rule(&self, repeats: bool) -> BTreeMap<&str, u32> {
        let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
        for entry in &self.entries {
            *counts.entry(entry.rule.as_str()).or_default() +=
                if repeats { entry.count } else { 1 };
        }
        counts
    }

    /// Whether its entries' counts add up to the findings it was taken over. A file
    /// written by 0.5.0 has no counts, so when two findings shared an identity they
    /// add up to less.
    fn records_repeats(&self) -> bool {
        self.entries.iter().map(|entry| entry.count).sum::<u32>() == self.findings
    }

    /// The entries, ready to match a scan against as `matching` says.
    pub fn recorded(&self, matching: BaselineMatch) -> Recorded {
        let mut counts: HashMap<Identity, u32> = HashMap::new();
        for entry in &self.entries {
            if let Ok(raw) = u64::from_str_radix(&entry.identity, 16) {
                *counts.entry(Identity::from_raw(raw)).or_default() += entry.count;
            }
        }
        Recorded { counts, matching }
    }
}

/// What a baseline holds, by identity, and how a scan is matched against it. Shared by
/// every scan a run makes; each scan matches through its own `Matcher`.
#[derive(Debug)]
pub struct Recorded {
    counts: HashMap<Identity, u32>,
    matching: BaselineMatch,
}

impl Recorded {
    pub fn matcher(self: &Arc<Self>) -> Matcher {
        Matcher {
            recorded: Arc::clone(self),
            claimed: HashMap::new(),
        }
    }
}

/// One scan's claims on a baseline: how many findings of each identity it has already
/// matched, so a baseline that recorded two of something covers two and not three.
pub struct Matcher {
    recorded: Arc<Recorded>,
    claimed: HashMap<Identity, u32>,
}

impl Matcher {
    /// Marks each finding the baseline covers as known, and every other one as new.
    /// `identities` holds each finding's identity at the same position. Findings that
    /// share an identity claim it in source order, so when a file holds more of them
    /// than the baseline recorded, the new ones are the last ones in the file.
    pub fn mark(&mut self, findings: &mut [Finding], identities: &[Identity]) {
        let mut order: Vec<usize> = (0..findings.len()).collect();
        order.sort_by_key(|&at| (findings[at].location.start, findings[at].location.end));
        for at in order {
            findings[at].known = self.claim(identities[at]);
        }
    }

    fn claim(&mut self, identity: Identity) -> bool {
        let Some(&recorded) = self.recorded.counts.get(&identity) else {
            return false;
        };
        if self.recorded.matching == BaselineMatch::Set {
            return true;
        }
        let claimed = self.claimed.entry(identity).or_default();
        let covered = *claimed < recorded;
        if covered {
            *claimed += 1;
        }
        covered
    }
}

/// What taking a baseline produced, for whichever surface asked.
pub struct Taken {
    pub path: PathBuf,
    /// How many entries it holds: one per identity, however many findings share it.
    pub recorded: usize,
    pub findings: u32,
    /// How many entries the previous baseline held, when one was replaced.
    pub replaced: Option<usize>,
    /// How many findings the previous baseline recorded, when one was replaced.
    pub replaced_findings: Option<u32>,
    /// Whether the gate already names this file, so the caller can say what to set.
    pub configured: bool,
}

/// Records every finding the project has now. Refuses to overwrite an existing
/// baseline unless `update`, and to grow one unless `force`, because a baseline that
/// silently absorbs new debt is the opposite of what one is for.
pub fn take(root: &Path, settings: &Resolved, update: bool, force: bool) -> Result<Taken> {
    take_drawn(root, settings, update, force, Mode::hidden())
}

/// `take`, with the scan's progress drawn on stderr as `progress` says.
pub fn take_drawn(
    root: &Path,
    settings: &Resolved,
    update: bool,
    force: bool,
    progress: Mode,
) -> Result<Taken> {
    let file = path_of(root, settings.config.gate.baseline.as_deref());
    let previous = file.exists().then(|| Baseline::read(&file)).transpose()?;
    if previous.is_some() && !update {
        anyhow::bail!(
            "{} already exists. Run `rqx baseline --update` to take it again.",
            file.display()
        );
    }
    let mut settings = settings.clone();
    // The cap drops findings before they can be recorded, and a baseline that holds
    // the first five hundred is worse than none.
    settings.config.scan.max_findings = 0;
    // Recording against an existing baseline would tag everything `known`, which
    // changes nothing here but makes the counts confusing to read.
    settings.baseline = None;
    let outcome = crate::scan::scan_drawn(root, &settings, progress)?;
    let taken = Baseline::of(&outcome);
    if let Some(previous) = &previous
        && !force
    {
        refuse_growth(previous, &taken)?;
    }
    taken.write(&file)?;
    Ok(Taken {
        path: file,
        recorded: taken.len(),
        findings: taken.findings(),
        replaced: previous.as_ref().map(Baseline::len),
        replaced_findings: previous.as_ref().map(Baseline::findings),
        configured: settings.config.gate.baseline.is_some(),
    })
}

/// A baseline may shrink, but no rule's count may grow, even where another rule's
/// fell further: a total alone would let new debt in under a cleanup.
fn refuse_growth(previous: &Baseline, taken: &Baseline) -> Result<()> {
    let risen = previous.risen_in(taken);
    if !risen.is_empty() {
        let listed: Vec<String> = risen
            .iter()
            .map(|(rule, was, now)| format!("{rule} from {was} to {now}"))
            .collect();
        anyhow::bail!(
            "this would record more findings than the baseline holds for {}: {}. \
             Fix them, or pass --force to accept the larger set.",
            if risen.len() == 1 {
                "one rule"
            } else {
                "these rules"
            },
            listed.join(", ")
        );
    }
    if taken.findings() > previous.findings() {
        anyhow::bail!(
            "this would record {} findings against the {} already recorded. \
             Fix them, or pass --force to accept the larger set.",
            taken.findings(),
            previous.findings()
        );
    }
    Ok(())
}

/// Where the baseline lives: what the gate names, or the conventional path.
pub fn path_of(root: &Path, configured: Option<&str>) -> PathBuf {
    root.join(configured.unwrap_or(DEFAULT_PATH))
}

/// Each of a scan's findings with its identity. Reads each file once, because
/// resolving an identity needs the text a finding sits on.
pub fn identities(outcome: &ScanOutcome) -> impl Iterator<Item = (Identity, &Finding)> {
    let mut sources: HashMap<String, String> = HashMap::new();
    outcome.findings.iter().map(move |finding| {
        let file = finding.location.file.to_string();
        let source = sources.entry(file).or_insert_with(|| {
            std::fs::read_to_string(outcome.root.join(&*finding.location.file)).unwrap_or_default()
        });
        (Identity::resolved(finding, source), finding)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rqx_core::finding::{Confidence, Evidence, Severity, SourceRange};
    use rqx_core::rule::RuleId;

    fn finding(line: u32) -> Finding {
        Finding {
            rule: RuleId::ImpreciseType,
            location: SourceRange {
                file: "a.ts".into(),
                start: line * 10,
                end: line * 10 + 4,
                line,
                column: 1,
                end_line: line,
            },
            focus: None,
            severity: Severity::Warning,
            confidence: Confidence::Verified,
            evidence: Evidence::ImpreciseType {
                construct: "any".to_owned(),
            },
            fix_hint: String::new(),
            related: Vec::new(),
            known: false,
        }
    }

    fn identity() -> Identity {
        Identity::of(RuleId::ImpreciseType, "a.ts", "any")
    }

    fn file(entries: &str) -> Baseline {
        serde_json::from_str(&format!(
            r#"{{"version": 1, "created": 0, "rqx": "0.5.0", "findings": 3, "entries": [{entries}]}}"#
        ))
        .unwrap()
    }

    fn recorded(count: u32, matching: BaselineMatch) -> Arc<Recorded> {
        let entry = format!(
            r#"{{"identity": "{}", "rule": "imprecise-type", "file": "a.ts", "count": {count}}}"#,
            identity()
        );
        Arc::new(file(&entry).recorded(matching))
    }

    /// Arrival order is deliberately not source order, so the test shows the choice is
    /// made by position and not by whichever finding reached the matcher first.
    fn marked(recorded: &Arc<Recorded>) -> Vec<(u32, bool)> {
        let mut findings = vec![finding(30), finding(10), finding(20)];
        let identities = vec![identity(); findings.len()];
        recorded.matcher().mark(&mut findings, &identities);
        let mut marks: Vec<(u32, bool)> = findings
            .iter()
            .map(|finding| (finding.location.line, finding.known))
            .collect();
        marks.sort_unstable();
        marks
    }

    #[test]
    fn an_entry_covers_as_many_findings_as_it_recorded_and_the_last_ones_are_new() {
        let marks = marked(&recorded(2, BaselineMatch::Count));
        assert_eq!(marks, [(10, true), (20, true), (30, false)]);
    }

    #[test]
    fn set_matching_lets_one_entry_cover_every_repeat() {
        let marks = marked(&recorded(1, BaselineMatch::Set));
        assert_eq!(marks, [(10, true), (20, true), (30, true)]);
    }

    #[test]
    fn each_scan_matches_against_the_whole_count() {
        let recorded = recorded(1, BaselineMatch::Count);
        assert_eq!(marked(&recorded), [(10, true), (20, false), (30, false)]);
        assert_eq!(marked(&recorded), [(10, true), (20, false), (30, false)]);
    }

    #[test]
    fn a_finding_the_baseline_never_recorded_is_new() {
        let recorded = recorded(5, BaselineMatch::Set);
        let mut findings = vec![finding(1)];
        let other = Identity::of(RuleId::ImpreciseType, "b.ts", "any");
        recorded.matcher().mark(&mut findings, &[other]);
        assert!(!findings[0].known);
    }

    /// 0.5.0 wrote one entry per identity and no count. Each entry reads as one
    /// finding, so the repeats such a file hid are reported as new.
    #[test]
    fn a_file_written_without_counts_reads_each_entry_as_one_finding() {
        let entry = format!(
            r#"{{"identity": "{}", "rule": "imprecise-type", "file": "a.ts"}}"#,
            identity()
        );
        let baseline = file(&entry);
        let recorded = Arc::new(baseline.recorded(BaselineMatch::Count));
        assert_eq!(marked(&recorded), [(10, true), (20, false), (30, false)]);
        assert_eq!(baseline.findings(), 3);
    }

    #[test]
    fn a_count_is_written_only_when_it_is_more_than_one() {
        let entry = |identity: &str, count| Entry {
            identity: identity.to_owned(),
            rule: "imprecise-type".to_owned(),
            file: "a.ts".to_owned(),
            count,
        };
        let written = serde_json::to_value([entry("1", 1), entry("2", 3)]).unwrap();
        assert!(written[0].get("count").is_none(), "{written}");
        assert_eq!(written[1]["count"], 3);
    }

    /// A baseline over `findings` findings, one entry per `(rule, identity, count)`.
    fn taken_over(findings: u32, entries: &[(&str, &str, u32)]) -> Baseline {
        let entries: Vec<String> = entries
            .iter()
            .map(|(rule, identity, count)| {
                format!(
                    r#"{{"identity": "{identity}", "rule": "{rule}", "file": "a.go", "count": {count}}}"#
                )
            })
            .collect();
        serde_json::from_str(&format!(
            r#"{{"version": 1, "created": 0, "rqx": "0.5.1", "findings": {findings}, "entries": [{}]}}"#,
            entries.join(", ")
        ))
        .unwrap()
    }

    fn refusal(previous: &Baseline, taken: &Baseline) -> Option<String> {
        refuse_growth(previous, taken)
            .err()
            .map(|error| error.to_string())
    }

    #[test]
    fn a_rule_that_rises_is_refused_although_the_total_falls() {
        let previous = taken_over(
            12,
            &[("magic-number", "a1", 2), ("unused-parameter", "b1", 10)],
        );
        let taken = taken_over(
            5,
            &[("magic-number", "a1", 4), ("unused-parameter", "b1", 1)],
        );
        assert_eq!(
            refusal(&previous, &taken).as_deref(),
            Some(
                "this would record more findings than the baseline holds for one rule: \
                 magic-number from 2 to 4. Fix them, or pass --force to accept the larger set."
            )
        );
    }

    #[test]
    fn every_rule_that_rises_is_listed_and_a_new_rule_rises_from_zero() {
        let previous = taken_over(3, &[("magic-number", "a1", 3)]);
        let taken = taken_over(6, &[("deep-nesting", "c1", 1), ("magic-number", "a1", 5)]);
        let refused = refusal(&previous, &taken).expect("refused");
        assert!(
            refused.contains("these rules: deep-nesting from 0 to 1, magic-number from 3 to 5."),
            "{refused}"
        );
    }

    #[test]
    fn a_baseline_that_no_rule_grows_in_is_accepted() {
        let previous = taken_over(
            5,
            &[("magic-number", "a1", 2), ("unused-parameter", "b1", 3)],
        );
        let same = taken_over(
            5,
            &[("magic-number", "a2", 2), ("unused-parameter", "b1", 3)],
        );
        let smaller = taken_over(1, &[("unused-parameter", "b1", 1)]);
        assert_eq!(refusal(&previous, &same), None);
        assert_eq!(refusal(&previous, &smaller), None);
    }

    /// 0.5.0 wrote one entry per identity and no counts, so its rules can only be
    /// compared by identity; the total still stops repeats from growing.
    #[test]
    fn a_file_without_counts_is_compared_by_identities_and_by_its_total() {
        let previous = taken_over(
            5,
            &[("magic-number", "a1", 1), ("unused-parameter", "b1", 1)],
        );
        let same_repeats = taken_over(
            5,
            &[("magic-number", "a1", 3), ("unused-parameter", "b1", 2)],
        );
        assert_eq!(refusal(&previous, &same_repeats), None);

        let new_identity = taken_over(
            5,
            &[
                ("magic-number", "a1", 2),
                ("magic-number", "a2", 1),
                ("unused-parameter", "b1", 2),
            ],
        );
        assert!(
            refusal(&previous, &new_identity)
                .expect("refused")
                .contains("magic-number from 1 to 2"),
        );

        let more_repeats = taken_over(
            6,
            &[("magic-number", "a1", 4), ("unused-parameter", "b1", 2)],
        );
        assert_eq!(
            refusal(&previous, &more_repeats).as_deref(),
            Some(
                "this would record 6 findings against the 5 already recorded. \
                 Fix them, or pass --force to accept the larger set."
            )
        );
    }
}

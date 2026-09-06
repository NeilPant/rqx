use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Result, bail};
use rqx_core::config::{GateSeverity, RqxConfig};

use crate::config::Resolved;
use rqx_core::finding::{Finding, Severity};

use crate::history;
use crate::scan::ScanOutcome;

pub struct Review {
    pub outcome: ScanOutcome,
    pub reviewed: BTreeSet<String>,
    pub base: String,
}

impl Review {
    pub fn is_reviewed(&self, file: &str) -> bool {
        self.reviewed.contains(file)
    }

    pub fn findings(&self) -> impl Iterator<Item = &rqx_core::finding::Finding> {
        self.outcome
            .findings
            .iter()
            .filter(|finding| self.is_reviewed(&finding.location.file))
    }
}

/// The whole project is scanned even though only part of it is reported on: an
/// export looks dead until the file that imports it is parsed, so narrowing the scan
/// to the diff would invent findings that the full graph disproves.
pub fn review(root: &Path, settings: &Resolved, base: &str) -> Result<Review> {
    review_watched(root, settings, base, None)
}

/// A review is a whole-project scan, so it has the same progress to report as one.
pub fn review_watched(
    root: &Path,
    settings: &Resolved,
    base: &str,
    watch: Option<crate::scan::Watch>,
) -> Result<Review> {
    let Some(mut changed) = history::changed_since(root, base) else {
        bail!(
            "cannot diff against `{base}`. A review needs a git repository and a revision \
             that exists; try `--base HEAD~1`, or `rqx scan` to look at every file instead."
        );
    };
    changed.extend(history::uncommitted(root));

    let prefix = history::repository_prefix(root).unwrap_or_default();
    let changed: BTreeSet<String> = changed
        .into_iter()
        .filter_map(|path| path.strip_prefix(&prefix).map(str::to_owned))
        .collect();

    let outcome = match watch {
        Some(watch) => crate::scan::scan_with_progress(root, settings, watch)?,
        None => crate::scan::scan(root, settings)?,
    };

    // Direct dependents are reviewed too, because deleting an export is only visible
    // as a problem in the file that was importing it.
    let mut reviewed = changed.clone();
    for (importer, imported) in &outcome.import_edges {
        if imported.iter().any(|target| changed.contains(target)) {
            reviewed.insert(importer.clone());
        }
    }

    // A lockfile or a stray asset is a changed file, but it is not a file this tool
    // has an opinion about; counting it would overstate what was reviewed.
    reviewed.retain(|file| outcome.analyzed.contains(file));

    Ok(Review {
        outcome,
        reviewed,
        base: base.to_owned(),
    })
}

pub struct Verdict {
    pub failures: Vec<String>,
    pub counted: u32,
    /// Findings a baseline already recorded. Reported rather than counted, so a
    /// reader can see the debt is still there while the gate judges only what is new.
    pub known: u32,
}

impl Verdict {
    pub const fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

pub fn judge(review: &Review, config: &RqxConfig) -> Verdict {
    let Some(threshold) = threshold(config) else {
        return Verdict {
            failures: Vec::new(),
            counted: 0,
            known: 0,
        };
    };
    let reviewed: Vec<&Finding> = review
        .findings()
        .filter(|finding| finding.severity >= threshold)
        .collect();
    let known = reviewed.iter().filter(|finding| finding.known).count() as u32;
    // With a baseline, the gate is about what this change added; without one, every
    // finding is new by definition, because nothing recorded otherwise.
    let counted = reviewed.len() as u32 - known;
    let health = review
        .outcome
        .counts
        .weighted_health_score(review.outcome.code_lines, &review.outcome.impact);
    let mut judgement = verdict(counted, health, config);
    judgement.known = known;
    judgement
        .failures
        .extend(grade_failures(&review.outcome, &review.reviewed, config));
    judgement
}

/// A whole-project gate counts from `counts`, which is exact, rather than from the
/// retained findings, which the display cap has already thinned.
pub fn judge_scan(outcome: &ScanOutcome, config: &RqxConfig) -> Verdict {
    let Some(threshold) = threshold(config) else {
        let mut failures = grade_failures(outcome, &outcome.analyzed, config);
        failures.extend(half_applied(outcome, config));
        failures.extend(ratchet(outcome, config));
        return Verdict {
            failures,
            counted: 0,
            known: 0,
        };
    };
    let at_or_above = |counts: &std::collections::BTreeMap<Severity, u32>| -> u32 {
        counts
            .iter()
            .filter(|(severity, _)| **severity >= threshold)
            .map(|(_, count)| *count)
            .sum()
    };
    let total = at_or_above(&outcome.counts.by_severity);
    let counted = at_or_above(&outcome.counts.new_by_severity);
    let known = total - counted;
    let health = outcome
        .counts
        .weighted_health_score(outcome.code_lines, &outcome.impact);
    let mut judgement = verdict(counted, health, config);
    judgement.known = known;
    judgement
        .failures
        .extend(grade_failures(outcome, &outcome.analyzed, config));
    judgement.failures.extend(half_applied(outcome, config));
    judgement.failures.extend(ratchet(outcome, config));
    judgement
}

/// A baseline says which debt is tolerated. The ratchet says it may not grow, which
/// is the difference between recording debt and accepting it.
fn ratchet(outcome: &ScanOutcome, config: &RqxConfig) -> Option<String> {
    let recorded = config
        .gate
        .ratchet
        .then_some(outcome.baseline_size)
        .flatten()?;
    let total = outcome.counts.total;
    (total > recorded).then(|| {
        format!(
            "{total} findings against the {recorded} the baseline recorded; \
             remove {} or run `rqx baseline --update`",
            total - recorded
        )
    })
}

/// The one route from advice to an exit code, and only for the stance that names
/// something broken. A recognised pattern is inventory and one that would merely fit
/// is a suggestion; failing a build on either would make the gate meaningless.
fn half_applied(outcome: &ScanOutcome, config: &RqxConfig) -> Option<String> {
    if !config.gate.fail_on_incomplete {
        return None;
    }
    let broken: BTreeSet<&str> = outcome
        .advice
        .iter()
        .filter(|advice| advice.advisory.stance() == rqx_core::Stance::Incomplete)
        .map(|advice| advice.advisory.pattern().name())
        .collect();
    if broken.is_empty() {
        return None;
    }
    Some(format!(
        "{} half applied: {}",
        crate::report::plural(broken.len() as u32, "pattern", "patterns"),
        broken.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

const fn threshold(config: &RqxConfig) -> Option<Severity> {
    match config.gate.fail_on {
        GateSeverity::Never => None,
        GateSeverity::Error => Some(Severity::Error),
        GateSeverity::Warning => Some(Severity::Warning),
        GateSeverity::Info => Some(Severity::Info),
    }
}

/// A single file rotting on its own is invisible to a project-wide score, which is
/// why the grade gate looks per file rather than at the average.
fn grade_failures(
    outcome: &ScanOutcome,
    files: &BTreeSet<String>,
    config: &RqxConfig,
) -> Vec<String> {
    let Some(floor) = config.gate.min_grade.floor() else {
        return Vec::new();
    };
    let mut below: Vec<(&str, rqx_core::Cleanliness)> = files
        .iter()
        .filter_map(|file| {
            let lines = outcome
                .lines_by_file
                .get(file.as_str())
                .copied()
                .unwrap_or(0);
            let findings = outcome
                .findings_by_file
                .get(file.as_str())
                .copied()
                .unwrap_or(0);
            let grade = rqx_core::Cleanliness::of(outcome.counts.file_score(file, lines), findings);
            (grade < floor).then_some((file.as_str(), grade))
        })
        .collect();
    below.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(right.0)));

    below
        .first()
        .map(|(file, grade)| {
            let extra = below.len() - 1;
            let tail = if extra > 0 {
                format!(" and {extra} more")
            } else {
                String::new()
            };
            format!(
                "`{file}` grades {} against a floor of {}{tail}",
                grade.label(),
                floor.label()
            )
        })
        .into_iter()
        .collect()
}

fn verdict(counted: u32, health: u32, config: &RqxConfig) -> Verdict {
    let gate = &config.gate;
    let mut failures = Vec::new();
    if counted > gate.max_findings {
        failures.push(format!(
            "{counted} findings at or above `{}`, limit is {}",
            match gate.fail_on {
                GateSeverity::Error => "error",
                GateSeverity::Warning => "warning",
                GateSeverity::Info => "info",
                GateSeverity::Never => "never",
            },
            gate.max_findings
        ));
    }

    if gate.min_health > 0 && health < gate.min_health {
        failures.push(format!(
            "health score {health} is below the required {}",
            gate.min_health
        ));
    }

    Verdict {
        failures,
        counted,
        known: 0,
    }
}

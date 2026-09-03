use std::collections::BTreeMap;

use serde::Serialize;

use std::sync::Arc;

use crate::finding::{Confidence, Finding, Severity};
use crate::impact::ImpactMap;
use crate::rule::RuleId;

/// Weighted findings per 1000 LOC at which the score decays to 1/e. Crude and
/// deliberately so: the churn and blast-radius weighting that makes a score mean
/// something is a later phase, and a tunable constant is cheaper to replace than
/// a curve fitted to guesses.
const DECAY_DENSITY: f64 = 20.0;

/// Added to the denominator because without it a 45-line file with three warnings
/// is extrapolated to a whole-repo rate and scores zero. Negligible at real sizes.
const SMOOTHING_LINES: f64 = 500.0;

#[derive(Debug, Default, Serialize)]
pub struct FindingCounts {
    pub by_rule: BTreeMap<RuleId, u32>,
    pub by_severity: BTreeMap<Severity, u32>,
    pub total: u32,
    /// Accumulated per file rather than per finding, so applying impact weighting at
    /// the end costs one entry per file instead of retaining every finding.
    #[serde(skip)]
    weighted_by_file: BTreeMap<Arc<str>, f64>,
}

impl FindingCounts {
    pub fn record(&mut self, finding: &Finding) {
        *self.by_rule.entry(finding.rule).or_default() += 1;
        *self.by_severity.entry(finding.severity).or_default() += 1;
        self.total += 1;
        if finding.confidence != Confidence::Low {
            *self
                .weighted_by_file
                .entry(Arc::clone(&finding.location.file))
                .or_default() += finding.severity.weight();
        }
    }

    pub fn health_score(&self, loc: u64) -> u32 {
        self.weighted_health_score(loc, &ImpactMap::default())
    }

    /// The same curve, with each file's contribution scaled by how much its problems
    /// actually cost: a smell in a hub everyone edits outweighs the same smell in a
    /// leaf nobody touches.
    pub fn weighted_health_score(&self, loc: u64, impact: &ImpactMap) -> u32 {
        if loc == 0 {
            return 100;
        }
        let weighted: f64 = self
            .weighted_by_file
            .iter()
            .map(|(file, weight)| weight * impact.get(file).multiplier())
            .sum();
        let density = weighted / ((loc as f64 + SMOOTHING_LINES) / 1000.0);
        (100.0 * (-density / DECAY_DENSITY).exp()).round() as u32
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::finding::{Evidence, SourceRange};

    fn finding(severity: Severity, confidence: Confidence) -> Finding {
        Finding {
            rule: RuleId::LongFunction,
            location: SourceRange {
                file: Arc::from("a.ts"),
                start: 0,
                end: 1,
                line: 1,
                column: 1,
                end_line: 1,
            },
            severity,
            confidence,
            evidence: Evidence::StatementCount { count: 99, threshold: 40 },
            fix_hint: String::new(),
            related: Vec::new(),
        }
    }

    #[test]
    fn clean_project_scores_full() {
        assert_eq!(FindingCounts::default().health_score(50_000), 100);
    }

    #[test]
    fn empty_project_does_not_divide_by_zero() {
        let mut counts = FindingCounts::default();
        counts.record(&finding(Severity::Error, Confidence::Verified));
        assert_eq!(counts.health_score(0), 100);
    }

    #[test]
    fn low_confidence_findings_are_counted_but_not_scored() {
        let mut counts = FindingCounts::default();
        for _ in 0..50 {
            counts.record(&finding(Severity::Info, Confidence::Low));
        }
        assert_eq!(counts.total, 50);
        assert_eq!(counts.health_score(10_000), 100);
    }

    #[test]
    fn impact_weighting_lowers_the_score_but_never_raises_it() {
        let mut counts = FindingCounts::default();
        for _ in 0..10 {
            counts.record(&finding(Severity::Warning, Confidence::Heuristic));
        }
        let plain = counts.health_score(20_000);

        let mut impact = ImpactMap::default();
        impact.set_commits(Arc::from("a.ts"), 60);
        impact.set_importers(Arc::from("a.ts"), 25);
        let weighted = counts.weighted_health_score(20_000, &impact);

        assert!(weighted < plain, "a hot file should score worse: {weighted} vs {plain}");
        assert_eq!(
            counts.weighted_health_score(20_000, &ImpactMap::default()),
            plain,
            "an empty impact map must leave the score exactly as it was"
        );
    }

    #[test]
    fn a_small_file_is_not_extrapolated_to_a_repo_rate() {
        let mut counts = FindingCounts::default();
        for _ in 0..3 {
            counts.record(&finding(Severity::Warning, Confidence::Heuristic));
        }
        let score = counts.health_score(45);
        assert!((30..=70).contains(&score), "3 warnings in 45 lines scored {score}");
    }

    #[test]
    fn score_falls_as_weighted_density_rises() {
        let score_of = |n: usize| {
            let mut counts = FindingCounts::default();
            for _ in 0..n {
                counts.record(&finding(Severity::Warning, Confidence::Heuristic));
            }
            counts.health_score(10_000)
        };
        let (few, some, many) = (score_of(5), score_of(50), score_of(500));
        assert!(few > some && some > many, "{few} > {some} > {many}");
        assert!(few > 90, "5 warnings in 10k LOC should stay high, got {few}");
    }
}

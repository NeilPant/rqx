use std::collections::BTreeMap;

use rqx_core::finding::{Finding, Severity};

/// The findings with the findings taken out: how many per family and per rule,
/// biggest first. Shared by every report that answers "which rule is my actual
/// problem" with a count rather than a listing.
pub struct Tally {
    pub families: Vec<(&'static str, u32)>,
    pub rules: Vec<RuleCount>,
}

pub struct RuleCount {
    pub slug: String,
    pub count: u32,
    pub severity: Severity,
}

impl Tally {
    pub fn of(findings: &[&Finding]) -> Self {
        let mut by_family: BTreeMap<&'static str, u32> = BTreeMap::new();
        let mut by_rule: BTreeMap<&str, (u32, Severity)> = BTreeMap::new();
        for finding in findings {
            *by_family.entry(finding.category().slug()).or_default() += 1;
            by_rule
                .entry(finding.slug())
                .or_insert((0, finding.severity))
                .0 += 1;
        }

        let mut families: Vec<(&'static str, u32)> = by_family.into_iter().collect();
        families.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let mut rules: Vec<RuleCount> = by_rule
            .into_iter()
            .map(|(slug, (count, severity))| RuleCount {
                slug: slug.to_owned(),
                count,
                severity,
            })
            .collect();
        rules.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.slug.cmp(&b.slug)));
        Self { families, rules }
    }
}

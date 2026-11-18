//! Compares findings with expectations, and derives the expectations that bless them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::Finding;
use super::sidecar::{Expectation, Verdict};
use super::sources::Sources;

/// One line of the failure message.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Difference {
    file: String,
    line: Option<usize>,
    message: String,
}

impl Difference {
    pub fn at(file: String, line: usize, message: String) -> Self {
        Self {
            file,
            line: Some(line),
            message,
        }
    }

    pub fn in_file(file: String, message: String) -> Self {
        Self {
            file,
            line: None,
            message,
        }
    }
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{line}  {}", self.file, self.message),
            None => write!(f, "{}  {}", self.file, self.message),
        }
    }
}

/// A rule on a line of a file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site {
    file: String,
    line: usize,
    rule: String,
}

impl Site {
    fn of(expectation: &Expectation, sources: &Sources) -> Option<Self> {
        let line = sources.line_of(&expectation.file, &expectation.text, expectation.occurrence)?;
        Some(Self {
            file: expectation.file.clone(),
            line,
            rule: expectation.rule.clone(),
        })
    }

    fn difference(&self, label: &str, message: String) -> Difference {
        Difference::at(format!("{label}/{}", self.file), self.line, message)
    }
}

/// How many times each rule fired on each line.
pub fn fired(findings: &[Finding]) -> BTreeMap<Site, u32> {
    let mut counts = BTreeMap::new();
    for finding in findings {
        let site = Site {
            file: finding.file.clone(),
            line: finding.line,
            rule: finding.rule.clone(),
        };
        *counts.entry(site).or_default() += 1;
    }
    counts
}

pub fn compare(
    expectations: &[Expectation],
    fired: &BTreeMap<Site, u32>,
    sources: &Sources,
    label: &str,
) -> Vec<Difference> {
    let mut unclaimed = fired.clone();
    let mut listed = BTreeSet::new();
    let mut differences = Vec::new();
    for expectation in expectations {
        let Some(site) = Site::of(expectation, sources) else {
            differences.push(matches_no_line(expectation, label));
            continue;
        };
        if !listed.insert(site.clone()) {
            let message = format!("{} is listed twice for this line", expectation.rule);
            differences.push(site.difference(label, message));
            continue;
        }
        let count = unclaimed.remove(&site).unwrap_or(0);
        if let Some(message) = judge(expectation, count) {
            differences.push(site.difference(label, message));
        }
    }
    for (site, count) in unclaimed {
        differences.push(site.difference(label, unexpected(&site.rule, count, 0)));
    }
    differences.sort();
    differences
}

fn matches_no_line(expectation: &Expectation, label: &str) -> Difference {
    Difference::in_file(
        format!("{label}/{}", expectation.file),
        format!(
            "no line matches the entry for {}: {}",
            expectation.rule, expectation.text
        ),
    )
}

fn judge(expectation: &Expectation, count: u32) -> Option<String> {
    let rule = &expectation.rule;
    let wanted = expectation.repeats;
    match &expectation.verdict {
        Verdict::Either => None,
        Verdict::Silent => (count > 0).then(|| format!("unexpected {rule} (marked none=)")),
        _ if count > wanted => Some(unexpected(rule, count, wanted)),
        Verdict::Fires if count < wanted => Some(missing(rule, count, wanted)),
        Verdict::KnownFalsePositive { reason } if count < wanted => Some(format!(
            "known false positive no longer fires: {rule} ({reason}); change known-fp= to none="
        )),
        _ => None,
    }
}

fn unexpected(rule: &str, count: u32, wanted: u32) -> String {
    match (count, wanted) {
        (1, 0) => format!("unexpected {rule}"),
        (_, 0) => format!("unexpected {rule} (fires {count} times)"),
        _ => format!("unexpected {rule} (fires {count} times, expected {wanted})"),
    }
}

fn missing(rule: &str, count: u32, wanted: u32) -> String {
    if count == 0 && wanted == 1 {
        format!("missing {rule}")
    } else {
        format!("missing {rule} (fires {count} times, expected {wanted})")
    }
}

/// The `rule=` entries from the findings, plus every other entry as it was written.
/// A finding on a line that already has such an entry gets no `rule=` beside it, so a
/// `none=` that is violated stays violated until someone decides what it should say.
pub fn blessed(
    existing: &[Expectation],
    fired: &BTreeMap<Site, u32>,
    sources: &Sources,
) -> Vec<Expectation> {
    let decided: Vec<&Expectation> = existing
        .iter()
        .filter(|expectation| expectation.verdict != Verdict::Fires)
        .collect();
    let covered: BTreeSet<Site> = decided
        .iter()
        .filter_map(|expectation| Site::of(expectation, sources))
        .collect();
    let mut expectations: Vec<Expectation> = decided.into_iter().cloned().collect();
    expectations.extend(
        fired
            .iter()
            .filter(|(site, _)| !covered.contains(*site))
            .map(|(site, count)| fires_at(site, *count, sources)),
    );
    expectations.sort_by_cached_key(|expectation| {
        let line = Site::of(expectation, sources).map_or(usize::MAX, |site| site.line);
        (
            expectation.file.clone(),
            line,
            expectation.rule.clone(),
            expectation.text.clone(),
        )
    });
    expectations
}

fn fires_at(site: &Site, count: u32, sources: &Sources) -> Expectation {
    let (text, occurrence) = sources
        .text_at(&site.file, site.line)
        .zip(sources.occurrence_of(&site.file, site.line))
        .unwrap_or_else(|| panic!("{} has no line {}", site.file, site.line));
    Expectation {
        file: site.file.clone(),
        rule: site.rule.clone(),
        verdict: Verdict::Fires,
        repeats: count,
        text: text.to_owned(),
        occurrence,
    }
}

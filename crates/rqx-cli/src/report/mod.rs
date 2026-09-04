mod brief;
mod catalogue;
mod docs;
mod filter;
mod fixes;
mod github;
mod gitlab;
mod json;
mod junit;
mod markdown;
mod patterns;
pub mod projection;
mod sarif;
mod settings;
mod terminal;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use anyhow::Result;
use clap::ValueEnum;
use rqx_core::Cleanliness;
use rqx_core::finding::{Finding, Severity};

use crate::check::{Review, Verdict};
use crate::scan::ScanOutcome;

pub use brief::{json as brief_json, markdown as brief_markdown, render as brief};
pub use catalogue::{explain, levels, rules, techniques};
pub use docs::write as docs;
pub use fixes::{json as fixes_json, render as fixes};
pub use patterns::{
    coverage as pattern_coverage, explain as explain_pattern, json as advice_json, render as advice,
};

/// A misspelling is the common case, and a list of sixty slugs is not an answer to
/// one. Shared so every "no such thing" error in the tool reads the same way.
pub fn unknown(kind: &str, named: &str, known: &[&str], hint: &str) -> anyhow::Error {
    let near = rqx_core::naming::nearest(named, known)
        .map(|near| format!(" Did you mean `{near}`?"))
        .unwrap_or_default();
    anyhow::anyhow!("there is no {kind} called `{named}`.{near} {hint}")
}
pub use filter::FindingFilter;
pub use json::finding_of;
pub use settings::{render as config, rows as setting_rows};
pub use terminal::Layout;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Reporter {
    Terminal,
    Json,
    Sarif,
    Junit,
    Gitlab,
    Github,
    Markdown,
}

impl Reporter {
    pub const ALL: [Self; 7] = [
        Self::Terminal,
        Self::Json,
        Self::Sarif,
        Self::Junit,
        Self::Gitlab,
        Self::Github,
        Self::Markdown,
    ];

    /// Only the console report is written for a person to read; anything else is a
    /// document a machine parses, and an escape sequence in it is corruption.
    pub const fn is_coloured(self) -> bool {
        matches!(self, Self::Terminal)
    }

    /// A document has to arrive whole to parse at all, so it cannot share a stream.
    /// The line-oriented reports can: a CI log is meant to carry both a readable
    /// run and the annotations that mark it up.
    pub const fn is_document(self) -> bool {
        !matches!(self, Self::Terminal | Self::Github)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Json => "json",
            Self::Sarif => "sarif",
            Self::Junit => "junit",
            Self::Gitlab => "gitlab",
            Self::Github => "github",
            Self::Markdown => "markdown",
        }
    }
}

/// What a scan or a review looks like to a reporter. `check` differs from `scan`
/// only by a narrower file set, a base revision and a verdict, so one type carries
/// both rather than every format growing a second rendering path.
pub struct Report<'a> {
    pub outcome: &'a ScanOutcome,
    pub findings: Vec<&'a Finding>,
    pub files: &'a BTreeSet<String>,
    pub base: Option<&'a str>,
    pub verdict: Option<&'a Verdict>,
    pub totals: Totals,
}

/// What is in scope before any display filter, so the summary can say how much the
/// screen is not showing.
#[derive(Default)]
pub struct Totals {
    pub findings: u32,
    pub by_severity: BTreeMap<Severity, u32>,
    pub rules_fired: usize,
}

impl<'a> Report<'a> {
    pub fn scan(outcome: &'a ScanOutcome, filter: &FindingFilter) -> Self {
        Self {
            outcome,
            findings: outcome
                .findings
                .iter()
                .filter(|f| filter.matches(f))
                .collect(),
            files: &outcome.analyzed,
            base: None,
            verdict: None,
            totals: Totals {
                findings: outcome.counts.total,
                by_severity: outcome.counts.by_severity.clone(),
                rules_fired: outcome.counts.by_rule.len(),
            },
        }
    }

    /// Everything the scan found, with no display filter. What a consumer that is not
    /// a terminal wants, and the only way to build a report without naming the private
    /// filter type.
    pub fn everything(outcome: &'a ScanOutcome) -> Self {
        Self::scan(outcome, &FindingFilter::default())
    }

    /// A scan gates only when asked to, so the verdict is attached rather than built
    /// in: without this the run exits non-zero having explained nothing.
    pub fn judged(mut self, verdict: &'a Verdict) -> Self {
        self.verdict = Some(verdict);
        self
    }

    /// The reviewed slice with no display filter, for the same reason as `everything`.
    pub fn reviewed(review: &'a Review, verdict: &'a Verdict) -> Self {
        Self::review(review, verdict, &FindingFilter::default())
    }

    pub fn review(review: &'a Review, verdict: &'a Verdict, filter: &FindingFilter) -> Self {
        Self {
            outcome: &review.outcome,
            findings: review.findings().filter(|f| filter.matches(f)).collect(),
            files: &review.reviewed,
            base: Some(&review.base),
            verdict: Some(verdict),
            totals: totals_of(review.findings()),
        }
    }

    /// Findings name files relative to what was scanned, but a CI provider resolves
    /// an annotation against the checkout root, so scanning a subdirectory has to
    /// say which one.
    pub fn path_prefix(&self) -> String {
        let root = self.outcome.root.as_path();
        if root == std::path::Path::new(".") {
            return String::new();
        }
        let absolute = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let shown = std::env::current_dir()
            .ok()
            .and_then(|cwd| {
                absolute
                    .strip_prefix(&cwd)
                    .ok()
                    .map(std::path::Path::to_path_buf)
            })
            .unwrap_or(absolute);
        match shown.to_string_lossy().replace('\\', "/") {
            text if text.is_empty() => String::new(),
            text => format!("{text}/"),
        }
    }

    /// One file's grade, on the project's own curve applied to that file's lines.
    pub fn grade_of(&self, file: &str) -> Cleanliness {
        let lines = self.outcome.lines_by_file.get(file).copied().unwrap_or(0);
        let findings = self
            .outcome
            .findings_by_file
            .get(file)
            .copied()
            .unwrap_or(0);
        Cleanliness::of(self.outcome.counts.file_score(file, lines), findings)
    }

    pub fn grades(&self) -> BTreeMap<Cleanliness, u32> {
        let mut grades = BTreeMap::new();
        for file in self.files {
            *grades.entry(self.grade_of(file)).or_default() += 1;
        }
        grades
    }

    /// What to open first. The grade already carries how dense a file's problems are,
    /// so blast radius only breaks ties — which is why a hot file with nothing wrong
    /// stops being recommended.
    pub fn priority(&self) -> Vec<(&'a str, Cleanliness)> {
        let mut ranked: Vec<(&'a str, Cleanliness)> = self
            .totals_by_file()
            .into_keys()
            .map(|file| (file, self.grade_of(file)))
            .collect();
        ranked.sort_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| {
                    self.outcome
                        .impact
                        .get(right.0)
                        .multiplier()
                        .total_cmp(&self.outcome.impact.get(left.0).multiplier())
                })
                .then_with(|| left.0.cmp(right.0))
        });
        ranked
    }

    pub fn health(&self) -> u32 {
        self.outcome
            .counts
            .weighted_health_score(self.outcome.code_lines, &self.outcome.impact)
    }

    /// Findings per in-scope file before the display cap and before any filter, so a
    /// file with nothing to report is never confused with one whose findings were
    /// dropped on the way to the screen.
    pub fn totals_by_file(&self) -> BTreeMap<&'a str, u32> {
        self.outcome
            .findings_by_file
            .iter()
            .filter(|(file, _)| self.files.contains(file.as_ref()))
            .map(|(file, count)| (file.as_ref(), *count))
            .collect()
    }

    pub fn shown_by_file(&self) -> BTreeMap<&'a str, Vec<&'a Finding>> {
        let mut by_file: BTreeMap<&'a str, Vec<&'a Finding>> = BTreeMap::new();
        for finding in &self.findings {
            by_file
                .entry(&finding.location.file)
                .or_default()
                .push(finding);
        }
        by_file
    }
}

pub fn emit(
    report: &Report<'_>,
    reporter: Reporter,
    layout: &Layout,
    out: &mut dyn Write,
) -> Result<()> {
    match reporter {
        Reporter::Terminal => terminal::render(report, layout, out),
        Reporter::Json => json::render(report, out),
        Reporter::Sarif => sarif::render(report, out),
        Reporter::Junit => junit::render(report, out),
        Reporter::Gitlab => gitlab::render(report, out),
        Reporter::Github => github::render(report, out),
        Reporter::Markdown => markdown::render(report, out),
    }
}

/// A review's severity split cannot come from `counts`, which covers the whole
/// project rather than the reviewed subset.
fn totals_of<'a>(findings: impl Iterator<Item = &'a Finding>) -> Totals {
    let mut totals = Totals::default();
    let mut rules = BTreeSet::new();
    for finding in findings {
        totals.findings += 1;
        *totals.by_severity.entry(finding.severity).or_default() += 1;
        rules.insert(finding.rule);
    }
    totals.rules_fired = rules.len();
    totals
}

pub fn plural(count: u32, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use rqx_core::finding::{Confidence, Evidence, Severity, SourceRange};
    use rqx_core::rule::RuleId;
    use rqx_core::score::FindingCounts;

    use super::*;

    /// A scan outcome assembled by hand, so a reporter can be tested without a
    /// repository to scan.
    pub fn outcome_with(findings: &[(&str, Severity)], files_scanned: u32) -> ScanOutcome {
        let mut outcome = ScanOutcome {
            root: std::path::PathBuf::from("."),
            counts: FindingCounts::default(),
            analyzed: BTreeSet::new(),
            impact: Default::default(),
            import_edges: BTreeMap::new(),
            findings: Vec::new(),
            advice: Vec::new(),
            findings_by_file: BTreeMap::new(),
            lines_by_file: BTreeMap::new(),
            omitted: 0,
            code_lines: 10_000,
            files_scanned,
            skipped: Vec::new(),
            elapsed_ms: 840,
        };
        for (file, severity) in findings {
            let finding = Finding {
                rule: RuleId::LongFunction,
                location: SourceRange {
                    file: Arc::from(*file),
                    start: 0,
                    end: 1,
                    line: 1,
                    column: 1,
                    end_line: 1,
                },
                severity: *severity,
                confidence: Confidence::Verified,
                evidence: Evidence::StatementCount {
                    count: 99,
                    threshold: 40,
                },
                fix_hint: "extract".to_owned(),
                related: Vec::new(),
            };
            outcome.counts.record(&finding);
            *outcome
                .findings_by_file
                .entry(Arc::clone(&finding.location.file))
                .or_default() += 1;
            outcome.analyzed.insert((*file).to_owned());
            outcome.findings.push(finding);
        }
        for index in 0..files_scanned {
            if outcome.analyzed.len() as u32 == files_scanned {
                break;
            }
            outcome.analyzed.insert(format!("clean{index}.ts"));
        }
        outcome
    }
}

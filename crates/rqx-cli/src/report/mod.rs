mod brief;
mod catalogue;
mod clients;
mod docs;
mod filter;
mod fixes;
mod github;
mod gitlab;
mod graded;
mod json;
mod junit;
mod markdown;
mod patterns;
pub mod projection;
mod ranked;
mod sarif;
mod settings;
mod terminal;
mod waivers;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use anyhow::Result;
use clap::ValueEnum;
use rqx_core::Cleanliness;
use rqx_core::finding::{Finding, Severity};

use crate::check::Review;
use crate::gate::Verdict;
use crate::scan::ScanOutcome;

pub use brief::{json as brief_json, markdown as brief_markdown, render as brief};
pub use catalogue::{explain, levels, rules, techniques};
pub use clients::{matrix as editors, render as editor, write as write_editor};
pub use docs::write as docs;
pub use fixes::{json as fixes_json, render as fixes};
pub use graded::{json as verify_json, render as verify};
pub use patterns::{
    coverage as pattern_coverage, explain as explain_pattern, json as advice_json, render as advice,
};
pub use ranked::{json as plan_json, render as plan};
pub use waivers::{json as waivers_json, render as waivers};

/// Where a scan spent its time. Printed above the report rather than inside it, so
/// no reporter's format has to grow a field nothing else wants.
pub fn timings(
    outcome: &ScanOutcome,
    palette: crate::brand::Palette,
    out: &mut dyn Write,
) -> Result<()> {
    use crate::brand::{Ink, paint};
    let dim = palette.style(Ink::Muted);
    let total: u128 = outcome.timings.iter().map(|(_, took)| took).sum();
    let width = outcome
        .timings
        .iter()
        .map(|(stage, _)| stage.len())
        .max()
        .unwrap_or(0)
        .max("total".len());
    writeln!(out, "\n  {}", paint(palette.bold(Ink::Text), "Timings"))?;
    for (stage, took) in &outcome.timings {
        if *took == 0 {
            continue;
        }
        let share = took
            .checked_mul(100)
            .and_then(|of| of.checked_div(total))
            .unwrap_or(0);
        writeln!(
            out,
            "    {stage:<width$} {:>6}ms  {}",
            took,
            paint(dim, "\u{2588}".repeat((share / 4).max(1) as usize))
        )?;
    }
    writeln!(out, "    {:<width$} {total:>6}ms", "total")?;
    Ok(())
}

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
pub use markdown::{classification, comment};
pub use settings::{render as config, rows as setting_rows};
pub use terminal::{Detail, Grouping, Layout, Order};

/// The shape of the machine formats. Bumped only when a key is removed or changes
/// meaning, which `docs/json.md` promises never to do, so a consumer can refuse a
/// document newer than it understands.
pub const SCHEMA_VERSION: u32 = 1;

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

/// Which edge a column's text sits against.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

/// Pads text to a column width, measuring what the terminal will show rather than
/// what the string holds. `{:<n}` counts bytes, so a colour around the text spends
/// the column on escape sequences and the table shears — pad here, then paint.
pub fn column(text: &str, width: usize, align: Align) -> String {
    use unicode_width::UnicodeWidthStr;

    let shown = text.width();
    let Some(fill) = width.checked_sub(shown) else {
        return text.to_owned();
    };
    match align {
        Align::Left => format!("{text}{:fill$}", ""),
        Align::Right => format!("{:fill$}{text}", ""),
    }
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
    /// The same, for callers that care which rule fired: ranking reads the technique
    /// a rule prescribes, so every finding being one rule would make every item cost
    /// the same.
    pub fn outcome_of(findings: &[(RuleId, &str)]) -> ScanOutcome {
        let mut outcome = outcome_with(&[], 0);
        for (rule, file) in findings {
            let spec = rule.spec();
            let finding = Finding {
                rule: *rule,
                location: SourceRange {
                    file: Arc::from(*file),
                    start: 0,
                    end: 1,
                    line: 1,
                    column: 1,
                    end_line: 1,
                },
                severity: spec.severity,
                confidence: spec.confidence,
                evidence: Evidence::StatementCount {
                    count: 99,
                    threshold: 40,
                },
                fix_hint: "fix".to_owned(),
                related: Vec::new(),
                known: false,
            };
            outcome.counts.record(&finding);
            *outcome
                .findings_by_file
                .entry(Arc::clone(&finding.location.file))
                .or_default() += 1;
            outcome.analyzed.insert((*file).to_owned());
            outcome.findings.push(finding);
        }
        outcome.files_scanned = outcome.analyzed.len() as u32;
        outcome
    }

    pub fn outcome_with(findings: &[(&str, Severity)], files_scanned: u32) -> ScanOutcome {
        let mut outcome = ScanOutcome {
            configured: true,
            versioned: true,
            root: std::path::PathBuf::from("."),
            counts: FindingCounts::default(),
            analyzed: BTreeSet::new(),
            impact: Default::default(),
            import_edges: BTreeMap::new(),
            cycles: Vec::new(),
            findings: Vec::new(),
            advice: Vec::new(),
            findings_by_file: BTreeMap::new(),
            lines_by_file: BTreeMap::new(),
            shape_by_file: BTreeMap::new(),
            suppressed: Vec::new(),
            baseline_size: None,
            timings: Vec::new(),
            omitted: 0,
            code_lines: 10_000,
            files_scanned,
            skipped: Vec::new(),
            profile: None,
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
                known: false,
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

/// A monochrome palette exists so a writer that is not a terminal gets text and
/// nothing else — a file, a ratatui buffer, a string an agent will read. It used to
/// leak anyway: `style(ink)` is empty under `Monochrome`, but bold is an effect
/// rather than a colour, so `style(ink).bold()` still rendered `\x1b[1m` and its
/// reset. `--reporter terminal=<path>` wrote 29 of them into the file and the
/// interface's reading pane drew them as the literal text `[1m`.
#[cfg(test)]
mod plainness {
    use crate::brand::{ColorDepth, Ink, Palette};

    use super::tests::{outcome_of, outcome_with};
    use super::*;

    fn plain() -> Palette {
        Palette::with_depth(ColorDepth::Monochrome)
    }

    fn rendered(what: &str, render: impl FnOnce(&mut dyn Write)) {
        let mut out: Vec<u8> = Vec::new();
        render(&mut out);
        let text = String::from_utf8_lossy(&out);
        let escapes = text.matches('\u{1b}').count();
        assert_eq!(
            escapes, 0,
            "{what} emitted {escapes} escape sequences:\n{text}"
        );
    }

    #[test]
    fn no_reporter_emits_an_escape_at_monochrome() {
        let outcome = outcome_of(&[
            (rqx_core::rule::RuleId::LongFunction, "a.ts"),
            (rqx_core::rule::RuleId::MagicNumber, "b.ts"),
        ]);
        let filter = FindingFilter::default();
        let report = Report::scan(&outcome, &filter);
        let layout = Layout {
            detail: Detail::Full,
            palette: plain(),
            show_passed: true,
            quiet: false,
            grouping: Grouping::File,
            show_suppressed: true,
            order: Order::Line,
            top: None,
            stats: false,
            context: 2,
        };
        for reporter in Reporter::ALL {
            rendered(reporter.name(), |out| {
                emit(&report, reporter, &layout, out).expect("a report");
            });
        }
        // The counts-only view takes a different path through the same reporter.
        let stats = Layout {
            stats: true,
            ..layout
        };
        rendered("terminal --stats", |out| {
            emit(&report, Reporter::Terminal, &stats, out).expect("a report");
        });
        rendered("timings", |out| {
            timings(&outcome, plain(), out).expect("timings");
        });
    }

    /// The reading pane calls these four, and they were the ones the user saw.
    #[test]
    fn no_reference_page_emits_an_escape_at_monochrome() {
        rendered("techniques", |out| {
            techniques(plain(), out).expect("techniques");
        });
        rendered("levels", |out| {
            levels(plain(), out).expect("levels");
        });

        let outcome = outcome_with(&[("a.ts", rqx_core::finding::Severity::Error)], 1);
        rendered("waivers", |out| {
            waivers(std::path::Path::new("."), &outcome, false, plain(), out).expect("waivers");
        });
        rendered("advice", |out| {
            advice(&outcome, plain(), out).expect("advice");
        });
        let ranked = crate::plan::rank(&outcome, None, 20);
        rendered("plan", |out| {
            plan(&outcome, &ranked, None, true, plain(), out).expect("plan");
        });
    }

    /// Every style a palette hands out, not only the ones a reporter happens to use
    /// today: the leak was in `bold`, `mark` and `badge`, none of which the earlier
    /// test touched because it only compared `style(ink)` against an empty style.
    #[test]
    fn every_style_a_monochrome_palette_hands_out_is_empty() {
        let palette = plain();
        for ink in [
            Ink::Accent,
            Ink::Danger,
            Ink::Warn,
            Ink::Good,
            Ink::Text,
            Ink::Muted,
            Ink::Line,
            Ink::Ground,
        ] {
            for (name, style) in [
                ("style", palette.style(ink)),
                ("bold", palette.bold(ink)),
                ("mark", palette.mark(ink)),
                ("badge", palette.badge(ink)),
            ] {
                let painted = crate::brand::paint(style, "x");
                assert_eq!(painted, "x", "{name}({ink:?}) painted {painted:?}");
            }
        }
    }
}

mod brief;
mod catalogue;
mod clients;
mod docs;
mod escape;
mod filter;
mod fixes;
mod github;
mod gitlab;
mod graded;
mod html;
mod json;
mod junit;
mod link;
mod markdown;
mod patterns;
pub mod projection;
mod ranked;
mod sarif;
mod settings;
mod tally;
mod terminal;
mod validation;
mod waivers;
mod wrap;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use clap::ValueEnum;
use rqx_core::Cleanliness;
use rqx_core::finding::{Finding, Severity};

use crate::check::Review;
use crate::gate::Verdict;
use crate::scan::ScanOutcome;

pub use brief::{json as brief_json, markdown as brief_markdown, render as brief};
pub use catalogue::{explain, levels, rules, techniques};
pub use clients::{
    agent, agent_change, matrix as editors, render as editor, write as write_editor,
};
pub use docs::schema_text as config_schema;
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
///
/// `later` holds stages that only exist once the scan has returned: rendering is one,
/// because the outcome is finished before anything is drawn.
pub fn timings(
    outcome: &ScanOutcome,
    later: &[(&'static str, u128)],
    palette: crate::brand::Palette,
    out: &mut dyn Write,
) -> Result<()> {
    use crate::brand::{Ink, paint};
    let dim = palette.style(Ink::Muted);
    let stages: Vec<&(&'static str, u128)> = outcome.timings.iter().chain(later).collect();
    let total: u128 = stages.iter().map(|(_, took)| took).sum();
    let width = stages
        .iter()
        .map(|(stage, _)| stage.len())
        .max()
        .unwrap_or(0)
        .max("total".len());
    writeln!(out, "\n  {}", paint(palette.bold(Ink::Text), "Timings"))?;
    for (stage, took) in stages {
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
    counts(&outcome.stats, palette, out)
}

/// Every `git` command the scan ran, for a reader who wants to know what was asked of
/// the repository and how often.
pub fn git_commands(
    outcome: &ScanOutcome,
    palette: crate::brand::Palette,
    out: &mut dyn Write,
) -> Result<()> {
    use crate::brand::{Ink, paint};
    writeln!(
        out,
        "\n  {}",
        paint(palette.bold(Ink::Text), "Git commands")
    )?;
    if outcome.git_commands.is_empty() {
        writeln!(out, "    none")?;
    }
    for command in &outcome.git_commands {
        writeln!(out, "    {command}")?;
    }
    Ok(())
}

/// What the scan did, which a time cannot say: the same counts on every machine.
fn counts(
    stats: &crate::stats::ScanStats,
    palette: crate::brand::Palette,
    out: &mut dyn Write,
) -> Result<()> {
    use crate::brand::{Ink, paint};
    let rows = [
        ("files read", stats.files_read),
        ("cache hits", stats.cache_hits),
        ("cache misses", stats.cache_misses),
        ("walks", stats.walks),
        ("git processes", stats.git_processes),
        ("blames", stats.blames),
    ];
    let width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    writeln!(out, "\n  {}", paint(palette.bold(Ink::Text), "Counts"))?;
    for (name, count) in rows {
        writeln!(out, "    {name:<width$} {count:>6}")?;
    }
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
pub use link::Links;
pub use markdown::{classification, comment};
pub use settings::{render as config, render_json as config_json, rows as setting_rows};
pub use terminal::{Detail, Grouping, Layout, Order};
pub use validation::{json as validation_json, render as validation};

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
    Html,
}

impl Reporter {
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
    /// What the change paid off. Present only on a review that scanned its base.
    pub credit: Option<&'a crate::credit::Credit>,
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
            credit: None,
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
            credit: review.credit.as_ref(),
            totals: totals_of(review.findings()),
        }
    }

    /// Findings name files relative to what was scanned, but a CI provider resolves
    /// an annotation against the checkout root, so scanning a subdirectory has to
    /// say which one.
    pub fn path_prefix(&self) -> String {
        prefix_below(&self.outcome.root, std::env::current_dir().ok().as_deref())
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

    /// The owners of one file, empty when the project has no CODEOWNERS file or no rule
    /// names the file.
    pub fn owners_of(&self, file: &str) -> Vec<&'a str> {
        self.outcome
            .owners
            .as_ref()
            .map(|owners| owners.owners_of(file))
            .unwrap_or_default()
    }

    /// Each owner's files and health over what this report covers, worst first. Empty
    /// when the project has no CODEOWNERS file.
    pub fn owner_rows(&self) -> Vec<crate::ownership::OwnerRow> {
        crate::ownership::table(self.outcome, self.files)
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
        Reporter::Html => html::render(report, out),
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

/// `root` as a path below `cwd` with a trailing slash, or the canonical path when it
/// lies elsewhere. Both sides are canonicalized the same way: on Windows the standard
/// library's answer carries a verbatim prefix the working directory lacks, so it would
/// never be stripped.
fn prefix_below(root: &Path, cwd: Option<&Path>) -> String {
    if root == Path::new(".") {
        return String::new();
    }
    let absolute = rqx_core::paths::canonical_or_same(root);
    let shown = cwd
        .map(rqx_core::paths::canonical_or_same)
        .and_then(|cwd| absolute.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or(absolute);
    match rqx_core::paths::display_path(&shown) {
        text if text.is_empty() => String::new(),
        text => format!("{text}/"),
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

    #[test]
    fn a_subdirectory_of_the_working_directory_is_named_without_the_machine_path() {
        let cwd = std::env::temp_dir().join(format!("rqx-prefix-{}", std::process::id()));
        std::fs::create_dir_all(cwd.join("pkg")).expect("a directory to scan");
        let prefix = prefix_below(&cwd.join("pkg"), Some(&cwd));
        let _ = std::fs::remove_dir_all(&cwd);
        assert_eq!(prefix, "pkg/");
    }

    #[test]
    fn the_working_directory_itself_has_no_prefix() {
        assert_eq!(
            prefix_below(Path::new("."), Some(Path::new("/anywhere"))),
            ""
        );
    }

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
            outcome
                .severity_by_file
                .entry(Arc::clone(&finding.location.file))
                .or_default()
                .record(&finding);
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
            severity_by_file: BTreeMap::new(),
            lines_by_file: BTreeMap::new(),
            shape_by_file: BTreeMap::new(),
            suppressed: Vec::new(),
            baseline_size: None,
            timings: Vec::new(),
            stats: Default::default(),
            git_commands: Vec::new(),
            config_files: Vec::new(),
            owners: None,
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
            outcome
                .severity_by_file
                .entry(Arc::clone(&finding.location.file))
                .or_default()
                .record(&finding);
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
            verbose: false,
            width: None,
            links: None,
        };
        for reporter in Reporter::value_variants().iter().copied() {
            rendered(&crate::sink::Format::label(reporter), |out| {
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
            timings(&outcome, &[], plain(), out).expect("timings");
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

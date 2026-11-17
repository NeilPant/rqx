use std::io::Write;

use anyhow::Result;
use rqx_core::Cleanliness;
use rqx_core::LineIndex;
use rqx_core::finding::{Finding, Severity};
use rqx_core::rule::RuleId;

use crate::brand::{Ink, Palette, grade_ink, paint, score_ink, severity_ink};
use crate::preview::{self, HEAD_LINES};
use crate::report::tally::{RuleCount, Tally};
use crate::report::{Report, plural};
use crate::scan::ScanOutcome;

/// How much of the report a person asked for. Only the console report reads it;
/// a machine format has no notion of a line it may skip.
/// What the listing is a list of. A reader looking for the worst file and one
/// looking for the rule that fires most are asking different questions of the same
/// findings.
#[derive(Copy, Clone, Default, PartialEq, Eq)]
pub enum Grouping {
    #[default]
    File,
    Rule,
    Family,
    Severity,
}

/// What decides the order inside a group.
#[derive(Copy, Clone, Default, PartialEq, Eq)]
pub enum Order {
    /// Where it is in the file, which is how a reader walks one.
    #[default]
    Line,
    Severity,
    File,
}

/// How much of each finding to print. `Why:` is a property of the rule and reads
/// the same on all twenty of its findings; `Cost:` is a property of the file and
/// reads the same on every finding in it. Repeating both on every line is most of
/// what makes the report a wall.
#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub enum Detail {
    /// One line per finding: where, how bad, which rule, and what it found.
    Terse,
    /// The reason once per rule and the cost once per file.
    #[default]
    Normal,
    /// Every line on every finding.
    Full,
}

pub struct Layout {
    pub palette: Palette,
    pub detail: Detail,
    pub show_passed: bool,
    pub quiet: bool,
    pub grouping: Grouping,
    /// Whether to list what a comment waived. Off by default: a waiver is a decision
    /// already taken, and the point of taking it is not to read it again.
    pub show_suppressed: bool,
    pub order: Order,
    /// How many groups to list. The rest are counted, not shown.
    pub top: Option<usize>,
    /// Counts only: which rules and families fired, and how often. For deciding
    /// where to look before reading a single finding.
    pub stats: bool,
    /// Lines of unmarked code shown either side of the offence.
    pub context: u32,
    /// `-v`: what the run read and how long it took, and every skipped file instead of
    /// the first few.
    pub verbose: bool,
}

/// One listing in progress: what to print, what to print it from, and what has
/// already been said so it is not said again. The ledger is per report rather than
/// per file, because a rule's reason does not change between them.
struct Listing<'a> {
    layout: &'a Layout,
    report: &'a Report<'a>,
    prefix: String,
    said: std::collections::HashSet<&'static str>,
}

impl<'a> Listing<'a> {
    fn new(layout: &'a Layout, report: &'a Report<'a>) -> Self {
        Self {
            layout,
            report,
            prefix: report.path_prefix(),
            said: std::collections::HashSet::new(),
        }
    }
}

/// The report with the findings taken out: what fired, where, and how often. The
/// answer to "which of these forty rules is my actual problem" is a count, and
/// reading five hundred findings to arrive at it is the slow way.
fn stats(report: &Report<'_>, layout: &Layout, out: &mut dyn Write) -> Result<()> {
    let palette = layout.palette;
    let dim = palette.style(Ink::Muted);
    let tally = Tally::of(&report.findings);

    let total = report.findings.len().max(1) as u32;
    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.bold(Ink::Text), "By family"),
        paint(dim, plural(total, "finding", "findings"))
    )?;
    for (family, count) in &tally.families {
        writeln!(
            out,
            "    {:<14} {:>6}  {}",
            family,
            count,
            paint(dim, bar(*count, total))
        )?;
    }

    writeln!(out, "\n  {}", paint(palette.bold(Ink::Text), "By rule"))?;
    let width = tally
        .rules
        .iter()
        .map(|rule| rule.slug.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 34);
    for RuleCount {
        slug,
        count,
        severity,
    } in &tally.rules
    {
        writeln!(
            out,
            "    {} {:>6}  {}",
            paint(
                palette.style(severity_ink(*severity)),
                super::column(slug, width, super::Align::Left)
            ),
            count,
            paint(dim, bar(*count, total))
        )?;
    }
    write_footer(out, layout, report)
}

/// Proportional to the largest, not to the total: the point is the ranking.
fn bar(count: u32, total: u32) -> String {
    let width = ((count as usize * 30) / total.max(1) as usize).max(1);
    "\u{2588}".repeat(width.min(30))
}

const LABEL_WIDTH: usize = 10;
const MAX_RULE_ROWS: usize = 8;
const MAX_SKIPPED_ROWS: usize = 10;
const HOTSPOTS: usize = 3;

pub fn render(report: &Report<'_>, layout: &Layout, out: &mut dyn Write) -> Result<()> {
    if layout.stats {
        return stats(report, layout, out);
    }
    if layout.grouping != Grouping::File {
        return grouped(report, layout, out);
    }
    let shown = report.shown_by_file();
    let totals = report.totals_by_file();
    let prefix = report.path_prefix();

    write_header(out, layout, report)?;

    let mut listed: Vec<(&&str, &Vec<&Finding>)> = shown.iter().collect();
    if layout.top.is_some() {
        // Most findings first, so a cut list is the part worth reading.
        listed.sort_by(|left, right| {
            right
                .1
                .len()
                .cmp(&left.1.len())
                .then_with(|| left.0.cmp(right.0))
        });
        listed.truncate(layout.top.unwrap_or(listed.len()));
    }
    let mut listing = Listing::new(layout, report);
    for (file, findings) in listed {
        let mut findings: Vec<&Finding> = findings.clone();
        sort_findings(&mut findings, layout.order);
        write_file(
            out,
            &mut listing,
            file,
            &findings,
            totals.get(*file).copied(),
        )?;
    }

    if layout.show_passed {
        for file in report
            .files
            .iter()
            .filter(|file| !totals.contains_key(file.as_str()))
        {
            writeln!(
                out,
                "\n {}  {}",
                paint(layout.palette.badge(Ink::Good), " PASS "),
                format_args!("{prefix}{file}")
            )?;
        }
    }

    if layout.show_suppressed {
        write_waived(out, layout, report, &prefix)?;
    }

    let unlisted = totals.len() - shown.len();
    if unlisted > 0 {
        writeln!(
            out,
            "\n  {}",
            paint(
                layout.palette.style(Ink::Muted),
                format!(
                    "{} with findings that this view does not show",
                    plural(unlisted as u32, "more file", "more files")
                )
            )
        )?;
    }

    write_footer(out, layout, report)
}

fn write_header(out: &mut dyn Write, layout: &Layout, report: &Report<'_>) -> Result<()> {
    if let Some(base) = report.base {
        writeln!(
            out,
            "\n  {} against {}",
            paint(layout.palette.bold(Ink::Text), "Reviewing changes"),
            paint(layout.palette.style(Ink::Accent), base)
        )?;
    } else {
        // The summary is at the end where the eye lands, but a listing this long is
        // scrolled from the top, and the top used to say nothing about what it was.
        let outcome = report.outcome;
        writeln!(
            out,
            "\n  {}  {}",
            paint(
                layout.palette.bold(Ink::Text),
                outcome.root.display().to_string()
            ),
            paint(
                layout.palette.style(Ink::Muted),
                format!(
                    "{} \u{b7} {} \u{b7} health {}/100",
                    plural(outcome.files_scanned, "file", "files"),
                    plural(report.totals.findings, "finding", "findings"),
                    report.health()
                )
            )
        )?;
    }
    Ok(())
}

/// What every listing ends with, however it is arranged: the answer to "is this
/// project all right" and, when a gate was asked for, whether it passed.
fn write_footer(out: &mut dyn Write, layout: &Layout, report: &Report<'_>) -> Result<()> {
    let prefix = report.path_prefix();
    write_cycles(out, layout, report)?;
    write_architecture(out, layout, report, &prefix)?;
    write_summary(out, layout, report, &report.totals_by_file())?;
    write_hotspots(out, layout, report, &prefix)?;
    write_skipped(out, layout, report.outcome)?;
    write_notes(out, layout, report.outcome)?;
    write_gate(out, layout, report)?;
    write_run(out, layout, report.outcome)
}

/// The same findings under a heading that is not a file. The file listing shows a
/// grade and an excerpt per file; these headings have neither, so they are a count
/// and the findings under it.
fn grouped(report: &Report<'_>, layout: &Layout, out: &mut dyn Write) -> Result<()> {
    let mut listing = Listing::new(layout, report);
    let mut groups: std::collections::BTreeMap<String, Vec<&Finding>> =
        std::collections::BTreeMap::new();
    for finding in &report.findings {
        let key = match layout.grouping {
            Grouping::Rule => finding.slug().to_owned(),
            Grouping::Family => finding.category().slug().to_owned(),
            Grouping::Severity => finding.severity.label().to_owned(),
            Grouping::File => finding.location.file.to_string(),
        };
        groups.entry(key).or_default().push(finding);
    }
    let mut listed: Vec<(String, Vec<&Finding>)> = groups.into_iter().collect();
    listed.sort_by(|left, right| {
        right
            .1
            .len()
            .cmp(&left.1.len())
            .then_with(|| left.0.cmp(&right.0))
    });
    let held = listed.len();
    if let Some(top) = layout.top {
        listed.truncate(top);
    }

    let prefix = report.path_prefix();
    for (name, findings) in &mut listed {
        sort_findings(findings, layout.order);
        writeln!(
            out,
            "\n  {}  {}",
            paint(layout.palette.bold(Ink::Text), name.as_str()),
            paint(
                layout.palette.style(Ink::Muted),
                plural(findings.len() as u32, "finding", "findings")
            )
        )?;
        if layout.quiet {
            continue;
        }
        for finding in findings {
            writeln!(
                out,
                "     {}",
                paint(
                    layout.palette.style(Ink::Muted),
                    format!(
                        "{prefix}{}:{}",
                        finding.location.file, finding.location.line
                    )
                )
            )?;
            write_finding(out, &mut listing, finding, None, None)?;
        }
    }
    let unlisted = held - listed.len();
    if unlisted > 0 {
        writeln!(
            out,
            "\n  {}",
            paint(
                layout.palette.style(Ink::Muted),
                format!("{unlisted} more not shown")
            )
        )?;
    }
    write_footer(out, layout, report)
}

fn sort_findings(findings: &mut [&Finding], order: Order) {
    match order {
        Order::Line => findings.sort_by_key(|finding| finding.location.start),
        Order::File => findings.sort_by(|left, right| {
            left.location
                .file
                .cmp(&right.location.file)
                .then(left.location.start.cmp(&right.location.start))
        }),
        Order::Severity => findings.sort_by(|left, right| {
            left.severity
                .cmp(&right.severity)
                .then(left.location.start.cmp(&right.location.start))
        }),
    }
}

/// What a comment waived, and the reason it gave. Dim, and under everything else:
/// these are not the run's findings, they are the decisions somebody already made
/// about them.
fn write_waived(
    out: &mut dyn Write,
    layout: &Layout,
    report: &Report<'_>,
    prefix: &str,
) -> Result<()> {
    let waived = &report.outcome.suppressed;
    if waived.is_empty() {
        return Ok(());
    }
    let dim = layout.palette.style(Ink::Muted);
    writeln!(
        out,
        "\n  {}",
        paint(
            layout.palette.bold(Ink::Text),
            plural(waived.len() as u32, "waived finding", "waived findings")
        )
    )?;
    for one in waived {
        writeln!(
            out,
            "   {} {}  {}",
            paint(dim, "\u{25cb}"),
            paint(layout.palette.style(Ink::Accent), one.finding.slug()),
            paint(
                dim,
                format!(
                    "{prefix}{}:{}",
                    one.finding.location.file, one.finding.location.line
                )
            )
        )?;
        if let Some(reason) = &one.reason {
            writeln!(out, "     {} {reason}", paint(dim, "Because:"))?;
        }
        if let Some(until) = &one.until {
            writeln!(out, "     {} {until}", paint(dim, "Until:   "))?;
        }
    }
    Ok(())
}

fn write_file(
    out: &mut dyn Write,
    listing: &mut Listing<'_>,
    file: &str,
    findings: &[&Finding],
    total: Option<u32>,
) -> Result<()> {
    let (layout, report, prefix) = (listing.layout, listing.report, listing.prefix.clone());
    let grade = report.grade_of(file);
    let total = total.unwrap_or(findings.len() as u32);
    let tally = if total as usize == findings.len() {
        plural(total, "finding", "findings")
    } else {
        format!("{} of {} findings shown", findings.len(), total)
    };

    writeln!(
        out,
        "\n {}  {}  {} {}",
        paint(layout.palette.badge(grade_ink(grade)), " FAIL "),
        format_args!("{prefix}{file}"),
        paint(layout.palette.style(grade_ink(grade)), grade.label()),
        paint(layout.palette.style(Ink::Muted), format!("\u{b7} {tally}"))
    )?;

    if layout.quiet {
        return Ok(());
    }

    let source = std::fs::read_to_string(report.outcome.root.join(file)).ok();
    let index = source.as_deref().map(LineIndex::new);
    if layout.detail != Detail::Full
        && let Some(cost) = impact_line(report.outcome, file)
    {
        writeln!(
            out,
            "        {} {}",
            paint(layout.palette.style(Ink::Muted), "Cost:"),
            paint(layout.palette.style(Ink::Warn), cost)
        )?;
    }
    // Full repeats the cost on every finding; anything else has just printed it once
    // above, where it belongs — it is the file's number, not the finding's.
    let cost = (layout.detail == Detail::Full)
        .then(|| impact_line(report.outcome, file))
        .flatten();
    for finding in findings {
        write_finding(out, listing, finding, index.as_ref(), cost.as_deref())?;
    }
    Ok(())
}

fn write_finding(
    out: &mut dyn Write,
    listing: &mut Listing<'_>,
    finding: &Finding,
    index: Option<&LineIndex<'_>>,
    cost: Option<&str>,
) -> Result<()> {
    let layout = listing.layout;
    let palette = layout.palette;
    let spec = finding.rule.spec();
    let ink = severity_ink(finding.severity);
    let dim = palette.style(Ink::Muted);

    if layout.detail == Detail::Terse {
        return writeln!(
            out,
            "   {} {}  {}  {}",
            paint(palette.style(ink), finding.severity.label()),
            paint(
                palette.style(Ink::Muted),
                format!("{}:{}", finding.location.line, finding.location.column)
            ),
            paint(palette.style(Ink::Accent), finding.slug()),
            finding.evidence
        )
        .map_err(Into::into);
    }

    writeln!(
        out,
        "\n   {} {}  {}  {}",
        paint(palette.style(ink), "\u{25cf}"),
        paint(palette.style(Ink::Accent), finding.slug()),
        paint(
            palette.style(Ink::Muted),
            format!("{}:{}", finding.location.line, finding.location.column)
        ),
        paint(palette.style(ink), finding.severity.label())
    )?;

    if let Some(index) = index {
        write_excerpt(out, layout, finding, index)?;
    }

    writeln!(out, "     {}", finding.evidence)?;
    // A rule the project wrote has already said why in its own message; the
    // catalogue's rationale belongs to a rule rqx ships.
    let fresh = listing.said.insert(spec.slug) || layout.detail == Detail::Full;
    if fresh && !matches!(finding.evidence, rqx_core::finding::Evidence::Custom { .. }) {
        writeln!(out, "     {} {}", paint(dim, "Why: "), spec.why)?;
    }
    if let Some(cost) = cost {
        writeln!(
            out,
            "     {} {}",
            paint(dim, "Cost:"),
            paint(palette.style(Ink::Warn), cost)
        )?;
    }
    let techniques = spec
        .fixes
        .iter()
        .map(|fix| fix.name())
        .collect::<Vec<_>>()
        .join(" / ");
    if !techniques.is_empty() || !finding.fix_hint.is_empty() {
        let named = if techniques.is_empty() || finding.fix_hint.is_empty() {
            format!(
                "{}{}",
                paint(palette.style(Ink::Good), &techniques),
                finding.fix_hint
            )
        } else {
            format!(
                "{} \u{2014} {}",
                paint(palette.style(Ink::Good), &techniques),
                finding.fix_hint
            )
        };
        writeln!(out, "     {} {named}", paint(dim, "Fix: "))?;
    }

    for site in &finding.related {
        writeln!(out, "     {} {site}", paint(dim, "also at"))?;
    }
    Ok(())
}

/// The file is re-read at render time rather than held from the scan, so a file
/// edited mid-scan can be shorter than the recorded span; a stale excerpt is
/// dropped instead of underlining the wrong code.
fn write_excerpt(
    out: &mut dyn Write,
    layout: &Layout,
    finding: &Finding,
    index: &LineIndex<'_>,
) -> Result<()> {
    let Some(view) = preview::of(index, &finding.location, layout.context) else {
        return Ok(());
    };
    let palette = layout.palette;
    let dim = palette.style(Ink::Muted);
    let ink = severity_ink(finding.severity);
    let mut elided = view.elided;

    for (position, line) in view.lines.iter().enumerate() {
        if elided > 0
            && position == HEAD_LINES + usize::from(layout.context > 0) * layout.context as usize
        {
            writeln!(
                out,
                "{:6} {} {}",
                "",
                paint(dim, "\u{22ee}"),
                paint(dim, format!("{elided} more lines"))
            )?;
            elided = 0;
        }
        // Context is dimmed so the eye lands on the offending lines first; without it
        // a five-line window reads as five equally important lines.
        let body = match (&line.mark, line.role, palette.is_monochrome()) {
            (Some(mark), _, false) => format!(
                "{}{}{}",
                &line.text[..mark.start],
                paint(palette.mark(ink), &line.text[mark.clone()]),
                &line.text[mark.end..]
            ),
            (_, preview::Role::Context, false) => paint(dim, &line.text),
            _ => line.text.clone(),
        };
        writeln!(
            out,
            "{} {} {}",
            paint(dim, format!("{:>6}", line.number)),
            paint(dim, "\u{2502}"),
            body
        )?;

        // Kept under the highlight rather than replaced by it: a terminal that drops
        // background colour, or a run with `--color never`, still shows what is meant.
        if let Some((lead, width)) = line.ruler() {
            let caret: String = " "
                .repeat(lead)
                .chars()
                .chain(std::iter::repeat_n('^', width))
                .collect();
            writeln!(
                out,
                "{:6} {} {}",
                "",
                paint(dim, "\u{2502}"),
                paint(palette.style(ink), caret)
            )?;
        }
    }
    Ok(())
}

/// The finding says what is wrong; this says what it costs. A smell in a file
/// nothing imports and nobody edits is genuinely lower priority than the same smell
/// in a hub, and saying so is the difference between a list and a plan.
fn impact_line(outcome: &ScanOutcome, file: &str) -> Option<String> {
    let impact = outcome.impact.get(file);
    if !impact.is_known() {
        return None;
    }
    let mut parts = Vec::new();
    if impact.commits > 0 {
        parts.push(format!(
            "changed {} in the last 90 days",
            plural(impact.commits, "time", "times")
        ));
    }
    if impact.importers > 0 {
        parts.push(format!(
            "{} {} it",
            plural(impact.importers, "module", "modules"),
            if impact.importers == 1 {
                "imports"
            } else {
                "import"
            }
        ));
    }
    Some(parts.join(", "))
}

struct Row {
    label: &'static str,
    value: String,
    ink: Option<Ink>,
}

/// Built as plain strings so the summary can be asserted on without a terminal;
/// `render` is the only thing that knows how to paint them.
fn summary_rows(report: &Report<'_>, with_findings: usize) -> Vec<Row> {
    let scanned = report.files.len();
    let severity = |level: Severity| report.totals.by_severity.get(&level).copied().unwrap_or(0);
    let health = report.health();
    let hidden = report
        .totals
        .findings
        .saturating_sub(report.findings.len() as u32);

    let mut findings = format!(
        "{} errors \u{b7} {} warnings \u{b7} {} info",
        severity(Severity::Error),
        severity(Severity::Warning),
        severity(Severity::Info)
    );
    if hidden > 0 {
        findings.push_str(&format!(" \u{b7} {hidden} not shown"));
    }

    let mut rows = vec![
        Row {
            label: "Files",
            value: format!(
                "{with_findings} with findings \u{b7} {} clean \u{b7} {scanned} scanned",
                scanned.saturating_sub(with_findings)
            ),
            ink: None,
        },
        Row {
            label: "Findings",
            value: findings,
            ink: None,
        },
        Row {
            label: "Grades",
            value: {
                let grades = report.grades();
                Cleanliness::ALL
                    .iter()
                    .rev()
                    .filter_map(|grade| {
                        let count = grades.get(grade).copied().unwrap_or(0);
                        (count > 0).then(|| format!("{count} {}", grade.label()))
                    })
                    .collect::<Vec<_>>()
                    .join(" \u{b7} ")
            },
            ink: None,
        },
        Row {
            label: "Rules",
            value: format!(
                "{} of {} fired",
                report.totals.rules_fired,
                RuleId::ALL.len()
            ),
            ink: None,
        },
        Row {
            label: "Health",
            // The score is always the whole project's: a review that scored only the
            // files it looked at would improve by touching fewer of them.
            value: match report.base {
                Some(_) => format!("{health}/100 (whole project)"),
                None => format!("{health}/100"),
            },
            ink: Some(score_ink(health)),
        },
        Row {
            label: "Duration",
            value: format!("{:.2}s", report.outcome.elapsed_ms as f64 / 1000.0),
            ink: None,
        },
    ];
    // Only when there are any: a project that waives nothing should not have to read
    // a row saying so.
    if let Some(verdict) = report.verdict
        && report.outcome.baseline_size.is_some()
    {
        let at = rows.len() - 2;
        rows.insert(
            at,
            Row {
                label: "Baseline",
                value: format!(
                    "{} known \u{b7} {} new",
                    plural(verdict.known, "finding", "findings"),
                    verdict.counted
                ),
                ink: Some(if verdict.counted == 0 {
                    Ink::Good
                } else {
                    Ink::Warn
                }),
            },
        );
    }
    let waived = report.outcome.suppressed.len();
    if waived > 0 {
        let at = rows.len() - 2;
        rows.insert(
            at,
            Row {
                label: "Waived",
                value: format!(
                    "{waived} suppressed by a comment \u{b7} rqx suppressions to read them"
                ),
                ink: Some(Ink::Muted),
            },
        );
    }
    rows
}

fn write_summary(
    out: &mut dyn Write,
    layout: &Layout,
    report: &Report<'_>,
    totals: &std::collections::BTreeMap<&str, u32>,
) -> Result<()> {
    writeln!(out)?;
    for row in summary_rows(report, totals.len()) {
        let value = match row.ink {
            Some(ink) => paint(layout.palette.bold(ink), row.value),
            None => row.value,
        };
        writeln!(
            out,
            "  {}{value}",
            paint(
                layout.palette.style(Ink::Muted),
                format!("{:LABEL_WIDTH$}", row.label)
            )
        )?;
    }

    if !layout.quiet && report.totals.findings > 0 {
        writeln!(out)?;
        for (rule, count) in report.outcome.counts.by_rule.iter().take(MAX_RULE_ROWS) {
            writeln!(
                out,
                "  {count:>5}  {}",
                paint(layout.palette.style(Ink::Muted), rule.slug())
            )?;
        }
        let extra = report
            .outcome
            .counts
            .by_rule
            .len()
            .saturating_sub(MAX_RULE_ROWS);
        if extra > 0 {
            writeln!(
                out,
                "  {}",
                paint(
                    layout.palette.style(Ink::Muted),
                    format!("\u{2026} and {extra} more rules \u{b7} --stats for all of them")
                )
            )?;
        }
    }
    Ok(())
}

/// The rings, as rings. A cycle is already reported as a finding, but it is one
/// finding anchored on whichever member sorted first and buried in that file's
/// group — which is not where anyone looks for "what are the cycles".
fn write_cycles(out: &mut dyn Write, layout: &Layout, report: &Report<'_>) -> Result<()> {
    let cycles = &report.outcome.cycles;
    if cycles.is_empty() || layout.quiet {
        return Ok(());
    }
    writeln!(
        out,
        "\n  {}",
        paint(layout.palette.bold(Ink::Text), "Cycles")
    )?;
    for ring in cycles {
        writeln!(
            out,
            "    {}",
            paint(layout.palette.style(Ink::Danger), ring.join(" \u{2192} "))
        )?;
    }
    Ok(())
}

/// Only when a contract is declared: a project with no `[architecture]` has no
/// layering to break, and a heading over an empty list says the wrong thing.
fn write_architecture(
    out: &mut dyn Write,
    layout: &Layout,
    report: &Report<'_>,
    prefix: &str,
) -> Result<()> {
    if layout.quiet {
        return Ok(());
    }
    let broken: Vec<&Finding> = report
        .findings
        .iter()
        .copied()
        .filter(|finding| {
            matches!(
                finding.rule,
                rqx_core::rule::RuleId::LayerViolation | rqx_core::rule::RuleId::ForbiddenImport
            )
        })
        .collect();
    if broken.is_empty() {
        return Ok(());
    }
    writeln!(
        out,
        "\n  {}",
        paint(layout.palette.bold(Ink::Text), "Architecture")
    )?;
    for finding in broken {
        writeln!(
            out,
            "    {}  {}",
            paint(
                layout.palette.style(Ink::Muted),
                format!(
                    "{prefix}{}:{}",
                    finding.location.file, finding.location.line
                )
            ),
            paint(
                layout.palette.style(Ink::Danger),
                finding.evidence.to_string()
            )
        )?;
    }
    Ok(())
}

fn write_hotspots(
    out: &mut dyn Write,
    layout: &Layout,
    report: &Report<'_>,
    prefix: &str,
) -> Result<()> {
    if report.totals.findings == 0 {
        return Ok(());
    }
    let ranked = report.priority();
    if ranked.is_empty() {
        return Ok(());
    }
    writeln!(
        out,
        "\n  {}",
        paint(layout.palette.bold(Ink::Text), "Start here")
    )?;
    for (file, grade) in ranked.into_iter().take(HOTSPOTS) {
        let impact = report.outcome.impact.get(file);
        writeln!(
            out,
            "    {:10} {}  {}",
            paint(layout.palette.style(grade_ink(grade)), grade.label()),
            format_args!("{prefix}{file}"),
            paint(
                layout.palette.style(Ink::Muted),
                format!(
                    "{}, {}",
                    plural(impact.commits, "commit", "commits"),
                    plural(impact.importers, "importer", "importers")
                )
            )
        )?;
    }
    Ok(())
}

/// What a run assumed, on the run where the assumption matters. A pipeline pointed
/// at the wrong directory would otherwise pass with a perfect score, because nothing
/// was analysed and so nothing was wrong.
/// What a first run has no way to know: that it is reading the defaults, that there
/// is no history behind the churn rules, that nothing here is a language rqx reads.
/// One dim line each, on the run where it is true and never otherwise.
fn write_notes(out: &mut dyn Write, layout: &Layout, outcome: &ScanOutcome) -> Result<()> {
    if layout.quiet {
        return Ok(());
    }
    let dim = layout.palette.style(Ink::Muted);
    if outcome.files_scanned == 0 {
        let languages = rqx_core::Language::ALL
            .map(rqx_core::Language::slug)
            .join(", ");
        writeln!(
            out,
            "\n  {}\n  {}",
            paint(layout.palette.bold(Ink::Warn), "No files to analyse."),
            paint(
                dim,
                format!("rqx reads {languages}. Check the path, `[languages]` and `[ignore]`.")
            )
        )?;
        return Ok(());
    }
    if !outcome.configured {
        writeln!(
            out,
            "\n  {}",
            paint(
                dim,
                "No rqx.yaml, so these are the defaults \u{2014} `rqx init` writes one to edit."
            )
        )?;
    }
    if !outcome.versioned {
        writeln!(
            out,
            "\n  {}",
            paint(
                dim,
                "Not a git repository, so churn, hotspots and authorship stayed quiet."
            )
        )?;
    }
    Ok(())
}

/// What `-v` adds: the inputs the run used and what it cost, which a report about the
/// code does not say and a reader debugging a surprising result needs.
fn write_run(out: &mut dyn Write, layout: &Layout, outcome: &ScanOutcome) -> Result<()> {
    if !layout.verbose {
        return Ok(());
    }
    let dim = layout.palette.style(Ink::Muted);
    let configuration = if outcome.config_files.is_empty() {
        "none, the defaults".to_owned()
    } else {
        outcome
            .config_files
            .iter()
            .map(|file| file.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let stats = &outcome.stats;
    let cache = if stats.cache_hits + stats.cache_misses == 0 {
        "not used".to_owned()
    } else {
        format!("{} hits, {} misses", stats.cache_hits, stats.cache_misses)
    };
    writeln!(out, "\n  {}", paint(layout.palette.bold(Ink::Text), "Run"))?;
    for (label, value) in [
        ("config", configuration),
        ("cache", cache),
        (
            "time",
            format!("{:.2}s", outcome.elapsed_ms as f64 / 1000.0),
        ),
    ] {
        writeln!(out, "    {}  {value}", paint(dim, format!("{label:<6}")))?;
    }
    Ok(())
}

fn write_skipped(out: &mut dyn Write, layout: &Layout, outcome: &ScanOutcome) -> Result<()> {
    if outcome.skipped.is_empty() {
        return Ok(());
    }
    let dim = layout.palette.style(Ink::Muted);
    writeln!(
        out,
        "\n  {} skipped:",
        plural(outcome.skipped.len() as u32, "file", "files")
    )?;
    let shown = if layout.verbose {
        outcome.skipped.len()
    } else {
        MAX_SKIPPED_ROWS
    };
    for (path, reason) in outcome.skipped.iter().take(shown) {
        writeln!(out, "    {} \u{2014} {reason}", paint(dim, path.display()))?;
    }
    if outcome.skipped.len() > shown {
        let extra = outcome.skipped.len() - shown;
        writeln!(
            out,
            "    {}",
            paint(dim, format!("\u{2026} and {extra} more"))
        )?;
    }
    Ok(())
}

fn write_gate(out: &mut dyn Write, layout: &Layout, report: &Report<'_>) -> Result<()> {
    let Some(verdict) = report.verdict else {
        return Ok(());
    };
    if verdict.passed() {
        writeln!(
            out,
            "\n  {} gate passed",
            paint(layout.palette.style(Ink::Good), "\u{2713}")
        )?;
    } else {
        writeln!(
            out,
            "\n  {} gate failed",
            paint(layout.palette.style(Ink::Danger), "\u{2716}")
        )?;
        for failure in &verdict.failures {
            writeln!(out, "    {failure}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::brand::ColorDepth;
    use crate::report::FindingFilter;
    use crate::report::tests::outcome_with;

    use super::*;

    /// Found by label rather than by position: the summary gains rows over time, and
    /// an index makes every one of these tests fail for the wrong reason.
    fn row(rows: &[Row], label: &str) -> String {
        rows.iter()
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("no {label} row"))
            .value
            .clone()
    }

    fn layout() -> Layout {
        Layout {
            detail: Detail::default(),
            show_suppressed: false,
            grouping: Grouping::File,
            order: Order::Line,
            top: None,
            palette: Palette::with_depth(ColorDepth::Monochrome),
            show_passed: false,
            quiet: false,
            stats: false,
            context: 1,
            verbose: false,
        }
    }

    #[test]
    fn the_summary_accounts_for_every_scanned_file() {
        let outcome = outcome_with(&[("a.ts", Severity::Error), ("a.ts", Severity::Warning)], 4);
        let report = Report::scan(&outcome, &FindingFilter::default());
        let rows = summary_rows(&report, report.totals_by_file().len());
        assert_eq!(
            row(&rows, "Files"),
            "1 with findings \u{b7} 3 clean \u{b7} 4 scanned"
        );
        assert_eq!(
            row(&rows, "Findings"),
            "1 errors \u{b7} 1 warnings \u{b7} 0 info"
        );
    }

    #[test]
    fn a_filtered_view_says_how_much_it_is_hiding() {
        let outcome = outcome_with(&[("a.ts", Severity::Error), ("b.ts", Severity::Info)], 2);
        let filter = FindingFilter::new(Some(Severity::Error), &[], &[], &[]).unwrap();
        let report = Report::scan(&outcome, &filter);
        let rows = summary_rows(&report, report.totals_by_file().len());
        let findings = row(&rows, "Findings");
        assert!(findings.ends_with("1 not shown"), "{findings}");
    }

    #[test]
    fn a_clean_project_still_reports_its_files() {
        let outcome = outcome_with(&[], 12);
        let report = Report::scan(&outcome, &FindingFilter::default());
        let rows = summary_rows(&report, 0);
        assert_eq!(
            row(&rows, "Files"),
            "0 with findings \u{b7} 12 clean \u{b7} 12 scanned"
        );
        assert_eq!(row(&rows, "Health"), "100/100");
        assert_eq!(row(&rows, "Grades"), "12 pristine");
    }

    fn failing_gate() -> crate::gate::Verdict {
        crate::gate::Verdict {
            failures: vec!["1 error found".to_owned()],
            counted: 1,
            known: 0,
        }
    }

    #[test]
    fn every_arrangement_ends_with_the_summary_and_the_gate() {
        let outcome = outcome_with(&[("a.ts", Severity::Error), ("b.ts", Severity::Warning)], 3);
        let verdict = failing_gate();
        let report = Report::scan(&outcome, &FindingFilter::default()).judged(&verdict);

        let arrangements = [
            Layout {
                grouping: Grouping::File,
                ..layout()
            },
            Layout {
                grouping: Grouping::Rule,
                ..layout()
            },
            Layout {
                grouping: Grouping::Family,
                ..layout()
            },
            Layout {
                grouping: Grouping::Severity,
                ..layout()
            },
            Layout {
                stats: true,
                ..layout()
            },
        ];
        for arrangement in arrangements {
            let mut out = Vec::new();
            render(&report, &arrangement, &mut out).unwrap();
            let text = String::from_utf8(out).unwrap();
            let summary = text.find("Health").expect("no summary");
            let gate = text.find("gate failed").expect("no gate verdict");
            assert!(summary < gate, "{text}");
            assert!(text.contains("1 error found"), "{text}");
        }
    }

    #[test]
    fn passing_files_are_listed_only_when_asked_for() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 2);
        let report = Report::scan(&outcome, &FindingFilter::default());

        let mut quiet = Vec::new();
        render(&report, &layout(), &mut quiet).unwrap();
        let quiet = String::from_utf8(quiet).unwrap();
        assert!(quiet.contains("FAIL"), "{quiet}");
        assert!(!quiet.contains("PASS"), "{quiet}");

        let mut loud = Vec::new();
        render(
            &report,
            &Layout {
                show_passed: true,
                ..layout()
            },
            &mut loud,
        )
        .unwrap();
        assert!(String::from_utf8(loud).unwrap().contains("PASS"));
    }
    fn rendered_text(outcome: &ScanOutcome, layout: &Layout) -> String {
        let report = Report::scan(outcome, &FindingFilter::default());
        let mut out = Vec::new();
        render(&report, layout, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn the_run_footer_appears_only_when_asked_for() {
        let mut outcome = outcome_with(&[("a.ts", Severity::Error)], 2);
        outcome.config_files = vec!["rqx.yaml".into(), "policy.yaml".into()];
        outcome.stats.cache_hits = 7;
        outcome.stats.cache_misses = 2;

        let quiet = rendered_text(&outcome, &layout());
        assert!(!quiet.contains("config"), "{quiet}");

        let loud = rendered_text(
            &outcome,
            &Layout {
                verbose: true,
                ..layout()
            },
        );
        assert!(loud.contains("rqx.yaml, policy.yaml"), "{loud}");
        assert!(loud.contains("7 hits, 2 misses"), "{loud}");
        assert!(loud.contains("0.84s"), "{loud}");
    }

    #[test]
    fn a_run_with_no_configuration_says_it_used_the_defaults() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 2);

        let loud = rendered_text(
            &outcome,
            &Layout {
                verbose: true,
                ..layout()
            },
        );

        assert!(loud.contains("none, the defaults"), "{loud}");
        assert!(loud.contains("not used"), "{loud}");
    }

    #[test]
    fn every_skipped_file_is_listed_when_verbose() {
        let mut outcome = outcome_with(&[("a.ts", Severity::Error)], 2);
        outcome.skipped = (0..MAX_SKIPPED_ROWS + 5)
            .map(|n| (format!("big-{n}.ts").into(), "too large".to_owned()))
            .collect();

        let capped = rendered_text(&outcome, &layout());
        assert!(capped.contains("and 5 more"), "{capped}");
        assert!(!capped.contains("big-14.ts"), "{capped}");

        let full = rendered_text(
            &outcome,
            &Layout {
                verbose: true,
                ..layout()
            },
        );
        assert!(!full.contains("more"), "{full}");
        assert!(full.contains("big-14.ts"), "{full}");
    }
}

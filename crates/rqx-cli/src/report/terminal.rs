use std::io::Write;

use anyhow::Result;
use rqx_core::Cleanliness;
use rqx_core::LineIndex;
use rqx_core::config::RegressionGate;
use rqx_core::finding::{Finding, Severity};
use rqx_core::rule::RuleId;

use crate::brand::{Ink, Palette, grade_ink, paint, score_ink, severity_ink};
use crate::preview;
use crate::regressions::{Regressions, State, named};
use crate::report::tally::{RuleCount, Tally};
use crate::report::{Links, Report, plural, wrap};
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
    /// A finding in a file with several owners is listed under each of them.
    Owner,
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
    /// The whole reason once per rule, a one-line reason on every finding, and the cost
    /// once per file.
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
    /// Columns of the terminal the report is being written to, when a person is reading
    /// it there. Prose is wrapped to it; a file or a pipe has none, and its lines run
    /// as long as they always have.
    pub width: Option<usize>,
    /// Present only when the terminal this is written to opens OSC 8 links, so a file,
    /// a pipe and a terminal that would print the escape as text never see one.
    pub links: Option<Links>,
}

impl Layout {
    /// Past this a line is harder to read, however wide the window is.
    const MOST_PROSE_COLUMNS: usize = 100;
    /// Narrower than this a hanging indent leaves no room for words, and a ragged
    /// column of one word per line reads worse than a line the terminal wraps itself.
    const LEAST_PROSE_ROOM: usize = 20;

    /// What is left of a line for prose after `hang` columns of indent and label.
    fn prose_room(&self, hang: usize) -> Option<usize> {
        let columns = self.width?.min(Self::MOST_PROSE_COLUMNS);
        Some(columns.saturating_sub(hang)).filter(|room| *room >= Self::LEAST_PROSE_ROOM)
    }
}

/// `text`, as a link to the file at the line and column when the terminal opens them.
fn linked(
    layout: &Layout,
    outcome: &ScanOutcome,
    at: (&str, u32, u32),
    text: impl std::fmt::Display,
) -> String {
    let (file, line, column) = at;
    match &layout.links {
        Some(links) => links.wrap(text, &outcome.root.join(file), line, column),
        None => text.to_string(),
    }
}

/// A label and then prose, wrapped to the terminal with the continuation lines hung
/// under the first word rather than under the label. `ink` colours each line on its
/// own, so a wrapped line never leaves its colour open across the indent.
fn write_prose(
    out: &mut dyn Write,
    layout: &Layout,
    indent: usize,
    label: &str,
    text: &str,
    ink: Option<Ink>,
) -> Result<()> {
    let lead = if label.is_empty() {
        String::new()
    } else {
        format!("{} ", paint(layout.palette.style(Ink::Muted), label))
    };
    let hang = indent + wrap::visible_width(label) + usize::from(!label.is_empty());
    let lines = match layout.prose_room(hang) {
        Some(room) => wrap::lines(text, room),
        None => vec![text.to_owned()],
    };
    for (position, line) in lines.into_iter().enumerate() {
        let line = match ink {
            Some(ink) => paint(layout.palette.style(ink), line),
            None => line,
        };
        if position == 0 {
            writeln!(out, "{:indent$}{lead}{line}", "")?;
        } else {
            writeln!(out, "{:hang$}{line}", "")?;
        }
    }
    Ok(())
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
    write_owners(out, layout, report)?;
    write_hotspots(out, layout, report, &prefix)?;
    write_skipped(out, layout, report.outcome)?;
    write_notes(out, layout, report.outcome)?;
    write_gate(out, layout, report)?;
    write_run(out, layout, report.outcome)
}

/// The headings one finding is listed under: one, except for a file with several owners.
fn group_keys(report: &Report<'_>, grouping: Grouping, finding: &Finding) -> Vec<String> {
    match grouping {
        Grouping::Rule => vec![finding.slug().to_owned()],
        Grouping::Family => vec![finding.category().slug().to_owned()],
        Grouping::Severity => vec![finding.severity.label().to_owned()],
        Grouping::File => vec![finding.location.file.to_string()],
        Grouping::Owner => {
            let owners = report.owners_of(&finding.location.file);
            if owners.is_empty() {
                vec![crate::ownership::UNOWNED.to_owned()]
            } else {
                owners.into_iter().map(str::to_owned).collect()
            }
        }
    }
}

/// The same findings under a heading that is not a file. The file listing shows a
/// grade and an excerpt per file; these headings have neither, so they are a count
/// and the findings under it.
fn grouped(report: &Report<'_>, layout: &Layout, out: &mut dyn Write) -> Result<()> {
    write_header(out, layout, report)?;
    if layout.grouping == Grouping::Owner && report.outcome.owners.is_none() {
        writeln!(
            out,
            "\n  {}",
            paint(
                layout.palette.style(Ink::Muted),
                "no CODEOWNERS file found, so every finding is unowned"
            )
        )?;
    }
    let mut listing = Listing::new(layout, report);
    let mut groups: std::collections::BTreeMap<String, Vec<&Finding>> =
        std::collections::BTreeMap::new();
    for finding in &report.findings {
        for key in group_keys(report, layout.grouping, finding) {
            groups.entry(key).or_default().push(finding);
        }
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
                    linked(
                        layout,
                        report.outcome,
                        (
                            &finding.location.file,
                            finding.location.line,
                            finding.location.column
                        ),
                        format_args!(
                            "{prefix}{}:{}",
                            finding.location.file, finding.location.line
                        )
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
        linked(
            layout,
            report.outcome,
            (file, 1, 1),
            format_args!("{prefix}{file}")
        ),
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
        write_prose(out, layout, 8, "Cost:", &cost, Some(Ink::Warn))?;
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

    let at = (
        &*finding.location.file,
        finding.location.line,
        finding.location.column,
    );
    let place = || {
        linked(
            layout,
            listing.report.outcome,
            at,
            format!("{}:{}", finding.location.line, finding.location.column),
        )
    };

    if layout.detail == Detail::Terse {
        return writeln!(
            out,
            "   {} {}  {}  {}",
            paint(palette.style(ink), finding.severity.label()),
            paint(palette.style(Ink::Muted), place()),
            paint(palette.style(Ink::Accent), finding.slug()),
            finding.message()
        )
        .map_err(Into::into);
    }

    writeln!(
        out,
        "\n   {} {}  {}  {}",
        paint(palette.style(ink), "\u{25cf}"),
        paint(palette.style(Ink::Accent), finding.slug()),
        paint(palette.style(Ink::Muted), place()),
        paint(palette.style(ink), finding.severity.label())
    )?;

    if let Some(index) = index {
        write_excerpt(out, layout, finding, index)?;
    }

    write_prose(out, layout, 5, "", &finding.message(), None)?;
    // A rule the project wrote has already said why in its own message; the
    // catalogue's rationale belongs to a rule rqx ships. The whole rationale is printed
    // once per rule; every other finding of it gets the short form, so none is left
    // without a reason.
    if !matches!(finding.evidence, rqx_core::finding::Evidence::Custom { .. }) {
        let first = listing.said.insert(spec.slug) || layout.detail == Detail::Full;
        let why = if first { spec.why } else { spec.short_why() };
        write_prose(out, layout, 5, "Why: ", why, None)?;
    }
    if let Some(cost) = cost {
        write_prose(out, layout, 5, "Cost:", cost, Some(Ink::Warn))?;
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
        write_prose(out, layout, 5, "Fix: ", &named, None)?;
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
    let Some(view) = preview::of_finding(
        index,
        &finding.location,
        finding.focus.as_ref(),
        layout.context,
    ) else {
        return Ok(());
    };
    let palette = layout.palette;
    let dim = palette.style(Ink::Muted);
    let ink = severity_ink(finding.severity);

    for line in &view.lines {
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
        if line.gap_after > 0 {
            writeln!(
                out,
                "{:6} {} {}",
                "",
                paint(dim, "\u{22ee}"),
                paint(dim, more_lines(line.gap_after))
            )?;
        }
    }
    Ok(())
}

fn more_lines(count: u32) -> String {
    format!("{count} more {}", if count == 1 { "line" } else { "lines" })
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

pub(super) struct Row {
    pub(super) label: &'static str,
    pub(super) value: String,
    pub(super) ink: Option<Ink>,
}

/// Built as plain strings so the summary can be asserted on without a terminal;
/// `render` is the only thing that knows how to paint them.
pub(super) fn summary_rows(report: &Report<'_>, with_findings: usize) -> Vec<Row> {
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
    if let Some(credit) = report.credit.filter(|credit| !credit.resolved.is_empty()) {
        let at = rows.len() - 2;
        rows.insert(
            at,
            Row {
                label: "Resolved",
                value: format!(
                    "{} fixed since the base{}",
                    plural(credit.resolved.len() as u32, "finding", "findings"),
                    credit.health.map_or_else(String::new, |health| format!(
                        " \u{b7} health {} to {} ({:+})",
                        health.before,
                        health.after,
                        i64::from(health.after) - i64::from(health.before)
                    ))
                ),
                ink: Some(Ink::Good),
            },
        );
    }
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
    if let Some(regressions) = report.regressions {
        let at = rows.len() - 2;
        rows.insert(at, regression_row(regressions));
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

/// What the review found out about functions that cost more than at the base: the first
/// few by name, or why nothing was compared.
fn regression_row(regressions: &Regressions) -> Row {
    let (value, ink) = match &regressions.state {
        State::Skipped(reason) => (format!("not compared: {reason}"), Ink::Muted),
        State::Found(found) if found.is_empty() => (
            "no function costs more than at the base".to_owned(),
            Ink::Good,
        ),
        State::Found(found) => {
            let described: Vec<String> = found.iter().map(ToString::to_string).collect();
            let ink = match regressions.gate {
                RegressionGate::Error => Ink::Danger,
                RegressionGate::Warning | RegressionGate::Off => Ink::Warn,
            };
            (
                format!(
                    "{} \u{b7} {}",
                    plural(
                        found.len() as u32,
                        "function got slower",
                        "functions got slower"
                    ),
                    named(&described)
                ),
                ink,
            )
        }
    };
    Row {
        label: "Complexity",
        value,
        ink: Some(ink),
    }
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

/// Only when the project has a CODEOWNERS file: without one there is no one to divide
/// the health among, and a heading over nothing would say the wrong thing.
fn write_owners(out: &mut dyn Write, layout: &Layout, report: &Report<'_>) -> Result<()> {
    if report.outcome.owners.is_none() {
        return Ok(());
    }
    super::owners(&report.owner_rows(), layout.palette, out)
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
            paint(layout.palette.style(Ink::Danger), finding.message())
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
                format!("rqx reads {languages}. Check the path, `languages` and `ignore`.")
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

/// One line that counts the skipped files by reason; `-v` lists every one under it.
fn write_skipped(out: &mut dyn Write, layout: &Layout, outcome: &ScanOutcome) -> Result<()> {
    let Some(summary) = crate::scan::skip::summary(&outcome.skipped, &outcome.root) else {
        return Ok(());
    };
    let dim = layout.palette.style(Ink::Muted);
    if !layout.verbose {
        writeln!(out, "\n  {summary} {}", paint(dim, "\u{b7} -v lists them"))?;
        return Ok(());
    }
    writeln!(out, "\n  {summary}")?;
    let settings = crate::scan::skip::settings_to_include(&outcome.skipped);
    if !settings.is_empty() {
        writeln!(
            out,
            "  {}",
            paint(dim, format!("to include them: {}", settings.join(", ")))
        )?;
    }
    for skipped in &outcome.skipped {
        writeln!(
            out,
            "    {} \u{2014} {}",
            paint(dim, crate::scan::relative_to(&outcome.root, &skipped.path)),
            skipped.detail
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
            width: None,
            links: None,
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

    fn regression(qual: &str, line: u32) -> crate::regressions::Regression {
        crate::regressions::Regression {
            file: "src/x.ts".to_owned(),
            qual: qual.to_owned(),
            line,
            before: "O(n)".parse().unwrap(),
            after: "O(n^2)".parse().unwrap(),
            trace: Vec::new(),
        }
    }

    fn rows_with(regressions: Option<&Regressions>) -> Vec<Row> {
        let outcome = outcome_with(&[], 2);
        let mut report = Report::scan(&outcome, &FindingFilter::default());
        report.regressions = regressions;
        summary_rows(&report, 0)
    }

    #[test]
    fn a_report_that_was_not_asked_about_regressions_has_no_row_for_them() {
        let rows = rows_with(None);

        assert!(rows.iter().all(|row| row.label != "Complexity"));
    }

    #[test]
    fn the_summary_names_the_functions_that_got_slower() {
        let found = Regressions {
            gate: RegressionGate::Warning,
            state: State::Found(vec![
                regression("reconcileBatch", 90),
                regression("merge", 4),
            ]),
        };

        let rows = rows_with(Some(&found));

        assert_eq!(
            row(&rows, "Complexity"),
            "2 functions got slower \u{b7} `reconcileBatch` O(n) \u{2192} O(n\u{b2}) \
             (src/x.ts:90), `merge` O(n) \u{2192} O(n\u{b2}) (src/x.ts:4)"
        );
        let labels: Vec<&str> = rows.iter().map(|row| row.label).collect();
        assert_eq!(labels[labels.len() - 2..], ["Health", "Duration"]);
    }

    #[test]
    fn the_summary_says_when_nothing_got_slower_or_nothing_was_compared() {
        let none = Regressions {
            gate: RegressionGate::Error,
            state: State::Found(Vec::new()),
        };
        let skipped = Regressions::without_base(RegressionGate::Off);

        assert_eq!(
            row(&rows_with(Some(&none)), "Complexity"),
            "no function costs more than at the base"
        );
        assert_eq!(
            row(&rows_with(Some(&skipped)), "Complexity"),
            "not compared: there is no base revision to compare with"
        );
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
    fn skipped_files_are_one_line_and_every_one_is_listed_when_verbose() {
        let mut outcome = outcome_with(&[("a.ts", Severity::Error)], 2);
        outcome.root = std::path::PathBuf::from("project");
        outcome.skipped = (0..15)
            .map(|n| {
                crate::scan::Skipped::file(
                    &outcome.root.join(format!("src/big-{n}.ts")),
                    crate::scan::SkipReason::TooLarge,
                    "2048 KB exceeds the 1024 KB limit".to_owned(),
                )
            })
            .collect();

        let quiet = rendered_text(&outcome, &layout());
        assert!(
            quiet.contains("skipped 15 files: 15 too large \u{b7} -v lists them"),
            "{quiet}"
        );
        assert!(!quiet.contains("big-0.ts"), "{quiet}");

        let full = rendered_text(
            &outcome,
            &Layout {
                verbose: true,
                ..layout()
            },
        );
        assert!(!full.contains("-v lists them"), "{full}");
        assert!(
            full.contains("to include them: a larger scan.max_filesize"),
            "{full}"
        );
        assert!(
            full.contains("src/big-14.ts \u{2014} 2048 KB exceeds the 1024 KB limit"),
            "{full}"
        );
        assert!(!full.contains("project/src"), "{full}");
    }

    #[test]
    fn every_finding_gets_a_reason_and_the_whole_rationale_is_printed_once() {
        let outcome = outcome_with(
            &[
                ("a.ts", Severity::Error),
                ("a.ts", Severity::Error),
                ("a.ts", Severity::Error),
            ],
            2,
        );

        let text = rendered_text(&outcome, &layout());

        let spec = RuleId::LongFunction.spec();
        assert_eq!(text.matches("Why:  ").count(), 3, "{text}");
        assert_eq!(text.matches(spec.why).count(), 1, "{text}");
        assert_eq!(
            text.matches(&format!("Why:  {}\n", spec.short_why()))
                .count(),
            2,
            "{text}"
        );
    }

    #[test]
    fn prose_wraps_to_the_terminal_with_a_hanging_indent() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 2);

        let text = rendered_text(
            &outcome,
            &Layout {
                width: Some(50),
                ..layout()
            },
        );

        let why: Vec<&str> = text
            .lines()
            .skip_while(|line| !line.contains("Why:"))
            .take_while(|line| !line.contains("Fix:"))
            .collect();
        assert!(why.len() > 1, "the reason did not wrap:\n{text}");
        for line in text.lines().filter(|line| line.starts_with("     ")) {
            assert!(wrap::visible_width(line) <= 50, "{line:?} is too wide");
        }
        for continuation in &why[1..] {
            assert!(
                continuation.starts_with(&" ".repeat(11)),
                "{continuation:?} does not hang under the text"
            );
        }
    }

    #[test]
    fn prose_is_capped_at_a_readable_width_on_a_wide_terminal() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 2);

        let text = rendered_text(
            &outcome,
            &Layout {
                width: Some(400),
                ..layout()
            },
        );

        for line in text.lines().filter(|line| line.starts_with("     ")) {
            assert!(wrap::visible_width(line) <= 100, "{line:?} is too wide");
        }
    }

    #[test]
    fn without_a_terminal_prose_is_one_line_as_it_always_was() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 2);

        let text = rendered_text(&outcome, &layout());

        let why = RuleId::LongFunction.spec().why;
        assert!(text.contains(&format!("Why:  {why}")), "{text}");
    }

    #[test]
    fn a_terminal_too_narrow_to_hold_words_is_left_to_wrap_itself() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 2);

        let text = rendered_text(
            &outcome,
            &Layout {
                width: Some(16),
                ..layout()
            },
        );

        let why = RuleId::LongFunction.spec().why;
        assert!(text.contains(&format!("Why:  {why}")), "{text}");
    }

    #[test]
    fn a_place_becomes_a_link_only_when_the_layout_carries_links() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 2);

        let plain = rendered_text(&outcome, &layout());
        assert!(!plain.contains("\u{1b}]8"), "{plain:?}");

        let linked = rendered_text(
            &outcome,
            &Layout {
                links: Some(Links::default()),
                ..layout()
            },
        );
        assert!(linked.contains("\u{1b}]8;;file://"), "{linked:?}");
        assert!(linked.contains("a.ts"), "{linked:?}");
    }

    #[test]
    fn every_grouping_links_the_place_of_a_finding() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 2);

        for grouping in [Grouping::File, Grouping::Rule, Grouping::Severity] {
            let text = rendered_text(
                &outcome,
                &Layout {
                    grouping,
                    links: Some(Links::default()),
                    ..layout()
                },
            );
            assert!(text.contains("\u{1b}]8;;file://"), "{text:?}");
        }
    }

    #[test]
    fn every_arrangement_opens_with_the_same_header() {
        let outcome = outcome_with(&[("a.ts", Severity::Error), ("b.ts", Severity::Warning)], 3);
        let first_line = |layout: &Layout| {
            rendered_text(&outcome, layout)
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(str::to_owned)
                .unwrap()
        };

        let by_file = first_line(&layout());

        assert!(by_file.contains("3 files"), "{by_file}");
        for grouping in [
            Grouping::Rule,
            Grouping::Family,
            Grouping::Severity,
            Grouping::Owner,
        ] {
            let grouped = first_line(&Layout {
                grouping,
                ..layout()
            });
            assert_eq!(grouped, by_file);
        }
    }

    fn owned(codeowners: &str, findings: &[(&str, Severity)]) -> ScanOutcome {
        let mut outcome = outcome_with(findings, 3);
        outcome.owners = Some(rqx_core::ownership::CodeOwners::parse(
            codeowners,
            rqx_core::ownership::Dialect::GitHub,
        ));
        outcome
    }

    #[test]
    fn grouping_by_owner_lists_a_finding_under_each_owner_of_its_file() {
        let outcome = owned(
            "web/ @web @lead\n",
            &[
                ("web/a.ts", Severity::Error),
                ("tools/t.ts", Severity::Info),
            ],
        );
        let text = rendered_text(
            &outcome,
            &Layout {
                grouping: Grouping::Owner,
                ..layout()
            },
        );
        for heading in [
            "@web  1 finding",
            "@lead  1 finding",
            "(unowned)  1 finding",
        ] {
            assert!(text.contains(heading), "{heading} in {text}");
        }
    }

    #[test]
    fn grouping_by_owner_without_a_codeowners_file_says_so() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 2);
        let text = rendered_text(
            &outcome,
            &Layout {
                grouping: Grouping::Owner,
                ..layout()
            },
        );
        assert!(text.contains("no CODEOWNERS file found"), "{text}");
        assert!(text.contains("(unowned)"), "{text}");
    }

    #[test]
    fn the_summary_lists_owners_only_when_a_codeowners_file_exists() {
        let findings = [("web/a.ts", Severity::Error)];
        let with = rendered_text(&owned("web/ @web\n", &findings), &layout());
        assert!(with.contains("Owners") && with.contains("@web"), "{with}");

        let without = rendered_text(&outcome_with(&findings, 3), &layout());
        assert!(!without.contains("Owners"), "{without}");
    }

    #[test]
    fn a_single_line_offence_is_underlined_beneath_its_code() {
        let root = std::env::temp_dir().join(format!("rqx-ruler-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.ts"), "const total = 1;\n").unwrap();
        let mut outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        outcome.root = root.clone();
        let location = &mut outcome.findings[0].location;
        location.line = 1;
        location.column = 7;
        location.end_line = 1;
        location.start = 6;
        location.end = 11;

        let text = rendered_text(&outcome, &layout());
        let _ = std::fs::remove_dir_all(&root);

        let code = text
            .lines()
            .position(|line| line.contains("const total = 1;"))
            .expect("the code frame");
        let ruler = text.lines().nth(code + 1).unwrap();
        assert!(ruler.trim_end().ends_with("      ^^^^^"), "{ruler:?}");
    }
}

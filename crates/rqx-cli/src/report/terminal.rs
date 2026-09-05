use std::io::Write;

use anyhow::Result;
use rqx_core::Cleanliness;
use rqx_core::LineIndex;
use rqx_core::finding::{Finding, Severity};
use rqx_core::rule::RuleId;

use crate::brand::{Ink, Palette, grade_ink, paint, score_ink, severity_ink};
use crate::preview::{self, HEAD_LINES};
use crate::report::{Report, plural};
use crate::scan::ScanOutcome;

/// How much of the report a person asked for. Only the console report reads it;
/// a machine format has no notion of a line it may skip.
pub struct Layout {
    pub palette: Palette,
    pub show_passed: bool,
    pub quiet: bool,
    /// Counts only: which rules and families fired, and how often. For deciding
    /// where to look before reading a single finding.
    pub stats: bool,
    /// Lines of unmarked code shown either side of the offence.
    pub context: u32,
}

/// The report with the findings taken out: what fired, where, and how often. The
/// answer to "which of these forty rules is my actual problem" is a count, and
/// reading five hundred findings to arrive at it is the slow way.
fn stats(report: &Report<'_>, layout: &Layout, out: &mut dyn Write) -> Result<()> {
    use std::collections::BTreeMap;
    let palette = layout.palette;
    let dim = palette.style(Ink::Muted);

    let mut by_family: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut by_rule: BTreeMap<String, (u32, Severity)> = BTreeMap::new();
    for finding in &report.findings {
        *by_family.entry(finding.category().slug()).or_default() += 1;
        let entry = by_rule
            .entry(finding.slug().to_owned())
            .or_insert((0, finding.severity));
        entry.0 += 1;
    }

    let mut families: Vec<(&str, u32)> = by_family.into_iter().collect();
    families.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let mut rules: Vec<(String, (u32, Severity))> = by_rule.into_iter().collect();
    rules.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));

    let total = report.findings.len().max(1) as u32;
    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.style(Ink::Text).bold(), "By family"),
        paint(dim, plural(total, "finding", "findings"))
    )?;
    for (family, count) in &families {
        writeln!(
            out,
            "    {:<14} {:>6}  {}",
            family,
            count,
            paint(dim, bar(*count, total))
        )?;
    }

    writeln!(
        out,
        "\n  {}",
        paint(palette.style(Ink::Text).bold(), "By rule")
    )?;
    let width = rules
        .iter()
        .map(|(rule, _)| rule.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 34);
    for (rule, (count, severity)) in &rules {
        writeln!(
            out,
            "    {:<width$} {:>6}  {}",
            paint(palette.style(severity_ink(*severity)), rule),
            count,
            paint(dim, bar(*count, total))
        )?;
    }
    writeln!(out)?;
    write_summary(out, layout, report, &report.totals_by_file())
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
    let shown = report.shown_by_file();
    let totals = report.totals_by_file();
    let prefix = report.path_prefix();

    if let Some(base) = report.base {
        writeln!(
            out,
            "\n  {} against {}",
            paint(layout.palette.style(Ink::Text).bold(), "Reviewing changes"),
            paint(layout.palette.style(Ink::Accent), base)
        )?;
    }

    for (file, findings) in &shown {
        write_file(
            out,
            layout,
            report,
            &prefix,
            file,
            findings,
            totals.get(file).copied(),
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

    write_summary(out, layout, report, &totals)?;
    write_hotspots(out, layout, report, &prefix)?;
    write_skipped(out, layout, report.outcome)?;
    write_gate(out, layout, report)
}

fn write_file(
    out: &mut dyn Write,
    layout: &Layout,
    report: &Report<'_>,
    prefix: &str,
    file: &str,
    findings: &[&Finding],
    total: Option<u32>,
) -> Result<()> {
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
    let cost = impact_line(report.outcome, file);
    for finding in findings {
        write_finding(out, layout, finding, index.as_ref(), cost.as_deref())?;
    }
    Ok(())
}

fn write_finding(
    out: &mut dyn Write,
    layout: &Layout,
    finding: &Finding,
    index: Option<&LineIndex<'_>>,
    cost: Option<&str>,
) -> Result<()> {
    let palette = layout.palette;
    let spec = finding.rule.spec();
    let ink = severity_ink(finding.severity);
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

    let dim = palette.style(Ink::Muted);
    writeln!(out, "     {}", finding.evidence)?;
    // A rule the project wrote has already said why in its own message; the
    // catalogue's rationale belongs to a rule rqx ships.
    if !matches!(finding.evidence, rqx_core::finding::Evidence::Custom { .. }) {
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
            Some(ink) => paint(layout.palette.style(ink).bold(), row.value),
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
                    format!("\u{2026} and {extra} more rules")
                )
            )?;
        }
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
        paint(layout.palette.style(Ink::Text).bold(), "Start here")
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
    for (path, reason) in outcome.skipped.iter().take(MAX_SKIPPED_ROWS) {
        writeln!(out, "    {} \u{2014} {reason}", paint(dim, path.display()))?;
    }
    if outcome.skipped.len() > MAX_SKIPPED_ROWS {
        let extra = outcome.skipped.len() - MAX_SKIPPED_ROWS;
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
            palette: Palette::with_depth(ColorDepth::Monochrome),
            show_passed: false,
            quiet: false,
            stats: false,
            context: 1,
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
}

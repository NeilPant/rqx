use std::io::Write;

use anyhow::Result;
use rqx_core::finding::{Finding, Severity};
use rqx_core::rule::RuleId;
use rqx_ts::LineIndex;

use crate::brand::{Ink, Palette, paint, score_ink};
use crate::report::{Report, plural};
use crate::scan::ScanOutcome;

/// How much of the report a person asked for. Only the console report reads it;
/// a machine format has no notion of a line it may skip.
pub struct Layout {
    pub palette: Palette,
    pub show_passed: bool,
    pub quiet: bool,
}

const LABEL_WIDTH: usize = 10;
const MAX_RULE_ROWS: usize = 8;
const MAX_SKIPPED_ROWS: usize = 10;
const HOTSPOTS: usize = 3;
/// Ranked before the in-scope filter, so a review still has hot files to choose from.
const HOTSPOT_CANDIDATES: usize = 64;

pub fn render(report: &Report<'_>, layout: &Layout, out: &mut dyn Write) -> Result<()> {
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
        write_file(out, layout, report, &prefix, file, findings, totals.get(file).copied())?;
    }

    if layout.show_passed {
        for file in report.files.iter().filter(|file| !totals.contains_key(file.as_str())) {
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
    let worst = findings.iter().map(|finding| finding.severity).max().unwrap_or(Severity::Info);
    let badge = if worst == Severity::Error { Ink::Danger } else { Ink::Warn };
    let total = total.unwrap_or(findings.len() as u32);
    let tally = if total as usize == findings.len() {
        plural(total, "finding", "findings")
    } else {
        format!("{} of {} findings shown", findings.len(), total)
    };

    writeln!(
        out,
        "\n {}  {}  {}",
        paint(layout.palette.badge(badge), " FAIL "),
        format_args!("{prefix}{file}"),
        paint(layout.palette.style(Ink::Muted), tally)
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

fn severity_ink(severity: Severity) -> Ink {
    match severity {
        Severity::Error => Ink::Danger,
        Severity::Warning => Ink::Warn,
        Severity::Info => Ink::Muted,
    }
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
        paint(palette.style(Ink::Accent), spec.slug),
        paint(palette.style(Ink::Muted), format!("{}:{}", finding.location.line, finding.location.column)),
        paint(palette.style(ink), finding.severity.label())
    )?;

    if let Some(index) = index {
        write_excerpt(out, palette, finding, index)?;
    }

    let dim = palette.style(Ink::Muted);
    writeln!(out, "     {}", finding.evidence)?;
    writeln!(out, "     {} {}", paint(dim, "Why: "), spec.why)?;
    if let Some(cost) = cost {
        writeln!(out, "     {} {}", paint(dim, "Cost:"), paint(palette.style(Ink::Warn), cost))?;
    }
    let techniques = spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>().join(" / ");
    writeln!(
        out,
        "     {} {} \u{2014} {}",
        paint(dim, "Fix: "),
        paint(palette.style(Ink::Good), techniques),
        finding.fix_hint
    )?;

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
    palette: Palette,
    finding: &Finding,
    index: &LineIndex<'_>,
) -> Result<()> {
    let line_number = finding.location.line;
    let text = index.line_text(line_number);
    let column = finding.location.column as usize;
    if text.trim().is_empty() || column == 0 || column > text.chars().count() + 1 {
        return Ok(());
    }

    let (end_line, end_column) = index.locate(finding.location.end);
    let width = if end_line == line_number {
        (end_column.saturating_sub(finding.location.column) as usize).max(1)
    } else {
        text.chars().count() + 1 - column
    };

    let dim = palette.style(Ink::Muted);
    let gutter = format!("{line_number:>6}");
    writeln!(out, "{} {} {}", paint(dim, gutter), paint(dim, "\u{2502}"), text.trim_end())?;
    let leading = text.chars().take(column - 1).map(|c| if c == '\t' { '\t' } else { ' ' });
    let caret: String = leading.chain(std::iter::repeat_n('^', width)).collect();
    writeln!(
        out,
        "{:6} {} {}",
        "",
        paint(dim, "\u{2502}"),
        paint(palette.style(Ink::Warn), caret)
    )?;
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
            if impact.importers == 1 { "imports" } else { "import" }
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
    let hidden = report.totals.findings.saturating_sub(report.findings.len() as u32);

    let mut findings = format!(
        "{} errors \u{b7} {} warnings \u{b7} {} info",
        severity(Severity::Error),
        severity(Severity::Warning),
        severity(Severity::Info)
    );
    if hidden > 0 {
        findings.push_str(&format!(" \u{b7} {hidden} not shown"));
    }

    vec![
        Row {
            label: "Files",
            value: format!(
                "{with_findings} with findings \u{b7} {} clean \u{b7} {scanned} scanned",
                scanned.saturating_sub(with_findings)
            ),
            ink: None,
        },
        Row { label: "Findings", value: findings, ink: None },
        Row {
            label: "Rules",
            value: format!("{} of {} fired", report.totals.rules_fired, RuleId::ALL.len()),
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
    ]
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
            paint(layout.palette.style(Ink::Muted), format!("{:LABEL_WIDTH$}", row.label))
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
        let extra = report.outcome.counts.by_rule.len().saturating_sub(MAX_RULE_ROWS);
        if extra > 0 {
            writeln!(
                out,
                "  {}",
                paint(layout.palette.style(Ink::Muted), format!("\u{2026} and {extra} more rules"))
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
    let with_findings = report.totals_by_file();
    let hotspots: Vec<_> = report
        .outcome
        .impact
        .hotspots(HOTSPOT_CANDIDATES)
        .into_iter()
        .filter(|(file, _)| with_findings.contains_key(file))
        .take(HOTSPOTS)
        .collect();
    if hotspots.is_empty() {
        return Ok(());
    }
    writeln!(out, "\n  {}", paint(layout.palette.style(Ink::Text).bold(), "Start here"))?;
    for (file, impact) in hotspots {
        writeln!(
            out,
            "    {}  {}",
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
        writeln!(out, "    {}", paint(dim, format!("\u{2026} and {extra} more")))?;
    }
    Ok(())
}

fn write_gate(out: &mut dyn Write, layout: &Layout, report: &Report<'_>) -> Result<()> {
    let Some(verdict) = report.verdict else {
        return Ok(());
    };
    if verdict.passed() {
        writeln!(out, "\n  {} gate passed", paint(layout.palette.style(Ink::Good), "\u{2713}"))?;
    } else {
        writeln!(out, "\n  {} gate failed", paint(layout.palette.style(Ink::Danger), "\u{2716}"))?;
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

    fn layout() -> Layout {
        Layout {
            palette: Palette::with_depth(ColorDepth::Monochrome),
            show_passed: false,
            quiet: false,
        }
    }

    #[test]
    fn the_summary_accounts_for_every_scanned_file() {
        let outcome = outcome_with(&[("a.ts", Severity::Error), ("a.ts", Severity::Warning)], 4);
        let report = Report::scan(&outcome, &FindingFilter::default());
        let rows = summary_rows(&report, report.totals_by_file().len());
        assert_eq!(rows[0].value, "1 with findings \u{b7} 3 clean \u{b7} 4 scanned");
        assert_eq!(rows[1].value, "1 errors \u{b7} 1 warnings \u{b7} 0 info");
    }

    #[test]
    fn a_filtered_view_says_how_much_it_is_hiding() {
        let outcome = outcome_with(&[("a.ts", Severity::Error), ("b.ts", Severity::Info)], 2);
        let filter = FindingFilter::new(Some(Severity::Error), &[], &[], &[]).unwrap();
        let report = Report::scan(&outcome, &filter);
        let rows = summary_rows(&report, report.totals_by_file().len());
        assert!(rows[1].value.ends_with("1 not shown"), "{}", rows[1].value);
    }

    #[test]
    fn a_clean_project_still_reports_its_files() {
        let outcome = outcome_with(&[], 12);
        let report = Report::scan(&outcome, &FindingFilter::default());
        let rows = summary_rows(&report, 0);
        assert_eq!(rows[0].value, "0 with findings \u{b7} 12 clean \u{b7} 12 scanned");
        assert_eq!(rows[3].value, "100/100");
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
        render(&report, &Layout { show_passed: true, ..layout() }, &mut loud).unwrap();
        assert!(String::from_utf8(loud).unwrap().contains("PASS"));
    }
}

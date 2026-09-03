use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use rqx_core::finding::{Finding, Severity};
use rqx_core::rule::{RuleId, RuleSpec};
use rqx_ts::LineIndex;
use serde::Serialize;

use crate::check::{Review, Verdict};
use crate::scan::ScanOutcome;

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

pub fn render_terminal(outcome: &ScanOutcome) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let mut by_file: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for finding in &outcome.findings {
        by_file.entry(&finding.location.file).or_default().push(finding);
    }

    for (path, findings) in by_file {
        let source = std::fs::read_to_string(path).ok();
        let index = source.as_deref().map(LineIndex::new);
        let impact = impact_line(outcome, path);
        for finding in findings {
            write_finding(&mut out, finding, index.as_ref(), impact.as_deref())?;
        }
    }

    write_summary(&mut out, outcome)
}

fn severity_marker(severity: Severity) -> String {
    match severity {
        Severity::Error => format!("{} error", "\u{2716}".red()),
        Severity::Warning => format!("{} warning", "\u{26a0}".yellow()),
        Severity::Info => format!("{} info", "\u{2139}".blue()),
    }
}

fn write_finding(
    out: &mut impl Write,
    finding: &Finding,
    index: Option<&LineIndex<'_>>,
    impact: Option<&str>,
) -> Result<()> {
    let spec = finding.rule.spec();
    writeln!(
        out,
        "\n  {}  {}  {}",
        finding.location.to_string().bold(),
        spec.slug.cyan(),
        severity_marker(finding.severity)
    )?;

    if let Some(index) = index {
        write_excerpt(out, finding, index)?;
    }

    writeln!(out, "      {}", finding.evidence)?;
    writeln!(out, "      {} {}", "Why:".dimmed(), spec.why)?;
    if let Some(impact) = impact {
        writeln!(out, "      {} {}", "Cost:".dimmed(), impact.yellow())?;
    }
    let techniques = spec
        .fixes
        .iter()
        .map(|fix| fix.name())
        .collect::<Vec<_>>()
        .join(" / ");
    writeln!(out, "      {} {} \u{2014} {}", "Fix:".dimmed(), techniques.green(), finding.fix_hint)?;

    for site in &finding.related {
        writeln!(out, "      {} {}", "also at".dimmed(), site)?;
    }
    Ok(())
}

/// The file is re-read at render time rather than held from the scan, so a file
/// edited mid-scan can be shorter than the recorded span; a stale excerpt is
/// dropped instead of underlining the wrong code.
fn write_excerpt(out: &mut impl Write, finding: &Finding, index: &LineIndex<'_>) -> Result<()> {
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

    let gutter = format!("{line_number:>6}");
    writeln!(out, "{} {} {}", gutter.dimmed(), "\u{2502}".dimmed(), text.trim_end())?;
    let leading = text.chars().take(column - 1).map(|c| if c == '\t' { '\t' } else { ' ' });
    let caret: String = leading.chain(std::iter::repeat_n('^', width)).collect();
    writeln!(out, "{:6} {} {}", "", "\u{2502}".dimmed(), caret.yellow())?;
    Ok(())
}

fn plural(count: u32, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn write_summary(out: &mut impl Write, outcome: &ScanOutcome) -> Result<()> {
    let score = outcome.counts.weighted_health_score(outcome.code_lines, &outcome.impact);
    let score_text = format!("{score}/100");
    let painted = match score {
        90..=100 => score_text.green().to_string(),
        70..=89 => score_text.yellow().to_string(),
        _ => score_text.red().to_string(),
    };

    writeln!(out, "\n  {}  {}", "Health".bold(), painted)?;
    writeln!(
        out,
        "  {} across {} ({}) in {}ms",
        plural(outcome.counts.total, "finding", "findings"),
        plural(outcome.files_scanned, "file", "files"),
        plural(outcome.code_lines as u32, "code line", "code lines"),
        outcome.elapsed_ms
    )?;

    for (rule, count) in &outcome.counts.by_rule {
        writeln!(out, "    {count:>5}  {}", rule.slug().dimmed())?;
    }

    if outcome.omitted > 0 {
        writeln!(
            out,
            "\n  showing {} of {} findings \u{2014} `--max-findings 0` for all, or `--format json`",
            outcome.findings.len(),
            outcome.counts.total
        )?;
    }

    let hotspots = outcome.impact.hotspots(3);
    if !hotspots.is_empty() && outcome.counts.total > 0 {
        writeln!(out, "\n  {}", "Start here".bold())?;
        for (file, impact) in hotspots {
            writeln!(
                out,
                "    {}  {}",
                file.dimmed(),
                format!(
                    "{}, {}",
                    plural(impact.commits, "commit", "commits"),
                    plural(impact.importers, "importer", "importers")
                )
                .dimmed()
            )?;
        }
    }

    if !outcome.skipped.is_empty() {
        writeln!(out, "\n  {} skipped:", plural(outcome.skipped.len() as u32, "file", "files"))?;
        for (path, reason) in outcome.skipped.iter().take(10) {
            writeln!(out, "    {} \u{2014} {reason}", path.display())?;
        }
        if outcome.skipped.len() > 10 {
            writeln!(out, "    \u{2026} and {} more", outcome.skipped.len() - 10)?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct JsonFinding<'a> {
    #[serde(flatten)]
    finding: &'a Finding,
    why: &'static str,
    fixes: Vec<&'static str>,
    guru_smell: Option<&'static str>,
    clean_code_smell: &'static str,
    metric: &'static str,
}

#[derive(Serialize)]
struct JsonSummary<'a> {
    kind: &'static str,
    health_score: u32,
    files_scanned: u32,
    code_lines: u64,
    elapsed_ms: u128,
    omitted: u32,
    counts: &'a rqx_core::score::FindingCounts,
    skipped: Vec<String>,
}

fn json_finding(finding: &Finding) -> JsonFinding<'_> {
    let spec = finding.rule.spec();
    JsonFinding {
        finding,
        why: spec.why,
        fixes: spec.fixes.iter().map(|fix| fix.name()).collect(),
        guru_smell: spec.guru_smell,
        clean_code_smell: spec.clean_code,
        metric: spec.metric,
    }
}

pub fn render_json(outcome: &ScanOutcome) -> Result<()> {
    let mut out = std::io::stdout().lock();
    for finding in &outcome.findings {
        writeln!(out, "{}", serde_json::to_string(&json_finding(finding))?)?;
    }
    let summary = JsonSummary {
        kind: "summary",
        health_score: outcome
            .counts
            .weighted_health_score(outcome.code_lines, &outcome.impact),
        files_scanned: outcome.files_scanned,
        code_lines: outcome.code_lines,
        elapsed_ms: outcome.elapsed_ms,
        omitted: outcome.omitted,
        counts: &outcome.counts,
        skipped: outcome
            .skipped
            .iter()
            .map(|(path, reason)| format!("{}: {reason}", path.display()))
            .collect(),
    };
    writeln!(out, "{}", serde_json::to_string(&summary)?)?;
    Ok(())
}

pub fn explain(rule: RuleId) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let RuleSpec { slug, guru_smell, clean_code, metric, fixes, confidence, severity, why, category, .. } =
        rule.spec();
    writeln!(out, "\n  {}\n", slug.bold().cyan())?;
    writeln!(out, "  {:18} {}", "Smell".dimmed(), guru_smell.unwrap_or("no direct catalog entry"))?;
    writeln!(out, "  {:18} {}", "Family".dimmed(), category.title())?;
    writeln!(out, "  {:18} {clean_code}", "Clean Code".dimmed())?;
    writeln!(out, "  {:18} {metric}", "Metric".dimmed())?;
    writeln!(
        out,
        "  {:18} {}",
        "Fix".dimmed(),
        fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>().join(" / ")
    )?;
    writeln!(out, "  {:18} {} / {}", "Reported as".dimmed(), severity.label(), confidence.label())?;
    writeln!(out, "\n  {why}\n")?;
    Ok(())
}

pub fn render_review(review: &Review, verdict: &Verdict) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let findings: Vec<&Finding> = review.findings().collect();

    let mut by_file: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for finding in &findings {
        by_file.entry(&finding.location.file).or_default().push(finding);
    }
    for (path, group) in by_file {
        let source = std::fs::read_to_string(path).ok();
        let index = source.as_deref().map(LineIndex::new);
        let impact = impact_line(&review.outcome, path);
        for finding in group {
            write_finding(&mut out, finding, index.as_ref(), impact.as_deref())?;
        }
    }

    writeln!(
        out,
        "\n  {} {} in {} against {}",
        "Reviewed".bold(),
        plural(findings.len() as u32, "finding", "findings"),
        plural(review.reviewed.len() as u32, "changed source file", "changed source files"),
        review.base.cyan()
    )?;
    let skipped = outcome_total(review) - findings.len() as u32;
    if skipped > 0 {
        writeln!(
            out,
            "  {} elsewhere in the project were not reviewed",
            plural(skipped, "finding", "findings").dimmed()
        )?;
    }

    if verdict.passed() {
        writeln!(out, "\n  {} gate passed", "\u{2713}".green())?;
    } else {
        writeln!(out, "\n  {} gate failed", "\u{2716}".red())?;
        for failure in &verdict.failures {
            writeln!(out, "    {failure}")?;
        }
    }
    Ok(())
}

fn outcome_total(review: &Review) -> u32 {
    review.outcome.counts.total
}

pub fn render_review_json(review: &Review, verdict: &Verdict) -> Result<()> {
    let mut out = std::io::stdout().lock();
    for finding in review.findings() {
        writeln!(out, "{}", serde_json::to_string(&json_finding(finding))?)?;
    }
    let summary = serde_json::json!({
        "kind": "review",
        "base": review.base,
        "reviewed_files": review.reviewed.len(),
        "reviewed_findings": review.findings().count(),
        "project_findings": review.outcome.counts.total,
        "counted_against_gate": verdict.counted,
        "passed": verdict.passed(),
        "failures": verdict.failures,
    });
    writeln!(out, "{}", serde_json::to_string(&summary)?)?;
    Ok(())
}

pub fn render_rules(only: Option<&str>) -> Result<()> {
    use rqx_core::rule::Category;

    let mut out = anstream::stdout().lock();
    if let Some(name) = only
        && Category::parse(name).is_none()
    {
        anyhow::bail!(
            "unknown family `{name}`. Known families: {}",
            Category::ALL.map(Category::slug).join(", ")
        );
    }

    for category in Category::ALL {
        if only.is_some_and(|name| name != category.slug()) {
            continue;
        }
        let rules: Vec<_> = category.rules().collect();
        writeln!(out, "\n  {}  {}", category.title().bold(), category.slug().dimmed())?;
        for rule in rules {
            let spec = rule.spec();
            let state = if spec.off_by_default { "  (off by default)" } else { "" };
            writeln!(
                out,
                "    {:26} {}{}",
                spec.slug.cyan(),
                spec.clean_code.dimmed(),
                state.dimmed()
            )?;
        }
    }
    writeln!(out, "\n  {} rules. `rqx explain <rule>` for any of them.\n", RuleId::ALL.len())?;
    Ok(())
}

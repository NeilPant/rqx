use std::io::Write;

use anyhow::Result;
use rqx_core::finding::Severity;

use crate::credit::{Credit, Scope};
use crate::report::{Report, plural};

const TOP_FINDINGS: usize = 10;

/// A pipe would end the cell it appears in, and a rule's evidence can quote code.
fn escape(text: &str) -> String {
    text.replace('|', "\\|")
}

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    let outcome = report.outcome;
    let prefix = report.path_prefix();
    let severity = |level: Severity| report.totals.by_severity.get(&level).copied().unwrap_or(0);

    writeln!(out, "## rqx — health {}/100\n", report.health())?;
    if let Some(base) = report.base {
        writeln!(out, "Reviewed changes against `{}`.\n", escape(base))?;
    }
    writeln!(
        out,
        "{} across {} in {}ms.\n",
        plural(report.totals.findings, "finding", "findings"),
        plural(report.files.len() as u32, "file", "files"),
        outcome.elapsed_ms
    )?;

    writeln!(out, "| Severity | Count |\n| --- | ---: |")?;
    for level in [Severity::Error, Severity::Warning, Severity::Info] {
        writeln!(out, "| {} | {} |", level.label(), severity(level))?;
    }

    if !outcome.counts.by_rule.is_empty() {
        writeln!(out, "\n| Rule | Count |\n| --- | ---: |")?;
        for (rule, count) in &outcome.counts.by_rule {
            writeln!(out, "| `{}` | {count} |", escape(rule.slug()))?;
        }
    }

    let grades = report.grades();
    if grades.values().any(|count| *count > 0) {
        writeln!(out, "\n| Grade | Files |\n| --- | ---: |")?;
        for grade in rqx_core::Cleanliness::ALL.iter().rev() {
            let count = grades.get(grade).copied().unwrap_or(0);
            if count > 0 {
                writeln!(out, "| {} | {count} |", grade.label())?;
            }
        }
    }

    if !report.findings.is_empty() {
        writeln!(out, "\n### Start with these\n")?;
        for finding in report.findings.iter().take(TOP_FINDINGS) {
            writeln!(
                out,
                "- `{}:{}` **{}** — {}",
                escape(&format!("{prefix}{}", finding.location.file)),
                finding.location.line,
                escape(finding.slug()),
                escape(&finding.evidence.to_string())
            )?;
        }
        let extra = report.findings.len().saturating_sub(TOP_FINDINGS);
        if extra > 0 {
            writeln!(out, "\n_… and {extra} more._")?;
        }
    }

    if let Some(credit) = report.credit.filter(|credit| !credit.resolved.is_empty()) {
        write_resolved(credit, &prefix, report.base, out)?;
    }

    if let Some(verdict) = report.verdict {
        writeln!(out, "\n### Gate\n")?;
        if verdict.passed() {
            writeln!(out, "Passed.")?;
        } else {
            writeln!(out, "**Failed.**\n")?;
            for failure in &verdict.failures {
                writeln!(out, "- {}", escape(failure))?;
            }
        }
    }
    Ok(())
}

/// The findings the base revision had that the change removed, and what that did to
/// the score.
fn write_resolved(
    credit: &Credit,
    prefix: &str,
    base: Option<&str>,
    out: &mut dyn Write,
) -> Result<()> {
    writeln!(out, "\n### Resolved\n")?;
    writeln!(
        out,
        "{} present at `{}` and gone now{}.{}\n",
        plural(credit.resolved.len() as u32, "finding", "findings"),
        escape(base.unwrap_or("the base")),
        scope_note(credit),
        health_change(credit)
    )?;
    for resolved in credit.resolved.iter().take(TOP_FINDINGS) {
        writeln!(
            out,
            "- `{}:{}` **{}**",
            escape(&format!("{prefix}{}", resolved.file)),
            resolved.line,
            escape(&resolved.rule)
        )?;
    }
    let extra = credit.resolved.len().saturating_sub(TOP_FINDINGS);
    if extra > 0 {
        writeln!(out, "\n_… and {extra} more._")?;
    }
    Ok(())
}

/// The credit of a change whose base was not scanned covers the files it touched.
fn scope_note(credit: &Credit) -> &'static str {
    match credit.scope {
        Scope::Project => "",
        Scope::ChangedFiles => " in the files it changed",
    }
}

/// A sentence, with its leading space, or nothing when no score was taken at the base.
fn health_change(credit: &Credit) -> String {
    credit.health.map_or_else(String::new, |health| {
        format!(
            " Health went from {} to {} ({:+}).",
            health.before,
            health.after,
            health.after as i32 - health.before as i32
        )
    })
}

/// The same report, shaped for a merge request: a verdict anyone reads at a glance,
/// then the detail folded away. A comment that opens with three tables is a comment
/// that gets collapsed by the reader instead.
pub fn comment(report: &Report<'_>, job: Option<&str>) -> Result<String> {
    let health = report.health();
    let grade = rqx_core::Cleanliness::of(health, report.totals.findings);
    let passed = report.verdict.is_none_or(crate::gate::Verdict::passed);

    let mut out: Vec<u8> = Vec::new();
    writeln!(
        out,
        "### {} rqx — {} · health {health}/100\n",
        if passed { "\u{2705}" } else { "\u{274c}" },
        grade.label()
    )?;

    if let Some(verdict) = report.verdict.filter(|verdict| !verdict.passed()) {
        for failure in &verdict.failures {
            writeln!(out, "> {}", escape(failure))?;
        }
        writeln!(out)?;
    }

    let severity = |level: Severity| report.totals.by_severity.get(&level).copied().unwrap_or(0);
    writeln!(
        out,
        "{} across {} — {} errors, {} warnings, {} info.\n",
        plural(report.totals.findings, "finding", "findings"),
        plural(report.files.len() as u32, "file", "files"),
        severity(Severity::Error),
        severity(Severity::Warning),
        severity(Severity::Info),
    )?;

    if let Some(credit) = report.credit.filter(|credit| !credit.resolved.is_empty()) {
        writeln!(
            out,
            "Resolves {}{}.{}\n",
            plural(credit.resolved.len() as u32, "finding", "findings"),
            scope_note(credit),
            health_change(credit)
        )?;
    }

    // Folded, because a merge request is a conversation and this is one voice in it.
    writeln!(out, "<details><summary>What it found</summary>\n")?;
    render(report, &mut out)?;
    writeln!(out, "\n</details>")?;

    if let Some(job) = job {
        writeln!(out, "\n<sub>[Full report]({job})</sub>")?;
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// One line for a commit status, which has a hard limit and no markdown.
pub fn classification(report: &Report<'_>) -> String {
    let health = report.health();
    let grade = rqx_core::Cleanliness::of(health, report.totals.findings);
    format!(
        "{} \u{b7} health {health}/100 \u{b7} {}",
        grade.label(),
        plural(report.totals.findings, "finding", "findings")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipe_cannot_break_out_of_its_cell() {
        assert_eq!(escape("a|b"), "a\\|b");
    }

    /// The verdict is the first thing in the comment and the detail is folded: a
    /// merge request is a conversation, and this is one voice in it.
    #[test]
    fn the_comment_leads_with_the_verdict_and_folds_the_rest() {
        let outcome = crate::report::tests::outcome_with(&[("a.ts", Severity::Error)], 1);
        let filter = crate::report::FindingFilter::default();
        let report = Report::scan(&outcome, &filter);
        let body = comment(&report, Some("https://ci.example/job/1")).expect("a comment");

        let headline = body.lines().next().unwrap_or_default();
        assert!(headline.starts_with("### "), "{headline}");
        assert!(headline.contains("health"), "{headline}");
        assert!(
            body.contains("<details>"),
            "the detail is not folded:\n{body}"
        );
        assert!(body.contains("https://ci.example/job/1"));
        assert!(
            !classification(&report).contains('\n'),
            "a commit status is one line"
        );
    }
}

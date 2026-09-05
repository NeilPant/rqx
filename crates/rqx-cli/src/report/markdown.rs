use std::io::Write;

use anyhow::Result;
use rqx_core::finding::Severity;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipe_cannot_break_out_of_its_cell() {
        assert_eq!(escape("a|b"), "a\\|b");
    }
}

use std::io::Write;

use anyhow::Result;
use rqx_core::finding::Severity;

use crate::credit::{Credit, Scope};
use crate::regressions::{Regression, State};
use crate::report::{Report, plural};

const TOP_FINDINGS: usize = 10;

/// A pipe would end the cell it appears in, and a rule's evidence can quote code.
fn escape(text: &str) -> String {
    text.replace('|', "\\|")
}

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    write_overview(report, out)?;
    write_regressions(report, out)?;
    write_gate(report, out)
}

fn write_overview(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
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
                escape(&finding.message())
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
    Ok(())
}

fn write_gate(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
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

/// The functions that cost more than at the base, each with the way it got there folded
/// away. Written only for a review that looked, and says so when there was nothing to
/// compare or nothing to report, so a reader can tell a clean change from a check that
/// did not run.
fn write_regressions(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    let Some(regressions) = report.regressions else {
        return Ok(());
    };
    writeln!(out, "\n### Complexity regressions\n")?;
    match &regressions.state {
        State::Skipped(reason) => writeln!(out, "Not compared: {}.", escape(reason))?,
        State::Found(found) if found.is_empty() => writeln!(
            out,
            "No function costs more than at `{}`.",
            escape(report.base.unwrap_or("the base"))
        )?,
        State::Found(found) => write_found(found, &report.path_prefix(), out)?,
    }
    Ok(())
}

fn write_found(found: &[Regression], prefix: &str, out: &mut dyn Write) -> Result<()> {
    for regression in found.iter().take(TOP_FINDINGS) {
        writeln!(
            out,
            "- **`{}`** {} \u{2192} {} \u{b7} `{}`",
            escape(&regression.qual),
            regression.before,
            regression.after,
            escape(&format!("{prefix}{}:{}", regression.file, regression.line))
        )?;
    }
    let extra = found.len().saturating_sub(TOP_FINDINGS);
    if extra > 0 {
        writeln!(out, "\n_\u{2026} and {extra} more._")?;
    }
    writeln!(out, "\n<details><summary>Trace</summary>\n")?;
    for regression in found.iter().take(TOP_FINDINGS) {
        write_trace(regression, prefix, out)?;
    }
    writeln!(out, "</details>")?;
    Ok(())
}

/// How one function reaches its order, from the loop or call that dominates it down to
/// the cost at the bottom.
fn write_trace(regression: &Regression, prefix: &str, out: &mut dyn Write) -> Result<()> {
    writeln!(out, "**`{}`**\n", escape(&regression.qual))?;
    for step in &regression.trace {
        writeln!(
            out,
            "- {} \u{b7} `{}` \u{b7} {}",
            escape(&step.what),
            escape(&format!("{prefix}{}:{}", step.file, step.line)),
            step.order
        )?;
    }
    writeln!(out)?;
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
pub fn comment(report: &Report<'_>, job: Option<&str>, mention_owners: bool) -> Result<String> {
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
    write_owners_to_notify(&mut out, report, mention_owners)?;

    write_regressions(report, &mut out)?;
    writeln!(out)?;

    // Folded, because a merge request is a conversation and this is one voice in it.
    writeln!(out, "<details><summary>What it found</summary>\n")?;
    write_overview(report, &mut out)?;
    write_gate(report, &mut out)?;
    writeln!(out, "\n</details>")?;

    if let Some(job) = job {
        writeln!(out, "\n<sub>[Full report]({job})</sub>")?;
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Who to look at the findings, by the CODEOWNERS file. A name inside a code span is
/// not a mention to GitHub or GitLab, so by default the comment names owners without
/// notifying them; `comment.mention_owners` writes them bare, which does.
fn write_owners_to_notify(
    out: &mut Vec<u8>,
    report: &Report<'_>,
    mention_owners: bool,
) -> Result<()> {
    let owners = report.owners_of_new_findings();
    if owners.is_empty() {
        return Ok(());
    }
    let named: Vec<String> = owners
        .iter()
        .map(|(owner, count)| {
            let name = if mention_owners {
                (*owner).to_owned()
            } else {
                format!("`{owner}`")
            };
            format!("{name} ({})", plural(*count, "finding", "findings"))
        })
        .collect();
    writeln!(
        out,
        "Owners of files with new findings: {}.\n",
        named.join(", ")
    )?;
    Ok(())
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
        let body = comment(&report, Some("https://ci.example/job/1"), false).expect("a comment");

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
        assert!(!body.contains("Owners of files"), "no CODEOWNERS file");
    }

    fn owned_comment(mention_owners: bool) -> String {
        let mut outcome = crate::report::tests::outcome_with(
            &[("web/a.ts", Severity::Error), ("web/b.ts", Severity::Info)],
            3,
        );
        outcome.owners = Some(rqx_core::ownership::CodeOwners::parse(
            "web/ @web\nweb/b.ts @web @lead\n",
            rqx_core::ownership::Dialect::GitHub,
        ));
        outcome.findings[1].known = true;
        let filter = crate::report::FindingFilter::default();
        let report = Report::scan(&outcome, &filter);
        comment(&report, None, mention_owners).expect("a comment")
    }

    #[test]
    fn the_comment_names_owners_in_code_spans_so_the_forge_does_not_notify_them() {
        let body = owned_comment(false);
        assert!(
            body.contains("Owners of files with new findings: `@web` (1 finding)."),
            "{body}"
        );
        assert!(!body.contains("@lead"), "only a known finding: {body}");
        assert!(!body.contains(" @web"), "no bare mention: {body}");
    }

    fn regression(qual: &str, line: u32) -> Regression {
        Regression {
            file: "src/x.ts".to_owned(),
            qual: qual.to_owned(),
            line,
            before: "O(n)".parse().unwrap(),
            after: "O(n^2)".parse().unwrap(),
            trace: vec![
                rqx_core::cost::TraceStep {
                    file: "src/x.ts".to_owned(),
                    line,
                    what: "loop".to_owned(),
                    order: "O(n^2)".parse().unwrap(),
                },
                rqx_core::cost::TraceStep {
                    file: "src/y.ts".to_owned(),
                    line: 12,
                    what: "findMatch".to_owned(),
                    order: "O(n)".parse().unwrap(),
                },
            ],
        }
    }

    fn comment_with(regressions: Option<&crate::regressions::Regressions>) -> String {
        let outcome = crate::report::tests::outcome_with(&[], 1);
        let mut report = Report::scan(&outcome, &crate::report::FindingFilter::default());
        report.regressions = regressions;
        comment(&report, None, false).expect("a comment")
    }

    #[test]
    fn the_comment_lists_the_functions_that_got_slower_and_folds_the_trace() {
        let found = crate::regressions::Regressions {
            gate: rqx_core::config::RegressionGate::Warning,
            state: State::Found(vec![regression("reconcileBatch", 90)]),
        };

        let body = comment_with(Some(&found));

        assert!(body.contains("### Complexity regressions"), "{body}");
        assert!(
            body.contains("- **`reconcileBatch`** O(n) \u{2192} O(n\u{b2}) \u{b7} `src/x.ts:90`"),
            "{body}"
        );
        let trace = body
            .find("<details><summary>Trace</summary>")
            .expect("a trace");
        let step = body
            .find("- findMatch \u{b7} `src/y.ts:12` \u{b7} O(n)")
            .expect("a step");
        let folded = body
            .find("<details><summary>What it found</summary>")
            .expect("a fold");
        assert!(
            trace < step && step < folded,
            "the trace is not folded:\n{body}"
        );
        assert_eq!(body.matches("Complexity regressions").count(), 1, "{body}");
    }

    #[test]
    fn the_comment_says_when_nothing_got_slower_or_nothing_was_compared() {
        let none = crate::regressions::Regressions {
            gate: rqx_core::config::RegressionGate::Error,
            state: State::Found(Vec::new()),
        };
        let skipped =
            crate::regressions::Regressions::without_base(rqx_core::config::RegressionGate::Off);

        assert!(comment_with(Some(&none)).contains("No function costs more than at `the base`."));
        assert!(comment_with(Some(&skipped)).contains("Not compared: there is no base revision"));
    }

    #[test]
    fn a_comment_that_was_not_asked_about_regressions_has_no_section_for_them() {
        assert!(!comment_with(None).contains("Complexity"));
    }

    #[test]
    fn mention_owners_writes_real_mentions() {
        let body = owned_comment(true);
        assert!(
            body.contains("Owners of files with new findings: @web (1 finding)."),
            "{body}"
        );
        assert!(!body.contains("`@web`"), "{body}");
    }
}

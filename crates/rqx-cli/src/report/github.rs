use std::io::Write;

use anyhow::Result;
use rqx_core::finding::{Finding, Severity};

use crate::gate::Verdict;
use crate::report::Report;

const fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "notice",
    }
}

/// A workflow command ends at the first newline, so a multi-line message has to
/// carry its breaks encoded or the annotation is silently truncated.
/// https://docs.github.com/actions/reference/workflow-commands-for-github-actions
fn encode_message(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// A property sits between `::level ` and `::`, separated from the next by a comma and
/// from its value by an equals sign, so a colon or a comma in a path or a title ends
/// the annotation early.
fn encode_property(text: &str) -> String {
    encode_message(text).replace(':', "%3A").replace(',', "%2C")
}

fn annotation(finding: &Finding, prefix: &str) -> String {
    let message = format!(
        "{}\nWhy: {}\nFix: {}",
        finding.message(),
        finding.rule.spec().why,
        finding.fix_hint
    );
    format!(
        "::{} file={},line={},col={},title={}::{}",
        level(finding.severity),
        encode_property(&format!("{prefix}{}", finding.location.file)),
        finding.location.line.max(1),
        finding.location.column.max(1),
        encode_property(finding.slug()),
        encode_message(&message)
    )
}

/// The last line of the run: the annotation a person scrolling the log, or reading
/// the checks tab, sees for whether the gate held.
fn gate_annotation(verdict: &Verdict) -> String {
    if verdict.passed() {
        "::notice title=rqx gate::gate passed".to_owned()
    } else {
        format!(
            "::error title=rqx gate::{}",
            encode_message(&verdict.failures.join(
                "
"
            ))
        )
    }
}

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    let prefix = report.path_prefix();
    for finding in &report.findings {
        writeln!(out, "{}", annotation(finding, &prefix))?;
    }
    if let Some(verdict) = report.verdict {
        writeln!(out, "{}", gate_annotation(verdict))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rqx_core::finding::{Confidence, Evidence, SourceRange};
    use rqx_core::rule::{Category, RuleId};

    use super::*;

    fn custom_finding(file: &str, slug: &str, message: &str) -> Finding {
        Finding {
            rule: RuleId::Custom,
            location: SourceRange {
                file: Arc::from(file),
                start: 0,
                end: 1,
                line: 3,
                column: 5,
                end_line: 3,
            },
            severity: Severity::Warning,
            confidence: Confidence::Verified,
            evidence: Evidence::Custom {
                id: Arc::from(slug),
                message: message.to_owned(),
                family: Category::Safety,
            },
            fix_hint: String::new(),
            related: Vec::new(),
            known: false,
            focus: None,
        }
    }

    #[test]
    fn a_message_keeps_its_line_breaks_as_escapes() {
        assert_eq!(encode_message("a\nb"), "a%0Ab");
        assert_eq!(encode_message("100%"), "100%25");
    }

    #[test]
    fn percent_is_encoded_before_the_escapes_it_introduces() {
        assert_eq!(encode_message("%0A\n"), "%250A%0A");
    }

    #[test]
    fn a_message_may_hold_a_colon_and_a_comma() {
        assert_eq!(encode_message("a: b, c"), "a: b, c");
    }

    #[test]
    fn a_property_escapes_the_colon_and_the_comma() {
        assert_eq!(encode_property("src/a,b:c.ts"), "src/a%2Cb%3Ac.ts");
        assert_eq!(encode_property("50%\r\n"), "50%25%0D%0A");
    }

    #[test]
    fn a_custom_rule_is_titled_with_its_own_slug() {
        let finding = custom_finding("src/a.ts", "no-console", "console.log, left in");

        let line = annotation(&finding, "");

        assert!(line.starts_with("::warning file=src/a.ts,line=3,col=5,title=no-console::"));
        assert!(line.contains("console.log, left in"));
    }

    #[test]
    fn the_gate_closes_the_run_as_an_error_or_a_notice() {
        let failed = Verdict {
            failures: vec!["2 errors".to_owned(), "health 40 is below 60".to_owned()],
            counted: 2,
            known: 0,
        };
        assert_eq!(
            gate_annotation(&failed),
            "::error title=rqx gate::2 errors%0Ahealth 40 is below 60"
        );

        let passed = Verdict {
            failures: Vec::new(),
            counted: 0,
            known: 0,
        };
        assert_eq!(
            gate_annotation(&passed),
            "::notice title=rqx gate::gate passed"
        );
    }

    #[test]
    fn a_comma_or_colon_in_a_path_or_title_cannot_cut_the_annotation() {
        let finding = custom_finding("src/a,b:c.ts", "team:no,log", "m");

        let line = annotation(&finding, "");

        assert!(
            line.starts_with(
                "::warning file=src/a%2Cb%3Ac.ts,line=3,col=5,title=team%3Ano%2Clog::"
            )
        );
    }
}

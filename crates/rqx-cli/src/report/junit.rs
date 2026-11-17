use std::io::Write;

use anyhow::Result;
use rqx_core::finding::Severity;

use crate::gate::Verdict;
use crate::report::Report;

/// The five characters XML reserves. Hand-written because a whole parser crate to
/// emit forty lines of markup is a dependency nobody should audit.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 has no escape for these at all, so they are dropped rather
            // than written literally, which would make the document unparseable.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// The gate as a suite of its own, so a CI provider that shows only tests shows the
/// verdict as one that passed or failed, beside the files.
fn write_gate(out: &mut dyn Write, verdict: &Verdict) -> Result<()> {
    writeln!(
        out,
        r#"  <testsuite name="rqx gate" tests="1" failures="{}">"#,
        usize::from(!verdict.passed())
    )?;
    if verdict.passed() {
        writeln!(out, r#"    <testcase classname="rqx gate" name="gate" />"#)?;
    } else {
        let summary = verdict.failures.first().map_or("", String::as_str);
        writeln!(out, r#"    <testcase classname="rqx gate" name="gate">"#)?;
        writeln!(
            out,
            r#"      <failure type="gate" message="{}">{}</failure>"#,
            escape(summary),
            escape(&verdict.failures.join(
                "
"
            ))
        )?;
        writeln!(out, "    </testcase>")?;
    }
    writeln!(out, "  </testsuite>")?;
    Ok(())
}

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    let shown = report.shown_by_file();
    let prefix = report.path_prefix();
    let gate_cases = usize::from(report.verdict.is_some());
    let cases: usize = gate_cases
        + report
            .files
            .iter()
            .map(|file| {
                shown
                    .get(file.as_str())
                    .map_or(1, |findings| findings.len().max(1))
            })
            .sum::<usize>();
    let gate_failures = usize::from(report.verdict.is_some_and(|verdict| !verdict.passed()));
    let failures = gate_failures
        + report
            .findings
            .iter()
            .filter(|finding| finding.severity >= Severity::Warning)
            .count();

    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
    writeln!(
        out,
        r#"<testsuites name="rqx" tests="{}" failures="{}" time="{:.3}">"#,
        cases,
        failures,
        report.outcome.elapsed_ms as f64 / 1000.0
    )?;

    for file in report.files {
        let path = format!("{prefix}{file}");
        let findings = shown
            .get(file.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        writeln!(
            out,
            r#"  <testsuite name="{}" tests="{}" failures="{}">"#,
            escape(&path),
            findings.len().max(1),
            findings
                .iter()
                .filter(|f| f.severity >= Severity::Warning)
                .count()
        )?;

        // A clean file still emits a passing case, so a provider showing a pass rate
        // reports one against the whole tree rather than against the findings alone.
        if findings.is_empty() {
            writeln!(
                out,
                r#"    <testcase classname="{}" name="rqx" />"#,
                escape(&path)
            )?;
        }

        for finding in findings {
            let spec = finding.rule.spec();
            let name = format!(
                "{} at {}:{}",
                spec.slug, finding.location.line, finding.location.column
            );
            writeln!(
                out,
                r#"    <testcase classname="{}" name="{}">"#,
                escape(&path),
                escape(&name)
            )?;
            let message = escape(&finding.evidence.to_string());
            let body = escape(&format!(
                "{}\n\nWhy: {}\nFix: {}",
                finding.evidence, spec.why, finding.fix_hint
            ));
            match finding.severity {
                Severity::Info => {
                    writeln!(out, r#"      <skipped message="{message}" />"#)?;
                }
                _ => {
                    writeln!(
                        out,
                        r#"      <failure type="{}" message="{message}">{body}</failure>"#,
                        escape(spec.slug)
                    )?;
                }
            }
            writeln!(out, "    </testcase>")?;
        }
        writeln!(out, "  </testsuite>")?;
    }

    if let Some(verdict) = report.verdict {
        write_gate(out, verdict)?;
    }
    writeln!(out, "</testsuites>")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reserved_character_is_escaped() {
        assert_eq!(escape(r#"a<b>&"c'"#), "a&lt;b&gt;&amp;&quot;c&apos;");
    }

    fn gate_suite(verdict: &Verdict) -> String {
        let mut out = Vec::new();
        write_gate(&mut out, verdict).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_failed_gate_is_one_failing_test() {
        let suite = gate_suite(&Verdict {
            failures: vec!["2 errors <found>".to_owned()],
            counted: 2,
            known: 0,
        });
        assert!(suite.contains(r#"<testsuite name="rqx gate" tests="1" failures="1">"#));
        assert!(suite.contains(r#"<failure type="gate" message="2 errors &lt;found&gt;">"#));
    }

    #[test]
    fn a_passed_gate_is_one_passing_test() {
        let suite = gate_suite(&Verdict {
            failures: Vec::new(),
            counted: 0,
            known: 0,
        });
        assert!(suite.contains(r#"failures="0""#));
        assert!(!suite.contains("<failure"));
    }

    #[test]
    fn a_control_character_is_dropped_rather_than_written() {
        assert_eq!(escape("a\u{1}b"), "ab");
        assert_eq!(escape("a\tb"), "a\tb");
    }
}

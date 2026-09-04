use std::io::Write;

use anyhow::Result;
use rqx_core::finding::Severity;

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
fn encode(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    let prefix = report.path_prefix();
    for finding in &report.findings {
        let spec = finding.rule.spec();
        let message = format!(
            "{}\nWhy: {}\nFix: {}",
            finding.evidence, spec.why, finding.fix_hint
        );
        writeln!(
            out,
            "::{} file={},line={},col={},title={}::{}",
            level(finding.severity),
            encode(&format!("{prefix}{}", finding.location.file)),
            finding.location.line.max(1),
            finding.location.column.max(1),
            encode(spec.slug),
            encode(&message)
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_keeps_its_line_breaks_as_escapes() {
        assert_eq!(encode("a\nb"), "a%0Ab");
        assert_eq!(encode("100%"), "100%25");
    }

    #[test]
    fn percent_is_encoded_before_the_escapes_it_introduces() {
        assert_eq!(encode("%0A\n"), "%250A%0A");
    }
}

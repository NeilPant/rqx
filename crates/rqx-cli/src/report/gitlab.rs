use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;

use anyhow::Result;
use rqx_core::finding::{Finding, Severity};
use serde::Serialize;

use crate::report::Report;

#[derive(Serialize)]
struct Entry {
    description: String,
    check_name: &'static str,
    fingerprint: String,
    severity: &'static str,
    location: Location,
}

#[derive(Serialize)]
struct Location {
    path: String,
    lines: Lines,
}

#[derive(Serialize)]
struct Lines {
    begin: u32,
}

const fn severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "major",
        Severity::Warning => "minor",
        Severity::Info => "info",
    }
}

/// Deliberately excludes the line number. GitLab treats a changed fingerprint as a
/// new finding, so hashing the position would re-report every finding in a file the
/// moment anything above it moved.
fn fingerprint(finding: &Finding) -> String {
    let mut hasher = DefaultHasher::new();
    finding.rule.slug().hash(&mut hasher);
    finding.location.file.hash(&mut hasher);
    finding.evidence.to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    let prefix = report.path_prefix();
    let entries: Vec<Entry> = report
        .findings
        .iter()
        .map(|finding| Entry {
            description: format!("{} — {}", finding.evidence, finding.rule.spec().why),
            check_name: finding.rule.slug(),
            fingerprint: fingerprint(finding),
            severity: severity(finding.severity),
            location: Location {
                path: format!("{prefix}{}", finding.location.file),
                lines: Lines { begin: finding.location.line.max(1) },
            },
        })
        .collect();
    writeln!(out, "{}", serde_json::to_string_pretty(&entries)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rqx_core::finding::{Confidence, Evidence, SourceRange};
    use rqx_core::rule::RuleId;

    use super::*;

    fn finding(line: u32) -> Finding {
        Finding {
            rule: RuleId::LongFunction,
            location: SourceRange {
                file: Arc::from("a.ts"),
                start: 0,
                end: 1,
                line,
                column: 1,
                end_line: line,
            },
            severity: Severity::Warning,
            confidence: Confidence::Verified,
            evidence: Evidence::StatementCount { count: 99, threshold: 40 },
            fix_hint: String::new(),
            related: Vec::new(),
        }
    }

    #[test]
    fn a_finding_that_only_moved_keeps_its_fingerprint() {
        assert_eq!(fingerprint(&finding(10)), fingerprint(&finding(400)));
    }

    #[test]
    fn a_different_measurement_is_a_different_finding() {
        let mut other = finding(10);
        other.evidence = Evidence::StatementCount { count: 120, threshold: 40 };
        assert_ne!(fingerprint(&finding(10)), fingerprint(&other));
    }
}

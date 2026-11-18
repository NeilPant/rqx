use std::io::Write;

use anyhow::Result;
use rqx_core::Identity;
use rqx_core::finding::{Finding, Severity};
use serde::Serialize;

use crate::report::Report;

#[derive(Serialize)]
struct Entry {
    description: String,
    check_name: String,
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
///
/// `repeat` disambiguates findings a file holds several of — three empty catches in
/// one file are one identity three times over, and GitLab shows one entry per
/// fingerprint, so without it two of the three are silently dropped. An ordinal is
/// stable under an edit elsewhere in the file, which a line number is not.
fn fingerprint(finding: &Finding, repeat: usize) -> String {
    let identity = Identity::exact(finding).to_string();
    if repeat == 0 {
        identity
    } else {
        format!("{identity}{repeat:04x}")
    }
}

/// One fingerprint per finding, in order, with repeats numbered. Shared with SARIF,
/// whose `partialFingerprints` mean the same thing: the same problem, wherever it
/// has moved to.
pub(super) fn fingerprints<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> Vec<String> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    findings
        .into_iter()
        .map(|finding| {
            let repeat = seen
                .entry(Identity::exact(finding).to_string())
                .or_default();
            let ordinal = *repeat;
            *repeat += 1;
            fingerprint(finding, ordinal)
        })
        .collect()
}

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    let prefix = report.path_prefix();
    let fingerprints = fingerprints(report.findings.iter().copied());
    let entries: Vec<Entry> = report
        .findings
        .iter()
        .zip(fingerprints)
        .map(|(finding, fingerprint)| Entry {
            description: format!("{} — {}", finding.evidence, finding.rule.spec().why),
            check_name: finding.slug().to_owned(),
            fingerprint,
            severity: severity(finding.severity),
            location: Location {
                path: format!("{prefix}{}", finding.location.file),
                lines: Lines {
                    begin: finding.location.line.max(1),
                },
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
            evidence: Evidence::StatementCount {
                count: 99,
                threshold: 40,
            },
            fix_hint: String::new(),
            related: Vec::new(),
            known: false,
            focus: None,
        }
    }

    #[test]
    fn a_finding_that_only_moved_keeps_its_fingerprint() {
        assert_eq!(fingerprint(&finding(10), 0), fingerprint(&finding(400), 0));
    }

    /// GitLab shows one entry per fingerprint, so a file with three identical findings
    /// has to produce three fingerprints or two of them never reach the merge request.
    #[test]
    fn a_file_with_the_same_finding_twice_reports_both() {
        let first = fingerprint(&finding(10), 0);
        let second = fingerprint(&finding(40), 1);
        assert_ne!(first, second);
        assert_eq!(
            first,
            fingerprint(&finding(999), 0),
            "the first one still moves"
        );
    }

    #[test]
    fn a_different_measurement_is_a_different_finding() {
        let mut other = finding(10);
        other.evidence = Evidence::StatementCount {
            count: 120,
            threshold: 40,
        };
        assert_ne!(fingerprint(&finding(10), 0), fingerprint(&other, 0));
    }
}

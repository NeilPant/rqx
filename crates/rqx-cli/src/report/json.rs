use std::io::Write;

use anyhow::Result;
use rqx_core::finding::Finding;
use serde::Serialize;

use crate::report::Report;

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
    /// Present only for `check`, so a scan's summary keeps the shape its consumers
    /// already parse.
    #[serde(skip_serializing_if = "Option::is_none")]
    review: Option<JsonReview<'a>>,
}

#[derive(Serialize)]
struct JsonReview<'a> {
    base: &'a str,
    reviewed_files: usize,
    reviewed_findings: usize,
    counted_against_gate: u32,
    passed: bool,
    failures: &'a [String],
}

pub fn finding_of(finding: &Finding) -> impl Serialize + '_ {
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

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    for finding in &report.findings {
        writeln!(out, "{}", serde_json::to_string(&finding_of(finding))?)?;
    }
    let outcome = report.outcome;
    let summary = JsonSummary {
        kind: "summary",
        health_score: report.health(),
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
        review: report.base.zip(report.verdict).map(|(base, verdict)| JsonReview {
            base,
            reviewed_files: report.files.len(),
            reviewed_findings: report.findings.len(),
            counted_against_gate: verdict.counted,
            passed: verdict.passed(),
            failures: &verdict.failures,
        }),
    };
    writeln!(out, "{}", serde_json::to_string(&summary)?)?;
    Ok(())
}

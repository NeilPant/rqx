use std::io::Write;

use anyhow::Result;
use rqx_core::finding::Finding;
use serde::Serialize;

use crate::report::Report;

#[derive(Serialize)]
struct JsonSummary<'a> {
    kind: &'static str,
    health_score: u32,
    files_scanned: u32,
    code_lines: u64,
    elapsed_ms: u128,
    omitted: u32,
    counts: &'a rqx_core::score::FindingCounts,
    /// Findings a waiver in the source kept out of `counts`, so a consumer can tell
    /// a clean file from a quiet one.
    suppressed: usize,
    skipped: Vec<String>,
    /// Grade per file in scope, so a consumer can gate on one file rotting rather
    /// than on an average that hides it.
    grades: std::collections::BTreeMap<String, &'static str>,
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

/// The finding as an object, with the catalogue's answers merged into it. Built as a
/// map rather than flattened into a struct because `rule` has to be overwritten: a rule
/// the project wrote serialises as `custom` from the enum, and a pipeline filtering on
/// rule names needs the name the project gave it. Flattening can only append the key a
/// second time, which is a duplicate a strict parser rejects.
pub fn finding_of(finding: &Finding) -> serde_json::Value {
    let spec = finding.rule.spec();
    let mut object = match serde_json::to_value(finding) {
        Ok(serde_json::Value::Object(object)) => object,
        _ => serde_json::Map::new(),
    };
    object.insert("rule".to_owned(), finding.slug().into());
    object.insert("why".to_owned(), spec.why.into());
    object.insert(
        "fixes".to_owned(),
        spec.fixes.iter().map(|fix| fix.name()).collect(),
    );
    object.insert("category".to_owned(), finding.category().slug().into());
    object.insert("metric".to_owned(), spec.metric.into());
    serde_json::Value::Object(object)
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
        suppressed: outcome.suppressed.len(),
        skipped: outcome
            .skipped
            .iter()
            .map(|(path, reason)| format!("{}: {reason}", path.display()))
            .collect(),
        grades: report
            .files
            .iter()
            .map(|file| (file.clone(), report.grade_of(file).label()))
            .collect(),
        review: report
            .base
            .zip(report.verdict)
            .map(|(base, verdict)| JsonReview {
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

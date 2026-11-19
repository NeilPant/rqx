use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;
use rqx_core::finding::Finding;
use serde::Serialize;

use crate::report::{Report, SCHEMA_VERSION};

#[derive(Serialize)]
struct JsonSummary<'a> {
    kind: &'static str,
    schema_version: u32,
    rqx_version: &'static str,
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
    /// How many files were skipped for each reason, so a consumer can tell a
    /// submodule left out on purpose from a file rqx failed to read.
    skipped_by: BTreeMap<&'static str, crate::scan::skip::Tally>,
    /// Grade per file in scope, so a consumer can gate on one file rotting rather
    /// than on an average that hides it.
    grades: std::collections::BTreeMap<String, &'static str>,
    /// Present whenever a verdict exists, so a pipeline can read whether the run
    /// passed without reproducing the gate's arithmetic. An ungated scan has none and
    /// keeps the shape its consumers already parse.
    #[serde(skip_serializing_if = "Option::is_none")]
    gate: Option<JsonGate<'a>>,
    /// Present only for `check`.
    #[serde(skip_serializing_if = "Option::is_none")]
    review: Option<JsonReview<'a>>,
    /// Findings the base revision had that the change no longer has. Present only for
    /// a review that scanned its base.
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved: Option<&'a [crate::credit::Resolved]>,
    /// What `resolved` was counted over: `project` for a scan of the whole base, and
    /// `changed-files` for the findings one file's analysis produces in the files the
    /// change touched.
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_scope: Option<crate::credit::Scope>,
    /// The health score after the change minus the score at the base. Present only
    /// when `resolved_scope` is `project`.
    #[serde(skip_serializing_if = "Option::is_none")]
    health_delta: Option<i32>,
    /// Health per owner, present only when the project has a CODEOWNERS file.
    #[serde(skip_serializing_if = "Option::is_none")]
    owners: Option<BTreeMap<String, JsonOwner>>,
    /// The functions whose time order grew against the base. Present only when the review
    /// was asked to look, and empty when it could not compare, in which case
    /// `regressions_skipped` says why.
    #[serde(skip_serializing_if = "Option::is_none")]
    regressions: Option<Vec<JsonRegression>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    regressions_skipped: Option<&'static str>,
}

#[derive(Serialize)]
struct JsonRegression {
    file: String,
    /// The function's name qualified by its class.
    qual: String,
    line: u32,
    before: String,
    after: String,
    trace: Vec<JsonStep>,
}

#[derive(Serialize)]
struct JsonStep {
    file: String,
    line: u32,
    what: String,
    order: String,
}

fn regression_of(regression: &crate::regressions::Regression) -> JsonRegression {
    JsonRegression {
        file: regression.file.clone(),
        qual: regression.qual.clone(),
        line: regression.line,
        before: regression.before.to_string(),
        after: regression.after.to_string(),
        trace: regression
            .trace
            .iter()
            .map(|step| JsonStep {
                file: step.file.clone(),
                line: step.line,
                what: step.what.clone(),
                order: step.order.to_string(),
            })
            .collect(),
    }
}

#[derive(Serialize)]
struct JsonOwner {
    files: u32,
    findings: u32,
    health: u32,
    grade: &'static str,
}

fn owners_of(report: &Report<'_>) -> Option<BTreeMap<String, JsonOwner>> {
    report.outcome.owners.as_ref()?;
    Some(
        report
            .owner_rows()
            .into_iter()
            .map(|row| {
                let owner = JsonOwner {
                    files: row.files,
                    findings: row.findings,
                    health: row.health,
                    grade: row.grade.label(),
                };
                (row.owner, owner)
            })
            .collect(),
    )
}

#[derive(Serialize)]
struct JsonGate<'a> {
    passed: bool,
    failures: &'a [String],
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
    object.insert("why".to_owned(), finding.why().into());
    object.insert(
        "fixes".to_owned(),
        spec.fixes.iter().map(|fix| fix.name()).collect(),
    );
    object.insert("category".to_owned(), finding.category().slug().into());
    object.insert("metric".to_owned(), finding.metric().into());
    serde_json::Value::Object(object)
}

/// A line of the stream, tagged so a consumer can switch on `kind` instead of
/// guessing from which keys are present.
fn finding_line(finding: &Finding, report: &Report<'_>) -> serde_json::Value {
    let mut line = finding_of(finding);
    line["kind"] = "finding".into();
    if report.outcome.owners.is_some() {
        line["owners"] = report.owners_of(&finding.location.file).into();
    }
    line
}

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    for finding in &report.findings {
        writeln!(
            out,
            "{}",
            serde_json::to_string(&finding_line(finding, report))?
        )?;
    }
    let outcome = report.outcome;
    let summary = JsonSummary {
        kind: "summary",
        schema_version: SCHEMA_VERSION,
        rqx_version: env!("CARGO_PKG_VERSION"),
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
            .map(|skipped| skipped.listed(&outcome.root))
            .collect(),
        skipped_by: crate::scan::skip::tally(&outcome.skipped, &outcome.root),
        grades: report
            .files
            .iter()
            .map(|file| (file.clone(), report.grade_of(file).label()))
            .collect(),
        gate: report.verdict.map(|verdict| JsonGate {
            passed: verdict.passed(),
            failures: &verdict.failures,
        }),
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
        resolved: report.credit.map(|credit| credit.resolved.as_slice()),
        resolved_scope: report.credit.map(|credit| credit.scope),
        health_delta: report.credit.and_then(crate::credit::Credit::health_delta),
        owners: owners_of(report),
        regressions: report
            .regressions
            .map(|regressions| regressions.found().iter().map(regression_of).collect()),
        regressions_skipped: report
            .regressions
            .and_then(crate::regressions::Regressions::skipped),
    };
    writeln!(out, "{}", serde_json::to_string(&summary)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rqx_core::finding::Severity;
    use rqx_core::ownership::{CodeOwners, Dialect};

    use super::*;
    use crate::report::tests::outcome_with;

    fn lines_of(outcome: &crate::scan::ScanOutcome) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        render(&Report::everything(outcome), &mut out).expect("rendering to memory");
        String::from_utf8(out)
            .expect("utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a JSON line"))
            .collect()
    }

    #[test]
    fn without_a_codeowners_file_nothing_about_owners_is_written() {
        let outcome = outcome_with(&[("web/a.ts", Severity::Error)], 1);
        let lines = lines_of(&outcome);
        assert!(lines.iter().all(|line| line.get("owners").is_none()));
    }

    #[test]
    fn findings_and_the_summary_name_owners_when_a_codeowners_file_exists() {
        let mut outcome = outcome_with(&[("web/a.ts", Severity::Error)], 2);
        outcome.owners = Some(CodeOwners::parse("web/ @web @lead\n", Dialect::GitHub));
        let lines = lines_of(&outcome);

        assert_eq!(lines[0]["owners"], serde_json::json!(["@web", "@lead"]));
        let summary = &lines[1]["owners"];
        assert_eq!(summary["@web"]["findings"], 1);
        assert_eq!(summary["@web"]["files"], 1);
        assert_eq!(summary["(unowned)"]["files"], 1);
        assert!(summary["@web"]["grade"].is_string());
    }
}

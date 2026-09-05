use anyhow::Result;
use rqx_core::finding::{Finding, Severity, SourceRange};
use serde_json::{Value, json};

const SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";

/// SARIF speaks in `error`/`warning`/`note`, and code scanning treats `note` as
/// informational rather than hiding it, which is where low-confidence findings
/// belong.
const fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "note",
    }
}

/// SARIF regions are one-based in both axes, which already matches how findings
/// carry their position, but columns are exclusive at the end.
fn location(range: &SourceRange, prefix: &str) -> Value {
    json!({
        "physicalLocation": {
            "artifactLocation": {
                "uri": format!("{prefix}{}", range.file),
                "uriBaseId": "%SRCROOT%"
            },
            "region": {
                "startLine": range.line.max(1),
                "startColumn": range.column.max(1),
                "endLine": range.end_line.max(range.line).max(1)
            }
        }
    })
}

/// Built from a finding rather than from a `RuleId`, because a rule the project
/// wrote has no spec: its name, family and message travel with the finding, and
/// describing them all under one id would collapse them into one rule in the UI.
fn rule_descriptor(finding: &Finding) -> Value {
    let spec = finding.rule.spec();
    let slug = finding.slug();
    let family = finding.category();
    let (metric, why) = match &finding.evidence {
        rqx_core::finding::Evidence::Custom { message, .. } => {
            (message.as_str(), "this project asked to be told about it")
        }
        _ => (spec.metric, spec.why),
    };
    let tags = vec![
        "rqx".to_owned(),
        "maintainability".to_owned(),
        family.slug().to_owned(),
    ];
    json!({
        "id": slug,
        "name": slug,
        "shortDescription": { "text": metric },
        "fullDescription": { "text": why },
        "help": {
            "text": format!(
                "{why}\n\nFix: {}\nMetric: {metric}",
                spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>().join(" / "),
            )
        },
        "defaultConfiguration": { "level": level(finding.severity) },
        "properties": { "tags": tags, "problem.severity": level(finding.severity) }
    })
}

fn result(finding: &Finding, prefix: &str) -> Value {
    let spec = finding.rule.spec();
    let mut value = json!({
        "ruleId": finding.slug(),
        "level": level(finding.severity),
        "message": {
            "text": format!("{} {} Fix: {}", finding.evidence, spec.why, finding.fix_hint)
                .trim_end()
                .to_owned()
        },
        "locations": [location(&finding.location, prefix)],
        "properties": {
            "confidence": match finding.confidence {
                rqx_core::finding::Confidence::Verified => "verified",
                rqx_core::finding::Confidence::Heuristic => "heuristic",
                rqx_core::finding::Confidence::Low => "low",
            }
        }
    });
    if !finding.related.is_empty() {
        value["relatedLocations"] = Value::Array(
            finding
                .related
                .iter()
                .map(|range| location(range, prefix))
                .collect(),
        );
    }
    value
}

fn document(
    findings: &[&Finding],
    waived: &[rqx_core::suppress::Suppressed],
    prefix: &str,
) -> Result<String> {
    // Only rules that actually fired are described, so the report does not carry a
    // catalogue of rules the run had nothing to say about.
    let mut fired: Vec<&Finding> = findings
        .iter()
        .copied()
        .chain(waived.iter().map(|waived| &waived.finding))
        .collect();
    fired.sort_by_key(|finding| finding.slug().to_owned());
    fired.dedup_by_key(|finding| finding.slug().to_owned());

    // SARIF carries suppression as a property of the result rather than as an
    // absence, so a waived finding is reported and marked rather than dropped —
    // which is what lets a code-scanning UI show it as suppressed.
    let results = findings
        .iter()
        .map(|finding| result(finding, prefix))
        .chain(waived.iter().map(|waived| {
            let mut entry = result(&waived.finding, prefix);
            entry["suppressions"] = json!([{
                "kind": "inSource",
                "justification": waived.reason.clone().unwrap_or_default(),
            }]);
            entry
        }))
        .collect::<Vec<_>>();

    let document = json!({
        "$schema": SCHEMA,
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "rqx",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": "https://github.com/jadhamwi21/rqx",
                "rules": fired.iter().map(|finding| rule_descriptor(finding)).collect::<Vec<_>>()
            }},
            "results": results
        }]
    });
    Ok(serde_json::to_string_pretty(&document)?)
}

pub fn render(report: &crate::report::Report<'_>, out: &mut dyn std::io::Write) -> Result<()> {
    writeln!(
        out,
        "{}",
        document(
            &report.findings,
            &report.outcome.suppressed,
            &report.path_prefix()
        )?
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rqx_core::finding::{Confidence, Evidence};
    use rqx_core::rule::RuleId;

    use super::*;

    fn finding(rule: RuleId, line: u32) -> Finding {
        let spec = rule.spec();
        Finding {
            rule,
            location: SourceRange {
                file: Arc::from("src/a.ts"),
                start: 0,
                end: 4,
                line,
                column: 3,
                end_line: line,
            },
            severity: spec.severity,
            confidence: Confidence::Verified,
            evidence: Evidence::StatementCount {
                count: 90,
                threshold: 40,
            },
            fix_hint: "split it".to_owned(),
            related: Vec::new(),
            known: false,
        }
    }

    #[test]
    fn produces_a_well_formed_sarif_run() {
        let items = [
            finding(RuleId::LongFunction, 12),
            finding(RuleId::DeepNesting, 30),
        ];
        let refs: Vec<&Finding> = items.iter().collect();
        let document: Value = serde_json::from_str(&document(&refs, &[], "").unwrap()).unwrap();

        assert_eq!(document["version"], "2.1.0");
        let run = &document["runs"][0];
        assert_eq!(run["tool"]["driver"]["name"], "rqx");
        assert_eq!(run["results"].as_array().unwrap().len(), 2);
        assert_eq!(run["tool"]["driver"]["rules"].as_array().unwrap().len(), 2);
    }

    /// Code scanning drops a result whose `ruleId` names no described rule, so the
    /// two lists have to agree or findings silently vanish in the UI.
    #[test]
    fn every_result_names_a_described_rule() {
        let items = [
            finding(RuleId::LongFunction, 1),
            finding(RuleId::LongFunction, 2),
        ];
        let refs: Vec<&Finding> = items.iter().collect();
        let document: Value = serde_json::from_str(&document(&refs, &[], "").unwrap()).unwrap();
        let run = &document["runs"][0];

        let described: Vec<&str> = run["tool"]["driver"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|rule| rule["id"].as_str().unwrap())
            .collect();
        assert_eq!(described.len(), 1, "a repeated rule is described once");
        for result in run["results"].as_array().unwrap() {
            assert!(described.contains(&result["ruleId"].as_str().unwrap()));
        }
    }

    #[test]
    fn regions_are_one_based_even_for_a_synthetic_span() {
        let mut item = finding(RuleId::UnusedFile, 0);
        item.location.column = 0;
        let refs = vec![&item];
        let document: Value = serde_json::from_str(&document(&refs, &[], "").unwrap()).unwrap();
        let region =
            &document["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startLine"], 1);
        assert_eq!(region["startColumn"], 1);
    }

    #[test]
    fn an_empty_scan_is_still_a_valid_report() {
        let document: Value = serde_json::from_str(&document(&[], &[], "").unwrap()).unwrap();
        assert_eq!(document["runs"][0]["results"].as_array().unwrap().len(), 0);
    }
}

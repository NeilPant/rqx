use anyhow::Result;
use rqx_core::finding::{Finding, Severity, SourceRange};
use rqx_core::rule::RuleId;
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

fn rule_descriptor(rule: RuleId) -> Value {
    let spec = rule.spec();
    let mut tags = vec!["rqx".to_owned(), "maintainability".to_owned()];
    if let Some(smell) = spec.guru_smell {
        tags.push(smell.to_owned());
    }
    json!({
        "id": spec.slug,
        "name": spec.slug,
        "shortDescription": { "text": spec.metric },
        "fullDescription": { "text": spec.why },
        "help": {
            "text": format!(
                "{}\n\nFix: {}\n\nClean Code: {}\nMetric: {}",
                spec.why,
                spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>().join(" / "),
                spec.clean_code,
                spec.metric
            )
        },
        "defaultConfiguration": { "level": level(spec.severity) },
        "properties": { "tags": tags, "problem.severity": level(spec.severity) }
    })
}

fn result(finding: &Finding, prefix: &str) -> Value {
    let spec = finding.rule.spec();
    let mut value = json!({
        "ruleId": spec.slug,
        "level": level(finding.severity),
        "message": {
            "text": format!("{} {} Fix: {}", finding.evidence, spec.why, finding.fix_hint)
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
        value["relatedLocations"] =
            Value::Array(finding.related.iter().map(|range| location(range, prefix)).collect());
    }
    value
}

fn document(findings: &[&Finding], prefix: &str) -> Result<String> {
    // Only rules that actually fired are described, so the report does not carry a
    // catalogue of rules the run had nothing to say about.
    let mut fired: Vec<RuleId> = findings.iter().map(|finding| finding.rule).collect();
    fired.sort();
    fired.dedup();

    let document = json!({
        "$schema": SCHEMA,
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "rqx",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": "https://github.com/jadhamwi/rqx",
                "rules": fired.iter().map(|rule| rule_descriptor(*rule)).collect::<Vec<_>>()
            }},
            "results": findings.iter().map(|finding| result(finding, prefix)).collect::<Vec<_>>()
        }]
    });
    Ok(serde_json::to_string_pretty(&document)?)
}

pub fn render(report: &crate::report::Report<'_>, out: &mut dyn std::io::Write) -> Result<()> {
    writeln!(out, "{}", document(&report.findings, &report.path_prefix())?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rqx_core::finding::{Confidence, Evidence};

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
            evidence: Evidence::StatementCount { count: 90, threshold: 40 },
            fix_hint: "split it".to_owned(),
            related: Vec::new(),
        }
    }

    #[test]
    fn produces_a_well_formed_sarif_run() {
        let items = [finding(RuleId::LongFunction, 12), finding(RuleId::DeepNesting, 30)];
        let refs: Vec<&Finding> = items.iter().collect();
        let document: Value = serde_json::from_str(&document(&refs, "").unwrap()).unwrap();

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
        let items = [finding(RuleId::LongFunction, 1), finding(RuleId::LongFunction, 2)];
        let refs: Vec<&Finding> = items.iter().collect();
        let document: Value = serde_json::from_str(&document(&refs, "").unwrap()).unwrap();
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
        let document: Value = serde_json::from_str(&document(&refs, "").unwrap()).unwrap();
        let region = &document["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startLine"], 1);
        assert_eq!(region["startColumn"], 1);
    }

    #[test]
    fn an_empty_scan_is_still_a_valid_report() {
        let document: Value = serde_json::from_str(&document(&[], "").unwrap()).unwrap();
        assert_eq!(document["runs"][0]["results"].as_array().unwrap().len(), 0);
    }
}

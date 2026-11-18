use anyhow::Result;
use rqx_core::config::RegressionGate;
use rqx_core::finding::{Finding, Severity, SourceRange};
use serde_json::{Value, json};

use crate::gate::Verdict;
use crate::regressions::{Regression, Regressions};
use crate::report::SCHEMA_VERSION;

const SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";

/// The rule id of a function that costs more than it did at the base revision.
const REGRESSION_RULE: &str = "complexity-regression";

/// What changes how code scanning keys its alerts, so it is asked for rather than
/// assumed. Fingerprints replace the matching GitHub uses for alerts it already
/// holds, and an `automationDetails` category files the run under a different
/// category from the one it used before; either one can close every open alert and
/// open the same ones again.
#[derive(Default)]
pub struct Options {
    pub fingerprints: bool,
    pub category: Option<String>,
}

impl Options {
    pub fn from_env() -> Self {
        Self {
            fingerprints: std::env::var("RQX_SARIF_FINGERPRINTS").is_ok_and(|value| value == "1"),
            category: std::env::var("RQX_SARIF_CATEGORY")
                .ok()
                .filter(|category| !category.is_empty()),
        }
    }
}

/// What the run concluded, so a viewer that reads only the document still learns
/// whether the gate held.
fn invocation(verdict: Option<&Verdict>) -> Value {
    let failures = verdict.map_or(&[][..], |verdict| verdict.failures.as_slice());
    json!({
        "executionSuccessful": true,
        "exitCode": i32::from(!failures.is_empty()),
        "toolExecutionNotifications": failures
            .iter()
            .map(|failure| json!({ "level": "error", "message": { "text": failure } }))
            .collect::<Vec<_>>()
    })
}

fn run_properties(verdict: Option<&Verdict>) -> Value {
    let mut properties = json!({ "schema_version": SCHEMA_VERSION });
    if let Some(verdict) = verdict {
        properties["gate"] = json!({ "passed": verdict.passed(), "failures": verdict.failures });
    }
    json!({ "rqx": properties })
}

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
    place(
        &range.file,
        (range.line, range.column, range.end_line),
        prefix,
    )
}

/// `lines` are the first line, the first column and the last line.
fn place(file: &str, lines: (u32, u32, u32), prefix: &str) -> Value {
    let (line, column, end_line) = lines;
    json!({
        "physicalLocation": {
            "artifactLocation": {
                "uri": format!("{prefix}{file}"),
                "uriBaseId": "%SRCROOT%"
            },
            "region": {
                "startLine": line.max(1),
                "startColumn": column.max(1),
                "endLine": end_line.max(line).max(1)
            }
        }
    })
}

/// What the gate's stance on regressions makes of a result. A review that looked
/// without the gate asking is informational.
const fn regression_level(gate: RegressionGate) -> &'static str {
    match gate {
        RegressionGate::Error => "error",
        RegressionGate::Warning => "warning",
        RegressionGate::Off => "note",
    }
}

fn regression_descriptor(gate: RegressionGate) -> Value {
    let why = "A function's time order is larger than it was at the base revision, whether \
               its own code changed or a function it calls did.";
    json!({
        "id": REGRESSION_RULE,
        "name": REGRESSION_RULE,
        "shortDescription": { "text": "A function costs more than at the base revision" },
        "fullDescription": { "text": why },
        "help": { "text": format!("{why}\n\nThe trace on the result shows how the function reaches its order.") },
        "defaultConfiguration": { "level": regression_level(gate) },
        "properties": {
            "tags": ["rqx", "performance"],
            "problem.severity": regression_level(gate)
        }
    })
}

fn regression_result(regression: &Regression, gate: RegressionGate, prefix: &str) -> Value {
    let trace: Vec<Value> = regression
        .trace
        .iter()
        .map(|step| {
            let mut related = place(&step.file, (step.line, 1, step.line), prefix);
            related["message"] = json!({ "text": format!("{} {}", step.what, step.order) });
            related
        })
        .collect();
    let mut value = json!({
        "ruleId": REGRESSION_RULE,
        "level": regression_level(gate),
        "message": {
            "text": format!(
                "`{}` went from {} to {} against the base revision.",
                regression.qual, regression.before, regression.after
            )
        },
        "locations": [place(&regression.file, (regression.line, 1, regression.line), prefix)],
        "properties": {
            "function": regression.qual,
            "before": regression.before.to_string(),
            "after": regression.after.to_string()
        }
    });
    if !trace.is_empty() {
        value["relatedLocations"] = Value::Array(trace);
    }
    value
}

/// Built from a finding rather than from a `RuleId`, because a rule the project
/// wrote has no spec: its name, family and message travel with the finding, and
/// describing them all under one id would collapse them into one rule in the UI.
fn rule_descriptor(finding: &Finding) -> Value {
    let spec = finding.rule.spec();
    let slug = finding.slug();
    let family = finding.category();
    let (metric, why) = match &finding.evidence {
        rqx_core::finding::Evidence::Custom { message, .. } => (message.as_str(), finding.why()),
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
    verdict: Option<&Verdict>,
    regressions: Option<&Regressions>,
    options: &Options,
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
    let mut results = findings
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
    let regressed = regressions
        .filter(|regressions| !regressions.found().is_empty())
        .map(|regressions| (regressions.gate, regressions.found()));
    if let Some((gate, found)) = regressed {
        results.extend(
            found
                .iter()
                .map(|regression| regression_result(regression, gate, prefix)),
        );
    }
    if options.fingerprints {
        let reported = findings
            .iter()
            .copied()
            .chain(waived.iter().map(|waived| &waived.finding));
        for (entry, fingerprint) in results
            .iter_mut()
            .zip(super::gitlab::fingerprints(reported))
        {
            entry["partialFingerprints"] = json!({ "rqxIdentity/v1": fingerprint });
        }
    }

    let mut run = json!({
        "tool": { "driver": {
            "name": "rqx",
            "version": env!("CARGO_PKG_VERSION"),
            "informationUri": "https://github.com/jadhamwi21/rqx",
            "rules": fired
                .iter()
                .map(|finding| rule_descriptor(finding))
                .chain(regressed.map(|(gate, _)| regression_descriptor(gate)))
                .collect::<Vec<_>>()
        }},
        "invocations": [invocation(verdict)],
        "properties": run_properties(verdict),
        "results": results
    });
    if let Some(category) = &options.category {
        run["automationDetails"] = json!({ "id": category });
    }
    let document = json!({ "$schema": SCHEMA, "version": "2.1.0", "runs": [run] });
    Ok(serde_json::to_string_pretty(&document)?)
}

pub fn render(report: &crate::report::Report<'_>, out: &mut dyn std::io::Write) -> Result<()> {
    writeln!(
        out,
        "{}",
        document(
            &report.findings,
            &report.outcome.suppressed,
            &report.path_prefix(),
            report.verdict,
            report.regressions,
            &Options::from_env(),
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

    fn plain(findings: &[&Finding]) -> Result<String> {
        document(findings, &[], "", None, None, &Options::default())
    }

    #[test]
    fn produces_a_well_formed_sarif_run() {
        let items = [
            finding(RuleId::LongFunction, 12),
            finding(RuleId::DeepNesting, 30),
        ];
        let refs: Vec<&Finding> = items.iter().collect();
        let document: Value = serde_json::from_str(&plain(&refs).unwrap()).unwrap();

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
        let document: Value = serde_json::from_str(&plain(&refs).unwrap()).unwrap();
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
        let document: Value = serde_json::from_str(&plain(&refs).unwrap()).unwrap();
        let region =
            &document["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startLine"], 1);
        assert_eq!(region["startColumn"], 1);
    }

    #[test]
    fn an_empty_scan_is_still_a_valid_report() {
        let document: Value = serde_json::from_str(&plain(&[]).unwrap()).unwrap();
        assert_eq!(document["runs"][0]["results"].as_array().unwrap().len(), 0);
    }

    fn with(verdict: Option<&Verdict>, options: &Options) -> Value {
        let item = finding(RuleId::LongFunction, 3);
        let refs = [&item, &item];
        serde_json::from_str(&document(&refs, &[], "", verdict, None, options).unwrap()).unwrap()
    }

    #[test]
    fn a_failed_gate_is_recorded_in_the_run() {
        let verdict = Verdict {
            failures: vec!["2 findings at or above warning".to_owned()],
            counted: 2,
            known: 0,
        };
        let run = &with(Some(&verdict), &Options::default())["runs"][0];

        let invocation = &run["invocations"][0];
        assert_eq!(invocation["executionSuccessful"], true);
        assert_eq!(invocation["exitCode"], 1);
        let notice = &invocation["toolExecutionNotifications"][0];
        assert_eq!(notice["level"], "error");
        assert_eq!(notice["message"]["text"], "2 findings at or above warning");
        assert_eq!(run["properties"]["rqx"]["schema_version"], 1);
        assert_eq!(run["properties"]["rqx"]["gate"]["passed"], false);
    }

    #[test]
    fn an_ungated_run_exits_zero_and_carries_no_gate() {
        let run = &with(None, &Options::default())["runs"][0];
        assert_eq!(run["invocations"][0]["exitCode"], 0);
        assert!(run["properties"]["rqx"].get("gate").is_none());
    }

    fn regression() -> Regression {
        Regression {
            file: "src/x.ts".to_owned(),
            qual: "reconcileBatch".to_owned(),
            line: 90,
            before: "O(n)".parse().unwrap(),
            after: "O(n^2)".parse().unwrap(),
            trace: vec![rqx_core::cost::TraceStep {
                file: "src/y.ts".to_owned(),
                line: 12,
                what: "findMatch".to_owned(),
                order: "O(n)".parse().unwrap(),
            }],
        }
    }

    fn with_regressions(gate: RegressionGate, found: Vec<Regression>) -> Value {
        let regressions = Regressions {
            gate,
            state: crate::regressions::State::Found(found),
        };
        let text = document(&[], &[], "", None, Some(&regressions), &Options::default());
        serde_json::from_str(&text.unwrap()).unwrap()
    }

    #[test]
    fn a_regression_is_a_result_at_the_function_with_its_trace_as_related_locations() {
        let document = with_regressions(RegressionGate::Error, vec![regression()]);
        let run = &document["runs"][0];

        let result = &run["results"][0];
        assert_eq!(result["ruleId"], "complexity-regression");
        assert_eq!(result["level"], "error");
        assert_eq!(
            result["message"]["text"],
            "`reconcileBatch` went from O(n) to O(n\u{b2}) against the base revision."
        );
        let at = &result["locations"][0]["physicalLocation"];
        assert_eq!(at["artifactLocation"]["uri"], "src/x.ts");
        assert_eq!(at["region"]["startLine"], 90);
        assert_eq!(result["properties"]["before"], "O(n)");
        assert_eq!(result["properties"]["after"], "O(n\u{b2})");
        let related = &result["relatedLocations"][0];
        assert_eq!(related["physicalLocation"]["region"]["startLine"], 12);
        assert_eq!(related["message"]["text"], "findMatch O(n)");
        let rules = run["tool"]["driver"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["id"], "complexity-regression");
    }

    #[test]
    fn the_level_of_a_regression_follows_the_gate() {
        for (gate, level) in [
            (RegressionGate::Error, "error"),
            (RegressionGate::Warning, "warning"),
            (RegressionGate::Off, "note"),
        ] {
            let document = with_regressions(gate, vec![regression()]);
            let run = &document["runs"][0];
            assert_eq!(run["results"][0]["level"], level);
            assert_eq!(
                run["tool"]["driver"]["rules"][0]["defaultConfiguration"]["level"],
                level
            );
        }
    }

    #[test]
    fn a_review_with_no_regression_describes_no_rule_for_them() {
        let document = with_regressions(RegressionGate::Error, Vec::new());
        let run = &document["runs"][0];

        assert!(run["results"].as_array().unwrap().is_empty());
        assert!(
            run["tool"]["driver"]["rules"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn fingerprints_and_a_category_appear_only_when_asked_for() {
        let plain = &with(None, &Options::default())["runs"][0];
        assert!(plain["results"][0].get("partialFingerprints").is_none());
        assert!(plain.get("automationDetails").is_none());

        let asked = Options {
            fingerprints: true,
            category: Some("rqx/backend".to_owned()),
        };
        let run = &with(None, &asked)["runs"][0];
        let first = &run["results"][0]["partialFingerprints"]["rqxIdentity/v1"];
        let second = &run["results"][1]["partialFingerprints"]["rqxIdentity/v1"];
        assert!(first.is_string() && second.is_string());
        assert_ne!(first, second, "a repeat of the same finding is numbered");
        assert_eq!(run["automationDetails"]["id"], "rqx/backend");
    }
}

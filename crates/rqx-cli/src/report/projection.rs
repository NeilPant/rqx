use std::collections::BTreeMap;

use rqx_core::rule::RuleId;
use serde_json::{Value, json};

use crate::report::Report;
use crate::scan::ScanOutcome;

/// The same projection the JSON reporter writes, so a panel rendering this, an agent
/// reading it over MCP and a CI job parsing `--reporter json` cannot disagree about
/// what the scan found.
pub fn scan(report: &Report<'_>, outcome: &ScanOutcome) -> Value {
    json!({
        "health": report.health(),
        "filesScanned": outcome.files_scanned,
        "codeLines": outcome.code_lines,
        "elapsedMs": outcome.elapsed_ms,
        "omitted": outcome.omitted,
        "counts": outcome.counts,
        "grades": report
            .files
            .iter()
            .map(|file| (file.clone(), report.grade_of(file).label()))
            .collect::<BTreeMap<_, _>>(),
        "impact": report
            .files
            .iter()
            .map(|file| {
                let impact = outcome.impact.get(file);
                (
                    file.clone(),
                    json!({ "commits": impact.commits, "importers": impact.importers }),
                )
            })
            .collect::<BTreeMap<_, _>>(),
        "startHere": start_here(report),
        "advice": outcome
            .advice
            .iter()
            .map(|advice| json!({
                "advisory": advice.advisory.spec().slug,
                "file": advice.location.file,
                "line": advice.location.line,
                "note": advice.note,
            }))
            .collect::<Vec<_>>(),
        "skipped": outcome
            .skipped
            .iter()
            .map(|(path, reason)| format!("{}: {reason}", path.display()))
            .collect::<Vec<_>>(),
    })
}

/// Twenty is what a reader opens files from; the whole ranking is every file that has
/// a grade, which is the same list `grades` already carries.
const PRIORITY: usize = 20;

pub fn start_here(report: &Report<'_>) -> Vec<Value> {
    report
        .priority()
        .iter()
        .take(PRIORITY)
        .map(|(file, grade)| {
            // The grade says how bad, and the blast radius says how much it matters —
            // a ranking that shows neither asks the reader to take the order on trust.
            let impact = report.outcome.impact.get(file);
            json!({
                "file": file,
                "grade": grade.label(),
                "findings": report.outcome.findings_by_file.get(*file).copied().unwrap_or(0),
                "importers": impact.importers,
                "commits": impact.commits,
            })
        })
        .collect()
}

pub fn health(report: &Report<'_>, outcome: &ScanOutcome) -> Value {
    json!({
        "health": report.health(),
        "findings": outcome.counts.total,
        "files": outcome.files_scanned,
        "elapsedMs": outcome.elapsed_ms,
    })
}

/// Renders the whole catalogue entry, so a client asking about a rule gets the same
/// page the terminal prints rather than a second, thinner description.
pub fn rule(slug: &str) -> Option<Value> {
    let rule = RuleId::ALL.iter().find(|rule| rule.spec().slug == slug)?;
    let spec = rule.spec();
    let fixes = spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>();
    let markdown = format!(
        "# `{}`\n\n**{}**\n\n{}\n\n**Measured by** {}\n\n**Fix — {}**\n\n---\n\n*{}*\n",
        spec.slug,
        spec.category.title(),
        spec.why,
        spec.metric,
        fixes.join(" / "),
        spec.citation(),
    );
    Some(json!({
        "slug": spec.slug,
        "category": spec.category.title(),
        "metric": spec.metric,
        "why": spec.why,
        "citation": spec.citation(),
        "fixes": fixes,
        "markdown": markdown,
    }))
}

pub fn catalogue() -> Value {
    json!(
        RuleId::ALL
            .iter()
            .map(|rule| {
                let spec = rule.spec();
                json!({
                    "slug": spec.slug,
                    "category": spec.category.slug(),
                    "family": spec.category.title(),
                    "metric": spec.metric,
                    "why": spec.why,
                    "citation": spec.citation(),
                    "severity": format!("{:?}", spec.severity).to_lowercase(),
                    "offByDefault": spec.off_by_default,
                    "fixes": spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>()
    )
}

/// A pattern and an advisory slug are the same question asked two ways — `strategy`
/// and `strategy-fits` both mean "tell me about Strategy" — so both resolve here.
pub fn pattern(slug: &str) -> Option<Value> {
    let pattern = rqx_core::Pattern::parse(slug)
        .or_else(|| rqx_core::AdviceId::parse(slug).map(rqx_core::AdviceId::pattern))?;
    let stances = pattern
        .advisories()
        .map(|advisory| {
            let spec = advisory.spec();
            format!("- **{}** — {}", spec.stance.label(), spec.signature)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let markdown = format!(
        "# {}\n\n**{} · {}**\n\n{}\n\n{stances}\n",
        pattern.name(),
        pattern.family().title(),
        pattern.section(),
        pattern.intent(),
    );
    Some(json!({
        "markdown": markdown,
        "slug": pattern.slug(),
        "name": pattern.name(),
        "family": pattern.family().title(),
        "section": pattern.section(),
        "intent": pattern.intent(),
        "advisories": pattern
            .advisories()
            .map(|advisory| {
                let spec = advisory.spec();
                json!({
                    "slug": spec.slug,
                    "stance": spec.stance.slug(),
                    "stanceLabel": spec.stance.label(),
                    "signature": spec.signature,
                    "confidence": spec.confidence,
                    "why": spec.why,
                    "offByDefault": spec.off_by_default,
                    "techniques": spec
                        .techniques
                        .iter()
                        .map(|technique| technique.name())
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
    }))
}

pub fn advisory(advice: &rqx_core::Advice) -> Value {
    let spec = advice.advisory.spec();
    json!({
        "advisory": spec.slug,
        "pattern": spec.pattern.name(),
        "family": spec.pattern.family().title(),
        "stance": spec.stance.slug(),
        "signature": spec.signature,
        "confidence": spec.confidence,
        "why": spec.why,
        "file": advice.location.file,
        "line": advice.location.line,
        "column": advice.location.column,
        "note": advice.note,
        "techniques": spec
            .techniques
            .iter()
            .map(|technique| technique.name())
            .collect::<Vec<_>>(),
    })
}

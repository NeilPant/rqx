use std::collections::BTreeMap;
use std::path::Path;

use rqx_core::LineIndex;
use rqx_core::finding::Finding;
use rqx_core::rule::RuleId;
use rqx_core::span::Span;
use serde_json::{Value, json};

use crate::config::Resolved;
use crate::report::{FindingFilter, Report, finding_of, setting_rows};
use crate::scan::ScanOutcome;

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

/// The verdict of a diff review, shaped once so the LSP reply and the MCP tool
/// cannot drift apart.
pub fn review(review: &crate::check::Review, verdict: &crate::gate::Verdict) -> Value {
    let report = Report::reviewed(review, verdict);
    json!({
        "base": review.base,
        "reviewedFiles": review.reviewed.len(),
        "reviewedFindings": review.findings().count(),
        "countedAgainstGate": verdict.counted,
        "known": verdict.known,
        "passed": verdict.passed(),
        "failures": verdict.failures,
        "health": report.health(),
    })
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
    let measured = rqx_core::precision::measured(spec.slug);
    let mut markdown = format!(
        "# `{}`\n\n**{}**\n\n{}\n\n**Measured by** {}\n\n**Fix — {}**\n",
        spec.slug,
        spec.category.title(),
        spec.why,
        spec.metric,
        fixes.join(" / "),
    );
    let mut answer = json!({
        "slug": spec.slug,
        "category": spec.category.title(),
        "metric": spec.metric,
        "why": spec.why,
        "fixes": fixes,
    });
    if let Some(measured) = measured {
        markdown.push_str(&format!("\n{}\n", measured.line()));
        answer["measuredPrecision"] = json!({
            "precision": measured.precision(),
            "correct": measured.correct,
            "reported": measured.reported,
        });
    }
    answer["markdown"] = json!(markdown);
    Some(answer)
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

/// One finding as an editor or a page reads it: the JSON reporter's shape, plus the
/// two answers only the project's config and baseline can give.
pub fn finding(finding: &Finding, settings: &Resolved, root: &Path) -> Value {
    let mut value = json!(finding_of(finding));
    value["autofixable"] = json!(
        settings
            .for_file(&root.join(&*finding.location.file))
            .fix
            .admits(finding)
    );
    value["known"] = json!(finding.known);
    value
}

pub fn findings(outcome: &ScanOutcome, filter: &FindingFilter, settings: &Resolved) -> Value {
    json!(
        outcome
            .findings
            .iter()
            .filter(|candidate| filter.matches(candidate))
            .map(|candidate| finding(candidate, settings, &outcome.root))
            .collect::<Vec<_>>()
    )
}

/// The ranked plan, in the shape the LSP's `rqx.plan` reply has always read.
/// The bottleneck rows, shaped once so an agent and the command line cannot
/// disagree about which function to open first.
pub fn perf(outcome: &ScanOutcome, threshold: u32, limit: usize) -> Value {
    let view = crate::perf::View {
        by: crate::perf::By::Function,
        sort: crate::perf::Sort::Weight,
        top: limit,
        min_depth: 0,
    };
    json!({
        "threshold": threshold,
        "functions": crate::perf::ranked(outcome, &view)
            .iter()
            .take(limit)
            .map(|row| row.with_cost(json!({
                "name": row.name,
                "at": row.at,
                "order": crate::perf::order(row.loops, row.self_calls),
                "loops": row.loops,
                "io": row.io,
                "reach": row.reach,
                "weight": row.weight,
                "sampled": row.sampled.map(|share| json!({
                    "self": share.own,
                    "inclusive": share.total,
                })),
                "findings": row.findings.iter().map(|line| json!({
                    "rule": line.rule,
                    "evidence": line.evidence,
                    "fix": line.fix,
                })).collect::<Vec<_>>(),
            })))
            .collect::<Vec<_>>(),
        "profile": outcome.profile.as_ref().map(|profile| json!({
            "samples": profile.samples(),
            "matched": profile.matched(),
            "unmatched": profile.unmatched(),
        })),
    })
}

pub fn plan(outcome: &ScanOutcome, budget: Option<u32>, limit: usize) -> Value {
    let ranked = crate::plan::rank(outcome, budget, limit);
    let (before, after) = crate::plan::simulated(outcome, &ranked);
    json!({
        "healthBefore": before,
        "healthAfter": after,
        "items": ranked
            .iter()
            .map(|item| {
                let spec = item.finding.rule.spec();
                json!({
                    "rule": spec.slug,
                    "family": spec.category.slug(),
                    "file": item.finding.location.file,
                    "line": item.finding.location.line,
                    "effort": item.effort,
                    "healthAfter": item.health_after,
                    "technique": spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>(),
                    "autofixable": spec.autofix.is_automatic(),
                })
            })
            .collect::<Vec<_>>(),
    })
}

/// A fix run as data. `before` and `after` are carried only for a preview, because a
/// write already put `after` on disk and repeating every file's text would make the
/// reply as large as the project.
pub fn fixes(outcome: &crate::fix::Outcome, with_text: bool) -> Value {
    let fixes: Vec<Value> = outcome
        .fixes()
        .map(|fix| {
            let index = LineIndex::new(&fix.before);
            let edits: Vec<Value> = fix
                .applied
                .iter()
                .map(|edit| self::edit(edit, &fix.before, &index))
                .collect();
            let mut value = json!({ "file": fix.file, "edits": edits });
            if with_text {
                value["before"] = json!(fix.before);
                value["after"] = json!(fix.after);
            }
            value
        })
        .collect();
    json!({
        "fixes": fixes,
        "withheld": outcome
            .withheld()
            .map(|withholding| json!({
                "file": withholding.file,
                "edits": withholding.edits,
                "reason": withholding.reason.to_string(),
            }))
            .collect::<Vec<_>>(),
        "summary": {
            "files": outcome.fixes().count(),
            "applied": outcome.applied(),
            "withheld": outcome.withheld().count(),
            "rounds": outcome.rounds.len(),
            "healthBefore": outcome.health_before,
            "healthAfter": outcome.health_after,
            "written": outcome.written,
        },
    })
}

pub fn edit(edit: &rqx_core::edit::Edit, before: &str, index: &LineIndex<'_>) -> Value {
    let (line, column) = index.locate(edit.span.start);
    json!({
        "rule": edit.rule.slug(),
        "kind": edit.rule.spec().autofix.slug(),
        "line": line,
        "column": column,
        "start": edit.span.start,
        "end": edit.span.end,
        "removed": Span::new(edit.span.start, edit.span.end).text(before),
        "replacement": edit.replacement,
    })
}

pub fn verified(verified: &crate::verify::Verified) -> Value {
    json!({
        "base": verified.base,
        "passed": verified.passed(),
        "healthBefore": verified.health_before,
        "healthAfter": verified.health_after,
        "unmet": verified.unmet,
        "changes": verified
            .changes
            .iter()
            .filter(|change| change.outcome.slug() != "unchanged")
            .map(|change| json!({
                "outcome": change.outcome.slug(),
                "rule": change.slug,
                "subject": change.subject,
                "before": change.before.as_ref().map(ToString::to_string),
                "after": change.after.as_ref().map(ToString::to_string),
                "technique": change.technique.slug(),
            }))
            .collect::<Vec<_>>(),
    })
}

/// File names rather than paths for the sources, the way `Row::source` already names
/// provenance: a page has no use for the server's directory layout.
pub fn config(settings: &Resolved) -> Value {
    json!({
        "level": settings.level().slug(),
        "sources": settings
            .sources
            .iter()
            .filter_map(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
            .collect::<Vec<_>>(),
        "settings": setting_rows(settings)
            .iter()
            .map(|row| json!({
                "table": row.table,
                "name": row.name,
                "value": row.value,
                "isDefault": row.is_default,
                "source": row.source(settings),
            }))
            .collect::<Vec<_>>(),
    })
}

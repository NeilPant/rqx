use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;
use rqx_core::finding::Finding;
use rqx_core::span::Span;
use serde_json::json;

use crate::brand::{Ink, Palette, paint, severity_ink};
use crate::config::Resolved;
use crate::report::plural;
use crate::scan::ScanOutcome;

/// Enough of a file to say what it is, for a finding whose span is the file itself.
const OPENING_LINES: usize = 12;

/// A finding rqx will not touch, with everything needed to act on it without going
/// back to the source: where it is, what the code says there, what decided it and
/// what the refactoring is called.
struct Item<'a> {
    finding: &'a Finding,
    source: String,
    commits: u32,
    importers: u32,
}

fn collect<'a>(outcome: &'a ScanOutcome, settings: &Resolved) -> Vec<Item<'a>> {
    let mut sources: BTreeMap<&str, Option<String>> = BTreeMap::new();
    let mut items = Vec::new();

    for finding in &outcome.findings {
        let path = outcome.root.join(&*finding.location.file);
        if settings.for_file(&path).fix.admits(finding) {
            continue;
        }
        let file = &*finding.location.file;
        let text = sources
            .entry(file)
            .or_insert_with(|| std::fs::read_to_string(&path).ok());
        let impact = outcome.impact.get(file);
        items.push(Item {
            finding,
            source: text.as_deref().map_or_else(String::new, |source| {
                let span = Span::new(finding.location.start, finding.location.end);
                // A finding about the whole file has no span to quote, so the opening
                // lines stand in: an agent asked to act on it needs to see what the
                // file is before it can decide anything.
                if span.is_empty() {
                    source
                        .lines()
                        .take(OPENING_LINES)
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    span.text(source).to_owned()
                }
            }),
            commits: impact.commits,
            importers: impact.importers,
        });
    }
    items
}

pub fn render(
    outcome: &ScanOutcome,
    settings: &Resolved,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let items = collect(outcome, settings);
    let dim = palette.style(Ink::Muted);

    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.bold(Ink::Text), "Needs a decision"),
        paint(dim, plural(items.len() as u32, "finding", "findings"))
    )?;
    writeln!(
        out,
        "  {}\n",
        paint(
            dim,
            "rqx names the refactoring; choosing the shape is the work"
        )
    )?;

    for item in &items {
        let spec = item.finding.rule.spec();
        writeln!(
            out,
            "   {} {}  {}",
            paint(
                palette.style(severity_ink(item.finding.severity)),
                "\u{25cf}"
            ),
            paint(palette.style(Ink::Accent), spec.slug),
            paint(dim, &item.finding.location)
        )?;
        writeln!(out, "     {}", item.finding.message())?;
        writeln!(out, "     {} {}", paint(dim, "Why: "), spec.why)?;
        writeln!(
            out,
            "     {} {} \u{2014} {}",
            paint(dim, "Fix: "),
            paint(
                palette.style(Ink::Good),
                spec.fixes
                    .iter()
                    .map(|fix| fix.name())
                    .collect::<Vec<_>>()
                    .join(" / ")
            ),
            item.finding.fix_hint
        )?;
        if item.commits > 0 || item.importers > 0 {
            writeln!(
                out,
                "     {} changed {} in the window, {} importing",
                paint(dim, "Cost:"),
                plural(item.commits, "time", "times"),
                plural(item.importers, "module", "modules")
            )?;
        }
        writeln!(out)?;
    }
    Ok(())
}

pub fn json(outcome: &ScanOutcome, settings: &Resolved, out: &mut dyn Write) -> Result<()> {
    for item in collect(outcome, settings) {
        let spec = item.finding.rule.spec();
        writeln!(
            out,
            "{}",
            json!({
                "kind": "decision",
                "rule": spec.slug,
                "family": spec.category.slug(),
                "file": item.finding.location.file,
                "line": item.finding.location.line,
                "column": item.finding.location.column,
                "endLine": item.finding.location.end_line,
                "start": item.finding.location.start,
                "end": item.finding.location.end,
                "source": item.source,
                "severity": item.finding.severity.label(),
                "confidence": item.finding.confidence.label(),
                "evidence": item.finding.evidence,
                "metric": spec.metric,
                "why": spec.why,
                "technique": spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>(),
                "hint": item.finding.fix_hint,
                "impact": { "commits": item.commits, "importers": item.importers },
            })
        )?;
    }
    Ok(())
}

/// The same brief as markdown a person can paste, rather than a report to read.
pub fn markdown(outcome: &ScanOutcome, settings: &Resolved, out: &mut dyn Write) -> Result<()> {
    let items = collect(outcome, settings);
    writeln!(out, "# Findings that need a decision\n")?;
    writeln!(
        out,
        "{} rqx will not fix mechanically. Each names what decided it and the \
         refactoring that answers it; choosing the shape is the work.\n",
        plural(items.len() as u32, "finding", "findings")
    )?;

    for item in &items {
        let spec = item.finding.rule.spec();
        writeln!(
            out,
            "## `{}` \u{2014} {}:{}\n",
            spec.slug, item.finding.location.file, item.finding.location.line
        )?;
        writeln!(out, "{}\n", spec.why)?;
        writeln!(out, "- **Measured** {}", item.finding.message())?;
        writeln!(
            out,
            "- **Refactoring** {}",
            spec.fixes
                .iter()
                .map(|fix| fix.name())
                .collect::<Vec<_>>()
                .join(" / ")
        )?;
        writeln!(out, "- **Hint** {}", item.finding.fix_hint)?;
        if item.commits > 0 || item.importers > 0 {
            writeln!(
                out,
                "- **Cost** changed {} in the window, {} importing",
                plural(item.commits, "time", "times"),
                plural(item.importers, "module", "modules")
            )?;
        }
        if !item.source.is_empty() {
            writeln!(out, "\n```\n{}\n```", item.source.trim_end())?;
        }
        writeln!(out)?;
    }
    Ok(())
}

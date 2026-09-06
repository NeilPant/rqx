use std::io::Write;

use anyhow::Result;
use serde_json::json;

use crate::brand::{Ink, Palette, paint, score_ink, severity_ink};
use crate::plan::{Item, simulated};
use crate::report::plural;
use crate::scan::ScanOutcome;

const LABEL_WIDTH: usize = 10;

pub fn render(
    outcome: &ScanOutcome,
    plan: &[Item<'_>],
    budget: Option<u32>,
    simulate: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    let (before, after) = simulated(outcome, plan);

    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.bold(Ink::Text), "The plan"),
        paint(
            dim,
            match budget {
                Some(points) => format!(
                    "{} in {} of effort",
                    plural(plan.len() as u32, "item", "items"),
                    plural(points, "point", "points")
                ),
                None => plural(plan.len() as u32, "item", "items"),
            }
        )
    )?;
    writeln!(
        out,
        "  {}\n",
        paint(
            dim,
            "worth the most per unit of work, in the order to do it"
        )
    )?;

    for (position, item) in plan.iter().enumerate() {
        let spec = item.finding.rule.spec();
        writeln!(
            out,
            "  {} {} {}  {}",
            paint(dim, format!("{:>3}.", position + 1)),
            paint(
                palette.style(severity_ink(item.finding.severity)),
                "\u{25cf}"
            ),
            paint(palette.style(Ink::Accent), spec.slug),
            paint(dim, &item.finding.location)
        )?;
        writeln!(
            out,
            "       {} {}",
            paint(
                palette.style(Ink::Good),
                spec.fixes
                    .iter()
                    .map(|fix| fix.name())
                    .collect::<Vec<_>>()
                    .join(" / ")
            ),
            paint(
                dim,
                format!(
                    "\u{b7} {} \u{b7} health {}",
                    plural(u32::from(item.effort), "point", "points"),
                    item.health_after
                )
            )
        )?;
    }

    writeln!(out)?;
    let effort: u32 = plan.iter().map(|item| u32::from(item.effort)).sum();
    writeln!(
        out,
        "  {}{}",
        paint(dim, format!("{:LABEL_WIDTH$}", "Effort")),
        plural(effort, "point", "points")
    )?;
    writeln!(
        out,
        "  {}{}  {}",
        paint(dim, format!("{:LABEL_WIDTH$}", "Health")),
        paint(
            palette.bold(score_ink(after)),
            format!("{before} \u{2192} {after}")
        ),
        paint(dim, format!("(+{})", after.saturating_sub(before)))
    )?;

    if simulate {
        writeln!(out)?;
        write_grades(outcome, plan, palette, out)?;
    }

    writeln!(
        out,
        "\n  {}\n",
        paint(
            dim,
            "`rqx fix --write` does the mechanical ones; the rest are decisions."
        )
    )?;
    Ok(())
}

/// What the plan does to the shape of the project, not only its score: a run that
/// lifts two files out of `critical` is a different result from one that shaves a
/// point off every file.
fn write_grades(
    outcome: &ScanOutcome,
    plan: &[Item<'_>],
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    use rqx_core::Cleanliness;

    let after = outcome.counts.without(plan.iter().map(|item| item.finding));
    let dim = palette.style(Ink::Muted);

    writeln!(out, "  {}", paint(dim, "Grades"))?;
    for grade in Cleanliness::ALL {
        let count = |counts: &rqx_core::score::FindingCounts| {
            outcome
                .lines_by_file
                .iter()
                .filter(|(file, lines)| {
                    Cleanliness::of(
                        counts.file_score(file, **lines),
                        counts.findings_in(file).into(),
                    ) == grade
                })
                .count()
        };
        let (was, is) = (count(&outcome.counts), count(&after));
        if was == 0 && is == 0 {
            continue;
        }
        let arrow = if was == is {
            format!("{is}")
        } else {
            format!("{was} \u{2192} {is}")
        };
        writeln!(
            out,
            "  {:>12}  {}",
            paint(palette.style(crate::brand::grade_ink(grade)), grade.label()),
            arrow
        )?;
    }
    Ok(())
}

pub fn json(outcome: &ScanOutcome, plan: &[Item<'_>], out: &mut dyn Write) -> Result<()> {
    let (before, after) = simulated(outcome, plan);
    for (position, item) in plan.iter().enumerate() {
        let spec = item.finding.rule.spec();
        let impact = outcome.impact.get(&item.finding.location.file);
        writeln!(
            out,
            "{}",
            json!({
                "kind": "item",
                "position": position + 1,
                "rule": spec.slug,
                "family": spec.category.slug(),
                "file": item.finding.location.file,
                "line": item.finding.location.line,
                "technique": spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>(),
                "autofixable": spec.autofix.is_automatic(),
                "effort": item.effort,
                "value": item.value,
                "healthAfter": item.health_after,
                "why": spec.why,
                "hint": item.finding.fix_hint,
                "impact": { "commits": impact.commits, "importers": impact.importers },
            })
        )?;
    }
    writeln!(
        out,
        "{}",
        json!({
            "kind": "summary",
            "items": plan.len(),
            "effort": plan.iter().map(|item| u32::from(item.effort)).sum::<u32>(),
            "healthBefore": before,
            "healthAfter": after,
        })
    )?;
    Ok(())
}

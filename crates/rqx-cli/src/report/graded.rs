use std::io::Write;

use anyhow::Result;
use serde_json::json;

use crate::brand::{Ink, Palette, paint, score_ink};
use crate::report::plural;
use crate::verify::{Change, Consistency, Outcome, Verified};

const LABEL_WIDTH: usize = 12;

fn ink(outcome: &Outcome) -> Ink {
    match outcome {
        Outcome::Resolved | Outcome::Improved { .. } => Ink::Good,
        Outcome::Introduced | Outcome::Regressed { .. } => Ink::Danger,
        Outcome::Relocated { .. } => Ink::Warn,
        Outcome::Unchanged => Ink::Muted,
    }
}

fn detail(change: &Change) -> String {
    match &change.outcome {
        Outcome::Improved { from, to } | Outcome::Regressed { from, to } => {
            format!("{from} \u{2192} {to}")
        }
        Outcome::Relocated { to, .. } => format!("now at {to}"),
        _ => change
            .before
            .as_ref()
            .or(change.after.as_ref())
            .map_or_else(String::new, ToString::to_string),
    }
}

pub fn render(verified: &Verified, palette: Palette, out: &mut dyn Write) -> Result<()> {
    let dim = palette.style(Ink::Muted);

    writeln!(
        out,
        "\n  {} against {}",
        paint(palette.bold(Ink::Text), "Verifying the refactoring"),
        paint(palette.style(Ink::Accent), &verified.base)
    )?;
    writeln!(
        out,
        "  {}\n",
        paint(dim, "what rqx named, and what actually happened to it")
    )?;

    // Unchanged findings are the bulk of any diff and say nothing about the work.
    let moved: Vec<&Change> = verified
        .changes
        .iter()
        .filter(|change| !matches!(change.outcome, Outcome::Unchanged))
        .collect();

    for change in &moved {
        writeln!(
            out,
            "   {} {}  {}",
            paint(palette.style(ink(&change.outcome)), "\u{25cf}"),
            paint(
                palette.style(ink(&change.outcome)),
                format!("{:11}", change.outcome.slug())
            ),
            paint(palette.style(Ink::Accent), &change.slug)
        )?;
        writeln!(
            out,
            "       {} {}",
            paint(dim, format!("{:9}", "subject")),
            change.subject
        )?;
        writeln!(
            out,
            "       {} {}",
            paint(dim, format!("{:9}", "where")),
            paint(dim, detail(change))
        )?;
        if change.technique != Consistency::NotInferable {
            let technique = change
                .rule
                .spec()
                .fixes
                .iter()
                .map(|fix| fix.name())
                .collect::<Vec<_>>()
                .join(" / ");
            writeln!(
                out,
                "       {} {} {}",
                paint(dim, format!("{:9}", "as")),
                paint(
                    palette.style(if change.technique == Consistency::Consistent {
                        Ink::Good
                    } else {
                        Ink::Warn
                    }),
                    change.technique.slug()
                ),
                paint(dim, format!("with {technique}"))
            )?;
        }
        writeln!(out)?;
    }

    if moved.is_empty() {
        writeln!(out, "  {}\n", paint(dim, "Nothing moved."))?;
    }

    let row = |out: &mut dyn Write, label: &str, value: String, ink: Option<Ink>| -> Result<()> {
        let painted = match ink {
            Some(ink) => paint(palette.bold(ink), value),
            None => value,
        };
        writeln!(
            out,
            "  {}{painted}",
            paint(dim, format!("{label:LABEL_WIDTH$}"))
        )?;
        Ok(())
    };

    row(
        out,
        "Resolved",
        verified.counted(&Outcome::Resolved).to_string(),
        Some(Ink::Good),
    )?;
    let improved = verified.counted(&Outcome::Improved { from: 0, to: 0 });
    if improved > 0 {
        row(out, "Improved", improved.to_string(), Some(Ink::Good))?;
    }
    let relocated = verified.counted(&Outcome::Relocated {
        to: rqx_core::finding::SourceRange {
            file: "".into(),
            start: 0,
            end: 0,
            line: 0,
            column: 0,
            end_line: 0,
        },
    });
    if relocated > 0 {
        row(out, "Relocated", relocated.to_string(), Some(Ink::Warn))?;
    }
    let regressed = verified.counted(&Outcome::Regressed { from: 0, to: 0 });
    if regressed > 0 {
        row(out, "Regressed", regressed.to_string(), Some(Ink::Danger))?;
    }
    row(
        out,
        "Introduced",
        verified.counted(&Outcome::Introduced).to_string(),
        Some(Ink::Danger),
    )?;
    writeln!(
        out,
        "  {}{}  {}",
        paint(dim, format!("{:LABEL_WIDTH$}", "Health")),
        paint(
            palette.bold(score_ink(verified.health_after)),
            format!(
                "{} \u{2192} {}",
                verified.health_before, verified.health_after
            )
        ),
        paint(dim, "(whole project)")
    )?;

    writeln!(out)?;
    if !verified.unmet.is_empty() {
        writeln!(
            out,
            "  {}  {}",
            paint(palette.badge(Ink::Danger), " NOT DONE "),
            paint(
                dim,
                plural(verified.unmet.len() as u32, "expectation", "expectations")
            )
        )?;
        for wanted in &verified.unmet {
            writeln!(out, "    {}", paint(palette.style(Ink::Accent), wanted))?;
        }
        writeln!(out)?;
    }

    let verdict = if verified.passed() {
        paint(palette.style(Ink::Good), "\u{2713} the refactoring landed")
    } else {
        paint(
            palette.style(Ink::Danger),
            "\u{2716} the diff moved problems rather than removing them",
        )
    };
    writeln!(out, "  {verdict}\n")?;
    Ok(())
}

pub fn json(verified: &Verified, out: &mut dyn Write) -> Result<()> {
    for change in &verified.changes {
        writeln!(
            out,
            "{}",
            json!({
                "kind": "change",
                "outcome": change.outcome.slug(),
                "rule": change.slug,
                "subject": change.subject,
                "before": change.before.as_ref().map(ToString::to_string),
                "after": change.after.as_ref().map(ToString::to_string),
                "technique": change.technique.slug(),
                "detail": detail(change),
            })
        )?;
    }
    writeln!(
        out,
        "{}",
        json!({
            "kind": "summary",
            "base": verified.base,
            "resolved": verified.counted(&Outcome::Resolved),
            "introduced": verified.counted(&Outcome::Introduced),
            "healthBefore": verified.health_before,
            "healthAfter": verified.health_after,
            "unmet": verified.unmet,
            "passed": verified.passed(),
        })
    )?;
    Ok(())
}

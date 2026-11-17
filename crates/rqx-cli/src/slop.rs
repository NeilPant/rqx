//! Which files read as written rather than read.

use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;
use rqx_core::slop;

use crate::brand::{Ink, Palette, paint};
use crate::scan::ScanOutcome;

/// A file the slop signals single out, with the score that ranked it.
pub struct Suspect {
    pub file: String,
    pub score: u32,
    pub statements: u32,
    /// Which rules put it here, worst first, so the score can be checked rather than
    /// taken on trust.
    pub signals: Vec<(String, u32)>,
}

pub fn suspects(outcome: &ScanOutcome) -> Vec<Suspect> {
    let mut weighted: BTreeMap<&str, BTreeMap<String, u32>> = BTreeMap::new();
    for finding in &outcome.findings {
        let weight = slop::weight(finding.rule);
        if weight == 0 {
            continue;
        }
        *weighted
            .entry(&finding.location.file)
            .or_default()
            .entry(finding.slug().to_owned())
            .or_default() += weight;
    }

    let mut suspects: Vec<Suspect> = weighted
        .into_iter()
        .map(|(file, by_rule)| {
            let statements = outcome.shape_by_file.get(file).map_or(0, |shape| {
                shape.functions.iter().map(|f| f.statements).sum()
            });
            let total: u32 = by_rule.values().sum();
            let mut signals: Vec<(String, u32)> = by_rule.into_iter().collect();
            signals.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            Suspect {
                file: file.to_owned(),
                score: slop::score(total, statements),
                statements,
                signals,
            }
        })
        .collect();
    suspects.sort_by(|a, b| b.score.cmp(&a.score).then(a.file.cmp(&b.file)));
    suspects
}

fn ink(score: u32) -> Ink {
    match score {
        70..=100 => Ink::Danger,
        40..=69 => Ink::Warn,
        _ => Ink::Muted,
    }
}

pub fn render(
    outcome: &ScanOutcome,
    top: usize,
    least: u32,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    let suspects: Vec<Suspect> = suspects(outcome)
        .into_iter()
        .filter(|suspect| suspect.score >= least)
        .collect();

    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.bold(Ink::Text), "Reads as unread"),
        paint(
            dim,
            format!(
                "{} \u{b7} heuristic, and never a gate unless you ask",
                crate::report::plural(suspects.len() as u32, "file", "files")
            )
        )
    )?;

    if suspects.is_empty() {
        writeln!(
            out,
            "\n  {}\n",
            paint(dim, "nothing in this project reads that way")
        )?;
        return Ok(());
    }

    for suspect in suspects.iter().take(top) {
        writeln!(
            out,
            "\n    {} {}  {}",
            paint(
                palette.style(ink(suspect.score)),
                format!("{:>3}", suspect.score)
            ),
            suspect.file,
            paint(dim, format!("{} statements", suspect.statements))
        )?;
        writeln!(
            out,
            "        {}",
            paint(
                dim,
                suspect
                    .signals
                    .iter()
                    .map(|(rule, weight)| format!("{rule} ({weight})"))
                    .collect::<Vec<_>>()
                    .join("  \u{b7}  ")
            )
        )?;
    }

    writeln!(
        out,
        "\n    {}\n",
        paint(
            dim,
            "the number is a suspicion, not a verdict \u{2014} read the file before \
             deciding what it needs"
        )
    )?;
    Ok(())
}

pub fn json(outcome: &ScanOutcome, top: usize, least: u32, out: &mut dyn Write) -> Result<()> {
    for suspect in suspects(outcome)
        .into_iter()
        .filter(|suspect| suspect.score >= least)
        .take(top)
    {
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "file": suspect.file,
                "score": suspect.score,
                "statements": suspect.statements,
                "signals": suspect
                    .signals
                    .iter()
                    .map(|(rule, weight)| serde_json::json!({ "rule": rule, "weight": weight }))
                    .collect::<Vec<_>>(),
            })
        )?;
    }
    Ok(())
}

use std::io::Write;
use std::path::Path;

use anyhow::Result;
use serde_json::json;

use crate::brand::{Ink, Palette, paint, severity_ink};
use crate::report::plural;
use crate::scan::ScanOutcome;

pub fn render(
    root: &Path,
    outcome: &ScanOutcome,
    expired_only: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    let today = rqx_core::suppress::today();
    let waived: Vec<&rqx_core::suppress::Suppressed> = outcome
        .suppressed
        .iter()
        .filter(|waived| !expired_only || is_expiring(waived, &today))
        .collect();

    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.bold(Ink::Text), "Waivers"),
        paint(dim, plural(waived.len() as u32, "finding", "findings"))
    )?;

    if waived.is_empty() {
        writeln!(
            out,
            "\n  {}\n",
            paint(dim, "Nothing in this project is waived.")
        )?;
        return Ok(());
    }
    writeln!(out)?;

    for waived in &waived {
        let finding = &waived.finding;
        writeln!(
            out,
            "  {} {}  {}",
            paint(palette.style(severity_ink(finding.severity)), "\u{25cf}"),
            paint(palette.style(Ink::Text), finding.slug()),
            paint(
                dim,
                format!("{}:{}", finding.location.file, finding.location.line)
            )
        )?;
        writeln!(
            out,
            "    {}",
            waived.reason.as_deref().unwrap_or("no reason given")
        )?;

        let mut aside = Vec::new();
        if let Some(until) = &waived.until {
            aside.push(format!("until {until}"));
        }
        // Read one line at a time rather than blaming every file up front: a project
        // waives a handful of findings and blaming all of them is the slower way.
        if let Some((author, when)) =
            crate::history::blame_line(root, &finding.location.file, waived.at.line)
        {
            aside.push(format!("{author}, {when}"));
        }
        if !aside.is_empty() {
            writeln!(out, "    {}", paint(dim, aside.join("  \u{b7}  ")))?;
        }
        writeln!(out)?;
    }
    Ok(())
}

pub fn json(root: &Path, outcome: &ScanOutcome, out: &mut dyn Write) -> Result<()> {
    for waived in &outcome.suppressed {
        let blame = crate::history::blame_line(root, &waived.finding.location.file, waived.at.line);
        writeln!(
            out,
            "{}",
            json!({
                "rule": waived.finding.slug(),
                "family": waived.finding.category().slug(),
                "file": waived.finding.location.file,
                "line": waived.finding.location.line,
                "severity": waived.finding.severity.label(),
                "reason": waived.reason,
                "until": waived.until,
                "waivedAt": waived.at.line,
                "author": blame.as_ref().map(|(author, _)| author),
                "since": blame.as_ref().map(|(_, when)| when),
            })
        )?;
    }
    Ok(())
}

/// A waiver with a date on it is one somebody already agreed to revisit, so the
/// filter shows those rather than only the ones that have already lapsed — a lapsed
/// one is not waived at all any more and never reaches this list.
fn is_expiring(waived: &rqx_core::suppress::Suppressed, today: &str) -> bool {
    waived.until.as_deref().is_some_and(|until| until >= today)
}

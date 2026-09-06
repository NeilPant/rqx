use std::io::Write;
use std::path::Path;

use anyhow::Result;
use serde_json::json;

use crate::brand::{Ink, Palette, paint, severity_ink};
use crate::report::plural;
use crate::scan::ScanOutcome;

/// Who waived each finding, and when. One blame per file rather than one per
/// finding: a file with twelve waivers used to cost twelve processes, and every one
/// of them read the same file.
fn authors(
    root: &Path,
    waived: &[&rqx_core::suppress::Suppressed],
) -> std::collections::HashMap<(String, u32), (String, String)> {
    let mut files: Vec<std::sync::Arc<str>> = waived
        .iter()
        .map(|waived| std::sync::Arc::clone(&waived.finding.location.file))
        .collect();
    files.sort();
    files.dedup();

    let mut found = std::collections::HashMap::new();
    for (file, lines) in files.iter().zip(crate::history::blame_files(root, &files)) {
        for (line, who) in lines {
            found.insert((file.to_string(), line), who);
        }
    }
    found
}

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

    let blamed = authors(root, &waived);
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
        if let Some((author, when)) =
            blamed.get(&(finding.location.file.to_string(), waived.at.line))
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
    let every: Vec<&rqx_core::suppress::Suppressed> = outcome.suppressed.iter().collect();
    let blamed = authors(root, &every);
    for waived in &outcome.suppressed {
        let blame = blamed.get(&(waived.finding.location.file.to_string(), waived.at.line));
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

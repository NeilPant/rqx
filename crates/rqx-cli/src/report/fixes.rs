use std::io::Write;

use anyhow::Result;
use rqx_core::LineIndex;
use rqx_core::edit::Edit;

use crate::brand::{Ink, Palette, paint, score_ink, severity_ink};
use crate::fix::{FileFix, Outcome};
use crate::report::plural;

const LABEL_WIDTH: usize = 10;
/// Changed lines shown per edit before the middle is elided. A deletion of a whole
/// dead branch is still a reviewable fix; printing forty lines of it is not.
const MAX_LINES: usize = 8;

pub fn render(
    outcome: &Outcome,
    palette: Palette,
    invocation: &str,
    out: &mut dyn Write,
) -> Result<()> {
    for fix in outcome.fixes() {
        write_file(out, palette, fix)?;
    }
    write_withheld(out, palette, outcome)?;
    write_summary(out, palette, invocation, outcome)?;
    Ok(())
}

fn write_file(out: &mut dyn Write, palette: Palette, fix: &FileFix) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    writeln!(
        out,
        "\n {}  {}  {}",
        paint(palette.badge(Ink::Good), " FIX "),
        fix.file,
        paint(dim, plural(fix.applied.len() as u32, "fix", "fixes"))
    )?;

    let index = LineIndex::new(&fix.before);
    for edit in &fix.applied {
        write_edit(out, palette, &index, edit)?;
    }
    Ok(())
}

fn write_edit(
    out: &mut dyn Write,
    palette: Palette,
    index: &LineIndex<'_>,
    edit: &Edit,
) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    let spec = edit.rule.spec();
    let ink = severity_ink(edit.severity);
    let (line, column) = index.locate(edit.span.start);

    writeln!(
        out,
        "\n   {} {}  {}  {}",
        paint(palette.style(ink), "\u{25cf}"),
        paint(palette.style(Ink::Accent), spec.slug),
        paint(dim, format!("{line}:{column}")),
        paint(palette.style(Ink::Good), spec.autofix.slug())
    )?;

    let (last, _) = index.locate(edit.span.end.saturating_sub(1).max(edit.span.start));
    let removed: Vec<(u32, &str)> = (line..=last)
        .map(|number| (number, index.line_text(number)))
        .collect();
    write_lines(out, palette, &removed, '-', Ink::Danger)?;

    if !edit.replacement.is_empty() {
        let added: Vec<(u32, &str)> = edit
            .replacement
            .lines()
            .enumerate()
            .map(|(offset, text)| (line + offset as u32, text))
            .collect();
        write_lines(out, palette, &added, '+', Ink::Good)?;
    }

    writeln!(out, "     {} {}", paint(dim, "Why: "), spec.why)?;
    Ok(())
}

fn write_lines(
    out: &mut dyn Write,
    palette: Palette,
    lines: &[(u32, &str)],
    sign: char,
    ink: Ink,
) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    let elided = lines.len().saturating_sub(MAX_LINES);
    for (number, text) in lines.iter().take(MAX_LINES) {
        writeln!(
            out,
            "{} {} {}",
            paint(dim, format!("{number:>6}")),
            paint(dim, "\u{2502}"),
            paint(palette.style(ink), format!("{sign} {text}"))
        )?;
    }
    if elided > 0 {
        writeln!(
            out,
            "{:6} {} {}",
            "",
            paint(dim, "\u{22ee}"),
            paint(dim, format!("{elided} more lines"))
        )?;
    }
    Ok(())
}

fn write_withheld(out: &mut dyn Write, palette: Palette, outcome: &Outcome) -> Result<()> {
    // Named rather than counted: a silent refusal reads as a clean bill of health.
    for withholding in outcome.withheld() {
        writeln!(
            out,
            "\n {}  {}  {}",
            paint(palette.badge(Ink::Warn), " HELD "),
            withholding.file,
            paint(
                palette.style(Ink::Muted),
                plural(withholding.edits as u32, "edit", "edits")
            )
        )?;
        writeln!(
            out,
            "     {} {}",
            paint(palette.style(Ink::Muted), "Reason:"),
            withholding.reason
        )?;
    }

    let introduced: Vec<_> = outcome.introduced().collect();
    if !introduced.is_empty() {
        writeln!(
            out,
            "\n {}  {}",
            paint(palette.badge(Ink::Warn), " NEW "),
            paint(
                palette.style(Ink::Muted),
                "project-wide rules the written files changed the answer for"
            )
        )?;
        for finding in introduced {
            writeln!(
                out,
                "     {}  {}",
                paint(palette.style(Ink::Accent), finding.slug()),
                paint(palette.style(Ink::Muted), &finding.location)
            )?;
        }
    }
    Ok(())
}

fn write_summary(
    out: &mut dyn Write,
    palette: Palette,
    invocation: &str,
    outcome: &Outcome,
) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    let files = outcome.fixes().count();
    let scanned = outcome
        .rounds
        .first()
        .map_or(0, |round| round.files_scanned);
    let withheld = outcome.withheld().count();
    let undecided = outcome.rounds.first().map_or(0, |round| round.undecided);

    let mut rows: Vec<(&str, String)> =
        vec![("Files", format!("{files} to fix \u{b7} {scanned} scanned"))];

    let mut fixes = format!("{} ready", outcome.applied());
    if withheld > 0 {
        fixes.push_str(&format!(" \u{b7} {withheld} withheld"));
    }
    let dropped = outcome.dropped().count();
    if dropped > 0 {
        fixes.push_str(&format!(" \u{b7} {dropped} lost an overlap"));
    }
    fixes.push_str(&format!(" \u{b7} {undecided} need a decision"));
    rows.push(("Fixes", fixes));
    let below_floor = outcome.below_floor();
    if below_floor > 0 {
        rows.push((
            "Floor",
            format!(
                "{} below fix.min_confidence, not applied",
                plural(below_floor, "fix", "fixes")
            ),
        ));
    }

    writeln!(out)?;
    for (label, value) in &rows {
        writeln!(
            out,
            "  {}{value}",
            paint(dim, format!("{label:LABEL_WIDTH$}"))
        )?;
    }

    let delta = i64::from(outcome.health_after) - i64::from(outcome.health_before);
    let arrow = paint(
        palette.bold(score_ink(outcome.health_after)),
        format!(
            "{} \u{2192} {}",
            outcome.health_before, outcome.health_after
        ),
    );
    let sign = if delta >= 0 { "+" } else { "" };
    writeln!(
        out,
        "  {}{arrow}  {}",
        paint(dim, format!("{:LABEL_WIDTH$}", "Health")),
        paint(dim, format!("({sign}{delta})"))
    )?;

    if outcome.rounds.len() > 1 {
        writeln!(
            out,
            "  {}{}",
            paint(dim, format!("{:LABEL_WIDTH$}", "Rounds")),
            outcome.rounds.len()
        )?;
    }

    if outcome.applied() > 0 {
        writeln!(
            out,
            "  {}every fix re-parsed and re-analysed clean",
            paint(dim, format!("{:LABEL_WIDTH$}", "Verified"))
        )?;
    }

    writeln!(out)?;
    if outcome.written {
        writeln!(
            out,
            "  {}",
            paint(
                palette.style(Ink::Good),
                format!(
                    "Written. {}",
                    plural(outcome.applied() as u32, "fix", "fixes")
                )
            )
        )?;
    } else if outcome.applied() > 0 {
        writeln!(
            out,
            "  {}  {}",
            paint(palette.style(Ink::Text), "Nothing written."),
            paint(palette.style(Ink::Accent), invocation)
        )?;
    } else {
        writeln!(out, "  {}", paint(dim, "Nothing to fix mechanically."))?;
    }
    Ok(())
}

/// The same fixes as data, for an agent or an editor rather than a reader.
pub fn json(outcome: &Outcome, out: &mut dyn Write) -> Result<()> {
    for fix in outcome.fixes() {
        let index = LineIndex::new(&fix.before);
        let edits: Vec<serde_json::Value> = fix
            .applied
            .iter()
            .map(|edit| super::projection::edit(edit, &fix.before, &index))
            .collect();
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "kind": "fix",
                "file": fix.file,
                "edits": edits,
            })
        )?;
    }
    for withholding in outcome.withheld() {
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "kind": "withheld",
                "file": withholding.file,
                "edits": withholding.edits,
                "reason": withholding.reason.to_string(),
            })
        )?;
    }
    writeln!(
        out,
        "{}",
        serde_json::json!({
            "kind": "summary",
            "schema_version": super::SCHEMA_VERSION,
            "files": outcome.fixes().count(),
            "applied": outcome.applied(),
            "withheld": outcome.withheld().count(),
            "below_floor": outcome.below_floor(),
            "rounds": outcome.rounds.len(),
            "health_before": outcome.health_before,
            "health_after": outcome.health_after,
            "written": outcome.written,
        })
    )?;
    Ok(())
}

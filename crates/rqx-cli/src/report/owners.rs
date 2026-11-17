//! Each owner's health, as the table `rqx owners` prints and the report summaries repeat.

use std::io::Write;

use anyhow::Result;

use crate::brand::{Ink, Palette, grade_ink, paint, score_ink};
use crate::ownership::OwnerRow;
use crate::report::{Align, column};

const OWNER_WIDTH: std::ops::RangeInclusive<usize> = 8..=40;

/// The table, worst health first as the rows arrive, under a heading so it can sit in
/// the middle of a report.
pub fn render(rows: &[OwnerRow], palette: Palette, out: &mut dyn Write) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    writeln!(out, "\n  {}", paint(palette.bold(Ink::Text), "Owners"))?;
    if rows.is_empty() {
        return Ok(writeln!(
            out,
            "    {}",
            paint(dim, "no CODEOWNERS rules apply")
        )?);
    }
    let width = rows
        .iter()
        .map(|row| row.owner.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(*OWNER_WIDTH.start(), *OWNER_WIDTH.end());
    writeln!(
        out,
        "    {} {} {} {}  {}",
        column("", width, Align::Left),
        paint(dim, column("files", 6, Align::Right)),
        paint(dim, column("findings", 9, Align::Right)),
        paint(dim, column("health", 7, Align::Right)),
        paint(dim, "grade")
    )?;
    for row in rows {
        writeln!(
            out,
            "    {} {:>6} {:>9} {} {}",
            column(&row.owner, width, Align::Left),
            row.files,
            row.findings,
            paint(
                palette.style(score_ink(row.health)),
                column(&row.health.to_string(), 7, Align::Right)
            ),
            paint(palette.style(grade_ink(row.grade)), row.grade.label()),
        )?;
    }
    Ok(())
}

/// One object per owner, with the same keys the summary's `owners` object carries.
pub fn json(rows: &[OwnerRow], out: &mut dyn Write) -> Result<()> {
    for row in rows {
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "owner": row.owner,
                "files": row.files,
                "findings": row.findings,
                "health": row.health,
                "grade": row.grade.label(),
            })
        )?;
    }
    Ok(())
}

/// What a command about owners prints when there is nothing to read them from.
pub fn missing(palette: Palette, out: &mut dyn Write) -> Result<()> {
    let note = format!(
        "no CODEOWNERS file found in {}",
        crate::ownership::LOCATIONS.join(", ")
    );
    writeln!(out, "\n  {}", paint(palette.style(Ink::Muted), note))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rqx_core::Cleanliness;

    use super::*;
    use crate::brand::ColorDepth;

    fn rows() -> Vec<OwnerRow> {
        vec![
            OwnerRow {
                owner: "@web".into(),
                files: 3,
                findings: 12,
                health: 41,
                grade: Cleanliness::Critical,
            },
            OwnerRow {
                owner: "@api".into(),
                files: 8,
                findings: 0,
                health: 100,
                grade: Cleanliness::Pristine,
            },
        ]
    }

    fn text(render: impl FnOnce(&mut Vec<u8>) -> Result<()>) -> String {
        let mut out = Vec::new();
        render(&mut out).expect("rendering to memory");
        String::from_utf8(out).expect("utf-8")
    }

    #[test]
    fn the_table_lists_each_owner_with_its_numbers() {
        let table = text(|out| render(&rows(), Palette::with_depth(ColorDepth::Monochrome), out));
        assert!(table.contains("Owners"), "{table}");
        let web = table.lines().find(|line| line.contains("@web")).unwrap();
        assert!(web.contains("12") && web.contains("41") && web.contains("critical"));
    }

    #[test]
    fn an_empty_table_says_why() {
        let table = text(|out| render(&[], Palette::with_depth(ColorDepth::Monochrome), out));
        assert!(table.contains("no CODEOWNERS rules apply"), "{table}");
    }

    #[test]
    fn each_owner_is_one_json_object() {
        let lines = text(|out| json(&rows(), out));
        let first: serde_json::Value = serde_json::from_str(lines.lines().next().unwrap()).unwrap();
        assert_eq!(first["owner"], "@web");
        assert_eq!(first["grade"], "critical");
        assert_eq!(lines.lines().count(), 2);
    }
}

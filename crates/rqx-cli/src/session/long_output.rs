//! Output taller than the room above the prompt goes into scrollback folded: its first
//! rows and a line saying how many more there are. The whole of it is kept so `ctrl-o`
//! and `/open-last` can open it in the full-height viewer.

use std::io;

use ratatui::text::Line;

use super::text::thousands;
use super::view::{self, Look};
use super::viewer::Viewer;
use super::viewport::Viewport;
use super::{ansi, fullscreen};

const NOTHING_KEPT: &str =
    "There is no long output to open. Output taller than the window is kept for ctrl-o.";
/// The fewest rows a folded output keeps, for a window with little room above the prompt.
const MIN_HEAD: usize = 3;

#[derive(Default)]
pub struct LongOutput {
    /// The line that produced what is printed next, which titles the viewer.
    running: String,
    kept: Option<Kept>,
}

struct Kept {
    title: String,
    text: String,
}

impl LongOutput {
    /// Called with each line as it is sent.
    pub fn running(&mut self, line: &str) {
        line.clone_into(&mut self.running);
    }

    /// Prints a command's output into scrollback, folded when it does not fit above
    /// the prompt.
    pub fn print(&mut self, viewport: &mut Viewport, look: &Look, bytes: &[u8]) -> io::Result<()> {
        let text = String::from_utf8_lossy(bytes);
        let lines = self.fold(look, &text, viewport.width(), viewport.room());
        viewport.print(&lines)
    }

    /// The lines to print for `text`. Output that is folded is kept for the viewer, in
    /// place of whatever was kept before; output that fits is printed whole and leaves
    /// the kept output alone.
    fn fold(&mut self, look: &Look, text: &str, width: u16, room: u16) -> Vec<Line<'static>> {
        let mut lines = ansi::lines(text, width);
        let rows: Vec<String> = lines.iter().map(ToString::to_string).collect();
        let Some(head) = head_length(&rows, usize::from(room)) else {
            return lines;
        };
        let hidden = lines.len() - head;
        lines.truncate(head);
        lines.push(view::note(
            look,
            &hint(hidden, lists_findings(&self.running)),
        ));
        self.kept = Some(Kept {
            title: self.running.clone(),
            text: text.to_owned(),
        });
        lines
    }

    /// Opens the kept output in the viewer and, once it closes, redraws the session's
    /// rows, which the alternate screen left as they were.
    pub fn open(&self, viewport: &mut Viewport, look: &Look) -> io::Result<()> {
        let Some(kept) = &self.kept else {
            return viewport.print(&[view::note(look, NOTHING_KEPT)]);
        };
        let size = crossterm::terminal::size().unwrap_or((80, 24));
        let mut viewer = Viewer::new(&kept.title, &kept.text, size);
        let shown = fullscreen::show(&mut viewer, look);
        viewport.repaint()?;
        shown
    }
}

/// How many of `rows` to print when they are taller than the `room` above the prompt,
/// or `None` when they fit. The echoed command takes one row of that room, and a folded
/// output keeps half of what is left, so the command and what came before it stay in
/// view.
///
/// The cut moves up to the last blank row in that half, because every renderer puts
/// one between findings, files and sections, unless that would keep less than half of
/// the half; then it is cut at the row.
fn head_length(rows: &[String], room: usize) -> Option<usize> {
    let fits = room.saturating_sub(1);
    if rows.len() <= fits {
        return None;
    }
    let budget = (fits / 2).max(MIN_HEAD).min(rows.len() - 1);
    let blank = (1..=budget)
        .rev()
        .find(|at| rows[*at].trim().is_empty())
        .filter(|at| *at >= budget / 2);
    Some(blank.unwrap_or(budget))
}

fn hint(hidden: usize, lists_findings: bool) -> String {
    let count = thousands(u32::try_from(hidden).unwrap_or(u32::MAX));
    let mut hint = format!("\u{2026} {count} more lines \u{b7} ctrl-o to open");
    if lists_findings {
        hint.push_str(" \u{b7} /findings --rule <rule> to narrow");
    }
    hint
}

/// `/findings` and a line without a slash, which searches the findings, list findings;
/// a rule filter is how to make that list shorter.
fn lists_findings(line: &str) -> bool {
    !line.starts_with('/') || line.split_whitespace().next() == Some("/findings")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::{ColorDepth, Palette};

    fn look() -> Look {
        Look {
            palette: Palette::with_depth(ColorDepth::Monochrome),
        }
    }

    fn rows(count: usize) -> Vec<String> {
        (1..=count).map(|at| format!("row {at}")).collect()
    }

    /// Findings of five rows each, a blank row after each one.
    fn findings(count: usize) -> Vec<String> {
        (0..count)
            .flat_map(|at| {
                let mut block: Vec<String> = (0..5)
                    .map(|row| format!("finding {at} row {row}"))
                    .collect();
                block.push(String::new());
                block
            })
            .collect()
    }

    #[test]
    fn output_that_fits_above_the_prompt_is_not_folded() {
        assert_eq!(head_length(&rows(29), 30), None);
        assert_eq!(head_length(&rows(30), 30), Some(14));
    }

    #[test]
    fn a_folded_output_keeps_half_the_room_cut_at_a_row_when_nothing_is_blank() {
        assert_eq!(head_length(&rows(1575), 41), Some(20));
    }

    #[test]
    fn the_cut_moves_up_to_the_blank_row_between_findings() {
        let listed = findings(100);
        let head = head_length(&listed, 41).unwrap();
        assert_eq!(head, 17);
        assert!(listed[head].is_empty());
        assert_eq!(listed[head - 1], "finding 2 row 4");
    }

    #[test]
    fn a_blank_row_too_near_the_top_is_not_where_it_is_cut() {
        let mut listed = vec![String::new(), "header".to_owned(), String::new()];
        listed.extend(rows(100));
        assert_eq!(head_length(&listed, 41), Some(20));
    }

    #[test]
    fn a_tiny_window_still_shows_a_few_rows() {
        assert_eq!(head_length(&rows(100), 4), Some(MIN_HEAD));
    }

    #[test]
    fn the_hint_counts_what_was_left_out_and_names_the_filter_for_findings() {
        assert_eq!(
            hint(1575, false),
            "\u{2026} 1,575 more lines \u{b7} ctrl-o to open"
        );
        assert!(hint(3, true).ends_with("/findings --rule <rule> to narrow"));
        assert!(lists_findings("/findings --severity error"));
        assert!(lists_findings("dead export"));
        assert!(!lists_findings("/plan"));
        assert!(!lists_findings("/findingsx"));
    }

    #[test]
    fn a_folded_output_is_printed_short_and_kept_whole() {
        let mut output = LongOutput::default();
        output.running("/findings");
        let text: String = rows(100).iter().map(|row| format!("{row}\n")).collect();
        let printed = output.fold(&look(), &text, 80, 21);
        assert_eq!(printed.len(), 11);
        assert_eq!(printed[9].to_string(), "row 10");
        assert_eq!(
            printed[10].to_string(),
            " \u{2026} 90 more lines \u{b7} ctrl-o to open \u{b7} /findings --rule <rule> to narrow"
        );
        let kept = output.kept.as_ref().unwrap();
        assert_eq!(
            (kept.title.as_str(), kept.text.as_str()),
            ("/findings", text.as_str())
        );
    }

    #[test]
    fn short_output_is_printed_whole_and_the_kept_output_stays() {
        let mut output = LongOutput::default();
        output.running("/plan");
        let long: String = rows(100).iter().map(|row| format!("{row}\n")).collect();
        output.fold(&look(), &long, 80, 21);
        output.running("/health");
        let printed = output.fold(&look(), "one\ntwo\n", 80, 21);
        assert_eq!(printed.len(), 2);
        assert_eq!(output.kept.as_ref().unwrap().title, "/plan");
    }

    #[test]
    fn lines_wider_than_the_window_count_as_the_rows_they_wrap_to() {
        let mut output = LongOutput::default();
        let wide = format!("{}\n", "x".repeat(400));
        let printed = output.fold(&look(), &wide, 20, 11);
        assert!(printed.last().unwrap().to_string().contains("more lines"));
    }
}

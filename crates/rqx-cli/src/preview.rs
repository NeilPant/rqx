use std::ops::Range;

use rqx_core::LineIndex;
use rqx_core::finding::SourceRange;
use unicode_width::UnicodeWidthStr;

/// Offending lines shown before the middle of a long span is elided, and after.
pub const HEAD_LINES: usize = 6;
const TAIL: usize = 3;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Context,
    Offence,
}

pub struct Line {
    pub number: u32,
    pub text: String,
    /// Byte range within `text` that the finding points at. Empty on context lines.
    pub mark: Option<Range<usize>>,
    pub role: Role,
}

impl Line {
    /// Columns to skip and to underline, in display width rather than characters, so
    /// a caret still lands under the code on a line holding CJK or an emoji.
    pub fn ruler(&self) -> Option<(usize, usize)> {
        let mark = self.mark.clone()?;
        let lead = self.text.get(..mark.start)?.width();
        let width = self.text.get(mark.clone())?.width().max(1);
        Some((lead, width))
    }
}

pub struct Preview {
    pub lines: Vec<Line>,
    /// Offending lines dropped from the middle, so a large class does not print
    /// itself in full.
    pub elided: u32,
}

fn mark_of(text: &str, from: Option<u32>, to: Option<u32>) -> Option<Range<usize>> {
    let column_to_byte = |column: u32| -> Option<usize> {
        let index = (column as usize).checked_sub(1)?;
        text.char_indices()
            .nth(index)
            .map(|(at, _)| at)
            .or(Some(text.len()))
    };
    let start = from.map_or(Some(0), column_to_byte)?;
    let end = to.map_or(Some(text.len()), column_to_byte)?;
    (start < end).then(|| start..end.min(text.len()))
}

/// The whole span, not its first line, with the offending columns marked on each.
/// A finding whose span is the file — the synthetic `(0, 0)` the module rules use —
/// still gets a preview, because printing nothing says less than showing the top of
/// the file it is about.
pub fn of(index: &LineIndex<'_>, range: &SourceRange, context: u32) -> Option<Preview> {
    let total = index.line_count();
    if total == 0 {
        return None;
    }
    let whole_file = range.line == 0;
    let first = if whole_file { 1 } else { range.line.max(1) };
    let last = if whole_file {
        first
    } else {
        range.end_line.max(first).min(total)
    };

    let (end_line, end_column) = index.locate(range.end);
    let mut lines = Vec::new();
    let mut elided = 0;

    for number in first.saturating_sub(context).max(1)..first {
        push(&mut lines, index, number, None, Role::Context);
    }

    let offending: Vec<u32> = (first..=last).collect();
    for (position, number) in offending.iter().copied().enumerate() {
        if offending.len() > HEAD_LINES + TAIL + 1 && position == HEAD_LINES {
            elided = (offending.len() - HEAD_LINES - TAIL) as u32;
        }
        if elided > 0 && position >= HEAD_LINES && position < offending.len() - TAIL {
            continue;
        }
        let text = index.line_text(number);
        let mark = if whole_file {
            None
        } else {
            let from = (number == first).then_some(range.column);
            let to = (number == end_line && end_line >= first).then_some(end_column);
            mark_of(text, from, to)
        };
        push(&mut lines, index, number, mark, Role::Offence);
    }

    for number in (last + 1)..=(last + context).min(total) {
        push(&mut lines, index, number, None, Role::Context);
    }

    lines
        .is_empty()
        .then_some(())
        .map_or(Some(Preview { lines, elided }), |()| None)
}

fn push(
    lines: &mut Vec<Line>,
    index: &LineIndex<'_>,
    number: u32,
    mark: Option<Range<usize>>,
    role: Role,
) {
    let text = index.line_text(number).trim_end().to_owned();
    let mark = mark.filter(|range| range.end <= text.len());
    lines.push(Line {
        number,
        text,
        mark,
        role,
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn range(
        source: &str,
        line: u32,
        column: u32,
        end_line: u32,
        start: u32,
        end: u32,
    ) -> SourceRange {
        let _ = source;
        SourceRange {
            file: Arc::from("a.ts"),
            start,
            end,
            line,
            column,
            end_line,
        }
    }

    #[test]
    fn a_span_across_lines_marks_every_line_it_covers() {
        let source = "one\ntwo\nthree\nfour\nfive\n";
        let index = LineIndex::new(source);
        let preview = of(&index, &range(source, 2, 1, 4, 4, 18), 0).unwrap();
        assert_eq!(preview.lines.len(), 3);
        assert!(preview.lines.iter().all(|line| line.role == Role::Offence));
        assert!(
            preview.lines.iter().all(|line| line.mark.is_some()),
            "every line carries a mark"
        );
    }

    #[test]
    fn context_lines_surround_the_offence_without_being_marked() {
        let source = "a\nb\nc\nd\ne\n";
        let index = LineIndex::new(source);
        let preview = of(&index, &range(source, 3, 1, 3, 4, 5), 1).unwrap();
        assert_eq!(preview.lines.len(), 3);
        assert_eq!(preview.lines[0].role, Role::Context);
        assert_eq!(preview.lines[1].role, Role::Offence);
        assert!(preview.lines[0].mark.is_none());
    }

    #[test]
    fn a_whole_file_finding_still_previews_the_top_of_the_file() {
        let source = "first\nsecond\n";
        let index = LineIndex::new(source);
        let preview = of(&index, &range(source, 0, 0, 0, 0, 0), 1).unwrap();
        assert_eq!(preview.lines.first().map(|line| line.number), Some(1));
        assert!(
            preview.lines[0].mark.is_none(),
            "the file is the subject, not a column range"
        );
    }

    #[test]
    fn a_long_span_elides_its_middle_instead_of_printing_itself() {
        let source: String = (1..=40).map(|n| format!("line {n}\n")).collect();
        let index = LineIndex::new(&source);
        let preview = of(&index, &range(&source, 1, 1, 40, 0, source.len() as u32), 0).unwrap();
        assert_eq!(preview.elided, 40 - (HEAD_LINES + TAIL) as u32);
        assert_eq!(preview.lines.len(), HEAD_LINES + TAIL);
    }

    /// A caret counted in characters lands short on a line holding wide glyphs, which
    /// is the one place a preview must not be approximate.
    #[test]
    fn the_ruler_is_measured_in_display_width() {
        let source = "const 名前 = compute();\n";
        let index = LineIndex::new(source);
        let preview = of(&index, &range(source, 1, 12, 1, 14, 21), 0).unwrap();
        let (lead, width) = preview.lines[0].ruler().unwrap();
        assert_eq!(lead, 13, "two wide glyphs count as four columns, not two");
        assert!(width >= 1);
    }
}

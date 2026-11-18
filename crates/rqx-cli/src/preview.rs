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
    /// Source lines left out between this line and the next one shown, so a reporter
    /// can say so where the gap is rather than at a position it has to work out.
    pub gap_after: u32,
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
            if let Some(shown) = lines.last_mut() {
                shown.gap_after = elided;
            }
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
        gap_after: 0,
    });
}

/// What the terminal shows for a finding: only the focus is marked, and the statement
/// around it is shown as far as it helps to place the focus. A finding with no focus
/// whose location runs over several lines shows the first of them and says how many
/// were left out, so a long statement never prints as a wall of carets.
pub fn of_finding(
    index: &LineIndex<'_>,
    location: &SourceRange,
    focus: Option<&SourceRange>,
    context: u32,
) -> Option<Preview> {
    let total = index.line_count();
    if total == 0 {
        return None;
    }
    if location.line == 0 {
        return of(index, location, context);
    }
    let first = location.line.clamp(1, total);
    let last = location.end_line.clamp(first, total);
    // A focus outside the location, such as a name above a body, is shown on its own.
    if let Some(outside) = focus.filter(|inner| inner.line > 0 && !location.contains(inner)) {
        return of_finding(index, outside, None, context);
    }
    let focus = focus.filter(|inner| inner.line > 0);
    let marked = focus.unwrap_or(location);
    let marked_first = marked.line.clamp(first, last);
    let marked_last = if focus.is_some() {
        marked.end_line.clamp(marked_first, last)
    } else {
        marked_first
    };
    let (end_line, end_column) = index.locate(marked.end);

    let mut lines = Vec::new();
    for number in first.saturating_sub(context).max(1)..first {
        push(&mut lines, index, number, None, Role::Context);
    }

    let context_from = marked_first.saturating_sub(1).max(first);
    if first < context_from {
        push(&mut lines, index, first, None, Role::Context);
        set_gap(&mut lines, context_from - first - 1);
    }
    let marked_lines = marked_last - context_from + 1;
    let capped = marked_lines as usize > HEAD_LINES + TAIL + 1;
    for number in context_from..=marked_last {
        let position = (number - context_from) as usize;
        if capped && position == HEAD_LINES {
            set_gap(&mut lines, marked_lines - (HEAD_LINES + TAIL) as u32);
        }
        if capped && position >= HEAD_LINES && position < marked_lines as usize - TAIL {
            continue;
        }
        if number < marked_first {
            push(&mut lines, index, number, None, Role::Context);
            continue;
        }
        let text = index.line_text(number);
        let from = (number == marked_first).then_some(marked.column);
        let to = (number == end_line).then_some(end_column);
        push(
            &mut lines,
            index,
            number,
            mark_of(text, from, to),
            Role::Offence,
        );
    }
    set_gap(&mut lines, last - marked_last);

    for number in (last + 1)..=(last + context).min(total) {
        push(&mut lines, index, number, None, Role::Context);
    }
    Some(Preview { lines, elided: 0 })
}

fn set_gap(lines: &mut [Line], hidden: u32) {
    if hidden > 0
        && let Some(shown) = lines.last_mut()
    {
        shown.gap_after = hidden;
    }
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

    fn inner(range: &SourceRange, line: u32, column: u32, start: u32, end: u32) -> SourceRange {
        SourceRange {
            line,
            column,
            start,
            end,
            end_line: line,
            ..range.clone()
        }
    }

    #[test]
    fn only_the_focus_is_marked_and_the_rest_of_the_statement_is_a_count() {
        let source = "_ = call(a, func() {\n    one()\n    two()\n})\nafter()\n";
        let index = LineIndex::new(source);
        let whole = range(source, 1, 1, 4, 0, 46);
        let focus = inner(&whole, 1, 1, 0, 1);
        let preview = of_finding(&index, &whole, Some(&focus), 0).unwrap();
        assert_eq!(preview.lines.len(), 1);
        assert_eq!(preview.lines[0].ruler(), Some((0, 1)));
        assert_eq!(preview.lines[0].gap_after, 3);
    }

    #[test]
    fn a_focus_below_the_first_line_keeps_the_line_above_it_as_context() {
        let source = "start(\n  one,\n  two,\n  bad,\n)\n";
        let index = LineIndex::new(source);
        let whole = range(source, 1, 1, 5, 0, 28);
        let focus = inner(&whole, 4, 3, 23, 26);
        let preview = of_finding(&index, &whole, Some(&focus), 0).unwrap();
        let shown: Vec<(u32, Role, u32)> = preview
            .lines
            .iter()
            .map(|line| (line.number, line.role, line.gap_after))
            .collect();
        assert_eq!(
            shown,
            [
                (1, Role::Context, 1),
                (3, Role::Context, 0),
                (4, Role::Offence, 1)
            ]
        );
        assert_eq!(preview.lines[2].ruler(), Some((2, 3)));
    }

    #[test]
    fn a_multi_line_location_without_a_focus_shows_its_first_line_only() {
        let source: String = (1..=12).map(|n| format!("line {n}\n")).collect();
        let index = LineIndex::new(&source);
        let whole = range(&source, 3, 1, 10, 14, 70);
        let preview = of_finding(&index, &whole, None, 0).unwrap();
        assert_eq!(preview.lines.len(), 1);
        assert_eq!(preview.lines[0].number, 3);
        assert_eq!(preview.lines[0].gap_after, 7);
    }

    #[test]
    fn a_single_line_location_without_a_focus_is_marked_as_before() {
        let source = "alpha beta gamma\n";
        let index = LineIndex::new(source);
        let whole = range(source, 1, 7, 1, 6, 10);
        let preview = of_finding(&index, &whole, None, 0).unwrap();
        assert_eq!(preview.lines[0].ruler(), Some((6, 4)));
        assert_eq!(preview.lines[0].gap_after, 0);
    }

    #[test]
    fn a_focus_outside_the_location_is_shown_on_its_own() {
        let source = "func name() {\n  body()\n}\n";
        let index = LineIndex::new(source);
        let body = range(source, 2, 3, 3, 16, 25);
        let name = inner(&range(source, 1, 1, 1, 0, 0), 1, 6, 5, 9);
        let preview = of_finding(&index, &body, Some(&name), 0).unwrap();
        assert_eq!(preview.lines.len(), 1);
        assert_eq!(preview.lines[0].number, 1);
        assert_eq!(preview.lines[0].ruler(), Some((5, 4)));
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

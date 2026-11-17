//! A unified diff of the edits a fix plan makes to one file, in the form `git apply`
//! and `patch` read.
//!
//! The edits already say what changes, so the diff is built from them rather than by
//! comparing the two texts line by line: only the lines an edit touches are looked at,
//! however large the file is.

use std::ops::Range;

use rqx_core::edit::Edit;

/// Unchanged lines shown around each change, which is what `git diff` does by default.
const CONTEXT: usize = 3;

/// Whole lines of the original that one or more edits replace, and the edits that do.
struct Change<'a> {
    lines: Range<usize>,
    edits: Vec<&'a Edit>,
}

struct Source<'a> {
    text: &'a str,
    /// Byte offset where each line starts, then the length of the text.
    starts: Vec<usize>,
}

impl<'a> Source<'a> {
    fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        if starts.last() != Some(&text.len()) {
            starts.push(text.len());
        }
        Self { text, starts }
    }

    fn line_count(&self) -> usize {
        self.starts.len() - 1
    }

    fn line_of(&self, offset: u32) -> usize {
        let line = self
            .starts
            .partition_point(|start| *start <= offset as usize);
        line.saturating_sub(1)
            .min(self.line_count().saturating_sub(1))
    }

    fn lines(&self, range: Range<usize>) -> &'a str {
        &self.text[self.starts[range.start]..self.starts[range.end]]
    }
}

/// `edits` are sorted and do not overlap, which is what `edit::resolve` keeps.
/// Nothing is returned when they change nothing.
pub fn of(display: &str, before: &str, edits: &[Edit]) -> String {
    let source = Source::new(before);
    let changes = group(&source, edits);
    let mut body = String::new();
    let mut shift = 0isize;
    for hunk in hunks(&changes) {
        shift += write_hunk(&mut body, &source, hunk, shift);
    }
    if body.is_empty() {
        return body;
    }
    format!("--- a/{display}\n+++ b/{display}\n{body}")
}

/// Edits that touch the same line are one change, because a line cannot be replaced
/// twice.
fn group<'a>(source: &Source<'_>, edits: &'a [Edit]) -> Vec<Change<'a>> {
    let mut changes: Vec<Change<'a>> = Vec::new();
    for edit in edits {
        let first = source.line_of(edit.span.start);
        let last = source.line_of(edit.span.end.saturating_sub(1).max(edit.span.start));
        match changes.last_mut() {
            Some(change) if first < change.lines.end => {
                change.lines.end = change.lines.end.max(last + 1);
                change.edits.push(edit);
            }
            _ => changes.push(Change {
                lines: first..last + 1,
                edits: vec![edit],
            }),
        }
    }
    changes
}

/// Changes whose context would overlap or touch share a hunk.
fn hunks<'a, 'b>(changes: &'b [Change<'a>]) -> Vec<&'b [Change<'a>]> {
    let mut hunks = Vec::new();
    let mut from = 0;
    for at in 1..=changes.len() {
        let apart = changes
            .get(at)
            .is_none_or(|next| next.lines.start > changes[at - 1].lines.end + 2 * CONTEXT);
        if apart {
            hunks.push(&changes[from..at]);
            from = at;
        }
    }
    hunks
}

fn replaced(source: &Source<'_>, change: &Change<'_>) -> String {
    let base = source.starts[change.lines.start];
    let mut text = source.lines(change.lines.clone()).to_owned();
    for edit in change.edits.iter().rev() {
        let start = edit.span.start as usize - base;
        let end = edit.span.end as usize - base;
        text.replace_range(start..end, &edit.replacement);
    }
    text
}

fn write_lines(out: &mut String, sign: char, text: &str) {
    for line in text.split_inclusive('\n') {
        out.push(sign);
        out.push_str(line.strip_suffix('\n').unwrap_or(line));
        out.push('\n');
        if !line.ends_with('\n') {
            out.push_str("\\ No newline at end of file\n");
        }
    }
}

fn line_total(text: &str) -> usize {
    text.split_inclusive('\n').count()
}

/// Writes one hunk and returns how many lines it adds to the file, so the next hunk's
/// position in the new text is known.
fn write_hunk(out: &mut String, source: &Source<'_>, hunk: &[Change<'_>], shift: isize) -> isize {
    let first = hunk[0].lines.start.saturating_sub(CONTEXT);
    let last = (hunk[hunk.len() - 1].lines.end + CONTEXT).min(source.line_count());

    let mut body = String::new();
    let mut cursor = first;
    let (mut removed, mut added) = (0, 0);
    for change in hunk {
        write_lines(&mut body, ' ', source.lines(cursor..change.lines.start));
        write_lines(&mut body, '-', source.lines(change.lines.clone()));
        let new = replaced(source, change);
        write_lines(&mut body, '+', &new);
        removed += change.lines.len();
        added += line_total(&new);
        cursor = change.lines.end;
    }
    write_lines(&mut body, ' ', source.lines(cursor..last));

    let old_count = last - first;
    let new_count = old_count - removed + added;
    let old_start = if old_count == 0 { first } else { first + 1 };
    let new_start = old_start.saturating_add_signed(shift);
    out.push_str(&format!(
        "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n{body}"
    ));
    added as isize - removed as isize
}

#[cfg(test)]
mod tests {
    use super::*;
    use rqx_core::finding::{Confidence, Severity};
    use rqx_core::identity::Identity;
    use rqx_core::rule::RuleId;
    use rqx_core::span::Span;

    fn edit(source: &str, from: &str, to: &str) -> Edit {
        let start = source.find(from).expect("text to replace") as u32;
        Edit {
            span: Span::new(start, start + from.len() as u32),
            replacement: to.to_owned(),
            rule: RuleId::ALL[0],
            severity: Severity::Info,
            confidence: Confidence::Verified,
            answers: Identity::of(RuleId::ALL[0], "a.ts", "x"),
        }
    }

    #[test]
    fn a_deletion_is_shown_between_its_context() {
        let source = "one\ntwo\nthree\nfour\nfive\n";
        let diff = of("a.ts", source, &[edit(source, "three\n", "")]);
        assert_eq!(
            diff,
            "--- a/a.ts\n+++ b/a.ts\n@@ -1,5 +1,4 @@\n one\n two\n-three\n four\n five\n"
        );
    }

    #[test]
    fn a_replacement_shows_the_old_and_the_new_lines() {
        let source = "a\nb\nc\n";
        let diff = of("a.ts", source, &[edit(source, "b", "B\nB2")]);
        assert_eq!(
            diff,
            "--- a/a.ts\n+++ b/a.ts\n@@ -1,3 +1,4 @@\n a\n-b\n+B\n+B2\n c\n"
        );
    }

    #[test]
    fn edits_far_apart_are_separate_hunks_and_the_second_knows_the_first_moved_it() {
        let source: String = (1..=30).map(|n| format!("line {n}\n")).collect();
        let edits = [
            edit(&source, "line 3\n", ""),
            edit(&source, "line 25\n", ""),
        ];
        let diff = of("a.ts", &source, &edits);
        assert_eq!(diff.matches("@@ -").count(), 2);
        assert!(diff.contains("@@ -1,6 +1,5 @@\n"), "{diff}");
        assert!(diff.contains("@@ -22,7 +21,6 @@\n"), "{diff}");
    }

    #[test]
    fn edits_close_together_share_a_hunk() {
        let source: String = (1..=12).map(|n| format!("line {n}\n")).collect();
        let edits = [edit(&source, "line 4\n", ""), edit(&source, "line 8\n", "")];
        let diff = of("a.ts", &source, &edits);
        assert_eq!(diff.matches("@@ -").count(), 1);
        assert!(diff.contains("@@ -1,11 +1,9 @@\n"), "{diff}");
    }

    #[test]
    fn two_edits_on_one_line_are_one_change() {
        let source = "let a = 1; let b = 2;\nnext\n";
        let edits = [
            edit(source, "let a = 1; ", ""),
            edit(source, "let b = 2;", ""),
        ];
        let diff = of("a.ts", source, &edits);
        assert_eq!(diff.matches("\n-").count(), 1, "{diff}");
        assert!(diff.contains("-let a = 1; let b = 2;\n+\n"), "{diff}");
    }

    #[test]
    fn a_last_line_without_a_newline_is_marked() {
        let source = "a\nb";
        let diff = of("a.ts", source, &[edit(source, "b", "")]);
        assert!(
            diff.contains("-b\n\\ No newline at end of file\n"),
            "{diff}"
        );
    }

    #[test]
    fn no_edits_is_an_empty_diff() {
        assert_eq!(of("a.ts", "a\n", &[]), "");
    }
}

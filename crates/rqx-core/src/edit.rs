use crate::finding::{Confidence, Finding, Severity};
use crate::rule::RuleId;
use crate::span::Span;

/// One replacement of a byte range in one file. The file is not named: a plan is
/// built and applied per file, and carrying the path on every edit would let two
/// files' edits into the same list, where the offsets mean different things.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
    pub rule: RuleId,
    pub severity: Severity,
    pub confidence: Confidence,
}

impl Edit {
    pub fn new(finding: &Finding, span: Span, replacement: impl Into<String>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
            rule: finding.rule,
            severity: finding.severity,
            confidence: finding.confidence,
        }
    }

    pub fn deletion(finding: &Finding, span: Span) -> Self {
        Self::new(finding, span, String::new())
    }

    /// The finding's own ranking, carried because two edits are compared long after
    /// the findings that asked for them have been consumed.
    pub fn rank(&self) -> (Severity, Confidence, RuleId) {
        (self.severity, self.confidence, self.rule)
    }

    fn order(&self) -> (u32, u32, RuleId) {
        (self.span.start, self.span.end, self.rule)
    }
}

/// What `resolve` decided. `dropped` is reported rather than discarded: an edit that
/// lost a conflict is a fix the reader asked for and did not get.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolution {
    pub kept: Vec<Edit>,
    pub dropped: Vec<Edit>,
}

impl Resolution {
    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }
}

fn overlaps(one: Span, other: Span) -> bool {
    one.start < other.end && other.start < one.end
}

/// Keeps the highest-ranked edit of any overlapping pair and drops the rest. Two
/// edits are never merged: a merged edit is one no rule asked for.
pub fn resolve(mut edits: Vec<Edit>) -> Resolution {
    edits.sort_by(|one, other| {
        other
            .rank()
            .cmp(&one.rank())
            .then_with(|| one.order().cmp(&other.order()))
    });

    let mut resolution = Resolution::default();
    for edit in edits {
        if resolution
            .kept
            .iter()
            .any(|kept| overlaps(kept.span, edit.span))
        {
            resolution.dropped.push(edit);
        } else {
            resolution.kept.push(edit);
        }
    }

    resolution.kept.sort_by_key(Edit::order);
    resolution.dropped.sort_by_key(Edit::order);
    resolution
}

/// `None` when an edit does not land on a character boundary of `source`, because
/// applying it would split a code point and write a file that is no longer UTF-8.
pub fn apply(source: &str, resolution: &Resolution) -> Option<String> {
    let mut applied = source.to_owned();
    for edit in resolution.kept.iter().rev() {
        let start = edit.span.start as usize;
        let end = edit.span.end as usize;
        if start > end || end > applied.len() {
            return None;
        }
        if !applied.is_char_boundary(start) || !applied.is_char_boundary(end) {
            return None;
        }
        applied.replace_range(start..end, &edit.replacement);
    }
    Some(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Evidence, SourceRange};

    fn finding(rule: RuleId, severity: Severity, confidence: Confidence) -> Finding {
        Finding {
            rule,
            location: SourceRange {
                file: "a.ts".into(),
                start: 0,
                end: 0,
                line: 1,
                column: 1,
                end_line: 1,
            },
            severity,
            confidence,
            evidence: Evidence::Unreachable {
                after: "return".to_owned(),
            },
            fix_hint: String::new(),
            related: Vec::new(),
        }
    }

    fn deletion(rule: RuleId, start: u32, end: u32) -> Edit {
        Edit::deletion(
            &finding(rule, Severity::Warning, Confidence::Verified),
            Span::new(start, end),
        )
    }

    fn applied(source: &str, edits: Vec<Edit>) -> String {
        apply(source, &resolve(edits)).expect("edits within the source")
    }

    #[test]
    fn later_edits_do_not_shift_earlier_ones() {
        let source = "one two three";
        let edits = vec![
            deletion(RuleId::UnusedLocal, 0, 4),
            deletion(RuleId::UnusedLocal, 8, 13),
        ];
        assert_eq!(applied(source, edits), "two ");
    }

    #[test]
    fn a_replacement_of_a_different_length_leaves_the_rest_intact() {
        let source = "call(a, b)";
        let edits = vec![Edit::new(
            &finding(
                RuleId::UnusedParameter,
                Severity::Warning,
                Confidence::Verified,
            ),
            Span::new(5, 9),
            "everything",
        )];
        assert_eq!(applied(source, edits), "call(everything)");
    }

    #[test]
    fn the_severer_finding_wins_an_overlap() {
        let quiet = deletion(RuleId::RedundantComment, 0, 8);
        let loud = Edit::deletion(
            &finding(
                RuleId::UnreachableCode,
                Severity::Error,
                Confidence::Verified,
            ),
            Span::new(4, 12),
        );

        let resolution = resolve(vec![quiet.clone(), loud.clone()]);
        assert_eq!(resolution.kept, vec![loud]);
        assert_eq!(resolution.dropped, vec![quiet]);
    }

    #[test]
    fn edits_that_only_touch_are_both_kept() {
        let resolution = resolve(vec![
            deletion(RuleId::UnusedLocal, 0, 4),
            deletion(RuleId::UnusedLocal, 4, 8),
        ]);
        assert_eq!(resolution.kept.len(), 2);
        assert!(resolution.dropped.is_empty());
    }

    /// Two runs of the same edits reach the same plan whatever order they arrived in,
    /// because a fix that depended on which worker finished first would make a diff
    /// no one could reproduce.
    #[test]
    fn resolution_does_not_depend_on_arrival_order() {
        let one = deletion(RuleId::UnusedLocal, 10, 20);
        let two = deletion(RuleId::JournalComment, 15, 25);
        let three = deletion(RuleId::UnusedLocal, 30, 40);

        let forward = resolve(vec![one.clone(), two.clone(), three.clone()]);
        let backward = resolve(vec![three, two, one]);
        assert_eq!(forward, backward);
    }

    #[test]
    fn an_edit_that_splits_a_code_point_is_refused() {
        let source = "héllo";
        let resolution = resolve(vec![deletion(RuleId::UnusedLocal, 1, 2)]);
        assert_eq!(apply(source, &resolution), None);
    }

    #[test]
    fn an_edit_past_the_end_is_refused() {
        let resolution = resolve(vec![deletion(RuleId::UnusedLocal, 0, 99)]);
        assert_eq!(apply("short", &resolution), None);
    }
}

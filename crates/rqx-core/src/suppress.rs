//! A waiver a developer writes next to the code, and what it takes for rqx to
//! honour it.

use serde::{Deserialize, Serialize};

use crate::finding::{Finding, SourceRange};
use crate::span::Span;

const LINE_MARKER: &str = "rqx-ignore";
const FILE_MARKER: &str = "rqx-ignore-file";

/// How far below its own line a waiver reaches. One line, so a comment above a
/// declaration covers it and a comment above a blank line covers nothing — a
/// waiver that reached further would silence findings nobody looked at.
const REACH: u32 = 1;

/// A comment that asks for a finding to be left out of the count, and the record of
/// who asked and why. `rules` empty means every rule.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Suppression {
    pub span: Span,
    pub line: u32,
    pub rules: Vec<String>,
    pub reason: Option<String>,
    pub until: Option<String>,
    pub whole_file: bool,
}

/// A finding that was found, and then waived. Kept rather than dropped so a reader
/// can audit every waiver in one place.
#[derive(Clone, Debug, Serialize)]
pub struct Suppressed {
    pub finding: Finding,
    pub reason: Option<String>,
    pub until: Option<String>,
    pub at: SourceRange,
}

impl Suppression {
    /// `rqx-ignore <slug>[, <slug>]* [until YYYY-MM-DD]: <reason>`, anywhere in a
    /// comment so the marker reads the same in `//`, `#` and `/* */`.
    pub fn parse(comment: &str, span: Span) -> Option<Self> {
        let at = comment.find(LINE_MARKER)?;
        let whole_file = comment[at..].starts_with(FILE_MARKER);
        let rest = &comment[at + if whole_file { FILE_MARKER } else { LINE_MARKER }.len()..];
        // A marker that runs straight into a word is part of that word, not a
        // directive: `rqx-ignoreable` is prose.
        if rest.starts_with(|c: char| c.is_alphanumeric() || c == '-') {
            return None;
        }

        let (head, reason) = match rest.split_once(':') {
            Some((head, reason)) => (head, non_empty(trim_comment(reason))),
            None => (rest, None),
        };
        let head = trim_comment(head);
        let (head, until) = match head.rsplit_once("until ") {
            Some((before, date)) if is_date(date.trim()) => {
                (before.trim(), Some(date.trim().to_owned()))
            }
            _ => (head, None),
        };

        Some(Self {
            span,
            line: 0,
            rules: head
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty() && *name != "all")
                .map(ToOwned::to_owned)
                .collect(),
            reason,
            until,
            whole_file,
        })
    }

    /// A waiver with no reason, or one whose date has passed, is not a waiver — the
    /// finding stands and `unjustified-suppression` reports the comment.
    pub fn is_honoured(&self, today: &str) -> bool {
        self.reason.is_some() && self.until.as_deref().is_none_or(|until| until >= today)
    }

    /// Why this one was refused, for the finding that reports it.
    pub fn refusal(&self, today: &str) -> Option<String> {
        if self.reason.is_none() {
            return Some(format!("{LINE_MARKER} with no reason"));
        }
        match &self.until {
            Some(until) if until.as_str() < today => Some(format!("{LINE_MARKER} expired {until}")),
            _ => None,
        }
    }

    pub fn covers(&self, finding: &Finding, today: &str) -> bool {
        self.is_honoured(today)
            && self.names(finding)
            && (self.whole_file || finding.location.line.saturating_sub(self.line) <= REACH)
    }

    fn names(&self, finding: &Finding) -> bool {
        self.rules.is_empty()
            || self
                .rules
                .iter()
                .any(|named| named == finding.slug() || named == finding.category().slug())
    }
}

/// Today as `YYYY-MM-DD`, so an `until` date can be compared as text. Written out
/// rather than pulled from a date crate: rqx needs one civil date a run and carries
/// no other use for one.
pub fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    civil_date(i64::try_from(seconds / 86_400).unwrap_or(0))
}

/// Days since 1970-01-01 to a civil date, by Howard Hinnant's `civil_from_days`.
pub fn civil_date(days: i64) -> String {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let months = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * months + 2) / 5 + 1;
    let month = if months < 10 { months + 3 } else { months - 9 };
    let year = era * 400 + year_of_era + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn is_date(text: &str) -> bool {
    let mut parts = text.split('-');
    let mut digits = |width: usize| {
        parts
            .next()
            .is_some_and(|part| part.len() == width && part.bytes().all(|b| b.is_ascii_digit()))
    };
    digits(4) && digits(2) && digits(2) && parts.next().is_none()
}

/// Comment syntax is not part of the reason: a `/* … */` waiver would otherwise
/// record its own closing marker as the justification.
fn trim_comment(text: &str) -> &str {
    text.trim()
        .trim_end_matches("*/")
        .trim_end_matches("--}")
        .trim()
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(comment: &str) -> Suppression {
        Suppression::parse(comment, Span::new(0, 1)).expect("a directive")
    }

    #[test]
    fn a_waiver_names_its_rules_and_its_reason() {
        let one = parsed("// rqx-ignore long-function: generated parser, PLAT-812");
        assert_eq!(one.rules, ["long-function"]);
        assert_eq!(one.reason.as_deref(), Some("generated parser, PLAT-812"));
        assert!(!one.whole_file);
        assert!(one.is_honoured("2026-09-05"));

        let many = parsed("# rqx-ignore long-function, deep-nesting: one decision");
        assert_eq!(many.rules, ["long-function", "deep-nesting"]);

        let all = parsed("// rqx-ignore all: vendored");
        assert!(all.rules.is_empty(), "`all` is every rule, not a rule");
    }

    #[test]
    fn a_waiver_without_a_reason_is_not_honoured() {
        let bare = parsed("// rqx-ignore long-function");
        assert!(!bare.is_honoured("2026-09-05"));
        assert_eq!(
            bare.refusal("2026-09-05").as_deref(),
            Some("rqx-ignore with no reason")
        );
    }

    #[test]
    fn a_date_that_has_passed_stops_honouring_it() {
        let dated = parsed("// rqx-ignore long-function until 2026-01-01: the rewrite lands in Q1");
        assert_eq!(dated.until.as_deref(), Some("2026-01-01"));
        assert_eq!(dated.rules, ["long-function"]);
        assert!(dated.is_honoured("2025-12-31"));
        assert!(dated.is_honoured("2026-01-01"), "the last day still counts");
        assert!(!dated.is_honoured("2026-01-02"));
        assert_eq!(
            dated.refusal("2026-06-01").as_deref(),
            Some("rqx-ignore expired 2026-01-01")
        );
    }

    /// A block comment carries its own terminator, and it is not the reason.
    #[test]
    fn a_block_comment_does_not_record_its_own_terminator() {
        assert_eq!(
            parsed("/* rqx-ignore empty-catch: the driver throws on close */")
                .reason
                .as_deref(),
            Some("the driver throws on close")
        );
    }

    #[test]
    fn the_marker_has_to_be_the_whole_word() {
        assert!(Suppression::parse("// rqx-ignorable, maybe", Span::new(0, 1)).is_none());
        assert!(Suppression::parse("// nothing to see", Span::new(0, 1)).is_none());
    }

    #[test]
    fn a_file_waiver_says_so() {
        let file = parsed("// rqx-ignore-file circular-dependency: the graph is being untangled");
        assert!(file.whole_file);
        assert_eq!(file.rules, ["circular-dependency"]);
    }

    /// The epoch and a leap day, so an off-by-one in the civil conversion shows up.
    #[test]
    fn days_since_the_epoch_are_a_civil_date() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(59), "1970-03-01");
        assert_eq!(civil_date(20_667), "2026-08-02");
        assert_eq!(civil_date(20_698), "2026-09-02", "a month rolls over");
        assert_eq!(today().len(), 10);
    }
}

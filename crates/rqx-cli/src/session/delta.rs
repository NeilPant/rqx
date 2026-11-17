//! What a re-analysis changed, as the lines `/watch` prints into scrollback.

use std::collections::BTreeSet;

use crate::scan::ScanOutcome;

use super::health_of;

/// Files named one to a line; past this the line counts them instead, because a
/// branch switch can move hundreds and the point of the line is to be glanced at.
const NAMED: usize = 3;

/// A file whose finding count moved, and by how much.
#[derive(Debug, PartialEq, Eq)]
pub struct Moved {
    pub file: String,
    pub by: i64,
}

/// One line per file that gained or lost findings, then the health the whole project
/// ended on. Nothing when no count moved: an edit that changes no finding is not news.
pub fn describe(before: &ScanOutcome, after: &ScanOutcome) -> Vec<String> {
    let moved = moved_files(before, after);
    if moved.is_empty() {
        return Vec::new();
    }
    render(&moved, health_of(before), health_of(after))
}

pub fn moved_files(before: &ScanOutcome, after: &ScanOutcome) -> Vec<Moved> {
    let files: BTreeSet<&std::sync::Arc<str>> = before
        .findings_by_file
        .keys()
        .chain(after.findings_by_file.keys())
        .collect();
    files
        .into_iter()
        .filter_map(|file| {
            let count = |outcome: &ScanOutcome| {
                i64::from(outcome.findings_by_file.get(file).copied().unwrap_or(0))
            };
            let by = count(after) - count(before);
            (by != 0).then(|| Moved {
                file: file.to_string(),
                by,
            })
        })
        .collect()
}

pub fn render(moved: &[Moved], health_before: u32, health_after: u32) -> Vec<String> {
    let health = if health_before == health_after {
        format!("health {health_after}")
    } else {
        format!("health {health_before} \u{2192} {health_after}")
    };
    if let [only] = moved {
        return vec![format!(
            "{}  {} \u{b7} {health}",
            only.file,
            change(only.by)
        )];
    }
    if moved.len() <= NAMED {
        let mut lines: Vec<String> = moved
            .iter()
            .map(|moved| format!("{}  {}", moved.file, change(moved.by)))
            .collect();
        lines.push(health);
        return lines;
    }
    let total: i64 = moved.iter().map(|moved| moved.by).sum();
    vec![format!(
        "{} files changed  {} \u{b7} {health}",
        moved.len(),
        change(total)
    )]
}

/// `−1 finding`, `+3 findings`; a file that gained one finding and lost one is `±0`
/// only in the multi-file total, which is why that case still reads.
fn change(by: i64) -> String {
    let sign = if by < 0 { '\u{2212}' } else { '+' };
    let count = by.unsigned_abs();
    let noun = if count == 1 { "finding" } else { "findings" };
    format!("{sign}{count} {noun}")
}

#[cfg(test)]
mod tests {
    use rqx_core::finding::Severity;

    use super::*;
    use crate::report::tests::outcome_with;

    fn moved(file: &str, by: i64) -> Moved {
        Moved {
            file: file.to_owned(),
            by,
        }
    }

    #[test]
    fn one_file_is_one_line_with_the_health_it_ended_on() {
        assert_eq!(
            render(&[moved("src/x.ts", -1)], 82, 83),
            ["src/x.ts  \u{2212}1 finding \u{b7} health 82 \u{2192} 83"]
        );
        assert_eq!(
            render(&[moved("src/x.ts", 3)], 90, 90),
            ["src/x.ts  +3 findings \u{b7} health 90"]
        );
    }

    #[test]
    fn a_few_files_are_named_and_the_health_follows_on_its_own_line() {
        assert_eq!(
            render(&[moved("a.ts", -2), moved("b.ts", 1)], 70, 72),
            [
                "a.ts  \u{2212}2 findings",
                "b.ts  +1 finding",
                "health 70 \u{2192} 72"
            ]
        );
    }

    #[test]
    fn many_files_are_counted_rather_than_listed() {
        let many: Vec<Moved> = (0..5).map(|at| moved(&format!("f{at}.ts"), -1)).collect();
        assert_eq!(
            render(&many, 60, 65),
            ["5 files changed  \u{2212}5 findings \u{b7} health 60 \u{2192} 65"]
        );
    }

    #[test]
    fn counts_are_compared_per_file_between_two_scans() {
        let before = outcome_with(
            &[("src/a.ts", Severity::Error), ("src/a.ts", Severity::Error)],
            10,
        );
        let after = outcome_with(&[("src/a.ts", Severity::Error)], 10);
        assert_eq!(moved_files(&before, &after), [moved("src/a.ts", -1)]);
        assert!(describe(&before, &before).is_empty());
    }
}

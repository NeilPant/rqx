use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::rule::RuleId;

/// Which lines of a file a test run actually executed. rqx decides dead code from the
/// import graph, which cannot see a dynamic import, a container wiring things up by
/// name, or reflection — so a symbol the graph called dead and a test ran is not a
/// finding rqx should keep insisting on.
#[derive(Debug, Default)]
pub struct Hits {
    by_file: BTreeMap<String, BTreeSet<u32>>,
}

impl Hits {
    pub fn is_empty(&self) -> bool {
        self.by_file.is_empty()
    }

    pub fn files(&self) -> usize {
        self.by_file.len()
    }

    /// Whether anything ran in the file at all. Paths are matched by suffix, because
    /// a coverage file records the path the test runner saw and rqx records the one
    /// the scan walked, and only one of them is relative to the project root.
    pub fn ran(&self, file: &str) -> bool {
        self.lines(file).is_some_and(|lines| !lines.is_empty())
    }

    /// How many of the lines in a range were executed.
    pub fn covered(&self, file: &str, from: u32, to: u32) -> u32 {
        self.lines(file)
            .map_or(0, |lines| lines.range(from..=to.max(from)).count() as u32)
    }

    fn lines(&self, file: &str) -> Option<&BTreeSet<u32>> {
        if let Some(lines) = self.by_file.get(file) {
            return Some(lines);
        }
        self.by_file
            .iter()
            .find(|(recorded, _)| recorded.ends_with(file) || file.ends_with(recorded.as_str()))
            .map(|(_, lines)| lines)
    }

    /// Two runs of the same suite, or one per language in a monorepo: a line either
    /// ran or it did not, so the union is the answer.
    #[must_use]
    pub fn merged(mut self, other: Self) -> Self {
        for (file, lines) in other.by_file {
            self.by_file.entry(file).or_default().extend(lines);
        }
        self
    }

    fn record(&mut self, file: &str, line: u32) {
        self.by_file
            .entry(normalise(file))
            .or_default()
            .insert(line);
    }

    /// The format is read from the file rather than declared, because a project
    /// naming its coverage file is not also going to want to name its dialect.
    pub fn read(path: &Path) -> Result<Self, String> {
        let body = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut hits = Self::default();

        if body.trim_start().starts_with('{') {
            hits.read_json(&body)?;
        } else if body
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("mode:"))
        {
            hits.read_go(&body);
        } else if body.contains("SF:") {
            hits.read_lcov(&body);
        } else {
            return Err(format!(
                "{}: not an lcov file, a Go cover profile, or coverage.py JSON",
                path.display()
            ));
        }
        Ok(hits)
    }

    /// `SF:` opens a file, `DA:line,count` is one line, and a zero count means the
    /// line was instrumented and never reached.
    fn read_lcov(&mut self, body: &str) {
        let mut file = String::new();
        for line in body.lines() {
            if let Some(rest) = line.strip_prefix("SF:") {
                file = rest.trim().to_owned();
            } else if let Some(rest) = line.strip_prefix("DA:")
                && let Some((number, count)) = rest.split_once(',')
                && count.trim().parse::<u64>().is_ok_and(|count| count > 0)
                && let Ok(number) = number.trim().parse::<u32>()
            {
                self.record(&file, number);
            }
        }
    }

    /// `file:startLine.col,endLine.col statements count`, one region per line.
    fn read_go(&mut self, body: &str) {
        for line in body.lines().skip(1) {
            let Some((location, rest)) = line.split_once(' ') else {
                continue;
            };
            let Some((file, span)) = location.rsplit_once(':') else {
                continue;
            };
            let count: u64 = rest
                .split_whitespace()
                .nth(1)
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            if count == 0 {
                continue;
            }
            let Some((start, end)) = span.split_once(',') else {
                continue;
            };
            let parse = |part: &str| part.split('.').next()?.parse::<u32>().ok();
            if let (Some(first), Some(last)) = (parse(start), parse(end)) {
                for number in first..=last.max(first) {
                    self.record(file, number);
                }
            }
        }
    }

    /// coverage.py: `{"files": {"path": {"executed_lines": [...]}}}`.
    fn read_json(&mut self, body: &str) -> Result<(), String> {
        let value: serde_json::Value =
            serde_json::from_str(body).map_err(|error| error.to_string())?;
        let files = value
            .get("files")
            .and_then(serde_json::Value::as_object)
            .ok_or("coverage JSON with no `files` object")?;
        for (file, report) in files {
            let executed = report
                .get("executed_lines")
                .and_then(serde_json::Value::as_array);
            for line in executed.into_iter().flatten() {
                if let Some(number) = line.as_u64() {
                    self.record(file, number as u32);
                }
            }
        }
        Ok(())
    }
}

/// Coverage tools write the path they were given: absolute, `./`-prefixed, or
/// relative to a package. Only the shape is normalised here; the suffix match in
/// `lines` does the rest.
fn normalise(file: &str) -> String {
    file.trim_start_matches("./").replace('\\', "/")
}

/// The rules a test run can contradict, and what their confidence drops to when it
/// does. Only rules that decide from the import graph belong here: a run says
/// something about whether code was reached, and nothing about whether a function is
/// too long.
pub const CONTRADICTED: [(RuleId, crate::finding::Confidence); 2] = [
    (RuleId::UnusedExport, crate::finding::Confidence::Low),
    (RuleId::UnusedFile, crate::finding::Confidence::Low),
];

pub fn contradicted(rule: RuleId) -> Option<crate::finding::Confidence> {
    CONTRADICTED
        .iter()
        .find(|(named, _)| *named == rule)
        .map(|(_, confidence)| *confidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcov_records_only_the_lines_that_ran() {
        let hits = {
            let mut hits = Hits::default();
            hits.read_lcov("SF:src/a.ts\nDA:1,4\nDA:2,0\nDA:7,1\nend_of_record\n");
            hits
        };
        assert!(hits.ran("src/a.ts"));
        assert_eq!(hits.covered("src/a.ts", 1, 10), 2, "line 2 never ran");
        assert!(!hits.ran("src/b.ts"));
    }

    #[test]
    fn a_go_profile_expands_a_region_into_its_lines() {
        let mut hits = Hits::default();
        hits.read_go("mode: set\nexample/a.go:3.10,6.2 2 1\nexample/a.go:9.2,11.4 1 0\n");
        assert_eq!(hits.covered("example/a.go", 3, 6), 4);
        assert_eq!(hits.covered("example/a.go", 9, 11), 0, "an unrun region");
    }

    #[test]
    fn coverage_json_reads_its_executed_lines() {
        let mut hits = Hits::default();
        hits.read_json(r#"{"files":{"pkg/a.py":{"executed_lines":[1,2,5]}}}"#)
            .expect("json");
        assert_eq!(hits.covered("pkg/a.py", 1, 5), 3);
    }

    /// A runner writes the path it was given and rqx walks its own, so the two agree
    /// on the end of the path and not the start.
    #[test]
    fn a_path_matches_by_its_end() {
        let mut hits = Hits::default();
        hits.read_lcov("SF:/build/checkout/src/a.ts\nDA:1,1\n");
        assert!(hits.ran("src/a.ts"));
        assert!(!hits.ran("other/a.ts"));
    }

    /// Only a rule the graph decided can be contradicted by a run: coverage says what
    /// was reached, never whether a function is too long.
    #[test]
    fn only_reachability_rules_can_be_contradicted() {
        for (rule, _) in CONTRADICTED {
            assert!(
                rule.is_project_scoped(),
                "{rule} is not decided by the graph"
            );
            assert_eq!(rule.spec().confidence, crate::finding::Confidence::Verified);
        }
        assert!(contradicted(RuleId::LongFunction).is_none());
    }
}

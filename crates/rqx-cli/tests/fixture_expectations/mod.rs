//! The findings each language's fixtures are expected to produce, line by line.
//!
//! `fixtures/<language>/.expect` holds one entry per line:
//!
//! ```text
//! bloat.go rule=no-base-case | func Loops(n int) int {
//! bloat.go rule=magic-number x2 | return n*60 + 3600
//! flow.go rule=unused-local #2 | total := 0
//! flow.go none=output-parameter | total := 0
//! flow.go may=scan-in-loop | for _, row := range rows {
//! hierarchy.ts known-fp=discarded-error countRows cannot fail | const _ = countRows(rows);
//! ```
//!
//! | Entry | Meaning |
//! |---|---|
//! | `rule=` | The rule fires on this line, `xN` times when more than once |
//! | `none=` | The rule must not fire on this line |
//! | `may=` | The rule may fire on this line or not |
//! | `known-fp=` | The rule fires here and is wrong, for the reason given |
//!
//! An entry finds its line by the trimmed source text, so editing a fixture moves no
//! entry. `#N` selects the Nth line of the file with that text. A finding no entry
//! covers is a failure, so the list is exhaustive.
//!
//! The list is a separate file because comments are analysed: a marker in the fixture
//! would change `comment-per-line`, `duplicate-code` and the line counts it describes.
//!
//! `RQX_BLESS=1` rewrites the `rule=` entries from the current findings. The other three
//! kinds record a decision, so blessing keeps them as written.

mod compare;
mod sidecar;
mod sources;
mod tests;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub use compare::Difference;
use compare::{blessed, compare, fired};
use sidecar::{Expectation, parse, render};
use sources::Sources;

const SIDECAR: &str = ".expect";
const BLESS_COMMAND: &str =
    "RQX_BLESS=1 cargo test -p rqx --test fixtures every_fixture_finding_is_expected";

/// One finding, with its file relative to the scan root.
#[derive(Clone, Debug)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule: String,
}

/// A language's fixture directory and the name its paths are reported under.
pub struct FixtureRoot {
    directory: PathBuf,
    label: String,
}

impl FixtureRoot {
    pub fn new(directory: &Path, label: &str) -> Self {
        Self {
            directory: directory.to_owned(),
            label: label.to_owned(),
        }
    }

    /// Every way the findings and the expectation file disagree.
    pub fn check(&self, findings: &[Finding]) -> Vec<Difference> {
        let Ok(text) = fs::read_to_string(self.directory.join(SIDECAR)) else {
            return vec![Difference::in_file(
                format!("{}/{SIDECAR}", self.label),
                "no expectation file".to_owned(),
            )];
        };
        let (expectations, problems) = parse(&text);
        let mut differences = self.malformed(problems);
        let sources = self.sources(&expectations, findings);
        differences.extend(compare(
            &expectations,
            &fired(findings),
            &sources,
            &self.label,
        ));
        differences.sort();
        differences
    }

    /// Rewrites the expectation file from the findings. What remains is what blessing
    /// cannot settle: a malformed file, which is left alone so that no decision in it
    /// is lost, and any entry written by hand that still does not hold.
    pub fn bless(&self, findings: &[Finding]) -> Vec<Difference> {
        let path = self.directory.join(SIDECAR);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let (existing, problems) = parse(&text);
        if !problems.is_empty() {
            return self.malformed(problems);
        }
        let sources = self.sources(&existing, findings);
        let fired = fired(findings);
        let expectations = blessed(&existing, &fired, &sources);
        let rendered = render(&self.label, &expectations);
        if rendered != text {
            fs::write(&path, rendered).expect("the expectation file is writable");
        }
        compare(&expectations, &fired, &sources, &self.label)
    }

    fn malformed(&self, problems: Vec<sidecar::Problem>) -> Vec<Difference> {
        problems
            .into_iter()
            .map(|problem| {
                Difference::at(
                    format!("{}/{SIDECAR}", self.label),
                    problem.line,
                    format!("malformed entry: {}", problem.message),
                )
            })
            .collect()
    }

    fn sources(&self, expectations: &[Expectation], findings: &[Finding]) -> Sources {
        let files: BTreeSet<&str> = expectations
            .iter()
            .map(|expectation| expectation.file.as_str())
            .chain(findings.iter().map(|finding| finding.file.as_str()))
            .collect();
        Sources::read(&self.directory, files)
    }
}

/// One line per difference, then the command that accepts them.
pub fn report(differences: &[Difference]) -> String {
    let lines: Vec<String> = differences.iter().map(ToString::to_string).collect();
    format!(
        "{}\n\n{} difference(s). If the change is intended, rewrite the expectations and \
         review their diff:\n  {BLESS_COMMAND}",
        lines.join("\n"),
        differences.len(),
    )
}

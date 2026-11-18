//! How well the cost inference does on the precision corpus, as the corpus's committed
//! `baseline.json` records it, and the per-rule excerpt of it that ships in the binary.
//!
//! ```json
//! {
//!   "format": 1,
//!   "functions": { "python/loops.py:sum_all": "exact" },
//!   "rules": { "linear-op-in-loop": { "tp": 2, "fp": 0, "fn": 1 } },
//!   "real": { "functions": { "real/go/scan.go:Next": "exact" }, "rules": {} }
//! }
//! ```
//!
//! The top level scores the functions written for the corpus. `real` scores functions
//! copied from public projects, kept apart so that the number for real code is published
//! beside the number for the written ones.
//!
//! An outcome is `exact` when the inferred time and space equal the annotation, `under`
//! when either is smaller, `over` when either is larger and neither is smaller, and
//! `abstained` when the analysis declined to answer. Precision is exact over the answered
//! functions, recall is exact over all of them. Metrics are derived and never stored, so
//! they cannot disagree with the entries.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const FORMAT: u32 = 1;

/// The rule counts of both sets of functions, which `rqx explain` reads. A copy of the
/// part of the baseline it needs, because a crate cannot embed a file from outside its
/// own directory and still be published; the corpus test fails when the two differ.
const EMBEDDED_RULES: &str = include_str!("precision/rules.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Exact,
    Over,
    Under,
    Abstained,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct RuleCounts {
    pub tp: u32,
    pub fp: u32,
    #[serde(rename = "fn")]
    pub missed: u32,
}

impl RuleCounts {
    pub const fn reported(&self) -> u32 {
        self.tp + self.fp
    }

    /// Reports that were right over all reports, when the rule reported anything.
    pub fn precision(&self) -> Option<f64> {
        (self.reported() > 0).then(|| f64::from(self.tp) / f64::from(self.reported()))
    }

    /// The functions the rule had to report that it did, when there were any.
    pub fn recall(&self) -> Option<f64> {
        let wanted = self.tp + self.missed;
        (wanted > 0).then(|| f64::from(self.tp) / f64::from(wanted))
    }

    fn add(&mut self, other: Self) {
        self.tp += other.tp;
        self.fp += other.fp;
        self.missed += other.missed;
    }
}

/// The functions of one set and the rules scored on them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    #[serde(default)]
    pub functions: BTreeMap<String, Outcome>,
    #[serde(default)]
    pub rules: BTreeMap<String, RuleCounts>,
}

impl Section {
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty() && self.rules.is_empty()
    }

    /// The tally of every group `group_of` sorts the functions into, by their ids, and
    /// of all of them under `TOTAL`.
    pub fn tallies(&self, group_of: &dyn Fn(&str) -> String) -> BTreeMap<String, Tally> {
        let mut found: BTreeMap<String, Tally> = BTreeMap::new();
        for (id, outcome) in &self.functions {
            found.entry(group_of(id)).or_default().record(*outcome);
            found.entry(TOTAL.to_owned()).or_default().record(*outcome);
        }
        found
    }

    /// The tally of the functions of each language, and of all of them.
    pub fn tallies_by_language(&self) -> BTreeMap<String, Tally> {
        self.tallies(&|id| language_of_id(id).to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub format: u32,
    pub functions: BTreeMap<String, Outcome>,
    #[serde(default)]
    pub rules: BTreeMap<String, RuleCounts>,
    #[serde(default, skip_serializing_if = "Section::is_empty")]
    pub real: Section,
}

impl Baseline {
    pub fn of(functions: BTreeMap<String, Outcome>) -> Self {
        Self {
            format: FORMAT,
            functions,
            rules: BTreeMap::new(),
            real: Section::default(),
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let baseline: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if baseline.format != FORMAT {
            return Err(format!(
                "format {} is not the format {FORMAT} this reads",
                baseline.format
            ));
        }
        Ok(baseline)
    }

    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("a baseline is always JSON");
        text.push('\n');
        text
    }

    /// The functions written for the corpus, as a section like the real one.
    pub fn synthetic(&self) -> Section {
        Section {
            functions: self.functions.clone(),
            rules: self.rules.clone(),
        }
    }

    pub fn count(&self, outcome: Outcome) -> usize {
        self.functions.values().filter(|o| **o == outcome).count()
    }

    /// The tally of every group of the written functions; see `Section::tallies`.
    pub fn tallies(&self, group_of: &dyn Fn(&str) -> String) -> BTreeMap<String, Tally> {
        self.synthetic().tallies(group_of)
    }

    /// What each rule did over both sets of functions.
    pub fn rules_overall(&self) -> BTreeMap<String, RuleCounts> {
        let mut all = self.rules.clone();
        for (rule, counts) in &self.real.rules {
            all.entry(rule.clone()).or_default().add(*counts);
        }
        all
    }
}

pub const TOTAL: &str = "total";

/// How many functions came out each way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub exact: usize,
    pub over: usize,
    pub under: usize,
    pub abstained: usize,
}

impl Tally {
    pub fn of(outcomes: impl IntoIterator<Item = Outcome>) -> Self {
        let mut tally = Self::default();
        for outcome in outcomes {
            tally.record(outcome);
        }
        tally
    }

    pub fn record(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Exact => self.exact += 1,
            Outcome::Over => self.over += 1,
            Outcome::Under => self.under += 1,
            Outcome::Abstained => self.abstained += 1,
        }
    }

    pub const fn functions(&self) -> usize {
        self.exact + self.over + self.under + self.abstained
    }

    pub fn precision(&self) -> Option<f64> {
        let answered = self.functions() - self.abstained;
        (answered > 0).then(|| self.exact as f64 / answered as f64)
    }

    pub fn recall(&self) -> Option<f64> {
        (self.functions() > 0).then(|| self.exact as f64 / self.functions() as f64)
    }
}

/// The language a corpus function id belongs to, from the extension of its file:
/// `projects/barrel/find.ts:findMatch` is TypeScript.
pub fn language_of_id(id: &str) -> &'static str {
    let file = id.split_once(':').map_or(id, |(file, _)| file);
    match file.rsplit_once('.').map(|(_, extension)| extension) {
        Some("ts") => "typescript",
        Some("js") => "javascript",
        Some("py") => "python",
        Some("go") => "go",
        Some("java") => "java",
        Some("cpp") => "cpp",
        _ => "unknown",
    }
}

/// The part of the baseline that ships in the binary: what each rule did over the
/// written and the real functions together.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleTable {
    pub format: u32,
    pub rules: BTreeMap<String, RuleCounts>,
}

impl RuleTable {
    pub fn of(baseline: &Baseline) -> Self {
        Self {
            format: FORMAT,
            rules: baseline.rules_overall(),
        }
    }

    pub fn embedded() -> Self {
        Self::parse(EMBEDDED_RULES).expect("the embedded rule precision is well formed")
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let table: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if table.format != FORMAT {
            return Err(format!(
                "format {} is not the format {FORMAT} this reads",
                table.format
            ));
        }
        Ok(table)
    }

    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("a table is always JSON");
        text.push('\n');
        text
    }
}

/// How often a rule was right on the corpus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measured {
    pub correct: u32,
    pub reported: u32,
}

impl Measured {
    pub fn precision(&self) -> f64 {
        f64::from(self.correct) / f64::from(self.reported)
    }

    /// `Measured precision: 0.94 (47 of 50 on the rqx corpus, synthetic + real)`
    pub fn line(&self) -> String {
        format!(
            "Measured precision: {:.2} ({} of {} on the rqx corpus, synthetic + real)",
            self.precision(),
            self.correct,
            self.reported
        )
    }
}

/// What the corpus says of a rule, when it reported anything on it.
pub fn measured(slug: &str) -> Option<Measured> {
    let counts = *RuleTable::embedded().rules.get(slug)?;
    (counts.reported() > 0).then_some(Measured {
        correct: counts.tp,
        reported: counts.reported(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> Baseline {
        Baseline {
            format: FORMAT,
            functions: BTreeMap::from([
                ("a".to_owned(), Outcome::Exact),
                ("b".to_owned(), Outcome::Over),
                ("c".to_owned(), Outcome::Under),
                ("d".to_owned(), Outcome::Abstained),
            ]),
            rules: BTreeMap::from([(
                "linear-op-in-loop".to_owned(),
                RuleCounts {
                    tp: 2,
                    fp: 0,
                    missed: 1,
                },
            )]),
            real: Section::default(),
        }
    }

    #[test]
    fn metrics_are_derived_from_the_outcomes() {
        let all = baseline().tallies(&|_| "all".to_owned())[TOTAL];
        assert_eq!(baseline().count(Outcome::Abstained), 1);
        assert!((all.precision().unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert!((all.recall().unwrap() - 0.25).abs() < 1e-9);
        let nothing = Tally::of([Outcome::Abstained]);
        assert_eq!(nothing.precision(), None);
        assert_eq!(nothing.recall(), Some(0.0));
    }

    #[test]
    fn functions_are_tallied_by_the_group_their_id_names_and_in_total() {
        let tallies = baseline().tallies(&|id| if id == "a" { "x" } else { "y" }.to_owned());
        assert_eq!(tallies["x"], Tally::of([Outcome::Exact]));
        assert_eq!(tallies["y"].functions(), 3);
        assert_eq!(tallies[TOTAL].functions(), 4);
        assert_eq!(tallies["y"].precision(), Some(0.0));
        assert_eq!(tallies[TOTAL].recall(), Some(0.25));
    }

    #[test]
    fn the_file_format_is_the_documented_shape_and_reads_back() {
        let json = serde_json::to_string(&baseline()).unwrap();
        assert!(
            json.contains(r#""functions":{"a":"exact","b":"over","c":"under","d":"abstained"}"#)
        );
        assert!(json.contains(r#""rules":{"linear-op-in-loop":{"tp":2,"fp":0,"fn":1}}"#));
        assert!(!json.contains("real"), "an empty real section is left out");
        assert_eq!(serde_json::from_str::<Baseline>(&json).unwrap(), baseline());
    }

    #[test]
    fn a_baseline_without_a_real_section_reads_and_one_with_it_reads_back() {
        let old = r#"{"format":1,"functions":{"a":"exact"}}"#;
        assert!(Baseline::parse(old).unwrap().real.is_empty());
        let mut with_real = baseline();
        with_real
            .real
            .functions
            .insert("r".to_owned(), Outcome::Over);
        assert_eq!(Baseline::parse(&with_real.to_json()).unwrap(), with_real);
    }

    #[test]
    fn an_unknown_format_or_field_is_refused() {
        let error = Baseline::parse(r#"{"format":2,"functions":{}}"#).unwrap_err();
        assert!(error.contains("format 2"));
        assert!(Baseline::parse(r#"{"format":1,"functions":{},"extra":1}"#).is_err());
        assert_eq!(Baseline::parse(&baseline().to_json()).unwrap(), baseline());
    }

    #[test]
    fn a_rule_is_counted_over_the_written_and_the_real_functions() {
        let mut both = baseline();
        both.real.rules.insert(
            "linear-op-in-loop".to_owned(),
            RuleCounts {
                tp: 1,
                fp: 1,
                missed: 0,
            },
        );
        let counts = both.rules_overall()["linear-op-in-loop"];
        assert_eq!((counts.tp, counts.fp, counts.missed), (3, 1, 1));
        assert_eq!(RuleTable::of(&both).rules["linear-op-in-loop"], counts);
        assert_eq!(counts.precision(), Some(0.75));
        assert_eq!(counts.recall(), Some(0.75));
    }

    #[test]
    fn a_rule_that_reported_nothing_has_no_precision() {
        assert_eq!(RuleCounts::default().precision(), None);
        let quiet = RuleCounts {
            tp: 0,
            fp: 0,
            missed: 2,
        };
        assert_eq!(quiet.precision(), None);
        assert_eq!(quiet.recall(), Some(0.0));
    }

    #[test]
    fn the_measured_line_names_the_counts() {
        let line = Measured {
            correct: 47,
            reported: 50,
        }
        .line();
        assert_eq!(
            line,
            "Measured precision: 0.94 (47 of 50 on the rqx corpus, synthetic + real)"
        );
    }

    #[test]
    fn the_language_is_read_from_the_file_of_the_id() {
        assert_eq!(language_of_id("python/loops.py:sum_all"), "python");
        assert_eq!(language_of_id("projects/barrel/find.ts:find"), "typescript");
        assert_eq!(language_of_id("real/cpp/format.cpp:pad"), "cpp");
        assert_eq!(language_of_id("odd"), "unknown");
    }

    #[test]
    fn the_embedded_table_is_well_formed() {
        assert_eq!(RuleTable::embedded().format, FORMAT);
    }
}

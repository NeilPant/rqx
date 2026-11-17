//! `fixtures/complexity/baseline.json`: what the inference got right the last time it
//! was blessed. The ratchet reads it and fails when a function that was exact stops
//! being exact, or a metric drops; `RQX_BLESS=1` rewrites it on purpose.
//!
//! ```json
//! {
//!   "format": 1,
//!   "functions": { "python/loops.py:sum_all": "exact" },
//!   "rules": { "linear-op-in-loop": { "tp": 2, "fp": 0, "fn": 1 } }
//! }
//! ```
//!
//! An outcome is `exact` when the inferred time and space equal the annotation,
//! `under` when either is smaller, `over` when either is larger and neither is smaller,
//! and `abstained` when the analysis declined to answer. Precision is exact over the
//! answered functions, recall is exact over all of them. Metrics are derived and never
//! stored, so they cannot disagree with the entries.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const FORMAT: u32 = 1;

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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub format: u32,
    pub functions: BTreeMap<String, Outcome>,
    #[serde(default)]
    pub rules: BTreeMap<String, RuleCounts>,
}

impl Baseline {
    pub fn of(functions: BTreeMap<String, Outcome>) -> Self {
        Self {
            format: FORMAT,
            functions,
            rules: BTreeMap::new(),
        }
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let baseline: Self = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        if baseline.format != FORMAT {
            return Err(format!(
                "format {} is not the format {FORMAT} this runner reads",
                baseline.format
            ));
        }
        Ok(baseline)
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        let mut text = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        text.push('\n');
        std::fs::write(path, text).map_err(|error| error.to_string())
    }

    pub fn count(&self, outcome: Outcome) -> usize {
        self.functions.values().filter(|o| **o == outcome).count()
    }

    pub fn precision(&self) -> Option<f64> {
        Tally::of(self.functions.values().copied()).precision()
    }

    pub fn recall(&self) -> Option<f64> {
        Tally::of(self.functions.values().copied()).recall()
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

    pub fn functions(&self) -> usize {
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
        }
    }

    #[test]
    fn metrics_are_derived_from_the_outcomes() {
        let baseline = baseline();
        assert_eq!(baseline.count(Outcome::Abstained), 1);
        assert!((baseline.precision().unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert!((baseline.recall().unwrap() - 0.25).abs() < 1e-9);
        let nothing = Baseline::of(BTreeMap::from([("a".to_owned(), Outcome::Abstained)]));
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
        assert_eq!(serde_json::from_str::<Baseline>(&json).unwrap(), baseline());
    }

    #[test]
    fn an_unknown_format_or_field_is_refused() {
        let path = std::env::temp_dir().join(format!("rqx-baseline-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"format":2,"functions":{}}"#).unwrap();
        assert!(Baseline::read(&path).unwrap_err().contains("format 2"));
        std::fs::write(&path, r#"{"format":1,"functions":{},"extra":1}"#).unwrap();
        assert!(Baseline::read(&path).is_err());
        baseline().write(&path).unwrap();
        assert_eq!(Baseline::read(&path).unwrap(), baseline());
        std::fs::remove_file(path).unwrap();
    }
}

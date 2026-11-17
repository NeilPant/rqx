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
//! `over` when either is larger, `under` when either is smaller, and `abstained` when
//! the analysis declined to answer. Precision is exact over the answered functions,
//! recall is exact over all of them. Metrics are derived and never stored, so they
//! cannot disagree with the entries.

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
    /// What a run records while the inference does not exist: every function
    /// abstained and no rule counted.
    pub fn all_abstained(ids: &[String]) -> Self {
        Self {
            format: FORMAT,
            functions: ids
                .iter()
                .map(|id| (id.clone(), Outcome::Abstained))
                .collect(),
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
        let answered = self.functions.len() - self.count(Outcome::Abstained);
        (answered > 0).then(|| self.count(Outcome::Exact) as f64 / answered as f64)
    }

    pub fn recall(&self) -> Option<f64> {
        (!self.functions.is_empty())
            .then(|| self.count(Outcome::Exact) as f64 / self.functions.len() as f64)
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
        let nothing = Baseline::all_abstained(&["a".to_owned()]);
        assert_eq!(nothing.precision(), None);
        assert_eq!(nothing.recall(), Some(0.0));
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

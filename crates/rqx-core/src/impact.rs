use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

/// How much a file's problems cost, as distinct from how wrong they are. A smell in
/// a file nobody imports and nobody edits is a smell you can leave alone; the same
/// smell in a file fifteen modules depend on and someone touches weekly is the one
/// that keeps causing incidents.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize)]
pub struct Impact {
    /// Commits touching this file inside the churn window.
    pub commits: u32,
    /// Modules that import this file directly.
    pub importers: u32,
}

impl Impact {
    /// Multiplies a finding's weight. Both terms are damped with a logarithm because
    /// the step from one importer to five matters far more than the step from fifty
    /// to fifty-five, and an undamped product lets one hub file dominate the score.
    pub fn multiplier(self) -> f64 {
        let churn = 1.0 + f64::from(self.commits).ln_1p() / 3.0;
        let reach = 1.0 + f64::from(self.importers).ln_1p() / 3.0;
        (churn * reach).min(4.0)
    }

    pub const fn is_known(self) -> bool {
        self.commits > 0 || self.importers > 0
    }
}

/// Per-file impact, looked up by the same path string findings carry.
#[derive(Debug, Default)]
pub struct ImpactMap {
    files: BTreeMap<Arc<str>, Impact>,
}

impl ImpactMap {
    pub fn set_commits(&mut self, file: Arc<str>, commits: u32) {
        self.files.entry(file).or_default().commits = commits;
    }

    pub fn set_importers(&mut self, file: Arc<str>, importers: u32) {
        self.files.entry(file).or_default().importers = importers;
    }

    pub fn get(&self, file: &str) -> Impact {
        self.files.get(file).copied().unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The files carrying the most risk, for a summary that says where to start.
    pub fn hotspots(&self, limit: usize) -> Vec<(&str, Impact)> {
        let mut ranked: Vec<(&str, Impact)> = self
            .files
            .iter()
            .map(|(file, impact)| (file.as_ref(), *impact))
            .collect();
        ranked.sort_by(|a, b| {
            b.1.multiplier()
                .partial_cmp(&a.1.multiplier())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(b.0))
        });
        ranked.truncate(limit);
        ranked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_file_neither_helps_nor_hurts() {
        assert_eq!(Impact::default().multiplier(), 1.0);
        assert!(!Impact::default().is_known());
    }

    #[test]
    fn reach_and_churn_both_raise_the_multiplier() {
        let quiet = Impact {
            commits: 0,
            importers: 0,
        };
        let churned = Impact {
            commits: 40,
            importers: 0,
        };
        let central = Impact {
            commits: 0,
            importers: 40,
        };
        let both = Impact {
            commits: 40,
            importers: 40,
        };
        assert!(churned.multiplier() > quiet.multiplier());
        assert!(central.multiplier() > quiet.multiplier());
        assert!(both.multiplier() > churned.multiplier());
    }

    /// Without damping a single hub file would swamp every other finding in the
    /// score, which is the failure mode that makes weighted scores useless.
    #[test]
    fn one_extreme_file_cannot_dominate_the_score() {
        let huge = Impact {
            commits: 10_000,
            importers: 10_000,
        };
        assert!(huge.multiplier() <= 4.0, "{}", huge.multiplier());
        let modest = Impact {
            commits: 5,
            importers: 5,
        };
        assert!(huge.multiplier() / modest.multiplier() < 3.0);
    }

    #[test]
    fn hotspots_come_back_worst_first() {
        let mut map = ImpactMap::default();
        map.set_commits(Arc::from("quiet.ts"), 1);
        map.set_commits(Arc::from("busy.ts"), 90);
        map.set_importers(Arc::from("busy.ts"), 30);
        map.set_importers(Arc::from("middling.ts"), 8);
        let ranked: Vec<&str> = map.hotspots(2).into_iter().map(|(file, _)| file).collect();
        assert_eq!(ranked, ["busy.ts", "middling.ts"]);
    }
}

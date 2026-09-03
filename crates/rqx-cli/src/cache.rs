use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rqx_core::config::RqxConfig;
use rqx_ts::FileAnalysis;
use serde::{Deserialize, Serialize};

const CACHE_DIRECTORY: &str = ".rqx";
const CACHE_FILE: &str = "cache.json";

#[derive(Deserialize, Serialize)]
struct Entry {
    fingerprint: u64,
    analysis: FileAnalysis,
}

#[derive(Default, Deserialize, Serialize)]
struct Contents {
    /// Bumped whenever a rule changes what it reports, because a cached finding from
    /// an older binary is worse than no cache: it is silently wrong.
    version: u64,
    entries: HashMap<String, Entry>,
}

pub struct Cache {
    path: PathBuf,
    version: u64,
    loaded: HashMap<String, Entry>,
    fresh: Mutex<HashMap<String, Entry>>,
}

/// Everything that changes what analysing a file produces. A cache keyed on file
/// content alone would serve stale findings the moment a threshold moved.
fn analysis_version(config: &RqxConfig) -> u64 {
    let mut hasher = DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    let thresholds = &config.thresholds;
    for value in [
        thresholds.long_function_statements,
        thresholds.cognitive_complexity,
        thresholds.nesting_depth,
        thresholds.max_params,
        thresholds.class_members,
        thresholds.class_statements,
        thresholds.clone_min_statements,
        thresholds.clone_min_nodes,
    ] {
        value.hash(&mut hasher);
    }
    for rule in &config.rules.disabled {
        rule.hash(&mut hasher);
    }
    hasher.finish()
}

fn fingerprint(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

impl Cache {
    pub fn open(root: &Path, config: &RqxConfig) -> Self {
        let path = root.join(CACHE_DIRECTORY).join(CACHE_FILE);
        let version = analysis_version(config);
        let loaded = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Contents>(&text).ok())
            .filter(|contents| contents.version == version)
            .map(|contents| contents.entries)
            .unwrap_or_default();
        Self { path, version, loaded, fresh: Mutex::new(HashMap::new()) }
    }

    pub fn get(&self, key: &str, source: &str) -> Option<&FileAnalysis> {
        let entry = self.loaded.get(key)?;
        (entry.fingerprint == fingerprint(source)).then_some(&entry.analysis)
    }

    pub fn put(&self, key: &str, source: &str, analysis: &FileAnalysis) {
        if let Ok(mut fresh) = self.fresh.lock() {
            fresh.insert(
                key.to_owned(),
                Entry { fingerprint: fingerprint(source), analysis: analysis.clone() },
            );
        }
    }

    /// Only files seen in this run are written back, so deleting source eventually
    /// evicts its entry instead of growing the cache forever.
    pub fn save(self) {
        let Ok(entries) = self.fresh.into_inner() else { return };
        if entries.is_empty() {
            return;
        }
        let Some(directory) = self.path.parent() else { return };
        if std::fs::create_dir_all(directory).is_err() {
            return;
        }
        let contents = Contents { version: self.version, entries };
        if let Ok(text) = serde_json::to_string(&contents) {
            let _ = std::fs::write(&self.path, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_threshold_change_invalidates_the_whole_cache() {
        let mut config = RqxConfig::default();
        let before = analysis_version(&config);
        config.thresholds.cognitive_complexity += 1;
        assert_ne!(before, analysis_version(&config), "cached findings would be stale");
    }

    #[test]
    fn disabling_a_rule_invalidates_the_cache() {
        let mut config = RqxConfig::default();
        let before = analysis_version(&config);
        config.rules.disabled.insert("long-function".to_owned());
        assert_ne!(before, analysis_version(&config));
    }

    #[test]
    fn identical_content_fingerprints_alike_and_a_single_edit_does_not() {
        assert_eq!(fingerprint("const a = 1;"), fingerprint("const a = 1;"));
        assert_ne!(fingerprint("const a = 1;"), fingerprint("const a = 2;"));
    }
}

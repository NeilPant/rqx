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
/// content alone would serve stale findings the moment a threshold moved. Both
/// structs are hashed whole rather than field by field, because the list that was
/// written out by hand covered eight of the seventeen thresholds and nothing said so.
fn analysis_version(config: &RqxConfig) -> u64 {
    let mut hasher = DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    config.thresholds.hash(&mut hasher);
    config.rules.hash(&mut hasher);
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

    /// This used to name `cognitive_complexity`, one of the eight thresholds the
    /// hand-written hash covered, so it passed while the other nine went unhashed and
    /// replayed findings computed under the old value.
    #[test]
    fn every_threshold_change_invalidates_the_whole_cache() {
        let base = RqxConfig::default();
        let baseline = analysis_version(&base);
        let mutations: [fn(&mut rqx_core::config::Thresholds); 10] = [
            |t| t.cognitive_complexity += 1,
            |t| t.condition_operands += 1,
            |t| t.feature_envy_ratio += 1,
            |t| t.magic_number_repeats += 1,
            |t| t.short_name_scope_lines += 1,
            |t| t.data_clump_sites += 1,
            |t| t.data_clump_size += 1,
            |t| t.message_chain_depth += 1,
            |t| t.commented_code_lines += 1,
            |t| t.class_public_members += 1,
        ];
        for mutate in mutations {
            let mut config = base.clone();
            mutate(&mut config.thresholds);
            assert_ne!(analysis_version(&config), baseline, "cached findings would be stale");
        }
    }

    #[test]
    fn enabling_a_rule_invalidates_the_cache() {
        let mut config = RqxConfig::default();
        let before = analysis_version(&config);
        config.rules.enabled.insert("meaningless-name".to_owned());
        assert_ne!(before, analysis_version(&config));
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

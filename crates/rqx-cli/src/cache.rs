use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use rqx_lang::FileAnalysis;
use serde::{Deserialize, Serialize};

use crate::config::Resolved;

pub(crate) const CACHE_DIRECTORY: &str = ".rqx";
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
    hits: Mutex<HashSet<String>>,
    misses: AtomicU32,
}

/// Everything that changes what analysing a file produces. A cache keyed on file
/// content alone would serve stale findings the moment a threshold moved, and the
/// signature is hashed whole rather than field by field because the list that was
/// written out by hand covered eight of the seventeen thresholds and nothing said so.
fn analysis_version(settings: &Resolved) -> u64 {
    let mut hasher = DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    settings.signature().hash(&mut hasher);
    catalogue_version().hash(&mut hasher);
    hasher.finish()
}

/// The catalogue itself, because the package version does not move between builds
/// and a rule added since a cache was written would never fire on a file that had
/// not changed. Found the hard way: a new rule reported nothing on the fixture tree
/// until the cache was deleted by hand.
fn catalogue_version() -> u64 {
    use std::sync::LazyLock;
    static VERSION: LazyLock<u64> = LazyLock::new(|| {
        let mut hasher = DefaultHasher::new();
        for rule in rqx_core::rule::RuleId::ALL {
            let spec = rule.spec();
            spec.slug.hash(&mut hasher);
            spec.severity.hash(&mut hasher);
            spec.confidence.hash(&mut hasher);
            spec.off_by_default.hash(&mut hasher);
        }
        for advisory in rqx_core::advice::AdviceId::ALL {
            advisory.spec().slug.hash(&mut hasher);
        }
        hasher.finish()
    });
    *VERSION
}

/// What the cache holds and what it cost, for `rqx cache status`.
pub struct Stored {
    pub path: PathBuf,
    pub entries: usize,
    pub bytes: u64,
    pub current: bool,
}

pub fn stored(root: &Path, settings: &Resolved) -> Option<Stored> {
    let path = root.join(CACHE_DIRECTORY).join(CACHE_FILE);
    let text = std::fs::read_to_string(&path).ok()?;
    let contents: Contents = serde_json::from_str(&text).ok()?;
    Some(Stored {
        bytes: text.len() as u64,
        entries: contents.entries.len(),
        current: contents.version == analysis_version(settings),
        path,
    })
}

/// Removing the file rather than the directory: `.rqx` also holds the baseline, and
/// clearing a cache should not throw that away.
pub fn clear(root: &Path) -> std::io::Result<bool> {
    let path = root.join(CACHE_DIRECTORY).join(CACHE_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn fingerprint(source: &str) -> u64 {
    rqx_core::identity::fnv1a(&[source])
}

impl Cache {
    pub fn open(root: &Path, settings: &Resolved) -> Self {
        let path = root.join(CACHE_DIRECTORY).join(CACHE_FILE);
        let version = analysis_version(settings);
        let loaded = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Contents>(&text).ok())
            .filter(|contents| contents.version == version)
            .map(|contents| contents.entries)
            .unwrap_or_default();
        Self {
            path,
            version,
            loaded,
            fresh: Mutex::new(HashMap::new()),
            hits: Mutex::new(HashSet::new()),
            misses: AtomicU32::new(0),
        }
    }

    pub fn get(&self, key: &str, source: &str) -> Option<&FileAnalysis> {
        let served = self
            .loaded
            .get(key)
            .filter(|entry| entry.fingerprint == fingerprint(source));
        let Some(entry) = served else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        if let Ok(mut hits) = self.hits.lock() {
            hits.insert(key.to_owned());
        }
        Some(&entry.analysis)
    }

    pub fn hit_count(&self) -> u32 {
        self.hits.lock().map_or(0, |hits| hits.len() as u32)
    }

    pub fn miss_count(&self) -> u32 {
        self.misses.load(Ordering::Relaxed)
    }

    pub fn put(&self, key: &str, source: &str, analysis: &FileAnalysis) {
        if let Ok(mut fresh) = self.fresh.lock() {
            fresh.insert(
                key.to_owned(),
                Entry {
                    fingerprint: fingerprint(source),
                    analysis: analysis.clone(),
                },
            );
        }
    }

    /// Only files seen in this run are written back, so deleting source eventually
    /// evicts its entry instead of growing the cache forever. A hit counts as seen:
    /// writing just the misses left one entry behind after a single edited file, and
    /// the next run analysed everything else again.
    pub fn save(self) {
        let (Ok(mut entries), Ok(hits)) = (self.fresh.into_inner(), self.hits.into_inner()) else {
            return;
        };
        if entries.is_empty() && hits.len() == self.loaded.len() {
            return;
        }
        entries.extend(
            self.loaded
                .into_iter()
                .filter(|(key, _)| hits.contains(key)),
        );
        let Some(directory) = self.path.parent() else {
            return;
        };
        if std::fs::create_dir_all(directory).is_err() {
            return;
        }
        let contents = Contents {
            version: self.version,
            entries,
        };
        // Written through a temporary and renamed: a ctrl-c mid-write used to leave
        // a truncated document, which the version check then silently threw away
        // along with every entry in it.
        if let Ok(text) = serde_json::to_string(&contents) {
            let _ = crate::fix::write_atomically(&self.path, &text);
        }
    }
}

#[cfg(test)]
mod tests {

    /// A rule added since the cache was written must invalidate it. The package
    /// version does not move between builds, so without the catalogue in the key a
    /// new rule reports nothing on any file that has not changed — which is exactly
    /// what happened while `empty-method` was being written.
    #[test]
    fn the_catalogue_is_part_of_the_key() {
        let settings = Resolved::of(rqx_core::config::RqxConfig::default());
        let version = analysis_version(&settings);
        assert_eq!(version, analysis_version(&settings), "not deterministic");

        // Standing in for a rule changing, since the catalogue is a compile-time
        // table: the same hash over a different rule set has to differ.
        let mut hasher = DefaultHasher::new();
        for rule in rqx_core::rule::RuleId::ALL.iter().skip(1) {
            rule.spec().slug.hash(&mut hasher);
        }
        assert_ne!(hasher.finish(), catalogue_version());
    }

    use rqx_core::config::RqxConfig;

    use super::*;

    /// This used to name `cognitive_complexity`, one of the eight thresholds the
    /// hand-written hash covered, so it passed while the other nine went unhashed and
    /// replayed findings computed under the old value.
    #[test]
    fn every_threshold_change_invalidates_the_whole_cache() {
        let base = RqxConfig::default();
        let baseline = analysis_version(&Resolved::of(base.clone()));
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
            assert_ne!(
                analysis_version(&Resolved::of(config)),
                baseline,
                "cached findings would be stale"
            );
        }
    }

    #[test]
    fn enabling_a_rule_invalidates_the_cache() {
        let mut config = RqxConfig::default();
        let before = analysis_version(&Resolved::of(config.clone()));
        config.rules.enabled.insert("meaningless-name".to_owned());
        assert_ne!(before, analysis_version(&Resolved::of(config)));
    }

    #[test]
    fn disabling_a_rule_invalidates_the_cache() {
        let mut config = RqxConfig::default();
        let before = analysis_version(&Resolved::of(config.clone()));
        config.rules.disabled.insert("long-function".to_owned());
        assert_ne!(before, analysis_version(&Resolved::of(config)));
    }

    #[test]
    fn identical_content_fingerprints_alike_and_a_single_edit_does_not() {
        assert_eq!(fingerprint("const a = 1;"), fingerprint("const a = 1;"));
        assert_ne!(fingerprint("const a = 1;"), fingerprint("const a = 2;"));
    }
}

use std::collections::{BTreeMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Instant;

use rqx_lang::FileAnalysis;
use serde::{Deserialize, Serialize};

use crate::config::Resolved;

pub(crate) const CACHE_DIRECTORY: &str = ".rqx";
pub(crate) const CACHE_FILE: &str = "cache.bin";
/// What the cache was called while it was one uncompressed document. Nothing reads it
/// any more, so it is removed rather than left to take up the room the new one saves.
const LEGACY_CACHE_FILE: &str = "cache.json";

/// Part of the version, so a change to how entries are laid out makes every cache
/// written before it a miss instead of a parse error.
const FORMAT: u32 = 2;

const MAGIC: &[u8; 8] = b"rqxcache";
const INDEX_LENGTH_BYTES: usize = 4;

/// The file is an index followed by one compressed blob per source file. Compressing
/// per file rather than the whole document is what lets a scan decode a hit on the
/// thread that needs it: inflating all of it first was 70ms with every walker idle
/// behind it, where here each walker inflates only the entries it serves and a hit
/// is written back as the bytes it was read as.
#[derive(Deserialize, Serialize)]
struct Slot {
    fingerprint: u64,
    offset: usize,
    length: usize,
}

#[derive(Default, Deserialize, Serialize)]
struct Index {
    /// Bumped whenever a rule changes what it reports, because a cached finding from
    /// an older binary is worse than no cache: it is silently wrong.
    version: u64,
    entries: BTreeMap<String, Slot>,
}

/// One file's analysis, compressed, and the fingerprint of the source it came from.
struct Packed {
    fingerprint: u64,
    bytes: Vec<u8>,
}

pub struct Cache {
    path: PathBuf,
    version: u64,
    loaded_in: std::time::Duration,
    index: BTreeMap<String, Slot>,
    blobs: Vec<u8>,
    fresh: Mutex<BTreeMap<String, Packed>>,
    hits: Mutex<HashSet<String>>,
    misses: AtomicU32,
}

/// Everything that changes what analysing a file produces. A cache keyed on file
/// content alone would serve stale findings the moment a threshold moved, and the
/// signature is hashed whole rather than field by field because the list that was
/// written out by hand covered eight of the seventeen thresholds and nothing said so.
fn analysis_version(settings: &Resolved) -> u64 {
    let mut hasher = DefaultHasher::new();
    FORMAT.hash(&mut hasher);
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
    let (index, _) = read_container(&path)?;
    Some(Stored {
        bytes: std::fs::metadata(&path).ok()?.len(),
        entries: index.entries.len(),
        current: index.version == analysis_version(settings),
        path,
    })
}

/// The index and the blobs it points into. A file that is not a cache, or is a
/// cut-off one, answers `None` and is treated as absent.
fn read_container(path: &Path) -> Option<(Index, Vec<u8>)> {
    let mut bytes = std::fs::read(path).ok()?;
    let header = MAGIC.len() + INDEX_LENGTH_BYTES;
    if bytes.get(..MAGIC.len())? != MAGIC {
        return None;
    }
    let length = u32::from_le_bytes(bytes.get(MAGIC.len()..header)?.try_into().ok()?) as usize;
    let index = serde_json::from_slice(bytes.get(header..header + length)?).ok()?;
    let blobs = bytes.split_off(header + length);
    Some((index, blobs))
}

fn encode_container(version: u64, packed: BTreeMap<String, Packed>) -> Option<Vec<u8>> {
    let mut index = Index {
        version,
        entries: BTreeMap::new(),
    };
    let mut blobs = Vec::new();
    for (key, entry) in packed {
        index.entries.insert(
            key,
            Slot {
                fingerprint: entry.fingerprint,
                offset: blobs.len(),
                length: entry.bytes.len(),
            },
        );
        blobs.extend_from_slice(&entry.bytes);
    }
    let index = serde_json::to_vec(&index).ok()?;
    let mut file = Vec::with_capacity(MAGIC.len() + INDEX_LENGTH_BYTES + index.len() + blobs.len());
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&u32::try_from(index.len()).ok()?.to_le_bytes());
    file.extend_from_slice(&index);
    file.extend_from_slice(&blobs);
    Some(file)
}

fn blob<'a>(blobs: &'a [u8], slot: &Slot) -> Option<&'a [u8]> {
    blobs.get(slot.offset..slot.offset.checked_add(slot.length)?)
}

fn pack(analysis: &FileAnalysis) -> Option<Vec<u8>> {
    let json = serde_json::to_vec(analysis).ok()?;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&json).ok()?;
    encoder.finish().ok()
}

fn unpack(bytes: &[u8]) -> Option<FileAnalysis> {
    let mut json = Vec::new();
    flate2::read::ZlibDecoder::new(bytes)
        .read_to_end(&mut json)
        .ok()?;
    serde_json::from_slice(&json).ok()
}

/// Removing the file rather than the directory: `.rqx` also holds the baseline, and
/// clearing a cache should not throw that away.
pub fn clear(root: &Path) -> std::io::Result<bool> {
    let directory = root.join(CACHE_DIRECTORY);
    let mut cleared = false;
    for file in [CACHE_FILE, LEGACY_CACHE_FILE] {
        match std::fs::remove_file(directory.join(file)) {
            Ok(()) => cleared = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(cleared)
}

fn fingerprint(source: &str) -> u64 {
    rqx_core::identity::fnv1a(&[source])
}

impl Cache {
    pub fn open(root: &Path, settings: &Resolved) -> Self {
        let began = Instant::now();
        let path = root.join(CACHE_DIRECTORY).join(CACHE_FILE);
        let version = analysis_version(settings);
        let (index, blobs) = read_container(&path)
            .filter(|(index, _)| index.version == version)
            .map(|(index, blobs)| (index.entries, blobs))
            .unwrap_or_default();
        Self {
            path,
            version,
            loaded_in: began.elapsed(),
            index,
            blobs,
            fresh: Mutex::new(BTreeMap::new()),
            hits: Mutex::new(HashSet::new()),
            misses: AtomicU32::new(0),
        }
    }

    /// How long reading the index took, on whichever thread did it.
    pub fn loaded_in(&self) -> std::time::Duration {
        self.loaded_in
    }

    /// Decoded here, on the thread that asked, so the cost of a hit is spread across
    /// the walk instead of being paid up front for every entry. An entry that no longer
    /// decodes is a miss: the file is analysed again and its entry replaced.
    pub fn get(&self, key: &str, source: &str) -> Option<FileAnalysis> {
        let served = self
            .index
            .get(key)
            .filter(|slot| slot.fingerprint == fingerprint(source))
            .and_then(|slot| blob(&self.blobs, slot))
            .and_then(unpack);
        let Some(analysis) = served else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        if let Ok(mut hits) = self.hits.lock() {
            hits.insert(key.to_owned());
        }
        Some(analysis)
    }

    pub fn hit_count(&self) -> u32 {
        self.hits.lock().map_or(0, |hits| hits.len() as u32)
    }

    pub fn miss_count(&self) -> u32 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Compressed by the caller's thread, which is a walker, so a cold run spreads the
    /// cost across every core instead of paying it in one piece when the cache is saved.
    pub fn put(&self, key: &str, source: &str, analysis: &FileAnalysis) {
        let Some(bytes) = pack(analysis) else {
            return;
        };
        if let Ok(mut fresh) = self.fresh.lock() {
            fresh.insert(
                key.to_owned(),
                Packed {
                    fingerprint: fingerprint(source),
                    bytes,
                },
            );
        }
    }

    /// Only files seen in this run are written back, so deleting source eventually
    /// evicts its entry instead of growing the cache forever. A hit counts as seen:
    /// writing just the misses left one entry behind after a single edited file, and
    /// the next run analysed everything else again.
    pub fn save(self) {
        let Self {
            path,
            version,
            index,
            blobs,
            fresh,
            hits,
            ..
        } = self;
        let (Ok(mut entries), Ok(hits)) = (fresh.into_inner(), hits.into_inner()) else {
            return;
        };
        if entries.is_empty() && hits.len() == index.len() {
            return;
        }
        for (key, slot) in &index {
            if let Some(bytes) = hits.contains(key).then(|| blob(&blobs, slot)).flatten() {
                entries.entry(key.clone()).or_insert_with(|| Packed {
                    fingerprint: slot.fingerprint,
                    bytes: bytes.to_vec(),
                });
            }
        }
        let Some(directory) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(directory).is_err() {
            return;
        }
        // Written through a temporary and renamed: a ctrl-c mid-write used to leave
        // a truncated document, which the version check then silently threw away
        // along with every entry in it.
        let written = encode_container(version, entries)
            .is_some_and(|file| crate::fix::write_bytes_atomically(&path, &file).is_ok());
        if written {
            let _ = std::fs::remove_file(directory.join(LEGACY_CACHE_FILE));
        }
    }
}

/// A cache that is still being read. The scan starts walking while a thread loads the
/// file, and a walker thread waits only when it has a file to look up, so the
/// directories it lists and the files it opens before then overlap the load.
pub struct Loading {
    cache: OnceLock<Cache>,
    done: Mutex<bool>,
    signal: Condvar,
}

impl Loading {
    pub fn new() -> Self {
        Self {
            cache: OnceLock::new(),
            done: Mutex::new(false),
            signal: Condvar::new(),
        }
    }

    /// Run on the loading thread. Waiters are released even if reading panics, so a
    /// broken cache costs the scan its cache and not the scan itself.
    pub fn load(&self, root: &Path, settings: &Resolved) {
        let _release = Release(self);
        let _ = self.cache.set(Cache::open(root, settings));
    }

    /// `None` when the load failed.
    pub fn wait(&self) -> Option<&Cache> {
        let mut done = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*done {
            done = self
                .signal
                .wait(done)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        self.cache.get()
    }

    pub fn into_cache(self) -> Option<Cache> {
        self.cache.into_inner()
    }
}

struct Release<'loading>(&'loading Loading);

impl Drop for Release<'_> {
    fn drop(&mut self) {
        *self
            .0
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.0.signal.notify_all();
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

    fn a_scratch_project(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rqx-{name}-{}", std::process::id()));
        std::fs::create_dir_all(root.join(CACHE_DIRECTORY)).unwrap();
        root
    }

    fn analysis_of(source: &str) -> FileAnalysis {
        rqx_lang::analyze(Path::new("a.ts"), "a.ts", source, &RqxConfig::default()).unwrap()
    }

    fn write_cache(root: &Path, settings: &Resolved, packed: BTreeMap<String, Packed>) -> PathBuf {
        let path = root.join(CACHE_DIRECTORY).join(CACHE_FILE);
        let file = encode_container(analysis_version(settings), packed).unwrap();
        std::fs::write(&path, file).unwrap();
        path
    }

    /// Hits are carried over as the bytes they were read as, so the entry is neither
    /// decoded nor compressed again just because another file changed.
    #[test]
    fn a_hit_is_written_back_as_the_bytes_it_was_read_as() {
        let root = a_scratch_project("untouched-hit");
        let settings = Resolved::of(RqxConfig::default());
        let source = "export const a = 1;\n";
        let mut bytes = pack(&analysis_of(source)).unwrap();
        bytes.extend_from_slice(b"trailing bytes zlib stops before");
        let path = write_cache(
            &root,
            &settings,
            BTreeMap::from([(
                "a.ts".to_owned(),
                Packed {
                    fingerprint: fingerprint(source),
                    bytes: bytes.clone(),
                },
            )]),
        );

        let cache = Cache::open(&root, &settings);
        assert!(cache.get("a.ts", source).is_some());
        cache.put(
            "b.ts",
            "export const b = 2;\n",
            &analysis_of("export const b = 2;\n"),
        );
        cache.save();

        let (index, blobs) = read_container(&path).unwrap();
        let slot = &index.entries["a.ts"];
        assert_eq!(&blobs[slot.offset..slot.offset + slot.length], bytes);
        assert!(index.entries.contains_key("b.ts"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_entry_that_no_longer_decodes_is_a_miss() {
        let root = a_scratch_project("undecodable-entry");
        let settings = Resolved::of(RqxConfig::default());
        let source = "export const a = 1;\n";
        write_cache(
            &root,
            &settings,
            BTreeMap::from([(
                "a.ts".to_owned(),
                Packed {
                    fingerprint: fingerprint(source),
                    bytes: b"not compressed at all".to_vec(),
                },
            )]),
        );

        let cache = Cache::open(&root, &settings);

        assert!(cache.get("a.ts", source).is_none());
        assert_eq!(cache.miss_count(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_truncated_file_is_an_empty_cache_rather_than_an_error() {
        let root = a_scratch_project("truncated");
        let settings = Resolved::of(RqxConfig::default());
        let source = "export const a = 1;\n";
        let path = write_cache(
            &root,
            &settings,
            BTreeMap::from([(
                "a.ts".to_owned(),
                Packed {
                    fingerprint: fingerprint(source),
                    bytes: pack(&analysis_of(source)).unwrap(),
                },
            )]),
        );
        let whole = std::fs::read(&path).unwrap();

        for kept in [3, MAGIC.len() + 2, whole.len() - 4] {
            std::fs::write(&path, &whole[..kept]).unwrap();
            let cache = Cache::open(&root, &settings);
            assert!(cache.get("a.ts", source).is_none(), "{kept} bytes kept");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn identical_content_fingerprints_alike_and_a_single_edit_does_not() {
        assert_eq!(fingerprint("const a = 1;"), fingerprint("const a = 1;"));
        assert_ne!(fingerprint("const a = 1;"), fingerprint("const a = 2;"));
    }
}

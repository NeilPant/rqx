use std::collections::{BTreeMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rqx_core::identity::Identity;
use rqx_lang::FileAnalysis;
use serde::{Deserialize, Serialize};

use crate::config::Resolved;

pub(crate) const CACHE_DIRECTORY: &str = ".rqx";
pub(crate) const CACHE_FILE: &str = "cache.bin";
/// What the cache was called while it was one uncompressed document. Nothing reads it
/// any more, so it is removed rather than left to take up the room the new one saves.
const LEGACY_CACHE_FILE: &str = "cache.json";
/// What `.rqx/` keeps out of git: the per-machine files rqx writes there, named one by
/// one rather than `*`, because `.rqx/baseline.json` is meant to be committed.
const OWN_GITIGNORE: &str = "# Written by rqx. These files are per machine; commit baseline.json.\n\
                             cache.bin\n\
                             cache.json\n\
                             .cache.bin.rqx-tmp\n\
                             trend.ndjson\n";

/// Part of the version, so a change to how entries are laid out makes every cache
/// written before it a miss instead of a parse error.
const FORMAT: u32 = 3;

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
    /// What the file looked like when it was read, so a later run that sees the same
    /// length and modification time can skip reading it at all.
    stamp: Option<Stamp>,
    offset: usize,
    length: usize,
}

/// What a directory entry says about a file without opening it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Stamp {
    pub len: u64,
    pub mtime_ns: u64,
}

impl Stamp {
    pub fn of(metadata: &std::fs::Metadata) -> Option<Self> {
        Some(Self {
            len: metadata.len(),
            mtime_ns: nanoseconds(metadata.modified().ok()?)?,
        })
    }
}

fn nanoseconds(time: SystemTime) -> Option<u64> {
    u64::try_from(time.duration_since(UNIX_EPOCH).ok()?.as_nanos()).ok()
}

/// A file edited within this long of the cache being written cannot be told from the
/// version the cache saw by its length and modification time alone: a coarse clock
/// gives both edits the same time. Such an entry is checked against the content.
const RACY_WINDOW_NS: u64 = 2_000_000_000;

/// What a hit hands back: the analysis, and the identity of each of its findings, so a
/// baseline can be applied without the source text the identities were made from.
#[derive(Deserialize)]
pub struct Cached {
    pub analysis: FileAnalysis,
    pub identities: Vec<Identity>,
}

#[derive(Serialize)]
struct Packing<'a> {
    analysis: &'a FileAnalysis,
    identities: &'a [Identity],
}

#[derive(Default, Deserialize, Serialize)]
struct Index {
    /// Bumped whenever a rule changes what it reports, because a cached finding from
    /// an older binary is worse than no cache: it is silently wrong.
    version: u64,
    /// When the file was saved, which is what decides whether an entry's modification
    /// time is old enough to trust.
    written_at_ns: u64,
    entries: BTreeMap<String, Slot>,
}

/// One file's analysis, compressed, and the fingerprint of the source it came from.
struct Packed {
    fingerprint: u64,
    stamp: Option<Stamp>,
    bytes: Vec<u8>,
}

pub struct Cache {
    path: PathBuf,
    version: u64,
    loaded_in: std::time::Duration,
    written_at_ns: u64,
    index: BTreeMap<String, Slot>,
    blobs: Vec<u8>,
    fresh: Mutex<BTreeMap<String, Packed>>,
    hits: Mutex<HashSet<String>>,
    /// Files served by their content whose stored stamp no longer describes them, so the
    /// next run can serve them by stamp.
    restamped: Mutex<BTreeMap<String, Stamp>>,
    misses: AtomicU32,
}

/// Everything that changes what analysing a file produces. A cache keyed on file
/// content alone would serve stale findings the moment a threshold moved, and the
/// signature is hashed whole rather than field by field because the list that was
/// written out by hand covered eight of the seventeen thresholds and nothing said so.
fn analysis_version(settings: &Resolved) -> u64 {
    analysis_version_of_format(FORMAT, settings)
}

fn analysis_version_of_format(format: u32, settings: &Resolved) -> u64 {
    let mut hasher = DefaultHasher::new();
    format.hash(&mut hasher);
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    settings.signature().hash(&mut hasher);
    catalogue_version().hash(&mut hasher);
    settings.cost_signature().hash(&mut hasher);
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

fn encode_container(
    version: u64,
    written_at_ns: u64,
    packed: BTreeMap<String, Packed>,
) -> Option<Vec<u8>> {
    let mut index = Index {
        version,
        written_at_ns,
        entries: BTreeMap::new(),
    };
    let mut blobs = Vec::new();
    for (key, entry) in packed {
        index.entries.insert(
            key,
            Slot {
                fingerprint: entry.fingerprint,
                stamp: entry.stamp,
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

fn pack(analysis: &FileAnalysis, identities: &[Identity]) -> Option<Vec<u8>> {
    let json = serde_json::to_vec(&Packing {
        analysis,
        identities,
    })
    .ok()?;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&json).ok()?;
    encoder.finish().ok()
}

fn unpack(bytes: &[u8]) -> Option<Cached> {
    let mut json = Vec::new();
    flate2::read::ZlibDecoder::new(bytes)
        .read_to_end(&mut json)
        .ok()?;
    serde_json::from_slice(&json).ok()
}

/// Writes `.rqx/.gitignore` when the directory has none, so the cache and the trend
/// file cannot be committed by accident. One that exists is the project's and is never
/// touched. Best effort: a directory that refuses the file costs the file, not the run.
pub(crate) fn ignore_own_files(directory: &Path) {
    let created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(".gitignore"));
    if let Ok(mut file) = created {
        let _ = file.write_all(OWN_GITIGNORE.as_bytes());
    }
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
            .unwrap_or_default();
        Self {
            path,
            version,
            loaded_in: began.elapsed(),
            written_at_ns: index.written_at_ns,
            index: index.entries,
            blobs,
            fresh: Mutex::new(BTreeMap::new()),
            hits: Mutex::new(HashSet::new()),
            restamped: Mutex::new(BTreeMap::new()),
            misses: AtomicU32::new(0),
        }
    }

    /// How long reading the index took, on whichever thread did it.
    pub fn loaded_in(&self) -> std::time::Duration {
        self.loaded_in
    }

    /// The entry for a file whose length and modification time are what they were when
    /// it was cached, and old enough that an edit since could not have kept them. Asked
    /// before the file is opened; `None` is not a miss, because `get` still has the
    /// content to look at.
    pub fn unchanged(&self, key: &str, stamp: Stamp) -> Option<Cached> {
        let slot = self.index.get(key)?;
        let cached = (slot.stamp == Some(stamp) && self.settled(stamp))
            .then(|| blob(&self.blobs, slot))
            .flatten()
            .and_then(unpack)?;
        self.note_hit(key);
        Some(cached)
    }

    /// Decoded here, on the thread that asked, so the cost of a hit is spread across
    /// the walk instead of being paid up front for every entry. An entry that no longer
    /// decodes is a miss: the file is analysed again and its entry replaced.
    pub fn get(&self, key: &str, source: &str, stamp: Option<Stamp>) -> Option<Cached> {
        let served = self
            .index
            .get(key)
            .filter(|slot| slot.fingerprint == fingerprint(source))
            .and_then(|slot| {
                let cached = blob(&self.blobs, slot).and_then(unpack)?;
                Some((slot, cached))
            });
        let Some((slot, cached)) = served else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        self.note_hit(key);
        if let Some(stamp) =
            stamp.filter(|stamp| slot.stamp != Some(*stamp) || !self.settled(*stamp))
        {
            self.note_restamp(key, stamp);
        }
        Some(cached)
    }

    fn settled(&self, stamp: Stamp) -> bool {
        stamp.mtime_ns.saturating_add(RACY_WINDOW_NS) <= self.written_at_ns
    }

    fn note_hit(&self, key: &str) {
        if let Ok(mut hits) = self.hits.lock() {
            hits.insert(key.to_owned());
        }
    }

    fn note_restamp(&self, key: &str, stamp: Stamp) {
        if let Ok(mut restamped) = self.restamped.lock() {
            restamped.insert(key.to_owned(), stamp);
        }
    }

    pub fn hit_count(&self) -> u32 {
        self.hits.lock().map_or(0, |hits| hits.len() as u32)
    }

    pub fn miss_count(&self) -> u32 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Compressed by the caller's thread, which is a walker, so a cold run spreads the
    /// cost across every core instead of paying it in one piece when the cache is saved.
    pub fn put(
        &self,
        key: &str,
        source: &str,
        stamp: Option<Stamp>,
        analysis: &FileAnalysis,
        identities: &[Identity],
    ) {
        let Some(bytes) = pack(analysis, identities) else {
            return;
        };
        if let Ok(mut fresh) = self.fresh.lock() {
            fresh.insert(
                key.to_owned(),
                Packed {
                    fingerprint: fingerprint(source),
                    stamp,
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
            restamped,
            ..
        } = self;
        let (Ok(mut entries), Ok(hits), Ok(restamped)) = (
            fresh.into_inner(),
            hits.into_inner(),
            restamped.into_inner(),
        ) else {
            return;
        };
        if entries.is_empty() && restamped.is_empty() && hits.len() == index.len() {
            return;
        }
        for (key, slot) in &index {
            if let Some(bytes) = hits.contains(key).then(|| blob(&blobs, slot)).flatten() {
                entries.entry(key.clone()).or_insert_with(|| Packed {
                    fingerprint: slot.fingerprint,
                    stamp: restamped.get(key).copied().or(slot.stamp),
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
        ignore_own_files(directory);
        // Written through a temporary and renamed: a ctrl-c mid-write used to leave
        // a truncated document, which the version check then silently threw away
        // along with every entry in it.
        let written_at_ns = nanoseconds(SystemTime::now()).unwrap_or_default();
        let written = encode_container(version, written_at_ns, entries)
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

    use rqx_core::config::{Performance, RqxConfig};

    use super::*;

    fn with_performance(yaml: &str) -> Resolved {
        let performance: Performance = serde_saphyr::from_str(yaml).unwrap();
        Resolved::of(RqxConfig {
            performance,
            ..RqxConfig::default()
        })
    }

    /// An entry the cost extraction read a call through is baked into the cached
    /// analysis as an index into the tables, so a table that changed makes every cached
    /// analysis wrong, not only the files that call the entry.
    #[test]
    fn a_change_to_performance_costs_changes_the_key() {
        let none = analysis_version(&Resolved::of(RqxConfig::default()));
        let linear = "costs:\n  - {id: mine.scan, receiver: [list], method: scan, cost: n}\n";
        let constant = "costs:\n  - {id: mine.scan, receiver: [list], method: scan, cost: 1}\n";
        let renamed = "costs:\n  - {id: mine.look, receiver: [list], method: scan, cost: n}\n";
        let versions: Vec<u64> = [linear, constant, renamed]
            .iter()
            .map(|yaml| analysis_version(&with_performance(yaml)))
            .collect();
        for version in &versions {
            assert_ne!(*version, none);
        }
        assert_ne!(versions[0], versions[1]);
        assert_ne!(versions[0], versions[2]);
        assert_eq!(versions[0], analysis_version(&with_performance(linear)));
    }

    #[test]
    fn performance_max_order_and_client_names_change_the_key() {
        let base = analysis_version(&Resolved::of(RqxConfig::default()));
        for yaml in [
            "max_order: O(n log n)\n",
            "receivers: [gateway]\n",
            "methods: [pull]\n",
        ] {
            assert_ne!(base, analysis_version(&with_performance(yaml)), "{yaml}");
        }
    }

    #[test]
    fn a_cache_written_before_performance_costs_changed_is_a_miss() {
        let root = a_scratch_project("costs-changed");
        let before = Resolved::of(RqxConfig::default());
        write_cache(
            &root,
            &before,
            BTreeMap::from([("a.ts".to_owned(), packed(SOURCE, Some(stamped(SETTLED))))]),
        );
        assert!(stored(&root, &before).unwrap().current);

        let after = with_performance(
            "costs:\n  - {id: mine.scan, receiver: [list], method: scan, cost: n}\n",
        );
        assert!(!stored(&root, &after).unwrap().current);
        let cache = Cache::open(&root, &after);
        assert!(cache.get("a.ts", SOURCE, None).is_none());
        assert_eq!(cache.miss_count(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

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

    /// Every file's analysis runs the project's own queries, so a cache written under
    /// one query must not serve its findings once the query is edited.
    #[test]
    fn editing_a_custom_rule_invalidates_the_cache() {
        let custom = |query: &str, message: &str| rqx_core::config::CustomRule {
            id: "no-eval".to_owned(),
            languages: vec![rqx_core::Language::TypeScript],
            query: query.to_owned(),
            message: message.to_owned(),
            severity: rqx_core::Severity::Warning,
            family: rqx_core::rule::Category::Safety,
            confidence: rqx_core::Confidence::Verified,
            hint: String::new(),
        };
        let version_with = |custom: Vec<rqx_core::config::CustomRule>| {
            analysis_version(&Resolved::of(RqxConfig {
                custom,
                ..RqxConfig::default()
            }))
        };
        let query = "(call_expression function: (identifier) @site (#eq? @site \"eval\"))";
        let written = version_with(vec![custom(query, "eval runs text as code")]);

        assert_eq!(
            written,
            version_with(vec![custom(query, "eval runs text as code")])
        );
        assert_ne!(written, version_with(Vec::new()));
        assert_ne!(
            written,
            version_with(vec![custom(
                &query.replace("eval", "Function"),
                "eval runs text as code"
            )])
        );
        assert_ne!(written, version_with(vec![custom(query, "do not eval")]));
    }

    fn a_scratch_project(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rqx-{name}-{}", std::process::id()));
        std::fs::create_dir_all(root.join(CACHE_DIRECTORY)).unwrap();
        root
    }

    fn analysis_of(source: &str) -> FileAnalysis {
        rqx_lang::analyze(Path::new("a.ts"), "a.ts", source, &RqxConfig::default()).unwrap()
    }

    const WRITTEN_AT_NS: u64 = 1_000_000_000_000;

    fn write_cache(root: &Path, settings: &Resolved, packed: BTreeMap<String, Packed>) -> PathBuf {
        let path = root.join(CACHE_DIRECTORY).join(CACHE_FILE);
        let file = encode_container(analysis_version(settings), WRITTEN_AT_NS, packed).unwrap();
        std::fs::write(&path, file).unwrap();
        path
    }

    fn packed(source: &str, stamp: Option<Stamp>) -> Packed {
        Packed {
            fingerprint: fingerprint(source),
            stamp,
            bytes: pack(&analysis_of(source), &[]).unwrap(),
        }
    }

    fn stamped(mtime_ns: u64) -> Stamp {
        Stamp { len: 20, mtime_ns }
    }

    const SOURCE: &str = "export const a = 1;\n";
    const SETTLED: u64 = WRITTEN_AT_NS - 10_000_000_000;

    /// Hits are carried over as the bytes they were read as, so the entry is neither
    /// decoded nor compressed again just because another file changed.
    #[test]
    fn a_hit_is_written_back_as_the_bytes_it_was_read_as() {
        let root = a_scratch_project("untouched-hit");
        let settings = Resolved::of(RqxConfig::default());
        let mut entry = packed(SOURCE, None);
        entry
            .bytes
            .extend_from_slice(b"trailing bytes zlib stops before");
        let bytes = entry.bytes.clone();
        let path = write_cache(
            &root,
            &settings,
            BTreeMap::from([("a.ts".to_owned(), entry)]),
        );

        let cache = Cache::open(&root, &settings);
        assert!(cache.get("a.ts", SOURCE, None).is_some());
        cache.put(
            "b.ts",
            "export const b = 2;\n",
            None,
            &analysis_of("export const b = 2;\n"),
            &[],
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
        write_cache(
            &root,
            &settings,
            BTreeMap::from([(
                "a.ts".to_owned(),
                Packed {
                    fingerprint: fingerprint(SOURCE),
                    stamp: None,
                    bytes: b"not compressed at all".to_vec(),
                },
            )]),
        );

        let cache = Cache::open(&root, &settings);

        assert!(cache.get("a.ts", SOURCE, None).is_none());
        assert_eq!(cache.miss_count(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_truncated_file_is_an_empty_cache_rather_than_an_error() {
        let root = a_scratch_project("truncated");
        let settings = Resolved::of(RqxConfig::default());
        let path = write_cache(
            &root,
            &settings,
            BTreeMap::from([("a.ts".to_owned(), packed(SOURCE, None))]),
        );
        let whole = std::fs::read(&path).unwrap();

        for kept in [3, MAGIC.len() + 2, whole.len() - 4] {
            std::fs::write(&path, &whole[..kept]).unwrap();
            let cache = Cache::open(&root, &settings);
            assert!(
                cache.get("a.ts", SOURCE, None).is_none(),
                "{kept} bytes kept"
            );
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn cache_holding(name: &str, stamp: Stamp) -> (PathBuf, Cache) {
        let root = a_scratch_project(name);
        let settings = Resolved::of(RqxConfig::default());
        write_cache(
            &root,
            &settings,
            BTreeMap::from([("a.ts".to_owned(), packed(SOURCE, Some(stamp)))]),
        );
        let cache = Cache::open(&root, &settings);
        (root, cache)
    }

    #[test]
    fn a_file_with_the_stamp_it_was_cached_under_is_served_without_its_source() {
        let (root, cache) = cache_holding("settled-stamp", stamped(SETTLED));

        assert!(cache.unchanged("a.ts", stamped(SETTLED)).is_some());
        assert_eq!(cache.hit_count(), 1);
        assert_eq!(cache.miss_count(), 0);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_different_length_or_time_is_not_served_by_stamp() {
        let (root, cache) = cache_holding("other-stamp", stamped(SETTLED));

        let longer = Stamp {
            len: 21,
            ..stamped(SETTLED)
        };
        assert!(cache.unchanged("a.ts", longer).is_none());
        assert!(cache.unchanged("a.ts", stamped(SETTLED + 1)).is_none());
        assert_eq!(cache.miss_count(), 0, "the content check still has its say");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// An edit that kept the length and landed in the same clock tick as the entry's
    /// own time looks exactly like no edit, which is why such a file is read instead.
    #[test]
    fn a_stamp_inside_the_window_before_the_cache_was_written_is_not_trusted() {
        let racy = stamped(WRITTEN_AT_NS - RACY_WINDOW_NS + 1);
        let (root, cache) = cache_holding("racy-stamp", racy);

        assert!(cache.unchanged("a.ts", racy).is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_content_hit_under_a_new_stamp_is_saved_with_the_new_stamp() {
        let (root, cache) = cache_holding("restamp", stamped(SETTLED));
        let touched = stamped(SETTLED + 5);

        assert!(cache.get("a.ts", SOURCE, Some(touched)).is_some());
        cache.save();

        let (index, _) = read_container(&root.join(CACHE_DIRECTORY).join(CACHE_FILE)).unwrap();
        assert_eq!(index.entries["a.ts"].stamp, Some(touched));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn saving_keeps_the_cache_out_of_git_but_never_the_baseline() {
        let (root, cache) = cache_holding("ignores-itself", stamped(SETTLED));
        assert!(
            cache
                .get("a.ts", SOURCE, Some(stamped(SETTLED + 5)))
                .is_some()
        );
        cache.save();

        let written = std::fs::read_to_string(root.join(CACHE_DIRECTORY).join(".gitignore"))
            .expect("the cache writes .rqx/.gitignore");
        let patterns: Vec<&str> = written
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect();
        assert_eq!(
            patterns,
            [
                CACHE_FILE,
                LEGACY_CACHE_FILE,
                ".cache.bin.rqx-tmp",
                "trend.ndjson"
            ]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_ignore_file_the_project_already_has_is_left_alone() {
        let (root, cache) = cache_holding("keeps-ignore", stamped(SETTLED));
        let ignore = root.join(CACHE_DIRECTORY).join(".gitignore");
        std::fs::write(&ignore, "*\n").unwrap();
        assert!(
            cache
                .get("a.ts", SOURCE, Some(stamped(SETTLED + 5)))
                .is_some()
        );
        cache.save();

        assert_eq!(std::fs::read_to_string(&ignore).unwrap(), "*\n");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn skipping_generated_files_invalidates_the_cache() {
        let mut config = RqxConfig::default();
        let before = analysis_version(&Resolved::of(config.clone()));
        config.scan.skip_generated = !config.scan.skip_generated;
        assert_ne!(before, analysis_version(&Resolved::of(config)));
    }

    /// A cache written by the previous release holds analyses without the cost facts.
    /// Serving one would hide every function's cost until the file next changed, so the
    /// format number is part of the version and the whole file is a miss.
    #[test]
    fn a_cache_written_in_the_previous_format_is_a_miss() {
        let root = a_scratch_project("previous-format");
        let settings = Resolved::of(RqxConfig::default());
        let path = root.join(CACHE_DIRECTORY).join(CACHE_FILE);
        let previous = analysis_version_of_format(FORMAT - 1, &settings);
        let file = encode_container(
            previous,
            WRITTEN_AT_NS,
            BTreeMap::from([("a.ts".to_owned(), packed(SOURCE, Some(stamped(SETTLED))))]),
        )
        .unwrap();
        std::fs::write(&path, file).unwrap();

        assert!(!stored(&root, &settings).unwrap().current);
        let cache = Cache::open(&root, &settings);
        assert!(cache.unchanged("a.ts", stamped(SETTLED)).is_none());
        assert!(cache.get("a.ts", SOURCE, None).is_none());
        assert_eq!(cache.miss_count(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_entry_without_cost_facts_decodes_with_an_empty_file_cost() {
        let mut entry = serde_json::to_value(analysis_of(SOURCE)).unwrap();
        entry.as_object_mut().unwrap().remove("cost");
        let analysis: FileAnalysis = serde_json::from_value(entry).unwrap();
        assert!(analysis.cost.is_empty());
    }

    #[test]
    fn identical_content_fingerprints_alike_and_a_single_edit_does_not() {
        assert_eq!(fingerprint("const a = 1;"), fingerprint("const a = 1;"));
        assert_ne!(fingerprint("const a = 1;"), fingerprint("const a = 2;"));
    }
}

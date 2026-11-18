//! The scan pipeline: the public API, `ScanOutcome` and the types its stages share.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Instant;

use anyhow::Result;
use ignore::WalkState;
use rqx_core::Language;
use rqx_core::finding::{Evidence, Finding, Severity, SourceRange};
use rqx_core::impact::ImpactMap;
use rqx_core::rule::RuleId;
use rqx_core::score::FindingCounts;
use rqx_core::span::Span;

use crate::cache::Loading;
use crate::config::Resolved;
use crate::stats::{FileReads, ScanStats};

use analyze::{analyze_entry, skipped};
use collect::collect_against;
use walk::{count_files, report_left_out, walker_noting};

mod analyze;
mod collect;
mod cost_rules;
mod gitattributes;
pub mod skip;
mod stages;
mod submodules;
mod walk;

pub use skip::{SkipReason, Skipped};

pub(crate) use analyze::{analyze_file, relative_to};
pub(crate) use collect::collect;
pub(crate) use stages::{PORT_MIN_NODES, PORT_MIN_STATEMENTS};
pub(crate) use walk::{build_walker, files_of};

/// The walk analyzes files far faster than one collector drains them, so an
/// unbounded channel becomes the place a whole repo's findings pile up. Bounding
/// it makes producers wait instead, which is what keeps peak memory a function of
/// worker count rather than file count.
const CHANNEL_DEPTH: usize = 256;

/// Everything the collector needs about one file, and nothing it would have to work
/// out for itself. Boxed because the channel holds `CHANNEL_DEPTH` of these and the
/// alternative is a queue sized by the largest variant rather than by the common one.
#[derive(Clone)]
pub(crate) struct Analyzed {
    /// The path as the report will name it, resolved on the worker. The collector is
    /// the one thread the whole parallel walk funnels through, so work done here is
    /// work every other thread waits behind.
    display: String,
    /// `Some` only when a rule needs the import graph, since that is the only thing
    /// that wants the canonical form and resolving it is a syscall.
    canonical: Option<PathBuf>,
    language: Language,
    findings: Vec<Finding>,
    /// The identity of the finding at the same position, resolved on the worker where
    /// the file's text is in hand, or kept from the cache when it was not read.
    identities: Vec<rqx_core::Identity>,
    advice: Vec<rqx_core::Advice>,
    clones: Vec<rqx_lang::CloneSite>,
    module: rqx_core::facts::ModuleFacts,
    code_lines: u32,
    shape: rqx_lang::FileShape,
    suppressions: Vec<rqx_core::suppress::Suppression>,
    todos: Vec<rqx_core::facts::Spot>,
    cost: rqx_core::cost::FileCost,
}

#[derive(Clone)]
pub(crate) enum ScanMessage {
    File(Box<Analyzed>),
    Skipped(Skipped),
    /// How many files the walk will visit, sent once by whoever counted them. It
    /// arrives mid-scan, so until it does the collector reports no total.
    Total(Option<u32>),
}

/// The file is carried as the `Arc<str>` the worker already built for it, so naming
/// what is being scanned costs a refcount rather than a path clone per file. The
/// display throttles to something readable; the counts do not.
#[derive(Clone, Debug, Default)]
pub struct ScanProgress {
    pub files_scanned: u32,
    pub findings: u32,
    pub total: Option<u32>,
    pub phase: crate::progress::Phase,
    pub file: Option<std::sync::Arc<str>>,
    /// Since the scan began, so a reader can be given a rate and a time left rather
    /// than a count that means nothing without one.
    pub elapsed_ms: u64,
}

/// How many findings one file holds at each severity, and how many of those a
/// baseline has not already recorded. The same two tallies the project keeps in
/// `FindingCounts`, kept per file so a subset of files can be summed.
#[derive(Default, Clone)]
pub struct SeverityTally {
    pub by_severity: std::collections::BTreeMap<Severity, u32>,
    pub new_by_severity: std::collections::BTreeMap<Severity, u32>,
}

impl SeverityTally {
    pub fn record(&mut self, finding: &Finding) {
        *self.by_severity.entry(finding.severity).or_default() += 1;
        if !finding.known {
            *self.new_by_severity.entry(finding.severity).or_default() += 1;
        }
    }
}

pub struct ScanOutcome {
    /// Findings name files relative to what was scanned, so reading one back needs
    /// the root they are relative to rather than whatever directory rqx was run in.
    pub root: PathBuf,
    pub counts: FindingCounts,
    /// Every file the walk actually analyzed, so a diff can tell "changed" from
    /// "changed and relevant".
    pub analyzed: std::collections::BTreeSet<String>,
    pub impact: ImpactMap,
    /// importer -> imported, as display paths, so a diff can find dependents.
    pub import_edges: std::collections::BTreeMap<String, Vec<String>>,
    /// The rings, as display paths. Kept beside the findings because a graph view
    /// wants every cycle whether or not the rule that reports them is switched on.
    pub cycles: Vec<Vec<String>>,
    pub findings: Vec<Finding>,
    /// What the pattern catalogue makes of the same files. Kept apart from `findings`
    /// because nothing here is a defect: `counts` never sees it, so no advisory can
    /// move the health score.
    pub advice: Vec<rqx_core::Advice>,
    /// Findings per file before the display cap, so a file with nothing to report
    /// can be told from one whose findings the cap dropped.
    pub findings_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    /// Findings per file by severity, also before the display cap, so a review can
    /// count what a set of files holds without depending on the findings kept for
    /// display.
    pub severity_by_file: std::collections::BTreeMap<Arc<str>, SeverityTally>,
    /// Code lines per file, so a file can be graded on its own density rather than
    /// on the project's.
    pub lines_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    /// What the analyser measured per function and per class, whether or not any
    /// threshold was crossed — the distribution a reader wants is the same set of
    /// numbers the rules only see the tail of.
    pub shape_by_file: std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
    /// What each function costs once the functions it calls are folded in, by file and by
    /// where the function starts. Worked out again by every scan and never cached,
    /// because a callee edited in another file changes it for a caller that did not change.
    /// The review compares it with the base revision's to find a function that got slower.
    pub summaries: rqx_core::cost::Summaries,
    /// The facts the summaries were composed from, when the caller asked for them or the
    /// configuration reads them.
    pub cost_facts: Option<CostFacts>,
    /// Findings a waiver in the source asked rqx to leave out of the count. Kept so
    /// every waiver can be audited in one place rather than only being obeyed.
    pub suppressed: Vec<rqx_core::suppress::Suppressed>,
    /// How many findings the baseline recorded, when there is one. The ratchet
    /// compares the total against it.
    pub baseline_size: Option<u32>,
    /// Wall time per stage, so `--timings` can say where a slow scan went rather
    /// than leaving a reader to guess.
    pub timings: Vec<(&'static str, u128)>,
    /// What the scan did, as counts, so a change in the work can be told apart from a
    /// change in the machine.
    pub stats: ScanStats,
    /// Every `git` command the scan ran, as typed, in the order they were taken.
    pub git_commands: Vec<String>,
    /// The configuration files that were in effect, lowest layer first, then the policy.
    pub config_files: Vec<PathBuf>,
    /// The project's CODEOWNERS rules, when it has any. Kept beside the findings so
    /// every report answers who owns a file from the same reading.
    pub owners: Option<rqx_core::ownership::CodeOwners>,
    pub omitted: u32,
    pub code_lines: u64,
    pub files_scanned: u32,
    pub skipped: Vec<Skipped>,
    /// What a run spent its time in, when a profile was read. Per run rather than
    /// per file, so it never reaches the cache the shapes come from.
    pub profile: Option<rqx_core::runtime::Profile>,
    pub elapsed_ms: u128,
    /// Whether any config file was found, and whether git answered. Both are things a
    /// first run needs told once and a configured project never needs told at all.
    pub configured: bool,
    pub versioned: bool,
}

/// What one of the stages after the walk found. A stage reads the facts the walk
/// collected and answers with one of these instead of writing into the collector, which
/// is what lets the stages run side by side and be taken in a fixed order afterwards.
#[derive(Default)]
struct Staged {
    /// Proposed, not yet admitted: the collector admits each under the configuration
    /// of the file it is reported in.
    findings: Vec<Finding>,
    /// Advice never passes through `accept`: it must not reach `counts`, and the way
    /// to guarantee that is for it to have no route there at all.
    advice: Vec<rqx_core::Advice>,
    import_edges: std::collections::BTreeMap<String, Vec<String>>,
    cycles: Vec<Vec<String>>,
    importers: Vec<(Arc<str>, u32)>,
    summaries: Option<rqx_core::cost::Summaries>,
    call_links: Option<rqx_lang::cost::compose::CallLinks>,
    commits: Vec<(Arc<str>, u32)>,
    blames: u32,
    git_commands: Vec<String>,
}

impl Staged {
    /// A stage does not decide whether its finding is reported: a stage sees the whole
    /// project, and the answer belongs to the configuration of one file.
    fn propose(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    fn suggest(&mut self, advice: rqx_core::Advice) {
        self.advice.push(advice);
    }
}

/// One file's cost facts, under the name findings give the file.
pub struct CostFile {
    pub name: Arc<str>,
    pub cost: rqx_core::cost::FileCost,
}

/// The cost facts of every file a scan read, beside the modules they were read from, the
/// functions each call was taken to reach and the tables the facts index. A summary says
/// what a function costs and these say why. Kept only for a caller that reads them: a
/// review composes them again against the base revision, and the performance report
/// explains a cost from them. A scan that has no such caller does not hold them.
pub struct CostFacts {
    pub modules: Vec<rqx_lang::module_graph::ModuleEntry>,
    /// The cost facts of the file at the same position in `modules`.
    pub costs: Vec<CostFile>,
    pub links: rqx_lang::cost::compose::CallLinks,
    /// The tables the facts were extracted against. A library call in the facts is an
    /// index into them, so reading it back needs the same ones.
    pub tables: Arc<rqx_lang::cost::tables::CostTables>,
}

impl Default for CostFacts {
    fn default() -> Self {
        Self {
            modules: Vec::new(),
            costs: Vec::new(),
            links: rqx_lang::cost::compose::CallLinks::default(),
            tables: rqx_lang::cost::tables::CostTables::built_in(),
        }
    }
}

impl CostFacts {
    /// The facts of the file findings call `name`.
    pub fn of_file(&self, name: &str) -> Option<&rqx_core::cost::FileCost> {
        self.costs
            .iter()
            .find(|file| &*file.name == name)
            .map(|file| &file.cost)
    }

    pub fn function(&self, file: &str, start: u32) -> Option<&rqx_core::cost::FunctionCost> {
        self.of_file(file)?
            .functions
            .iter()
            .find(|function| function.start == start)
    }

    pub fn files(&self) -> impl Iterator<Item = rqx_lang::cost::compose::Analysed<'_>> {
        self.modules.iter().zip(&self.costs).map(|(module, file)| {
            rqx_lang::cost::compose::Analysed {
                name: &file.name,
                module,
                cost: &file.cost,
            }
        })
    }
}

/// What a scan holds on to, or is compared against, beyond its own answer. The
/// default is a plain scan.
#[derive(Default)]
pub(crate) struct Comparison {
    /// The functions of the base revision, when the scan is a review that compares
    /// with them.
    pub reference: Option<Arc<crate::reimplemented::Reference>>,
    /// Keep the cost facts in the outcome, for a caller that reads them again.
    pub keep_costs: bool,
}

impl Comparison {
    /// A scan that keeps the cost facts, for the performance report and `--explain`.
    pub fn costed() -> Self {
        Self {
            reference: None,
            keep_costs: true,
        }
    }

    /// Whether the scan keeps the cost facts: when the request asks for them, or when
    /// the configuration holds a limit that reads them.
    fn keeps_costs(&self, settings: &Resolved) -> bool {
        self.keep_costs || settings.config.performance.max_order.is_some()
    }
}

fn build(
    rule: RuleId,
    location: SourceRange,
    evidence: Evidence,
    fix_hint: String,
    related: Vec<SourceRange>,
) -> Finding {
    let spec = rule.spec();
    Finding {
        rule,
        location,
        severity: spec.severity,
        confidence: spec.confidence,
        evidence,
        fix_hint,
        related,
        known: false,
        focus: None,
    }
}

/// Graph findings are discovered after the file's text is gone, so the line and
/// column are resolved by re-reading it; a file that vanished mid-scan simply
/// drops its finding rather than reporting a position that no longer exists.
/// Reading is memoised because a module with twelve dead exports is one file, and
/// the naive version read it twelve times.
/// Where a graph finding is, and what to call the file it is in. Both answers need the
/// file the walker has already moved on from, so both are cached here.
struct Positions {
    files: HashMap<PathBuf, Option<ReadFile>>,
    root: PathBuf,
    /// The walker yields paths as given while the module graph works in canonical ones,
    /// so `strip_prefix` fails for every graph path and the fallback resolved *both*
    /// sides again — two syscalls per import edge. Resolving the root once is the whole
    /// difference.
    canonical_root: Option<PathBuf>,
}

/// A file's text and where its lines begin, worked out the first time a finding is
/// placed in it.
struct ReadFile {
    text: String,
    lines: rqx_core::LineStarts,
}

impl ReadFile {
    fn read(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let lines = rqx_core::LineStarts::new(&text);
        Some(Self { text, lines })
    }
}

impl Positions {
    fn new(root: &Path) -> Self {
        Self {
            files: HashMap::new(),
            root: root.to_path_buf(),
            canonical_root: rqx_core::paths::canonicalize(root).ok(),
        }
    }

    fn relative(&self, path: &Path) -> String {
        if let Ok(relative) = path.strip_prefix(&self.root) {
            return rqx_core::paths::display_path(relative);
        }
        if let Some(root) = &self.canonical_root
            && let Ok(relative) = path.strip_prefix(root)
        {
            return rqx_core::paths::display_path(relative);
        }
        if let (Some(root), Ok(canonical)) =
            (&self.canonical_root, rqx_core::paths::canonicalize(path))
            && let Ok(relative) = canonical.strip_prefix(root)
        {
            return rqx_core::paths::display_path(relative);
        }
        rqx_core::paths::display_path(path)
    }

    /// The text of a line, without the indentation in front of it.
    fn line_range(&mut self, path: &Path, line: u32) -> Option<SourceRange> {
        let span = {
            let read = self
                .files
                .entry(path.to_path_buf())
                .or_insert_with(|| ReadFile::read(path))
                .as_ref()?;
            let start = read.lines.line_start(line) as usize;
            let text = read.text.get(start..)?.lines().next().unwrap_or_default();
            let indent = text.len() - text.trim_start().len();
            Span::new(
                (start + indent) as u32,
                (start + text.trim_end().len()) as u32,
            )
        };
        self.range(path, span)
    }

    fn range(&mut self, path: &Path, span: Span) -> Option<SourceRange> {
        let file: Arc<str> = Arc::from(self.relative(path));
        let read = self
            .files
            .entry(path.to_path_buf())
            .or_insert_with(|| ReadFile::read(path))
            .as_ref()?;
        let (line, column) = read.lines.locate(&read.text, span.start);
        let (end_line, _) = read.lines.locate(&read.text, span.end);
        Some(SourceRange {
            file,
            start: span.start,
            end: span.end,
            line,
            column,
            end_line,
        })
    }
}

/// A caller following a scan, and the switch it stops one with. The two travel
/// together because anything drawing progress is also the only thing in a position
/// to decide the answer is no longer wanted.
#[derive(Clone)]
pub struct Watch {
    pub updates: SyncSender<ScanProgress>,
    pub cancel: Arc<AtomicBool>,
}

impl Watch {
    pub fn new(updates: SyncSender<ScanProgress>) -> Self {
        Self {
            updates,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn stopped(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// A scan the caller asked to stop. Distinct from a failure: nothing went wrong, so
/// nothing is reported and the partial answer is thrown away rather than shown as if
/// it described the project.
pub const CANCELLED: &str = "the scan was cancelled";

pub fn scan(root: &Path, settings: &Resolved) -> Result<ScanOutcome> {
    scan_with(root, settings, None, CachePolicy::FromSettings)
}

/// A scan that neither reads nor writes the analysis cache, for callers that scan
/// directories they don't own, such as the fixtures the generated docs quote.
pub fn scan_uncached(root: &Path, settings: &Resolved) -> Result<ScanOutcome> {
    scan_with(root, settings, None, CachePolicy::Off)
}

pub fn scan_with_progress(root: &Path, settings: &Resolved, watch: Watch) -> Result<ScanOutcome> {
    scan_with(root, settings, Some(watch), CachePolicy::FromSettings)
}

/// A scan whose progress is drawn on stderr as `mode` says. The command line's way of
/// scanning; the library entry points stay silent.
pub fn scan_drawn(
    root: &Path,
    settings: &Resolved,
    mode: crate::progress::Mode,
) -> Result<ScanOutcome> {
    scan_drawn_against(root, settings, mode, Comparison::default())
}

/// A scan that keeps the cost facts, for the callers that explain a cost: the session and
/// the MCP server, which a person or an agent asks about any function.
pub fn scan_costed(root: &Path, settings: &Resolved, watch: Option<Watch>) -> Result<ScanOutcome> {
    scan_against(
        root,
        settings,
        watch,
        CachePolicy::FromSettings,
        Comparison::costed(),
        None,
    )
}

/// `scan_drawn` for a caller that holds on to more than the answer, as `comparison` says.
pub(crate) fn scan_drawn_against(
    root: &Path,
    settings: &Resolved,
    mode: crate::progress::Mode,
    comparison: Comparison,
) -> Result<ScanOutcome> {
    crate::progress::drawing(mode, |watch| {
        scan_against(
            root,
            settings,
            watch,
            CachePolicy::FromSettings,
            comparison,
            None,
        )
    })
}

/// Who decides whether a scan reads and writes the cache. The commands leave it to the
/// settings; an in-process caller that has to prove the cache's behaviour cannot, because
/// a development build never uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum CachePolicy {
    Off,
    FromSettings,
    Forced,
}

impl CachePolicy {
    fn enabled(self, config: &rqx_core::config::RqxConfig) -> bool {
        match self {
            Self::Off => false,
            Self::Forced => true,
            // Never in a development build. The key covers the version, the settings and
            // the catalogue, but not the analyser itself — so a collector fixed between two
            // builds of the same version would go on serving the answer it gave before it
            // was fixed. That is a confusing hour for a contributor and it has already cost
            // two; a released binary carries a version that moves.
            // The environment turns it off for one run without editing a file anybody else
            // shares: a CI job proving a cache bug should not have to commit a config.
            Self::FromSettings => {
                config.scan.cache
                    && std::env::var_os("RQX_NO_CACHE").is_none()
                    && !cfg!(debug_assertions)
            }
        }
    }
}

pub(crate) fn scan_with(
    root: &Path,
    settings: &Resolved,
    watch: Option<Watch>,
    policy: CachePolicy,
) -> Result<ScanOutcome> {
    scan_against(root, settings, watch, policy, Comparison::default(), None)
}

/// A scan that also reports what it added to the functions of a base revision in
/// `comparison`. With `lent_cache`, the scan reads the cache of that directory instead of
/// its own and does not write one: a file whose content is the same in both trees is
/// then served from the other tree's cache, and nothing this tree holds can reach it.
pub(crate) fn scan_against(
    root: &Path,
    settings: &Resolved,
    watch: Option<Watch>,
    policy: CachePolicy,
    comparison: Comparison,
    lent_cache: Option<&Path>,
) -> Result<ScanOutcome> {
    let config = &settings.config;
    // Git depends on nothing the walk produces, so it reads while the files are being
    // parsed rather than after — four processes on the critical path became one, off it.
    let history = {
        let root = root.to_path_buf();
        let days = config.scan.churn_days;
        thread::spawn(move || crate::history::History::read(&root, days))
    };
    // Read beside the git history rather than before the walk: a compressed document
    // of several megabytes, which used to be dead time with every core idle. The walkers
    // wait for it only when they have a file to look up.
    let loading = policy.enabled(config).then(crate::cache::Loading::new);

    let (sender, receiver) = mpsc::sync_channel(CHANNEL_DEPTH);
    let counter = build_walker(root, config)?;
    let walker = walker_noting(
        root,
        config,
        report_left_out(root.to_path_buf(), config.clone(), sender.clone()),
    )?;
    let walks = AtomicU32::new(0);
    let reads = FileReads::default();

    let collector = thread::scope(|scope| {
        let settings_ref = &*settings;
        let watch = watch.as_ref();
        let root_ref = root;
        let handle = scope.spawn(move || {
            collect_against(
                receiver,
                root_ref,
                settings_ref,
                None,
                watch,
                comparison,
                || Arc::new(history.join().unwrap_or_default()),
            )
        });
        let loading = loading.as_ref();
        if let Some(loading) = loading {
            scope.spawn(move || loading.load(lent_cache.unwrap_or(root_ref), settings_ref));
        }
        // The count only decorates the bar, so it runs beside the walk and sends its
        // total when it is done; until then the bar is indeterminate.
        let counting = sender.clone();
        let walks = &walks;
        let counted = scope.spawn(move || {
            let total = count_files(counter, watch);
            walks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let _ = counting.send(ScanMessage::Total(total));
        });
        let reads = &reads;
        walker.run(|| {
            let sender = sender.clone();
            let settings = &*settings;
            let mut cached = None;
            Box::new(move |entry| {
                if watch.is_some_and(Watch::stopped) {
                    return WalkState::Quit;
                }
                match entry {
                    Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => {
                        let cache = *cached.get_or_insert_with(|| loading.and_then(Loading::wait));
                        analyze_entry(&entry, root, settings, cache, reads, &sender);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let _ = sender.send(skipped(
                            Path::new("<walk>"),
                            SkipReason::Walk,
                            error.to_string(),
                        ));
                    }
                }
                WalkState::Continue
            })
        });
        walks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        drop(sender);
        let _ = counted.join();
        handle.join()
    });

    if watch.is_some_and(|watch| watch.stopped()) {
        anyhow::bail!(CANCELLED);
    }

    let mut outcome = collector.map_err(|_| anyhow::anyhow!("the collector thread panicked"))?;
    outcome.stats.walks = walks.into_inner();
    outcome.stats.files_read = reads.total();
    if let Some(cache) = loading.and_then(Loading::into_cache) {
        outcome.stats.cache_hits = cache.hit_count();
        outcome.stats.cache_misses = cache.miss_count();
        let saving = Instant::now();
        let loaded = cache.loaded_in().as_millis();
        if lent_cache.is_none() {
            cache.save();
        }
        outcome.timings.push(("cache load", loaded));
        outcome
            .timings
            .push(("cache save", saving.elapsed().as_millis()));
    }
    Ok(outcome)
}

#[cfg(test)]
mod cost_tests;

#[cfg(test)]
mod isolation_tests;

#[cfg(test)]
mod tests;

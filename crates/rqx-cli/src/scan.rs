use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result};
use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;
use ignore::{WalkBuilder, WalkState};
use rqx_core::Language;
use rqx_core::config::RqxConfig;
use rqx_core::finding::{Evidence, Finding, Severity, SourceRange};
use rqx_core::impact::ImpactMap;
use rqx_core::rule::RuleId;
use rqx_core::score::FindingCounts;
use rqx_core::span::Span;
use smallvec::SmallVec;

use crate::cache::{Cached, Loading, Stamp};
use crate::config::Resolved;
use crate::stats::{FileReads, ScanStats};

mod cost_rules;

/// The walk analyzes files far faster than one collector drains them, so an
/// unbounded channel becomes the place a whole repo's findings pile up. Bounding
/// it makes producers wait instead, which is what keeps peak memory a function of
/// worker count rather than file count.
const CHANNEL_DEPTH: usize = 256;
const MAX_CLONE_SITES: usize = 8;
/// What a block needs before sharing its shape and constants with a block in another
/// language says more than sharing a few branches and returns.
pub(crate) const PORT_MIN_STATEMENTS: u32 = 8;
pub(crate) const PORT_MIN_NODES: u32 = 40;

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
    Skipped {
        path: PathBuf,
        reason: String,
    },
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
    pub skipped: Vec<(PathBuf, String)>,
    /// What a run spent its time in, when a profile was read. Per run rather than
    /// per file, so it never reaches the cache the shapes come from.
    pub profile: Option<rqx_core::runtime::Profile>,
    pub elapsed_ms: u128,
    /// Whether any config file was found, and whether git answered. Both are things a
    /// first run needs told once and a configured project never needs told at all.
    pub configured: bool,
    pub versioned: bool,
}

struct Ranked(Finding);

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Ranked {}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ranked {
    /// The report order turned around, so the heap's smallest is the finding reported
    /// last. This order decides which findings the display cap keeps, so it has to be
    /// the same total key the findings are finally sorted by.
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.report_order(&self.0)
    }
}

/// Sites are kept in source order rather than arrival order and truncated from
/// the end, so the set retained for a group with more than `MAX_CLONE_SITES`
/// occurrences does not depend on which worker reached which file first.
fn insert_bounded(sites: &mut SmallVec<[SourceRange; 2]>, range: SourceRange) {
    let position = sites
        .partition_point(|existing| (&existing.file, existing.start) < (&range.file, range.start));
    if position >= MAX_CLONE_SITES {
        return;
    }
    sites.insert(position, range);
    sites.truncate(MAX_CLONE_SITES);
}

struct CloneGroup {
    occurrences: u32,
    statements: u32,
    nodes: u32,
    sites: SmallVec<[SourceRange; 2]>,
}

struct PortedGroup {
    statements: u32,
    nodes: u32,
    literals: u64,
    languages: std::collections::BTreeSet<Language>,
    sites: SmallVec<[SourceRange; 2]>,
}

impl PortedGroup {
    /// A block copied between TypeScript and JavaScript is plain duplication, which
    /// `duplicate-code` already reports; only another family makes it a port.
    fn families(&self) -> std::collections::BTreeSet<Language> {
        self.languages
            .iter()
            .map(|language| language.family())
            .collect()
    }
}

/// A stage that panicked takes the scan with it, as it did when the stages ran in line.
fn joined<T>(stage: thread::ScopedJoinHandle<'_, T>) -> T {
    stage
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// A finding the rules let through for its file, with the project's per-rule options
/// applied, at the confidence a run of the code leaves it. A run that reached the code contradicts the graph that called it dead: the
/// finding is kept and shown — the graph may still be right about the import — but at
/// a confidence that keeps it out of the health score.
fn offered(config: &RqxConfig, mut finding: Finding) -> Option<Finding> {
    if !config.rules.admits(finding.rule, &finding.location.file) {
        return None;
    }
    config.rules.adjust(&mut finding);
    if let Some(lowered) = rqx_core::runtime::contradicted(finding.rule)
        && ran(&finding.evidence)
    {
        finding.confidence = finding.confidence.min(lowered);
    }
    Some(finding)
}

/// What one of the stages after the walk found. A stage reads the facts the walk
/// collected and answers with one of these instead of writing into the collector, which
/// is what lets the stages run side by side and be taken in a fixed order afterwards.
#[derive(Default)]
struct Staged {
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
    fn accept_if_enabled(&mut self, config: &RqxConfig, finding: Finding) {
        self.findings.extend(offered(config, finding));
    }

    fn advise_if_enabled(&mut self, config: &RqxConfig, advice: rqx_core::Advice) {
        if config.patterns.enabled(advice.advisory) {
            self.advice.push(advice);
        }
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

/// Findings are capped for display while `counts` stays exact, because a health
/// score that moved with the display cap would not mean anything.
struct Collector {
    counts: FindingCounts,
    retained: BinaryHeap<Reverse<Ranked>>,
    max_findings: usize,
    findings_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    severity_by_file: std::collections::BTreeMap<Arc<str>, SeverityTally>,
    lines_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    suppressions: HashMap<Arc<str>, Vec<rqx_core::suppress::Suppression>>,
    /// Where each file records work still to do, so the history pass can ask how
    /// long each note has been there.
    todos: HashMap<Arc<str>, Vec<rqx_core::facts::Spot>>,
    cycles: Vec<Vec<String>>,
    baseline: Option<crate::baseline::Matcher>,
    baseline_findings: Option<u32>,
    /// Only the files a stage's finding names, read once each.
    texts: HashMap<Arc<str>, String>,
    root: PathBuf,
    suppressed: Vec<rqx_core::suppress::Suppressed>,
    today: String,
    phase: crate::progress::Phase,
    phase_began: std::time::Instant,
    timings: Vec<(&'static str, u128)>,
    stats: ScanStats,
    git_commands: Vec<String>,
    /// Where progress goes, whoever is drawing it: the command line's bar, the
    /// interface, the language server.
    watcher: Option<SyncSender<ScanProgress>>,
    showing: Option<Arc<str>>,
    shape_by_file: std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
    advice: Vec<rqx_core::Advice>,
    clone_groups: HashMap<u64, CloneGroup>,
    /// The same blocks keyed by a shape no grammar owns, so logic ported between two
    /// languages can be found. Kept apart from `clone_groups` because a group here is
    /// only interesting when it spans languages.
    ported: HashMap<u64, PortedGroup>,
    modules: Vec<rqx_lang::module_graph::ModuleEntry>,
    /// The cost facts of the file at the same position in `modules`.
    costs: Vec<CostFile>,
    /// The lines of each file where a per-file rule already reports work in a loop, for the
    /// cost stage to leave out of the rules that stand in for them.
    covered: HashMap<Arc<str>, Vec<u32>>,
    summaries: rqx_core::cost::Summaries,
    keep_costs: bool,
    call_links: rqx_lang::cost::compose::CallLinks,
    cost_facts: Option<CostFacts>,
    analyzed: std::collections::BTreeSet<String>,
    impact: ImpactMap,
    import_edges: std::collections::BTreeMap<String, Vec<String>>,
    track_clones: bool,
    track_ported: bool,
    track_modules: bool,
    /// The base revision's functions, when this scan is a review that compares with it.
    reference: Option<Arc<crate::reimplemented::Reference>>,
    /// What a test run reached, when the project pointed at a coverage file.
    coverage: Option<rqx_core::Hits>,
    /// What a run actually spent its time in, when the project pointed at a profile.
    profile: Option<rqx_core::runtime::Samples>,
    code_lines: u64,
    files_scanned: u32,
    skipped: Vec<(PathBuf, String)>,
    total: Option<u32>,
    started: Instant,
}

impl Collector {
    fn new(
        max_findings: usize,
        track_clones: bool,
        track_ported: bool,
        track_modules: bool,
        total: Option<u32>,
    ) -> Self {
        Self {
            counts: FindingCounts::default(),
            retained: BinaryHeap::new(),
            max_findings,
            findings_by_file: std::collections::BTreeMap::new(),
            severity_by_file: std::collections::BTreeMap::new(),
            lines_by_file: std::collections::BTreeMap::new(),
            suppressions: HashMap::new(),
            todos: HashMap::new(),
            cycles: Vec::new(),
            baseline: None,
            baseline_findings: None,
            texts: HashMap::new(),
            root: PathBuf::new(),
            suppressed: Vec::new(),
            today: rqx_core::suppress::today(),
            phase: crate::progress::Phase::Scanning,
            phase_began: std::time::Instant::now(),
            timings: Vec::new(),
            stats: ScanStats::default(),
            git_commands: Vec::new(),
            watcher: None,
            showing: None,
            shape_by_file: std::collections::BTreeMap::new(),
            advice: Vec::new(),
            clone_groups: HashMap::new(),
            ported: HashMap::new(),
            modules: Vec::new(),
            costs: Vec::new(),
            covered: HashMap::new(),
            summaries: rqx_core::cost::Summaries::default(),
            keep_costs: false,
            call_links: rqx_lang::cost::compose::CallLinks::default(),
            cost_facts: None,
            analyzed: std::collections::BTreeSet::new(),
            impact: ImpactMap::default(),
            import_edges: std::collections::BTreeMap::new(),
            track_clones,
            track_ported,
            track_modules,
            reference: None,
            coverage: None,
            profile: None,
            code_lines: 0,
            files_scanned: 0,
            skipped: Vec::new(),
            total,
            started: Instant::now(),
        }
    }

    /// One update to whoever is drawing. A dropped update costs nothing: the reader
    /// only ever wants the latest, so a full channel means it is already current.
    fn tick(&self, phase: Option<crate::progress::Phase>) {
        let Some(watcher) = &self.watcher else {
            return;
        };
        let _ = watcher.try_send(ScanProgress {
            files_scanned: self.files_scanned,
            findings: self.counts.total,
            total: self.total,
            phase: phase.unwrap_or(self.phase),
            file: self.showing.clone(),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        });
    }

    fn accept_if_enabled(&mut self, config: &RqxConfig, finding: Finding) {
        if let Some(finding) = offered(config, finding) {
            self.accept_all(vec![(finding, None)]);
        }
    }

    /// What the stages after the walk found, taken in the order the stages come in
    /// rather than the order they finished.
    fn absorb(&mut self, staged: Staged) {
        self.accept_all(
            staged
                .findings
                .into_iter()
                .map(|finding| (finding, None))
                .collect(),
        );
        self.advice.extend(staged.advice);
        self.import_edges.extend(staged.import_edges);
        self.cycles.extend(staged.cycles);
        if let Some(summaries) = staged.summaries {
            self.summaries = summaries;
        }
        if let Some(links) = staged.call_links {
            self.call_links = links;
        }
        for (file, importers) in staged.importers {
            self.impact.set_importers(file, importers);
        }
        for (file, commits) in staged.commits {
            self.impact.set_commits(file, commits);
        }
        self.stats.blames += staged.blames;
        self.stats.git_processes += staged.blames;
        self.git_commands.extend(staged.git_commands);
    }

    /// The waiver that covers this finding, if its file wrote one. Cross-file
    /// findings anchor at line 1, which only a whole-file waiver can reach.
    fn waived(&self, finding: &Finding) -> Option<&rqx_core::suppress::Suppression> {
        self.suppressions
            .get(&finding.location.file)?
            .iter()
            .find(|waiver| waiver.covers(finding, &self.today))
    }

    /// Every finding arrives here: one file's off the channel, and each stage's built
    /// after it drained, whatever rule they are of. So this is the one place a waiver
    /// and the baseline reach all of them. A batch pairs each finding with its identity
    /// when a worker already resolved it. A waived finding is set aside before the
    /// baseline is consulted, because a baseline never records a waived finding and so
    /// one must not use up what the baseline recorded for the others.
    fn accept_all(&mut self, batch: Vec<(Finding, Option<rqx_core::Identity>)>) {
        let mut findings = Vec::with_capacity(batch.len());
        let mut identities = Vec::with_capacity(batch.len());
        for (finding, identity) in batch {
            let Some(finding) = self.unless_waived(finding) else {
                continue;
            };
            if self.baseline.is_some() {
                identities.push(identity.unwrap_or_else(|| self.identity_of(&finding)));
            }
            findings.push(finding);
        }
        if let Some(baseline) = &mut self.baseline {
            baseline.mark(&mut findings, &identities);
        }
        for finding in findings {
            self.record(finding);
        }
    }

    /// A stage's finding was built after the walk drained, so no worker resolved its
    /// identity. There are a handful of these against a file per worker, so reading
    /// their text here costs almost nothing.
    fn identity_of(&mut self, finding: &Finding) -> rqx_core::Identity {
        let text = self
            .texts
            .entry(Arc::clone(&finding.location.file))
            .or_insert_with(|| {
                std::fs::read_to_string(self.root.join(&*finding.location.file)).unwrap_or_default()
            });
        rqx_core::Identity::resolved(finding, text)
    }

    /// Sets the finding aside with the waiver that covers it, or hands it back.
    fn unless_waived(&mut self, finding: Finding) -> Option<Finding> {
        let Some(waiver) = self.waived(&finding) else {
            return Some(finding);
        };
        let waived = rqx_core::suppress::Suppressed {
            at: SourceRange {
                file: Arc::clone(&finding.location.file),
                start: waiver.span.start,
                end: waiver.span.end,
                line: waiver.line,
                column: 1,
                end_line: waiver.line,
            },
            reason: waiver.reason.clone(),
            until: waiver.until.clone(),
            finding,
        };
        self.suppressed.push(waived);
        None
    }

    fn record(&mut self, finding: Finding) {
        self.counts.record(&finding);
        *self
            .findings_by_file
            .entry(Arc::clone(&finding.location.file))
            .or_default() += 1;
        self.severity_by_file
            .entry(Arc::clone(&finding.location.file))
            .or_default()
            .record(&finding);
        let candidate = Ranked(finding);
        if self.max_findings == 0 || self.retained.len() < self.max_findings {
            self.retained.push(Reverse(candidate));
            return;
        }
        let outranks_worst = self
            .retained
            .peek()
            .is_some_and(|Reverse(worst)| candidate > *worst);
        if outranks_worst {
            self.retained.pop();
            self.retained.push(Reverse(candidate));
        }
    }

    fn record_clone(&mut self, site: rqx_lang::CloneSite, language: Language) {
        if !self.track_clones {
            return;
        }
        let group = self
            .clone_groups
            .entry(site.hash)
            .or_insert_with(|| CloneGroup {
                occurrences: 0,
                statements: site.statements,
                nodes: site.nodes,
                sites: SmallVec::new(),
            });
        group.occurrences += 1;
        insert_bounded(&mut group.sites, site.range.clone());

        if self.track_ported {
            let ported = self
                .ported
                .entry(site.shape)
                .or_insert_with(|| PortedGroup {
                    statements: site.statements,
                    nodes: site.nodes,
                    literals: site.literals,
                    languages: std::collections::BTreeSet::new(),
                    sites: SmallVec::new(),
                });
            ported.languages.insert(language);
            insert_bounded(&mut ported.sites, site.range);
        }
    }

    /// The phases after the walk used to run with the line already wiped, so a large
    /// repository ended every scan with a silent pause that looked like a hang.
    /// Each phase closes the one before it, so the walk is timed by the first call
    /// and every later stage by its own.
    fn report_phase(&mut self, phase: crate::progress::Phase) {
        let now = std::time::Instant::now();
        self.timings.push((
            self.phase.label(),
            now.duration_since(self.phase_began).as_millis(),
        ));
        self.phase = phase;
        self.phase_began = now;
        self.tick(Some(phase));
    }

    /// Blocks whose shape appears in more than one language. Same-language repeats
    /// are `duplicate-code`'s job and are already reported with a fingerprint that
    /// knows about identifiers; this only claims the narrower, louder thing.
    fn ported_findings(
        ported: HashMap<u64, PortedGroup>,
        root: &Path,
        settings: &Resolved,
    ) -> Staged {
        let mut staged = Staged::default();
        // Ported code keeps its constants even when its syntax changes completely, so
        // a shared literal is what separates the same rule written twice from two
        // blocks that merely branch and return in the same order.
        let mut groups: Vec<PortedGroup> = ported
            .into_values()
            .filter(|group| {
                group.families().len() > 1
                    && group.sites.len() > 1
                    && group.statements >= PORT_MIN_STATEMENTS
                    && group.nodes >= PORT_MIN_NODES
                    && group.literals != 0
            })
            .collect();
        groups.sort_by(|a, b| {
            a.sites[0]
                .file
                .cmp(&b.sites[0].file)
                .then(a.sites[0].start.cmp(&b.sites[0].start))
        });

        for group in groups {
            let anchor = group.sites[0].clone();
            let spec = RuleId::CrossLanguageDuplicate.spec();
            let named: Vec<&str> = group.languages.iter().map(|l| l.title()).collect();
            let config = settings.for_file(&root.join(anchor.file.as_ref()));
            staged.accept_if_enabled(
                config,
                Finding {
                    rule: RuleId::CrossLanguageDuplicate,
                    location: anchor,
                    severity: spec.severity,
                    confidence: spec.confidence,
                    evidence: Evidence::ClonedBlock {
                        statements: group.statements,
                        nodes: group.nodes,
                        occurrences: group.sites.len() as u32,
                    },
                    fix_hint: format!(
                        "the same shape is in {}; give the rule one home and call it from \
                         both, or write down why they are allowed to differ",
                        named.join(" and ")
                    ),
                    related: group.sites.iter().skip(1).cloned().collect(),
                    known: false,
                    focus: None,
                },
            );
        }
        staged
    }

    fn clone_findings(
        clone_groups: HashMap<u64, CloneGroup>,
        root: &Path,
        settings: &Resolved,
    ) -> Staged {
        let mut staged = Staged::default();
        let mut groups: Vec<CloneGroup> = clone_groups
            .into_values()
            .filter(|group| group.occurrences > 1)
            .collect();
        groups.sort_by(|a, b| {
            let (first, second) = (&a.sites[0], &b.sites[0]);
            first
                .file
                .cmp(&second.file)
                .then(first.start.cmp(&second.start))
                .then(second.end.cmp(&first.end))
                .then(b.nodes.cmp(&a.nodes))
        });

        // Sorted by (file, start, widest first), so anything containing a range comes
        // before it and the last one reported is the only one that can still contain
        // the next. Rescanning every range reported so far was quadratic on generated
        // code, where near-identical blocks number in the thousands.
        let mut outer: Option<SourceRange> = None;
        for group in groups {
            let anchor = group.sites[0].clone();
            if outer.as_ref().is_some_and(|outer| outer.contains(&anchor)) {
                continue;
            }
            outer = Some(anchor.clone());
            let spec = RuleId::DuplicateCode.spec();
            let others: Vec<SourceRange> = group.sites.iter().skip(1).cloned().collect();
            let config = settings.for_file(&root.join(anchor.file.as_ref()));
            staged.accept_if_enabled(config, Finding {
                rule: RuleId::DuplicateCode,
                location: anchor,
                severity: spec.severity,
                confidence: spec.confidence,
                evidence: Evidence::ClonedBlock {
                    statements: group.statements,
                    nodes: group.nodes,
                    occurrences: group.occurrences,
                },
                fix_hint: if group.occurrences as usize > group.sites.len() {
                    format!(
                        "extract the shared block into one function and call it from all {} \
                         sites ({} shown)",
                        group.occurrences,
                        group.sites.len()
                    )
                } else {
                    format!(
                        "extract the shared block into one function and call it from all {} sites",
                        group.occurrences
                    )
                },
                related: others,
                known: false,
                focus: None,
            });
        }
        staged
    }

    /// The patterns built on inheritance, which cannot be seen from one file in a
    /// language that puts one class in each.
    fn advise_hierarchy(
        modules: &[rqx_lang::module_graph::ModuleEntry],
        config: &RqxConfig,
        at: &mut Positions,
        staged: &mut Staged,
    ) {
        use rqx_lang::hierarchy::{Hierarchy, Shape};

        let by_path: std::collections::BTreeMap<PathBuf, &rqx_core::facts::ModuleFacts> = modules
            .iter()
            .map(|module| (module.path.clone(), &module.facts))
            .collect();
        let language_of: std::collections::HashMap<&Path, Language> = modules
            .iter()
            .map(|module| (module.path.as_path(), module.language))
            .collect();
        let hierarchy = Hierarchy::of(&by_path);

        // Collected first: accepting a finding borrows the collector, and the tree it
        // reads borrows the modules.
        let mut deep: Vec<(PathBuf, Span, String, u32)> = Vec::new();
        for (name, declared) in hierarchy.declared() {
            let mut depth = 0_u32;
            let mut climbing = declared.facts.supertypes.first().map(String::as_str);
            let mut seen = std::collections::BTreeSet::new();
            while let Some(parent) = climbing {
                if !seen.insert(parent) {
                    break;
                }
                depth += 1;
                climbing = hierarchy
                    .get(parent)
                    .and_then(|above| above.facts.supertypes.first().map(String::as_str));
            }
            if depth > config.thresholds.inheritance_depth {
                deep.push((
                    declared.path.to_path_buf(),
                    declared.facts.span,
                    (*name).to_owned(),
                    depth,
                ));
            }
        }
        for (path, span, name, depth) in deep {
            let Some(range) = at.range(&path, span) else {
                continue;
            };
            staged.accept_if_enabled(
                config,
                build(
                    RuleId::DeepInheritance,
                    range,
                    Evidence::Breadth {
                        counted: format!("types above `{name}`"),
                        found: depth,
                        limit: config.thresholds.inheritance_depth,
                    },
                    "give it what it needs as a collaborator, so the answer is one \
                     object away rather than four types up"
                        .to_owned(),
                    Vec::new(),
                ),
            );
        }

        for found in rqx_lang::hierarchy::shapes(&hierarchy) {
            let advisory = match found.kind {
                Shape::FactoryMethod => rqx_core::AdviceId::FactoryMethodFound,
                Shape::AbstractFactory => rqx_core::AdviceId::AbstractFactoryFound,
                Shape::Bridge => rqx_core::AdviceId::BridgeFits,
                Shape::Interpreter => rqx_core::AdviceId::InterpreterFound,
                Shape::Visitor => rqx_core::AdviceId::VisitorFound,
            };
            // A declared exemption is about the language the type is written in, and
            // the graph is the one place advice is built without one in hand.
            let applies = language_of
                .get(found.path)
                .is_some_and(|language| advisory.applies_to(*language));
            if !applies {
                continue;
            }
            let Some(range) = at.range(found.path, found.span) else {
                continue;
            };
            let note = match found.kind {
                Shape::Bridge => format!(
                    "give `{}` a field for the second axis, so the two vary apart \
                     instead of multiplying",
                    found.name
                ),
                _ => format!("`{}` is the seam the family varies behind", found.name),
            };
            staged.advise_if_enabled(
                config,
                rqx_core::Advice {
                    advisory,
                    location: range,
                    signal: rqx_core::Signal::Family {
                        base: found.name.to_owned(),
                        detail: found.detail,
                    },
                    note,
                    related: Vec::new(),
                },
            );
        }
    }

    /// The graph can only be judged once every module has been seen, so these are
    /// the findings that cannot exist until the walk is over.
    /// The direction a project said its dependencies may point, checked against the
    /// direction they actually do. Silent unless `[architecture]` is declared: an
    /// architecture nobody wrote down is not one rqx should infer.
    fn check_architecture<'graph>(
        root: &Path,
        config: &RqxConfig,
        report: &'graph rqx_lang::module_graph::GraphReport,
        at: &mut Positions,
        staged: &mut Staged,
    ) {
        let plan = &config.architecture;
        if !plan.is_declared() {
            return;
        }
        let Ok(assignments) = Assignments::build(plan) else {
            return;
        };

        // A module is the target of many edges, and each lookup canonicalises the path.
        let mut relative: HashMap<&Path, String> = HashMap::new();
        let mut relative_to_root = |path: &'graph Path| {
            relative
                .entry(path)
                .or_insert_with(|| relative_to(root, path))
                .clone()
        };
        for (importer, imported) in &report.edges {
            let from = relative_to_root(importer);
            let Some(rules) = assignments.of(&from) else {
                continue;
            };
            for target in imported {
                let to = relative_to_root(target);
                let span = report
                    .edge_spans
                    .get(&(importer.clone(), target.clone()))
                    .copied()
                    .unwrap_or(WHOLE_FILE);
                let Some(range) = at.range(importer, span) else {
                    continue;
                };

                if let Some(glob) = rules.forbids(&to) {
                    staged.accept_if_enabled(
                        config,
                        build(
                            RuleId::ForbiddenImport,
                            range.clone(),
                            Evidence::ForbiddenImport {
                                target: to.clone(),
                                by: glob,
                            },
                            format!("`{to}` is outside what this subtree may reach for"),
                            Vec::new(),
                        ),
                    );
                    continue;
                }

                // Inner may import outward-of-itself never, itself and inward always.
                // A file in no declared layer is not governed rather than free: the
                // project named the subtrees it cares about.
                let (Some(inner), Some(outer)) = (
                    rules.layer().and_then(|name| plan.rank(name)),
                    assignments
                        .of(&to)
                        .and_then(Governed::layer)
                        .and_then(|name| plan.rank(name)),
                ) else {
                    continue;
                };
                if outer > inner {
                    staged.accept_if_enabled(
                        config,
                        build(
                            RuleId::LayerViolation,
                            range,
                            Evidence::LayerViolation {
                                from: plan.layers[inner].clone(),
                                to: plan.layers[outer].clone(),
                                target: to,
                            },
                            format!(
                                "invert it: let `{}` depend on an interface `{}` owns",
                                plan.layers[outer], plan.layers[inner]
                            ),
                            Vec::new(),
                        ),
                    );
                }
            }
        }
    }

    fn module_findings(
        modules: &[rqx_lang::module_graph::ModuleEntry],
        root: &Path,
        config: &RqxConfig,
        coverage: Option<&rqx_core::Hits>,
        at: &mut Positions,
    ) -> Staged {
        let mut staged = Staged::default();
        if modules.is_empty() {
            return staged;
        }
        Self::advise_hierarchy(modules, config, at, &mut staged);
        let report = graph_report(modules, root, config);
        Self::check_architecture(root, config, &report, at, &mut staged);

        for dead in report.dead_exports {
            let Some(range) = at.range(&dead.path, dead.span) else {
                continue;
            };
            // A run reaching code the graph called dead means a reader the graph
            // cannot see — a dynamic import, a container wiring by name, reflection.
            // The finding stays, at a confidence that keeps it out of the score.
            let ran = coverage.map(|hits| hits.covered(&range.file, range.line, range.end_line));
            staged.accept_if_enabled(
                config,
                build(
                    RuleId::UnusedExport,
                    range.clone(),
                    Evidence::UnusedExport {
                        symbol: dead.name.clone(),
                        kind: dead.kind.to_owned(),
                        ran,
                    },
                    match ran {
                        Some(count) if count > 0 => format!(
                            "find what reaches `{}` at runtime and import it, or add its \
                             module to `entrypoints`",
                            dead.name
                        ),
                        _ => format!(
                            "delete `{}`, or add its module to `entrypoints` if something \
                             outside the project imports it",
                            dead.name
                        ),
                    },
                    Vec::new(),
                ),
            );
        }

        for dead in report.dead_files {
            let Some(range) = at.range(&dead.path, WHOLE_FILE) else {
                continue;
            };
            let ran = coverage.map(|hits| u32::from(hits.ran(&range.file)));
            staged.accept_if_enabled(
                config,
                build(
                    RuleId::UnusedFile,
                    range,
                    Evidence::UnusedFile {
                        exports: dead.exports,
                        ran,
                    },
                    match ran {
                        Some(count) if count > 0 => "find what loads this file at runtime \
                             and import it, or add it to `entrypoints`"
                            .to_owned(),
                        _ => "delete the file, or add it to `entrypoints` if it is reached \
                             from outside the project"
                            .to_owned(),
                    },
                    Vec::new(),
                ),
            );
        }

        for (importer, targets) in &report.edges {
            staged.import_edges.insert(
                at.relative(importer),
                targets.iter().map(|target| at.relative(target)).collect(),
            );
        }

        for (path, importers) in &report.importers {
            staged
                .importers
                .push((Arc::from(at.relative(path)), *importers));
            if *importers > config.thresholds.fan_in
                && let Some(range) = at.range(path, WHOLE_FILE)
            {
                staged.accept_if_enabled(
                    config,
                    build(
                        RuleId::WideFanIn,
                        range,
                        Evidence::Breadth {
                            counted: "modules import this one".to_owned(),
                            found: *importers,
                            limit: config.thresholds.fan_in,
                        },
                        "split it along the seams its importers actually use, so a \
                         change reaches only the ones that asked for it"
                            .to_owned(),
                        Vec::new(),
                    ),
                );
            }
        }

        for (importer, targets) in &report.edges {
            let reaches = targets.len() as u32;
            if reaches > config.thresholds.fan_out
                && let Some(range) = at.range(importer, WHOLE_FILE)
            {
                staged.accept_if_enabled(
                    config,
                    build(
                        RuleId::WideFanOut,
                        range,
                        Evidence::Breadth {
                            counted: "modules imported".to_owned(),
                            found: reaches,
                            limit: config.thresholds.fan_out,
                        },
                        "give it one collaborator that knows the rest, or move what it \
                         is orchestrating to the things being orchestrated"
                            .to_owned(),
                        Vec::new(),
                    ),
                );
            }
        }

        for dead in report.unimplemented {
            let Some(range) = at.range(&dead.path, dead.span) else {
                continue;
            };
            staged.accept_if_enabled(
                config,
                build(
                    RuleId::SpeculativeGenerality,
                    range,
                    Evidence::UnusedBinding {
                        name: dead.name.clone(),
                        kind: format!("{} with no implementer", dead.kind),
                    },
                    format!(
                        "inline `{}` into its single user until a second one arrives",
                        dead.name
                    ),
                    Vec::new(),
                ),
            );
        }

        for cycle in report.cycles {
            let Some(anchor) = cycle.members.first() else {
                continue;
            };
            // Anchored on the import that starts the ring rather than on byte zero of
            // whichever member sorts first: the edge is the thing to delete, and a
            // reader sent to the top of a file has to find it themselves.
            let opening = cycle
                .ring
                .windows(2)
                .next()
                .and_then(|edge| report.edge_spans.get(&(edge[0].clone(), edge[1].clone())));
            let Some(range) = at
                .range(
                    cycle.ring.first().unwrap_or(anchor),
                    opening.copied().unwrap_or(WHOLE_FILE),
                )
                .or_else(|| at.range(anchor, WHOLE_FILE))
            else {
                continue;
            };
            // Every other edge of the ring, in order, so following the related list
            // walks the cycle rather than listing its members alphabetically.
            let mut related: Vec<SourceRange> = cycle
                .ring
                .windows(2)
                .skip(1)
                .filter_map(|edge| {
                    let span = report.edge_spans.get(&(edge[0].clone(), edge[1].clone()))?;
                    at.range(&edge[0], *span)
                })
                .collect();
            if related.is_empty() {
                related = cycle
                    .members
                    .iter()
                    .skip(1)
                    .filter_map(|member| at.range(member, WHOLE_FILE))
                    .collect();
            }
            let ring: Vec<String> = cycle
                .ring
                .iter()
                .map(|member| at.relative(member))
                .collect();
            staged.cycles.push(ring.clone());
            // Two modules importing each other is a cycle, which the rule above
            // reports; three or more is a group no member can be read apart from,
            // which is a different thing to say about the same fact.
            if cycle.members.len() >= MEDIATOR_MEMBERS {
                let names: Vec<String> = cycle
                    .members
                    .iter()
                    .map(|member| at.relative(member))
                    .collect();
                staged.advise_if_enabled(
                    config,
                    rqx_core::Advice {
                        advisory: rqx_core::AdviceId::MediatorFits,
                        location: range.clone(),
                        signal: rqx_core::Signal::Cluster {
                            names: names.join(", "),
                            members: cycle.members.len() as u32,
                        },
                        note: format!(
                            "give the group one place that knows how its members talk, \
                             so each of the {} only has to know that one",
                            cycle.members.len()
                        ),
                        related: related.clone(),
                    },
                );
            }
            staged.accept_if_enabled(
                config,
                build(
                    RuleId::CircularDependency,
                    range,
                    Evidence::ImportCycle {
                        modules: cycle.members.len() as u32,
                        ring,
                    },
                    "break the cycle by moving what they share into a module both can import, \
                 or invert the weaker of the two dependencies behind an interface"
                        .to_owned(),
                    related,
                ),
            );
        }

        for external in &report.external {
            if external.importers.len() < FACADE_IMPORTERS {
                continue;
            }
            let Some(range) = at.range(&external.path, external.span) else {
                continue;
            };
            staged.advise_if_enabled(
                config,
                rqx_core::Advice {
                    advisory: rqx_core::AdviceId::FacadeFits,
                    location: range,
                    signal: rqx_core::Signal::Reach {
                        name: external.specifier.clone(),
                        sites: external.importers.len() as u32,
                    },
                    note: format!(
                        "put `{}` behind one module of your own, so replacing it is one \
                         edit rather than {}",
                        external.specifier,
                        crate::report::plural(external.importers.len() as u32, "edit", "edits"),
                    ),
                    related: Vec::new(),
                },
            );
        }

        for pair in report.parallel {
            let Some(range) = at.range(&pair.path, pair.span) else {
                continue;
            };
            staged.accept_if_enabled(
                config,
                build(
                    RuleId::ParallelInheritance,
                    range,
                    Evidence::ParallelHierarchy {
                        parent: pair.parent.clone(),
                        twin: pair.twin.clone(),
                        shared: pair.shared.join(", "),
                    },
                    format!(
                        "fold what varies into one hierarchy, so adding a case means one \
                     subclass rather than one in `{}` and one in `{}`",
                        pair.parent, pair.twin
                    ),
                    Vec::new(),
                ),
            );
        }
        staged
    }

    fn size_findings(
        shape_by_file: &std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
        root: &Path,
        config: &RqxConfig,
        at: &mut Positions,
    ) -> Staged {
        let mut staged = Staged::default();
        let counted: Vec<(PathBuf, u32, Span)> = shape_by_file
            .iter()
            .map(|(file, shape)| {
                let statements = shape.functions.iter().map(|f| f.statements).sum();
                let opens = shape.functions.first().map_or(WHOLE_FILE, |function| {
                    Span::new(function.range.start, function.range.end)
                });
                (root.join(file.as_ref()), statements, opens)
            })
            .collect();
        for (path, statements, opens) in counted {
            if statements <= config.thresholds.module_statements {
                continue;
            }
            let Some(range) = at.range(&path, opens) else {
                continue;
            };
            staged.accept_if_enabled(
                config,
                build(
                    RuleId::OversizedModule,
                    range,
                    Evidence::Breadth {
                        counted: "statements in this file".to_owned(),
                        found: statements,
                        limit: config.thresholds.module_statements,
                    },
                    "split it along whatever its functions already group into".to_owned(),
                    Vec::new(),
                ),
            );
        }
        staged
    }

    /// What git says about each file, and the one pass over it that costs a process per
    /// file. Reads nothing the other stages produce, which is what lets it run beside
    /// them.
    fn history_findings(
        todos: HashMap<Arc<str>, Vec<rqx_core::facts::Spot>>,
        history: &crate::history::History,
        root: &Path,
        config: &RqxConfig,
        at: &mut Positions,
    ) -> Staged {
        let mut staged = Staged::default();
        if config.scan.churn_days == 0 {
            return staged;
        }
        let prefix = history.prefix();
        for (repo_path, commits) in history.commits_per_file() {
            let Some(relative) = repo_path.strip_prefix(prefix) else {
                continue;
            };
            staged.commits.push((Arc::from(relative), commits));
        }
        Self::stale_todo_findings(todos, root, config, at, &mut staged);
        staged
    }

    /// Churn is read once for the whole repository after the walk, because a file
    /// with no findings needs no history and most files have none. It counts the
    /// findings each file already has, so it runs once every other stage has been taken.
    fn finish_churn(
        &mut self,
        analyzed: &std::collections::BTreeSet<String>,
        config: &RqxConfig,
        at: &mut Positions,
        history: &crate::history::History,
    ) {
        if config.scan.churn_days == 0 {
            return;
        }
        let prefix = history.prefix();
        // Collected first: accepting a finding borrows the collector, and the counts
        // it reads live on it.
        let mut churned: Vec<(String, u32, u32, String)> = history
            .authors_per_file()
            .into_iter()
            .filter_map(|(repo_path, (authors, commits))| {
                let relative = repo_path.strip_prefix(prefix)?;
                analyzed.contains(relative).then(|| {
                    let findings = self.findings_by_file.get(relative).copied().unwrap_or(0);
                    let sole = if authors.len() == 1 {
                        authors.iter().next().copied().unwrap_or_default()
                    } else {
                        Default::default()
                    }
                    .to_owned();
                    (relative.to_owned(), commits, findings, sole)
                })
            })
            .collect();
        churned.sort();
        for (file, commits, findings, sole) in churned {
            let path = self.root.join(&file);
            if commits >= config.thresholds.hotspot_commits
                && findings > 0
                && let Some(range) = at.range(&path, WHOLE_FILE)
            {
                self.accept_if_enabled(
                    config,
                    build(
                        RuleId::Hotspot,
                        range,
                        Evidence::Breadth {
                            counted: format!("commits, with {findings} findings still in it"),
                            found: commits,
                            limit: config.thresholds.hotspot_commits,
                        },
                        "clean this one first: whatever is wrong here is paid for on \
                         every change"
                            .to_owned(),
                        Vec::new(),
                    ),
                );
            }
            if !sole.is_empty()
                && commits >= config.thresholds.sole_author_commits
                && let Some(range) = at.range(&path, WHOLE_FILE)
            {
                self.accept_if_enabled(
                    config,
                    build(
                        RuleId::SingleAuthorFile,
                        range,
                        Evidence::Breadth {
                            counted: "commits, all by one person".to_owned(),
                            found: commits,
                            limit: config.thresholds.sole_author_commits,
                        },
                        "get a second reader onto it before the first one moves on".to_owned(),
                        Vec::new(),
                    ),
                );
            }
        }
    }

    /// Blame is a process per file, so it runs only when its answer can become a
    /// finding: a project that switched `stale-todo` off pays nothing for it.
    fn stale_todo_findings(
        todos: HashMap<Arc<str>, Vec<rqx_core::facts::Spot>>,
        root: &Path,
        config: &RqxConfig,
        at: &mut Positions,
        staged: &mut Staged,
    ) {
        if !config.rules.enabled(RuleId::StaleTodo) {
            return;
        }
        // One blame per file, and only for the files that hold a note: the per-line
        // form costs a process each, and a repository with a thousand notes would
        // spend the whole scan in git.
        const MOST_FILES_WORTH_BLAMING: usize = 200;
        let today = rqx_core::suppress::today();
        let mut noted: Vec<(Arc<str>, Vec<rqx_core::facts::Spot>)> = todos.into_iter().collect();
        noted.sort_by(|left, right| left.0.cmp(&right.0));
        noted.truncate(MOST_FILES_WORTH_BLAMING);
        let names: Vec<Arc<str>> = noted.iter().map(|(file, _)| Arc::clone(file)).collect();
        let blamed = crate::history::blame_dates_for(root, &names);
        staged.blames += names.len() as u32;
        staged
            .git_commands
            .extend(names.iter().map(|file| crate::history::blame_command(file)));
        for ((file, spots), dates) in noted.into_iter().zip(blamed) {
            if dates.is_empty() {
                continue;
            }
            let path = root.join(file.as_ref());
            for spot in spots {
                let Some(range) = at.range(&path, spot.span) else {
                    continue;
                };
                let Some(written) = dates.get(&range.line) else {
                    continue;
                };
                let age = rqx_core::suppress::days_between(written, &today);
                if age < i64::from(config.thresholds.todo_age_days) {
                    continue;
                }
                staged.accept_if_enabled(
                    config,
                    build(
                        RuleId::StaleTodo,
                        range,
                        Evidence::Breadth {
                            counted: format!("days since this {} was written", spot.detail),
                            found: age.unsigned_abs() as u32,
                            limit: config.thresholds.todo_age_days,
                        },
                        "do it, ticket it, or delete it".to_owned(),
                        Vec::new(),
                    ),
                );
            }
        }
    }

    /// Two files that almost always change together are one decision split across
    /// two places, which is what Shotgun Surgery describes. It needs history, so it
    /// stays silent outside a repository.
    fn cochange_findings(
        analyzed: &std::collections::BTreeSet<String>,
        root: &Path,
        config: &RqxConfig,
        at: &mut Positions,
        history: &crate::history::History,
    ) -> Staged {
        let mut staged = Staged::default();
        // Reads git history, not the import graph, so it must not ride on the gate
        // that builds one — enabling only this rule used to leave it silent.
        if !(config.rules.enabled(RuleId::ShotgunSurgery)
            || config.rules.enabled(RuleId::DivergentChange))
        {
            return staged;
        }
        let commits: Vec<&Vec<String>> = history.co_changes().collect();
        if commits.len() < 5 {
            return staged;
        }
        let prefix = history.prefix();

        // Keyed on slices of the history rather than on owned copies: a commit of
        // twenty files is 190 pairs, and building each key by hand allocated two
        // strings whether or not the entry already existed.
        let mut totals: HashMap<&str, u32> = HashMap::new();
        let mut pairs: HashMap<(&str, &str), u32> = HashMap::new();
        for files in &commits {
            let mut relative: Vec<&str> = files
                .iter()
                .filter_map(|file| file.strip_prefix(prefix))
                .filter(|file| analyzed.contains(*file))
                .collect();
            relative.sort_unstable();
            relative.dedup();
            for file in &relative {
                *totals.entry(file).or_default() += 1;
            }
            for (index, left) in relative.iter().enumerate() {
                for right in &relative[index + 1..] {
                    *pairs.entry((left, right)).or_default() += 1;
                }
            }
        }

        // Divergent Change is the mirror image, read from the same data: a file that
        // changes often but with a different partner every time is changing for many
        // unrelated reasons.
        if config.rules.enabled(RuleId::DivergentChange) {
            let mut strongest: HashMap<&str, u32> = HashMap::new();
            for ((left, right), count) in &pairs {
                for file in [left, right] {
                    let entry = strongest.entry(file).or_default();
                    *entry = (*entry).max(*count);
                }
            }
            let mut diffuse: Vec<(&str, u32, u32)> = totals
                .iter()
                .filter(|(_, total)| **total >= 10)
                .map(|(file, total)| (*file, *total, strongest.get(file).copied().unwrap_or(0)))
                .filter(|(_, total, peak)| peak * 10 < total * 3)
                .collect();
            diffuse.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            for (file, total, peak) in diffuse.into_iter().take(5) {
                let Some(range) = at.range(&root.join(file), WHOLE_FILE) else {
                    continue;
                };
                staged.accept_if_enabled(
                    config,
                    build(
                        RuleId::DivergentChange,
                        range,
                        Evidence::CoChange {
                            partner: "no single file".to_owned(),
                            together: peak,
                            total,
                        },
                        "split this file along the reasons it changes, so each part has one"
                            .to_owned(),
                        Vec::new(),
                    ),
                );
            }
        }

        let mut reported: HashSet<&str> = HashSet::new();
        let mut ranked: Vec<((&str, &str), u32)> = pairs.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if !config.rules.enabled(RuleId::ShotgunSurgery) {
            return staged;
        }
        for ((left, right), together) in ranked {
            let total = totals.get(left).copied().unwrap_or(0);
            // Both a floor and a ratio: three commits together is coincidence, and a
            // file that changes constantly co-occurs with everything.
            if total < 5 || together * 4 < total * 3 || !reported.insert(left) {
                continue;
            }
            let Some(range) = at.range(&root.join(left), WHOLE_FILE) else {
                continue;
            };
            let Some(partner) = at.range(&root.join(right), WHOLE_FILE) else {
                continue;
            };
            staged.accept_if_enabled(
                config,
                build(
                    RuleId::ShotgunSurgery,
                    range,
                    Evidence::CoChange {
                        partner: right.to_owned(),
                        together,
                        total,
                    },
                    format!(
                        "the decision these two share has no home; move it into one place so \
                     `{right}` stops needing an edit every time this file does"
                    ),
                    vec![partner],
                ),
            );
        }
        staged
    }

    /// The functions this change added that repeat one the base revision had. Nothing
    /// when the scan has no base to compare with.
    fn reimplemented_findings(
        shape_by_file: &std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
        reference: Option<&crate::reimplemented::Reference>,
        root: &Path,
        settings: &Resolved,
    ) -> Staged {
        let mut staged = Staged::default();
        let Some(reference) = reference else {
            return staged;
        };
        for finding in crate::reimplemented::findings(shape_by_file, reference) {
            let config = settings.for_file(&root.join(&*finding.location.file));
            staged.accept_if_enabled(config, finding);
        }
        staged
    }

    /// What each function costs with its callees folded in, over every file the walk
    /// collected. `complete` says whether the walk covered the project or only part of it.
    fn cost_summaries(
        inputs: &CostFacts,
        covered: &HashMap<Arc<str>, Vec<u32>>,
        root: &Path,
        settings: &Resolved,
        complete: bool,
        keep: bool,
    ) -> Staged {
        let files: Vec<rqx_lang::cost::compose::Analysed<'_>> = inputs.files().collect();
        let reported =
            |file: &str, line: u32| covered.get(file).is_some_and(|lines| lines.contains(&line));
        let composed = rqx_lang::cost::compose::summarise_with_sites(
            &files,
            root,
            &inputs.tables,
            complete,
            cost_rules::wanted(settings).then_some(&reported),
        );
        let mut staged = cost_rules::staged(composed.sites, root, settings);
        staged.summaries = Some(composed.summaries);
        if keep {
            staged.call_links = Some(composed.links);
        }
        staged
    }

    /// What other tools reported, read from the SARIF files the project named.
    fn external_findings(root: &Path, config: &RqxConfig) -> Staged {
        Staged {
            findings: crate::external::read_all(root, &config.scan.sarif_in),
            ..Staged::default()
        }
    }

    /// The stages that need every file, run side by side: duplicates, the import graph,
    /// and what git knows touch nothing but what the walk collected, so none waits for
    /// another. What they found is taken in the order the stages are written in, which
    /// is the order they used to run in, so the output does not depend on which
    /// finished first. Only churn waits, because it counts the findings the others
    /// produced. The phases report what the scan is waiting on, so a stage that
    /// finished while an earlier one was still running shows no time of its own.
    fn finish_cross_file(
        &mut self,
        root: &Path,
        settings: &Resolved,
        history: impl FnOnce() -> Arc<crate::history::History> + Send,
    ) -> bool {
        use crate::progress::Phase;
        let config = &settings.config;
        let clone_groups = std::mem::take(&mut self.clone_groups);
        let ported = std::mem::take(&mut self.ported);
        let mut inputs = CostFacts {
            modules: std::mem::take(&mut self.modules),
            costs: std::mem::take(&mut self.costs),
            links: rqx_lang::cost::compose::CallLinks::default(),
            tables: Arc::clone(&settings.cost_tables),
        };
        let todos = std::mem::take(&mut self.todos);
        let shapes = std::mem::take(&mut self.shape_by_file);
        let analyzed = std::mem::take(&mut self.analyzed);
        let coverage = self.coverage.take();
        let reference = self.reference.clone();
        let (shapes_ref, analyzed_ref, coverage_ref) = (&shapes, &analyzed, coverage.as_ref());
        // The import graph is built only when a rule reads it; the cost analysis reads the
        // same modules whatever the rules are.
        let graph_modules: &[rqx_lang::module_graph::ModuleEntry] = if self.track_modules {
            &inputs.modules
        } else {
            &[]
        };
        let inputs_ref = &inputs;
        let covered = std::mem::take(&mut self.covered);
        let covered_ref = &covered;
        let reference_ref = reference.as_deref();
        let complete = !root.is_file();
        let keep_costs = self.keep_costs;

        let versioned = thread::scope(|scope| {
            let duplicates = scope.spawn(move || {
                [
                    Self::clone_findings(clone_groups, root, settings),
                    Self::ported_findings(ported, root, settings),
                ]
            });
            let imports = scope.spawn(move || {
                let mut at = Positions::new(root);
                [
                    Self::module_findings(graph_modules, root, config, coverage_ref, &mut at),
                    Self::size_findings(shapes_ref, root, config, &mut at),
                    Self::reimplemented_findings(shapes_ref, reference_ref, root, settings),
                ]
            });
            let costed = scope.spawn(move || {
                Self::cost_summaries(
                    inputs_ref,
                    covered_ref,
                    root,
                    settings,
                    complete,
                    keep_costs,
                )
            });
            let git = scope.spawn(move || {
                let history = history();
                let mut at = Positions::new(root);
                let noted = Self::history_findings(todos, &history, root, config, &mut at);
                let together =
                    Self::cochange_findings(analyzed_ref, root, config, &mut at, &history);
                (history, noted, together)
            });

            self.report_phase(Phase::Duplicates);
            for staged in joined(duplicates) {
                self.absorb(staged);
            }
            self.report_phase(Phase::Imports);
            for staged in joined(imports) {
                self.absorb(staged);
            }
            self.report_phase(Phase::Cost);
            self.absorb(joined(costed));
            self.report_phase(Phase::History);
            let (history, noted, together) = joined(git);
            self.stats.git_processes += history.git_processes();
            self.git_commands.extend(history.commands().iter().cloned());
            self.absorb(noted);
            self.finish_churn(analyzed_ref, config, &mut Positions::new(root), &history);
            self.report_phase(Phase::CoChange);
            self.absorb(together);
            self.absorb(Self::external_findings(root, config));
            history.versioned()
        });

        self.shape_by_file = shapes;
        self.analyzed = analyzed;
        self.coverage = coverage;
        inputs.links = std::mem::take(&mut self.call_links);
        self.cost_facts = self.keep_costs.then_some(inputs);
        versioned
    }

    /// `stopped` skips every cross-file phase rather than the assembly after them:
    /// what a cancelled scan returns is thrown away, and the phases are the seconds
    /// worth not spending.
    fn into_outcome(
        mut self,
        root: &Path,
        settings: &Resolved,
        history: impl FnOnce() -> Arc<crate::history::History> + Send,
        stopped: bool,
    ) -> ScanOutcome {
        let mut versioned = false;
        if !stopped {
            versioned = self.finish_cross_file(root, settings, history);
        }
        let retained = self.retained.len() as u32;
        let mut findings: Vec<Finding> = self
            .retained
            .into_iter()
            .map(|Reverse(ranked)| ranked.0)
            .collect();
        findings.sort_by(Finding::report_order);
        // Same reason the findings key is total: the collector sees files in whatever
        // order the workers finish, and that order depends on the thread count.
        self.advice.sort_by(|a, b| {
            a.location
                .file
                .cmp(&b.location.file)
                .then(a.location.start.cmp(&b.location.start))
                .then(a.advisory.cmp(&b.advisory))
        });
        // Same reason the findings key is total: waivers are recorded as their files
        // finish, and which file finishes first depends on the thread count.
        self.suppressed.sort_by(|a, b| {
            a.finding
                .location
                .file
                .cmp(&b.finding.location.file)
                .then(a.finding.location.start.cmp(&b.finding.location.start))
                .then(a.finding.slug().cmp(b.finding.slug()))
                .then(a.at.start.cmp(&b.at.start))
                .then_with(|| a.finding.report_order(&b.finding))
        });
        let profile = self.profile.take().map(|samples| {
            let shapes = crate::path_suffixes::PathSuffixes::of(&self.shape_by_file);
            rqx_core::runtime::Profile::placed(samples, |file, line| {
                shapes.find(file).is_some_and(|shape| {
                    shape
                        .functions
                        .iter()
                        .any(|function| function.range.line <= line && line <= function.last_line)
                })
            })
        });
        if let Some(profile) = &profile {
            speak_for(&mut findings, &self.shape_by_file, profile);
        }
        self.skipped.sort();
        // The last stage has no successor to close it, so it is closed here.
        self.timings
            .push((self.phase.label(), self.phase_began.elapsed().as_millis()));
        ScanOutcome {
            root: root.to_path_buf(),
            configured: !settings.sources.is_empty(),
            versioned,
            omitted: self.counts.total.saturating_sub(retained),
            counts: self.counts,
            analyzed: self.analyzed,
            impact: self.impact,
            import_edges: self.import_edges,
            cycles: self.cycles,
            findings,
            advice: self.advice,
            findings_by_file: self.findings_by_file,
            severity_by_file: self.severity_by_file,
            lines_by_file: self.lines_by_file,
            shape_by_file: self.shape_by_file,
            summaries: self.summaries,
            cost_facts: self.cost_facts,
            suppressed: self.suppressed,
            baseline_size: self.baseline_findings,
            timings: self.timings,
            stats: self.stats,
            git_commands: self.git_commands,
            config_files: settings
                .sources
                .iter()
                .chain(settings.policy.as_ref())
                .cloned()
                .collect(),
            owners: crate::ownership::load(root),
            code_lines: self.code_lines,
            files_scanned: self.files_scanned,
            skipped: self.skipped,
            profile,
            elapsed_ms: self.started.elapsed().as_millis(),
        }
    }
}

/// A performance finding names a shape that costs when the input grows. A profile
/// says whether anything reached it: one it never sampled keeps its finding and
/// stops counting, exactly as a covered symbol stops counting as dead code.
fn speak_for(
    findings: &mut [Finding],
    shapes: &std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
    profile: &rqx_core::runtime::Profile,
) {
    for finding in findings {
        let Some(lowered) = rqx_core::runtime::unsampled(finding.rule) else {
            continue;
        };
        let Some(shape) = shapes.get(&finding.location.file) else {
            continue;
        };
        // Code outside every function is code a profile has nothing to say about.
        let Some(function) = shape
            .functions
            .iter()
            .filter(|function| {
                function.range.start <= finding.location.start
                    && finding.location.start <= function.end
            })
            .min_by_key(|function| function.end.saturating_sub(function.range.start))
        else {
            continue;
        };
        let share = profile
            .of(
                &finding.location.file,
                function.range.line,
                function.last_line,
            )
            .map_or(0, |share| share.total);
        finding.evidence.mark_sampled(share);
        if share == 0 {
            finding.confidence = finding.confidence.min(lowered);
        }
    }
}

/// A finding about a whole file has no interesting span; it points at the first
/// byte so the report still opens the file at a real position.
const WHOLE_FILE: Span = Span::new(0, 0);

/// Two modules that import each other is a cycle and already reported as one. Three is
/// the smallest group where no member can be understood on its own.
const MEDIATOR_MEMBERS: usize = 3;

/// Two files reaching for the same package is a coincidence; three is a decision the
/// project has made without writing it down anywhere.
const FACADE_IMPORTERS: usize = 3;

/// Graph and history findings are constructed here rather than through
/// `FindingBuilder`, so the rule filter has to be applied on this path too — without
/// it, disabling a rule or selecting a family would silently miss them.
/// The globs from `[architecture]`, compiled once per run. `globset` rather than the
/// walker's `Override`, because a violation has to name the glob that forbade it and
/// an `Override` only answers whether something matched.
struct Assignments<'plan> {
    matcher: globset::GlobSet,
    order: Vec<Governed<'plan>>,
}

/// What one `[architecture.paths]` entry says, with the globs it names compiled once
/// rather than once per import edge it is asked about.
struct Governed<'plan> {
    assignment: &'plan rqx_core::config::Assignment,
    may_not_import: Vec<(&'plan String, globset::GlobMatcher)>,
    may_only_import: Vec<globset::GlobMatcher>,
}

impl<'plan> Governed<'plan> {
    fn new(assignment: &'plan rqx_core::config::Assignment) -> Self {
        // A pattern that is not a valid glob matches nothing, as it always has.
        let compile = |pattern: &'plan String| {
            globset::Glob::new(pattern)
                .ok()
                .map(|glob| (pattern, glob.compile_matcher()))
        };
        Self {
            assignment,
            may_not_import: assignment
                .may_not_import
                .iter()
                .filter_map(compile)
                .collect(),
            may_only_import: assignment
                .may_only_import
                .iter()
                .filter_map(compile)
                .map(|(_, matcher)| matcher)
                .collect(),
        }
    }

    fn layer(&self) -> Option<&str> {
        self.assignment.layer.as_deref()
    }

    fn forbids(&self, target: &str) -> Option<String> {
        if let Some((glob, _)) = self
            .may_not_import
            .iter()
            .find(|(_, matcher)| matcher.is_match(target))
        {
            return Some((*glob).clone());
        }
        let allowed = &self.assignment.may_only_import;
        if !allowed.is_empty() && !self.may_only_import.iter().any(|m| m.is_match(target)) {
            return Some(format!("may_only_import {allowed:?}"));
        }
        None
    }
}

impl<'plan> Assignments<'plan> {
    fn build(plan: &'plan rqx_core::config::Architecture) -> Result<Self, globset::Error> {
        let mut builder = globset::GlobSetBuilder::new();
        let mut order = Vec::new();
        for (glob, assignment) in &plan.paths {
            builder.add(globset::Glob::new(glob)?);
            order.push(Governed::new(assignment));
        }
        Ok(Self {
            matcher: builder.build()?,
            order,
        })
    }

    /// The last declaration wins, the way the most specific rule a project wrote is
    /// the one that answers elsewhere in the config.
    fn of(&self, path: &str) -> Option<&Governed<'plan>> {
        self.matcher
            .matches(path)
            .last()
            .map(|index| &self.order[*index])
    }
}

/// Every coverage file the project named, merged. A file that cannot be read is
/// skipped rather than fatal: a scan that failed because yesterday's coverage was
/// missing would be a scan nobody runs.
/// Every profile the project named, merged. Unreadable is skipped rather than fatal,
/// for the same reason a stale coverage file is.
fn read_profile(root: &Path, paths: &[String]) -> Option<rqx_core::runtime::Samples> {
    let mut merged: Option<rqx_core::runtime::Samples> = None;
    for name in paths {
        match rqx_core::runtime::Samples::read(&root.join(name)) {
            Ok(samples) => {
                merged = Some(match merged {
                    Some(existing) => existing.merged(samples),
                    None => samples,
                })
            }
            Err(reason) => eprintln!("rqx: skipping profile \u{2014} {reason}"),
        }
    }
    merged.filter(|samples| !samples.is_empty())
}

fn read_coverage(root: &Path, paths: &[String]) -> Option<rqx_core::Hits> {
    let mut merged: Option<rqx_core::Hits> = None;
    for name in paths {
        let path = root.join(name);
        match rqx_core::Hits::read(&path) {
            Ok(hits) => {
                merged = Some(match merged {
                    Some(existing) => existing.merged(hits),
                    None => hits,
                })
            }
            Err(reason) => eprintln!("rqx: skipping coverage — {reason}"),
        }
    }
    merged.filter(|hits| !hits.is_empty())
}

/// Whether a coverage file said this finding's code was reached.
const fn ran(evidence: &Evidence) -> bool {
    match evidence {
        Evidence::UnusedExport {
            ran: Some(hits), ..
        }
        | Evidence::UnusedFile {
            ran: Some(hits), ..
        } => *hits > 0,
        _ => false,
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

/// A file a generator owns is one nobody refactors: the next run overwrites whatever
/// was changed, so every finding in it is noise. Only the conventional markers count —
/// a path called `generated` may hold code somebody actually maintains.
fn generated_by(source: &str) -> Option<&'static str> {
    const HEAD: usize = 4096;
    // A cap counted in bytes can land inside a multi-byte character, which panics the
    // slice on any source file wide enough to have one.
    let mut cut = source.len().min(HEAD);
    while !source.is_char_boundary(cut) {
        cut -= 1;
    }
    let head = &source[..cut];
    // `@generated` is the convention shared by Bazel, Buck, protoc and gofmt tooling;
    // the other two are the exact headers of generators common enough to matter.
    let markers: [(&str, &str); 3] = [
        ("@generated", "generated code"),
        ("Code generated by", "generated code"),
        ("This is code generated by Prisma", "Prisma generated code"),
    ];
    markers
        .iter()
        .find(|(needle, _)| head.contains(needle))
        .map(|(_, label)| *label)
}

pub(crate) fn relative_to(root: &Path, path: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(root) {
        return rqx_core::paths::display_path(relative);
    }
    // The graph works in canonical paths while the walker yields them as given, so
    // the two only line up after both are resolved.
    if let (Ok(canonical_root), Ok(canonical)) = (
        rqx_core::paths::canonicalize(root),
        rqx_core::paths::canonicalize(path),
    ) && let Ok(relative) = canonical.strip_prefix(&canonical_root)
    {
        return rqx_core::paths::display_path(relative);
    }
    rqx_core::paths::display_path(path)
}

/// The cross-file half: clones, the module graph and the churn rules, which no
/// single file's analysis can reach. Takes a receiver rather than a slice so the
/// parallel walk feeds it as it goes and a caller replaying a cached map can feed it
/// all at once.
pub(crate) fn collect(
    receiver: Receiver<ScanMessage>,
    root: &Path,
    settings: &Resolved,
    total: Option<u32>,
    watch: Option<&Watch>,
    history: impl FnOnce() -> Arc<crate::history::History> + Send,
) -> ScanOutcome {
    collect_against(
        receiver,
        root,
        settings,
        total,
        watch,
        Comparison::default(),
        history,
    )
}

/// `collect`, comparing with the functions of a base revision when `comparison` holds
/// them, and keeping the cost facts when it says to or when the configuration reads
/// them.
pub(crate) fn collect_against(
    receiver: Receiver<ScanMessage>,
    root: &Path,
    settings: &Resolved,
    total: Option<u32>,
    watch: Option<&Watch>,
    comparison: Comparison,
    // A thunk rather than the history itself: reading git is the one part of a scan
    // that does not depend on the walk, and waiting for it before draining the
    // channel stalls every worker behind it once the buffer fills.
    history: impl FnOnce() -> Arc<crate::history::History> + Send,
) -> ScanOutcome {
    let mut collector = Collector::new(
        settings.config.scan.max_findings,
        settings.enabled_anywhere(RuleId::DuplicateCode),
        settings.enabled_anywhere(RuleId::CrossLanguageDuplicate),
        settings.needs_modules(),
        total,
    );
    collector.watcher = watch.map(|watch| watch.updates.clone());
    collector.coverage = read_coverage(root, &settings.config.scan.coverage);
    collector.profile = read_profile(root, &settings.config.scan.profile);
    collector.baseline = settings
        .baseline
        .as_ref()
        .map(crate::baseline::Recorded::matcher);
    collector.baseline_findings = settings.baseline_findings;
    collector.root = root.to_path_buf();
    collector.keep_costs = comparison.keeps_costs(settings);
    collector.reference = comparison.reference;
    for message in receiver {
        match message {
            ScanMessage::File(file) => {
                let Analyzed {
                    display,
                    canonical,
                    language,
                    findings,
                    identities,
                    advice,
                    clones,
                    module,
                    code_lines,
                    shape,
                    suppressions,
                    todos,
                    cost,
                } = *file;
                collector.files_scanned += 1;
                collector.code_lines += u64::from(code_lines);
                let shared: Arc<str> = Arc::from(display.as_str());
                // Recorded before this file's findings are offered, because a waiver
                // that arrives after the finding it covers waives nothing.
                if !suppressions.is_empty() {
                    collector
                        .suppressions
                        .insert(Arc::clone(&shared), suppressions);
                }
                if !todos.is_empty() {
                    collector.todos.insert(Arc::clone(&shared), todos);
                }
                let covered = rqx_core::cost_sites::lines_reported_in_loops(&findings);
                let resolved = identities
                    .into_iter()
                    .map(Some)
                    .chain(std::iter::repeat(None));
                collector.accept_all(findings.into_iter().zip(resolved).collect());
                collector.advice.extend(advice);
                for site in clones {
                    collector.record_clone(site, language);
                }
                collector
                    .lines_by_file
                    .insert(Arc::clone(&shared), code_lines);
                collector.shape_by_file.insert(Arc::clone(&shared), shape);
                if let Some(path) = canonical {
                    collector.modules.push(rqx_lang::module_graph::ModuleEntry {
                        language,
                        path,
                        facts: module,
                    });
                    collector.costs.push(CostFile {
                        name: Arc::clone(&shared),
                        cost,
                    });
                    collector.covered.insert(Arc::clone(&shared), covered);
                }
                collector.analyzed.insert(display);
                collector.showing = Some(Arc::clone(&shared));
                collector.tick(None);
            }
            ScanMessage::Skipped { path, reason } => collector.skipped.push((path, reason)),
            ScanMessage::Total(total) => collector.total = total.or(collector.total),
        }
    }
    collector.into_outcome(root, settings, history, watch.is_some_and(Watch::stopped))
}

/// Oversized files are rejected here rather than by the walker's own
/// `max_filesize`, which drops them before they are ever visible: a file that was
/// never analyzed has to be reported as such, or a clean score is a false one.
fn analyze_entry(
    entry: &ignore::DirEntry,
    root: &Path,
    settings: &Resolved,
    cache: Option<&crate::cache::Cache>,
    reads: &FileReads,
    sender: &SyncSender<ScanMessage>,
) {
    let metadata = entry.metadata().ok();
    let size = metadata.as_ref().map(std::fs::Metadata::len);
    let stamp = metadata.as_ref().and_then(Stamp::of);
    let _ = sender.send(analyze_file(
        entry.path(),
        root,
        size,
        stamp,
        settings,
        cache,
        reads,
    ));
}

/// One file, analysed. Split from the send so a caller holding its own map of results
/// — the server behind `rqx serve` — can re-analyse one file without a channel and a
/// walk around it.
pub(crate) fn analyze_file(
    path: &Path,
    root: &Path,
    size: Option<u64>,
    stamp: Option<Stamp>,
    settings: &Resolved,
    cache: Option<&crate::cache::Cache>,
    reads: &FileReads,
) -> ScanMessage {
    let (config, facts) = settings.for_file_with_facts(path);
    if size.is_some_and(|bytes| bytes > config.scan.max_filesize) {
        return ScanMessage::Skipped {
            path: path.to_owned(),
            reason: format!(
                "{} KB exceeds the {} KB limit (raise `scan.max_filesize`)",
                size.unwrap_or_default() / 1024,
                config.scan.max_filesize / 1024
            ),
        };
    }
    let display = relative_to(root, path);
    if let Some(cached) = cache
        .zip(stamp)
        .and_then(|(cache, stamp)| cache.unchanged(&display, stamp))
    {
        return analyzed(path, display, cached, settings);
    }
    reads.note();
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => return skipped(path, error.to_string()),
    };
    if config.scan.skip_generated
        && let Some(tool) = generated_by(&source)
    {
        return skipped(
            path,
            format!("{tool} (set `scan.skip_generated: false` to include it)"),
        );
    }
    let cached = match cache.and_then(|cache| cache.get(&display, &source, stamp)) {
        Some(cached) => cached,
        None => {
            match rqx_lang::analyze_planned(
                path,
                &display,
                &source,
                config,
                &settings.cost_tables,
                facts,
            ) {
                Ok(analysis) => {
                    let identities = identities_of(&analysis.findings, &source);
                    if let Some(cache) = cache {
                        cache.put(&display, &source, stamp, &analysis, &identities);
                    }
                    Cached {
                        analysis,
                        identities,
                    }
                }
                Err(error) => return skipped(path, error.to_string()),
            }
        }
    };
    analyzed(path, display, cached, settings)
}

fn skipped(path: &Path, reason: String) -> ScanMessage {
    ScanMessage::Skipped {
        path: path.to_owned(),
        reason,
    }
}

/// Resolved here, where the file's text is already in hand, and kept with the cached
/// analysis so that a file that is not read again can still be matched against a
/// baseline.
fn identities_of(findings: &[Finding], source: &str) -> Vec<rqx_core::Identity> {
    findings
        .iter()
        .map(|finding| rqx_core::Identity::resolved(finding, source))
        .collect()
}

fn analyzed(path: &Path, display: String, cached: Cached, settings: &Resolved) -> ScanMessage {
    let Cached {
        mut analysis,
        mut identities,
    } = cached;
    // Which Go a module is written for is in its `go.mod`, not in the file, so it is
    // applied here rather than kept in the file's cached analysis.
    if analysis.language == Language::Go
        && analysis
            .findings
            .iter()
            .any(|finding| finding.rule == RuleId::ClosureOverLoopVariable)
        && rqx_lang::go::loop_variables_per_iteration(path)
    {
        let kept: Vec<bool> = analysis
            .findings
            .iter()
            .map(|finding| finding.rule != RuleId::ClosureOverLoopVariable)
            .collect();
        let mut index = 0;
        identities.retain(|_| {
            index += 1;
            kept.get(index - 1).copied().unwrap_or(true)
        });
        analysis
            .findings
            .retain(|finding| finding.rule != RuleId::ClosureOverLoopVariable);
    }
    // The cost analysis follows calls across imports, which it can only do for a file it
    // can name the way the import resolver does.
    let takes_part_in_cost =
        !analysis.cost.is_empty() || rqx_lang::cost::compose::is_barrel(&analysis.module);
    ScanMessage::File(Box::new(Analyzed {
        canonical: (settings.needs_modules() || takes_part_in_cost)
            .then(|| rqx_core::paths::canonicalize(path).unwrap_or_else(|_| path.to_owned())),
        display,
        language: analysis.language,
        findings: analysis.findings,
        identities,
        advice: analysis.advice,
        clones: analysis.clones,
        module: analysis.module,
        code_lines: analysis.code_lines,
        shape: analysis.shape,
        suppressions: analysis.suppressions,
        todos: analysis.todos,
        cost: analysis.cost,
    }))
}

/// The modules of each language family, in the order the families sort.
fn by_family(
    modules: &[rqx_lang::module_graph::ModuleEntry],
) -> std::collections::BTreeMap<Language, Vec<&rqx_lang::module_graph::ModuleEntry>> {
    let mut families: std::collections::BTreeMap<Language, Vec<_>> =
        std::collections::BTreeMap::new();
    for module in modules {
        families
            .entry(module.language.family())
            .or_default()
            .push(module);
    }
    families
}

/// One graph per language family, merged. Import syntax means different things in
/// different languages, so resolving a Python module against a Java classpath would
/// invent edges — and a missing edge is a dead-code finding about live code.
/// TypeScript and JavaScript are one family, so a JavaScript module imported from
/// TypeScript is an edge rather than a dead export.
fn graph_report(
    modules: &[rqx_lang::module_graph::ModuleEntry],
    root: &Path,
    config: &RqxConfig,
) -> rqx_lang::module_graph::GraphReport {
    let mut merged = rqx_lang::module_graph::GraphReport::default();
    for (language, entries) in by_family(modules) {
        let support = rqx_lang::support(language);
        let resolver = (support.resolver)(root);
        let report = rqx_lang::module_graph::analyze_borrowed(
            &entries,
            root,
            &config.entrypoints.paths,
            resolver.as_ref(),
            support.profile.is_vendor,
        );
        merged.dead_exports.extend(report.dead_exports);
        merged.external.extend(report.external);
        merged.dead_files.extend(report.dead_files);
        merged.cycles.extend(report.cycles);
        merged.unimplemented.extend(report.unimplemented);
        merged.parallel.extend(report.parallel);
        merged.importers.extend(report.importers);
        merged.edges.extend(report.edges);
        merged.edge_spans.extend(report.edge_spans);
    }
    merged
        .dead_exports
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.span.start.cmp(&b.span.start)));
    merged.dead_files.sort_by(|a, b| a.path.cmp(&b.path));
    merged.cycles.sort_by(|a, b| a.members.cmp(&b.members));
    merged
        .external
        .sort_by(|a, b| a.specifier.cmp(&b.specifier));
    merged
}

/// Every file of one language the scan would look at. Collected rather than streamed
/// because the one caller wants them in a stable order and there are no findings to
/// race.
pub(crate) fn files_of(
    root: &Path,
    language: rqx_core::Language,
    config: &RqxConfig,
) -> Result<Vec<PathBuf>> {
    let found = std::sync::Mutex::new(Vec::new());
    build_walker(root, config)?.run(|| {
        Box::new(|entry| {
            if let Ok(entry) = entry
                && entry.file_type().is_some_and(|kind| kind.is_file())
                && rqx_core::Language::of_path(entry.path()) == Some(language)
            {
                found
                    .lock()
                    .expect("the walk holds no lock")
                    .push(entry.into_path());
            }
            ignore::WalkState::Continue
        })
    });
    let mut found = found.into_inner().expect("the walk is done");
    found.sort();
    Ok(found)
}

pub(crate) fn build_walker(root: &Path, config: &RqxConfig) -> Result<ignore::WalkParallel> {
    let mut types = TypesBuilder::new();
    for language in config.languages.enabled() {
        for extension in language.extensions() {
            types
                .add(language.slug(), &format!("*.{extension}"))
                .with_context(|| format!("registering a {} file glob", language.title()))?;
        }
        types.select(language.slug());
    }

    let mut overrides = OverrideBuilder::new(root);
    for pattern in &config.ignore.patterns {
        overrides
            .add(&format!("!{pattern}"))
            .with_context(|| format!("ignore pattern `{pattern}` is not a valid glob"))?;
    }

    let pruned: Vec<String> = config.scan.prune_dirs.clone();
    let mut builder = WalkBuilder::new(root);
    builder
        .types(types.build().context("building the source file matcher")?)
        .overrides(overrides.build().context("building ignore overrides")?)
        // `ignore` reads 0 as "pick for me" and then caps its own choice at 12, so a
        // machine with more cores than that leaves them idle with no way to ask.
        .threads(if config.scan.threads == 0 {
            std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
        } else {
            config.scan.threads
        })
        .filter_entry(move |entry| {
            if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                return !pruned.iter().any(|dir| *dir == name);
            }
            true
        });
    Ok(builder.build_parallel())
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

/// The same walker the scan uses, so the total it reports cannot disagree with what is
/// then scanned. Only directory entries are touched: nothing is opened or parsed.
fn count_files(walker: ignore::WalkParallel, watch: Option<&Watch>) -> Option<u32> {
    let counted = std::sync::atomic::AtomicU32::new(0);
    walker.run(|| {
        Box::new(|entry| {
            if watch.is_some_and(Watch::stopped) {
                return ignore::WalkState::Quit;
            }
            if entry.is_ok_and(|entry| entry.file_type().is_some_and(|kind| kind.is_file())) {
                counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            ignore::WalkState::Continue
        })
    });
    Some(counted.into_inner()).filter(|total| *total > 0)
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

    let counter = build_walker(root, config)?;
    let walker = build_walker(root, config)?;
    let walks = AtomicU32::new(0);
    let reads = FileReads::default();
    let (sender, receiver) = mpsc::sync_channel(CHANNEL_DEPTH);

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
                        let _ = sender.send(ScanMessage::Skipped {
                            path: PathBuf::from("<walk>"),
                            reason: error.to_string(),
                        });
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
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_scan_reports_no_answer_at_all() {
        let (updates, _drain) = mpsc::sync_channel(1);
        let watch = Watch::new(updates);
        watch
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures");
        let settings = Resolved::of(rqx_core::config::RqxConfig::default());
        let Err(failed) = scan_with_progress(&root, &settings, watch) else {
            panic!("a cancelled scan answered anyway");
        };

        assert!(failed.to_string().contains(CANCELLED), "{failed:#}");
    }

    #[test]
    fn a_forced_policy_uses_the_cache_whatever_the_settings_say() {
        let mut config = rqx_core::config::RqxConfig::default();
        config.scan.cache = false;

        assert!(CachePolicy::Forced.enabled(&config));
        assert!(!CachePolicy::Off.enabled(&config));
        assert!(!CachePolicy::FromSettings.enabled(&config));
    }

    const HOUR: u64 = 3600;

    /// A copy of the TypeScript fixtures that the cache tests are free to write into
    /// and edit, removed when it goes out of scope.
    struct Project {
        root: PathBuf,
        settings: Resolved,
    }

    impl Project {
        fn copied_from_fixtures() -> Self {
            static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("rqx-cache-test-{}-{unique}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/typescript");
            // Only the source files: a scan run on the fixtures by hand leaves its own
            // `.rqx` cache beside them, which is no part of the project under test.
            for entry in std::fs::read_dir(fixtures).unwrap() {
                let path = entry.unwrap().path();
                if path.is_file() {
                    std::fs::copy(&path, root.join(path.file_name().unwrap())).unwrap();
                }
            }
            let project = Self {
                root,
                settings: Resolved::of(rqx_core::config::RqxConfig::default()),
            };
            for entry in std::fs::read_dir(&project.root).unwrap() {
                project.age(&entry.unwrap().path(), HOUR);
            }
            project
        }

        /// Dated well before any cache is written, which is when a stamp is believed.
        fn age(&self, path: &Path, seconds_ago: u64) {
            let at = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds_ago);
            Self::stamp(path, at);
        }

        fn stamp(path: &Path, at: std::time::SystemTime) {
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(at)
                .unwrap();
        }

        fn scan(&self) -> ScanOutcome {
            scan_with(&self.root, &self.settings, None, CachePolicy::Forced).unwrap()
        }

        fn findings_of(&self, outcome: &ScanOutcome) -> String {
            serde_json::to_string(&outcome.findings).unwrap()
        }

        fn cached_entries(&self) -> usize {
            crate::cache::stored(&self.root, &self.settings).map_or(0, |stored| stored.entries)
        }

        fn cache_file(&self) -> PathBuf {
            self.root
                .join(crate::cache::CACHE_DIRECTORY)
                .join(crate::cache::CACHE_FILE)
        }

        /// A run that finds every file in the cache has nothing to write, so bytes
        /// appended beforehand are still there afterwards.
        fn next_run_hits_every_file(&self) -> bool {
            let mut marked = std::fs::read(self.cache_file()).unwrap();
            marked.extend_from_slice(b"untouched");
            std::fs::write(self.cache_file(), &marked).unwrap();
            self.scan();
            std::fs::read(self.cache_file()).unwrap() == marked
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_warm_run_serves_the_same_findings_from_the_cache() {
        let project = Project::copied_from_fixtures();

        let cold = project.scan();
        let analysed = cold.analyzed.len();
        assert!(analysed > 1);
        assert_eq!(project.cached_entries(), analysed);

        let warm = project.scan();
        assert_eq!(project.findings_of(&cold), project.findings_of(&warm));
        assert!(project.next_run_hits_every_file());
    }

    #[test]
    fn a_cold_run_misses_every_file_and_walks_the_tree_twice() {
        let project = Project::copied_from_fixtures();

        let stats = project.scan().stats;

        let files = project.cached_entries() as u32;
        assert!(files > 1);
        assert_eq!(stats.cache_misses, files);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.files_read, files);
        assert_eq!(stats.walks, 2);
    }

    #[test]
    fn the_count_reaches_the_progress_without_the_walk_waiting_for_it() {
        let project = Project::copied_from_fixtures();
        let (updates, drained) = mpsc::sync_channel(1000);

        let outcome = scan_with(
            &project.root,
            &project.settings,
            Some(Watch::new(updates)),
            CachePolicy::Forced,
        )
        .unwrap();

        let last = drained
            .try_iter()
            .last()
            .expect("the scan reported progress");
        assert_eq!(last.total, Some(outcome.files_scanned));
        assert_eq!(outcome.stats.walks, 2, "the walk and the count beside it");
    }

    #[test]
    fn a_cache_from_before_the_container_is_replaced_not_read() {
        let project = Project::copied_from_fixtures();
        let legacy = project
            .root
            .join(crate::cache::CACHE_DIRECTORY)
            .join("cache.json");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, r#"{"version":1,"entries":{}}"#).unwrap();

        let stats = project.scan().stats;

        assert_eq!(stats.cache_hits, 0);
        assert!(!legacy.exists());
        assert!(project.cache_file().exists());
    }

    #[test]
    fn a_warm_run_hits_every_file_and_misses_none() {
        let project = Project::copied_from_fixtures();
        let files = project.scan().analyzed.len() as u32;

        let stats = project.scan().stats;

        assert_eq!(stats.cache_hits, files);
        assert_eq!(stats.cache_misses, 0);
    }

    #[test]
    fn editing_one_file_leaves_every_other_entry_in_the_cache() {
        let project = Project::copied_from_fixtures();
        let analysed = project.scan().analyzed.len();

        let edited = project.root.join("clean.ts");
        let mut source = std::fs::read_to_string(&edited).unwrap();
        source.push_str("\nexport const edited = 1;\n");
        std::fs::write(&edited, source).unwrap();
        project.age(&edited, HOUR / 2);
        project.scan();

        assert_eq!(project.cached_entries(), analysed);
        assert!(project.next_run_hits_every_file());
    }

    #[test]
    fn blame_runs_only_when_stale_todo_can_report() {
        let project = Project::copied_from_fixtures();
        let blamed = project.scan().stats.blames;
        assert!(blamed > 0, "the fixtures hold a note, so a blame is wanted");

        let mut config = rqx_core::config::RqxConfig::default();
        config.rules.disabled.insert("stale-todo".to_owned());
        let settings = Resolved::of(config);
        let skipped = scan_with(&project.root, &settings, None, CachePolicy::Off).unwrap();

        assert_eq!(skipped.stats.blames, 0);
    }

    #[test]
    fn a_warm_run_opens_no_file() {
        let project = Project::copied_from_fixtures();
        let files = project.scan().analyzed.len() as u32;

        let stats = project.scan().stats;

        assert_eq!(stats.files_read, 0);
        assert_eq!(stats.cache_hits, files);
    }

    #[test]
    fn a_file_touched_without_changing_is_read_once_and_then_not_again() {
        let project = Project::copied_from_fixtures();
        project.scan();
        project.age(&project.root.join("clean.ts"), HOUR / 2);

        let touched = project.scan().stats;
        let after = project.scan().stats;

        assert_eq!(touched.files_read, 1);
        assert_eq!(touched.cache_misses, 0);
        assert_eq!(after.files_read, 0);
    }

    /// The edit keeps the length and the file's own time, and the time is later than
    /// the cache was written, which is the one edit a length and a time cannot see.
    #[test]
    fn an_edit_that_leaves_the_length_and_the_time_alone_is_still_seen() {
        let project = Project::copied_from_fixtures();
        let edited = project.root.join("clean.ts");
        let just_now = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        Project::stamp(&edited, just_now);
        project.scan();
        let before = std::fs::read_to_string(&edited).unwrap();

        std::fs::write(&edited, before.replacen("first", "other", 1)).unwrap();
        Project::stamp(&edited, just_now);
        let warm = project.scan();

        assert_eq!(
            std::fs::metadata(&edited).unwrap().len(),
            before.len() as u64
        );
        assert_eq!(warm.stats.cache_misses, 1);
        assert_eq!(warm.stats.files_read, 1);
    }

    #[test]
    fn a_baseline_still_applies_to_files_that_are_not_read() {
        let project = Project::copied_from_fixtures();
        let cold = project.scan();
        assert!(!cold.findings.is_empty());
        let mut settings = Resolved::of(rqx_core::config::RqxConfig::default());
        settings.baseline = Some(Arc::new(
            crate::baseline::Baseline::of(&cold).recorded(rqx_core::config::BaselineMatch::Count),
        ));

        let warm = scan_with(&project.root, &settings, None, CachePolicy::Forced).unwrap();

        assert_eq!(warm.stats.files_read, 0);
        assert!(
            warm.findings
                .iter()
                .any(|finding| finding.rule.is_project_scoped())
        );
        assert!(warm.findings.iter().all(|finding| finding.known));
    }

    fn baseline_holding(findings: &[Finding]) -> crate::baseline::Matcher {
        let mut outcome = crate::report::tests::outcome_with(&[], 0);
        outcome.root = PathBuf::from("/nowhere/rqx-baseline-test");
        outcome.findings = findings.to_vec();
        Arc::new(
            crate::baseline::Baseline::of(&outcome)
                .recorded(rqx_core::config::BaselineMatch::Count),
        )
        .matcher()
    }

    fn of_every_rule() -> Vec<Finding> {
        RuleId::ALL
            .iter()
            .map(|&rule| Finding {
                rule,
                ..ranked("a.ts", 1).0
            })
            .collect()
    }

    /// Matching does not depend on the rule or on the stage that built a finding, so no
    /// list of rules can leave one out: a finding of any rule that the baseline recorded
    /// is known whether it arrived with a file or was built after the walk.
    #[test]
    fn a_recorded_finding_of_every_rule_is_known_from_a_file_and_from_a_stage() {
        let findings = of_every_rule();
        let identities: Vec<Option<rqx_core::Identity>> = findings
            .iter()
            .map(|finding| Some(rqx_core::Identity::resolved(finding, "")))
            .collect();

        let mut from_files = Collector::new(0, false, false, false, None);
        from_files.baseline = Some(baseline_holding(&findings));
        from_files.accept_all(findings.iter().cloned().zip(identities).collect());

        let mut from_stages = Collector::new(0, false, false, false, None);
        from_stages.baseline = Some(baseline_holding(&findings));
        from_stages.absorb(Staged {
            findings: findings.clone(),
            ..Staged::default()
        });

        for collector in [from_files, from_stages] {
            let unknown: Vec<&str> = collector
                .retained
                .iter()
                .filter(|Reverse(Ranked(finding))| !finding.known)
                .map(|Reverse(Ranked(finding))| finding.slug())
                .collect();
            assert_eq!(collector.retained.len(), RuleId::ALL.len());
            assert!(unknown.is_empty(), "reported as new: {unknown:?}");
        }
    }

    /// A baseline never records a waived finding, so a waived repeat must not use up
    /// the one the baseline recorded for the finding that is not waived.
    #[test]
    fn a_waived_finding_does_not_use_up_what_the_baseline_recorded() {
        let recorded = ranked("a.ts", 9).0;
        let waived = ranked("a.ts", 1).0;
        let mut collector = Collector::new(0, false, false, false, None);
        collector.baseline = Some(baseline_holding(std::slice::from_ref(&recorded)));
        collector.suppressions.insert(
            Arc::from("a.ts"),
            vec![rqx_core::suppress::Suppression {
                span: Span::new(0, 1),
                line: 1,
                rules: vec!["imprecise-type".to_owned()],
                reason: Some("generated".to_owned()),
                until: None,
                whole_file: false,
            }],
        );

        collector.absorb(Staged {
            findings: vec![waived, recorded],
            ..Staged::default()
        });

        assert_eq!(collector.suppressed.len(), 1);
        let Some(Reverse(Ranked(kept))) = collector.retained.peek() else {
            panic!("the recorded finding was not kept");
        };
        assert!(kept.known);
    }

    #[test]
    fn a_deleted_file_is_evicted_from_the_cache() {
        let project = Project::copied_from_fixtures();
        let analysed = project.scan().analyzed.len();

        std::fs::remove_file(project.root.join("clean.ts")).unwrap();
        project.scan();

        assert_eq!(project.cached_entries(), analysed - 1);
    }

    fn ranked(file: &str, line: u32) -> Ranked {
        Ranked(Finding {
            rule: RuleId::ImpreciseType,
            location: SourceRange {
                file: Arc::from(file),
                start: line * 10,
                end: line * 10 + 4,
                line,
                column: 1,
                end_line: line,
            },
            severity: rqx_core::finding::Severity::Warning,
            confidence: rqx_core::finding::Confidence::Verified,
            evidence: rqx_core::finding::Evidence::ImpreciseType {
                construct: "as".to_owned(),
            },
            fix_hint: String::new(),
            related: Vec::new(),
            known: false,
            focus: None,
        })
    }

    /// This order decides which findings survive the display cap, so two findings it
    /// calls equal are two findings the arrival order chooses between — and arrival
    /// order is thread scheduling. A repository with more findings than the cap would
    /// show a different set on every run.
    #[test]
    fn the_display_cap_never_has_to_choose_between_two_equal_findings() {
        let (first, second) = (ranked("a.ts", 1), ranked("b.ts", 1));
        assert_ne!(first.cmp(&second), Ordering::Equal, "same rule, same line");
        assert_ne!(
            ranked("a.ts", 1).cmp(&ranked("a.ts", 2)),
            Ordering::Equal,
            "same rule, same file"
        );
        assert_eq!(ranked("a.ts", 1).cmp(&ranked("a.ts", 1)), Ordering::Equal);
        // The heap keeps the greatest, and the report reads best-first, so the
        // earlier file has to sort as the greater of two equally severe findings.
        assert_eq!(first.cmp(&second), Ordering::Greater);
    }
    use super::generated_by;

    #[test]
    fn a_nested_finding_path_uses_forward_slashes_whatever_the_platform() {
        let root = Path::new("repo");
        let nested = root.join("domain").join("billing").join("invoice.ts");

        assert_eq!(
            Positions::new(root).relative(&nested),
            "domain/billing/invoice.ts"
        );
        assert_eq!(relative_to(root, &nested), "domain/billing/invoice.ts");
    }

    #[test]
    fn a_path_outside_the_root_is_still_written_with_forward_slashes() {
        let elsewhere = Path::new("other").join("pkg").join("a.ts");

        assert_eq!(relative_to(Path::new("repo"), &elsewhere), "other/pkg/a.ts");
    }

    #[test]
    fn a_file_is_read_and_indexed_once_however_many_findings_land_in_it() {
        let root = std::env::temp_dir().join(format!("rqx-positions-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("wide.ts");
        std::fs::write(&path, "const é = 1;\nconst b = 2;\n").unwrap();
        let mut at = Positions::new(&root);

        let first = at.range(&path, Span::new(19, 24)).unwrap();
        std::fs::remove_file(&path).unwrap();
        let second = at.range(&path, Span::new(19, 24)).unwrap();

        assert_eq!((first.line, first.column), (2, 6));
        assert_eq!((second.line, second.column), (first.line, first.column));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_stage_keeps_only_the_findings_of_rules_that_are_on() {
        let mut config = rqx_core::config::RqxConfig::default();
        let mut staged = Staged::default();

        staged.accept_if_enabled(&config, ranked("a.ts", 1).0);
        config.rules.disabled.insert("imprecise-type".to_owned());
        staged.accept_if_enabled(&config, ranked("b.ts", 1).0);

        let kept: Vec<&str> = staged
            .findings
            .iter()
            .map(|finding| &*finding.location.file)
            .collect();
        assert_eq!(kept, ["a.ts"]);
    }

    #[test]
    fn what_the_stages_found_is_counted_whichever_collector_takes_it() {
        let mut collector = Collector::new(10, false, false, false, None);
        let mut staged = Staged::default();
        staged.findings.push(ranked("a.ts", 1).0);
        staged.importers.push((Arc::from("a.ts"), 3));
        staged.blames = 2;

        collector.absorb(staged);

        assert_eq!(collector.counts.total, 1);
        assert_eq!(collector.impact.get("a.ts").importers, 3);
        assert_eq!(
            (collector.stats.blames, collector.stats.git_processes),
            (2, 2)
        );
    }

    /// The base revision of a review is a second checkout of the same files. It reads
    /// the working tree's cache so its unchanged files are not analysed again, and it
    /// must leave that cache exactly as it found it.
    #[test]
    fn a_scan_that_borrows_a_cache_is_served_by_it_and_writes_nothing() {
        let lender = Project::copied_from_fixtures();
        lender.scan();
        let cache_file = lender.root.join(".rqx").join("cache.bin");
        let kept = std::fs::read(&cache_file).unwrap();

        let borrower = Project::copied_from_fixtures();
        let outcome = scan_against(
            &borrower.root,
            &borrower.settings,
            None,
            CachePolicy::Forced,
            Comparison::default(),
            Some(&lender.root),
        )
        .unwrap();

        assert!(outcome.stats.cache_hits > 0);
        assert_eq!(outcome.stats.cache_misses, 0, "every file is the lender's");
        assert!(
            !borrower.root.join(".rqx").exists(),
            "the borrower wrote a cache"
        );
        assert_eq!(std::fs::read(&cache_file).unwrap(), kept);
    }

    #[test]
    fn a_marker_past_a_wide_character_is_still_found() {
        let source = format!("{}\n// @generated\n", "\u{4e16}".repeat(2000));
        assert_eq!(generated_by(&source), None);
        assert_eq!(
            generated_by("// @generated\nfn main() {}"),
            Some("generated code")
        );
    }

    #[test]
    fn a_handwritten_file_is_not_claimed() {
        assert_eq!(generated_by("fn main() {}"), None);
    }
}

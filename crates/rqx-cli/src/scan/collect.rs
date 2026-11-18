//! Takes in per-file results: the `Collector` and the outcome it assembles.

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Instant;

use rqx_core::Language;
use rqx_core::finding::{Finding, SourceRange};
use rqx_core::impact::ImpactMap;
use rqx_core::rule::RuleId;
use rqx_core::score::FindingCounts;
use smallvec::SmallVec;

use super::skip::Skipped;
use crate::config::Resolved;
use crate::stats::ScanStats;

use super::stages::{offered, read_coverage, read_profile};
use super::{
    Analyzed, Comparison, CostFacts, CostFile, ScanMessage, ScanOutcome, ScanProgress,
    SeverityTally, Staged, Watch,
};

const MAX_CLONE_SITES: usize = 8;

pub(super) struct Ranked(pub(super) Finding);

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

pub(super) struct CloneGroup {
    pub(super) occurrences: u32,
    pub(super) statements: u32,
    pub(super) nodes: u32,
    pub(super) sites: SmallVec<[SourceRange; 2]>,
}

pub(super) struct PortedGroup {
    pub(super) statements: u32,
    pub(super) nodes: u32,
    pub(super) literals: u64,
    pub(super) languages: std::collections::BTreeSet<Language>,
    pub(super) sites: SmallVec<[SourceRange; 2]>,
}

impl PortedGroup {
    /// A block copied between TypeScript and JavaScript is plain duplication, which
    /// `duplicate-code` already reports; only another family makes it a port.
    pub(super) fn families(&self) -> std::collections::BTreeSet<Language> {
        self.languages
            .iter()
            .map(|language| language.family())
            .collect()
    }
}

/// Findings are capped for display while `counts` stays exact, because a health
/// score that moved with the display cap would not mean anything.
pub(super) struct Collector {
    pub(super) counts: FindingCounts,
    pub(super) retained: BinaryHeap<Reverse<Ranked>>,
    max_findings: usize,
    pub(super) findings_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    severity_by_file: std::collections::BTreeMap<Arc<str>, SeverityTally>,
    lines_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    pub(super) suppressions: HashMap<Arc<str>, Vec<rqx_core::suppress::Suppression>>,
    /// Where each file records work still to do, so the history pass can ask how
    /// long each note has been there.
    pub(super) todos: HashMap<Arc<str>, Vec<rqx_core::facts::Spot>>,
    cycles: Vec<Vec<String>>,
    pub(super) baseline: Option<crate::baseline::Matcher>,
    baseline_findings: Option<u32>,
    /// Only the files a stage's finding names, read once each.
    texts: HashMap<Arc<str>, String>,
    pub(super) root: PathBuf,
    pub(super) suppressed: Vec<rqx_core::suppress::Suppressed>,
    today: String,
    phase: crate::progress::Phase,
    phase_began: std::time::Instant,
    timings: Vec<(&'static str, u128)>,
    pub(super) stats: ScanStats,
    pub(super) git_commands: Vec<String>,
    /// Where progress goes, whoever is drawing it: the command line's bar, the
    /// interface, the language server.
    watcher: Option<SyncSender<ScanProgress>>,
    showing: Option<Arc<str>>,
    pub(super) shape_by_file: std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
    advice: Vec<rqx_core::Advice>,
    pub(super) clone_groups: HashMap<u64, CloneGroup>,
    /// The same blocks keyed by a shape no grammar owns, so logic ported between two
    /// languages can be found. Kept apart from `clone_groups` because a group here is
    /// only interesting when it spans languages.
    pub(super) ported: HashMap<u64, PortedGroup>,
    pub(super) modules: Vec<rqx_lang::module_graph::ModuleEntry>,
    /// The cost facts of the file at the same position in `modules`.
    pub(super) costs: Vec<CostFile>,
    /// The lines of each file where a per-file rule already reports work in a loop, for the
    /// cost stage to leave out of the rules that stand in for them.
    pub(super) covered: HashMap<Arc<str>, Vec<u32>>,
    summaries: rqx_core::cost::Summaries,
    pub(super) keep_costs: bool,
    pub(super) call_links: rqx_lang::cost::compose::CallLinks,
    pub(super) cost_facts: Option<CostFacts>,
    pub(super) analyzed: std::collections::BTreeSet<String>,
    pub(super) impact: ImpactMap,
    import_edges: std::collections::BTreeMap<String, Vec<String>>,
    track_clones: bool,
    track_ported: bool,
    pub(super) track_modules: bool,
    /// The base revision's functions, when this scan is a review that compares with it.
    pub(super) reference: Option<Arc<crate::reimplemented::Reference>>,
    /// What a test run reached, when the project pointed at a coverage file.
    pub(super) coverage: Option<rqx_core::Hits>,
    /// What a run actually spent its time in, when the project pointed at a profile.
    profile: Option<rqx_core::runtime::Samples>,
    code_lines: u64,
    files_scanned: u32,
    skipped: Vec<Skipped>,
    total: Option<u32>,
    started: Instant,
}

impl Collector {
    pub(super) fn new(
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

    /// What the stages after the walk found, taken in the order the stages come in
    /// rather than the order they finished. This is where every cross-file finding is
    /// admitted, under the configuration of the file it is reported in, which is the
    /// resolution a per-file finding gets in its worker. A finding that spans files, a
    /// duplicated block or an import cycle, is judged by the file of its primary
    /// location, the one it is reported at.
    pub(super) fn absorb(&mut self, staged: Staged, settings: &Resolved) {
        let root = &self.root;
        let config_of = |file: &str| settings.for_file(&root.join(file));
        let admitted = staged
            .findings
            .into_iter()
            .filter_map(|finding| offered(config_of(&finding.location.file), finding))
            .map(|finding| (finding, None))
            .collect();
        let advice: Vec<rqx_core::Advice> = staged
            .advice
            .into_iter()
            .filter(|advice| {
                config_of(&advice.location.file)
                    .patterns
                    .enabled(advice.advisory)
            })
            .collect();
        self.accept_all(admitted);
        self.advice.extend(advice);
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
    pub(super) fn accept_all(&mut self, batch: Vec<(Finding, Option<rqx_core::Identity>)>) {
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
    pub(super) fn report_phase(&mut self, phase: crate::progress::Phase) {
        let now = std::time::Instant::now();
        self.timings.push((
            self.phase.label(),
            now.duration_since(self.phase_began).as_millis(),
        ));
        self.phase = phase;
        self.phase_began = now;
        self.tick(Some(phase));
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
            ScanMessage::Skipped(skipped) => collector.skipped.push(skipped),
            ScanMessage::Total(total) => collector.total = total.or(collector.total),
        }
    }
    collector.into_outcome(root, settings, history, watch.is_some_and(Watch::stopped))
}

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result};
use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;
use ignore::{WalkBuilder, WalkState};
use rqx_core::config::RqxConfig;
use oxc_span as oxc_span_alias;
use rqx_core::finding::{Evidence, Finding, SourceRange};
use rqx_core::rule::RuleId;
use rqx_core::impact::ImpactMap;
use rqx_core::score::FindingCounts;
use smallvec::SmallVec;

const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(80);

/// The walk analyzes files far faster than one collector drains them, so an
/// unbounded channel becomes the place a whole repo's findings pile up. Bounding
/// it makes producers wait instead, which is what keeps peak memory a function of
/// worker count rather than file count.
const CHANNEL_DEPTH: usize = 256;
const MAX_CLONE_SITES: usize = 8;

enum ScanMessage {
    File {
        path: PathBuf,
        findings: Vec<Finding>,
        clones: Vec<rqx_ts::CloneSite>,
        module: rqx_ts::semantics::ModuleFacts,
        code_lines: u32,
    },
    Skipped { path: PathBuf, reason: String },
}

/// The scanned-file path is deliberately absent: at ~26,000 files per second a
/// filename field is an unreadable blur, and carrying one would clone a `PathBuf`
/// per file to render nothing a human can follow.
#[derive(Copy, Clone, Debug, Default)]
pub struct ScanProgress {
    pub files_scanned: u32,
    pub findings: u32,
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
    pub findings: Vec<Finding>,
    /// Findings per file before the display cap, so a file with nothing to report
    /// can be told from one whose findings the cap dropped.
    pub findings_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    pub omitted: u32,
    pub code_lines: u64,
    pub files_scanned: u32,
    pub skipped: Vec<(PathBuf, String)>,
    pub elapsed_ms: u128,
}

struct Ranked(Finding);

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.0.rank() == other.0.rank()
    }
}

impl Eq for Ranked {}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.rank().cmp(&other.0.rank())
    }
}

/// Sites are kept in source order rather than arrival order and truncated from
/// the end, so the set retained for a group with more than `MAX_CLONE_SITES`
/// occurrences does not depend on which worker reached which file first.
fn insert_bounded(sites: &mut SmallVec<[SourceRange; 2]>, range: SourceRange) {
    let position = sites.partition_point(|existing| {
        (&existing.file, existing.start) < (&range.file, range.start)
    });
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

/// Findings are capped for display while `counts` stays exact, because a health
/// score that moved with the display cap would not mean anything.
struct Collector {
    counts: FindingCounts,
    retained: BinaryHeap<Reverse<Ranked>>,
    max_findings: usize,
    findings_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    clone_groups: HashMap<u64, CloneGroup>,
    modules: Vec<rqx_ts::module_graph::ModuleEntry>,
    analyzed: std::collections::BTreeSet<String>,
    impact: ImpactMap,
    import_edges: std::collections::BTreeMap<String, Vec<String>>,
    track_clones: bool,
    track_modules: bool,
    code_lines: u64,
    files_scanned: u32,
    skipped: Vec<(PathBuf, String)>,
    show_progress: bool,
    started: Instant,
    painted: Instant,
}

impl Collector {
    fn new(
        max_findings: usize,
        track_clones: bool,
        track_modules: bool,
        show_progress: bool,
    ) -> Self {
        Self {
            counts: FindingCounts::default(),
            retained: BinaryHeap::new(),
            max_findings,
            findings_by_file: std::collections::BTreeMap::new(),
            clone_groups: HashMap::new(),
            modules: Vec::new(),
            analyzed: std::collections::BTreeSet::new(),
            impact: ImpactMap::default(),
            import_edges: std::collections::BTreeMap::new(),
            track_clones,
            track_modules,
            code_lines: 0,
            files_scanned: 0,
            skipped: Vec::new(),
            show_progress,
            started: Instant::now(),
            painted: Instant::now(),
        }
    }

    fn accept_if_enabled(&mut self, config: &RqxConfig, finding: Finding) {
        if config.rules.enabled(finding.rule) {
            self.accept(finding);
        }
    }

    fn accept(&mut self, finding: Finding) {
        self.counts.record(&finding);
        *self.findings_by_file.entry(Arc::clone(&finding.location.file)).or_default() += 1;
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

    fn record_clone(&mut self, site: rqx_ts::CloneSite) {
        if !self.track_clones {
            return;
        }
        let group = self.clone_groups.entry(site.hash).or_insert_with(|| CloneGroup {
            occurrences: 0,
            statements: site.statements,
            nodes: site.nodes,
            sites: SmallVec::new(),
        });
        group.occurrences += 1;
        insert_bounded(&mut group.sites, site.range);
    }

    /// Paced by the clock rather than by file count: 500 files is 20ms on a tree of
    /// small modules and several seconds on a tree of large ones, so a fixed count
    /// either flickers or looks frozen.
    fn report_progress(&mut self, file: &str) {
        if !self.show_progress || self.painted.elapsed() < PROGRESS_INTERVAL {
            return;
        }
        self.painted = Instant::now();
        let mut stderr = std::io::stderr();
        let _ = write!(
            stderr,
            "\r{:80}\r  RUNS  {}   {} files \u{b7} {} findings",
            "",
            elide(file, 44),
            self.files_scanned,
            self.counts.total
        );
        let _ = stderr.flush();
    }

    fn clear_progress(&self) {
        if self.show_progress {
            let _ = write!(std::io::stderr(), "\r{:80}\r", "");
        }
    }

    /// Clone findings are built here rather than per file because a duplicate is
    /// only a duplicate once a second site exists, which is knowable only after
    /// the whole walk.
    fn finish_clones(&mut self) {
        let mut groups: Vec<CloneGroup> = self
            .clone_groups
            .drain()
            .map(|(_, group)| group)
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

        let mut reported: Vec<SourceRange> = Vec::new();
        for group in groups {
            let anchor = group.sites[0].clone();
            if reported.iter().any(|outer| outer.contains(&anchor)) {
                continue;
            }
            reported.push(anchor.clone());
            let spec = RuleId::DuplicateCode.spec();
            let others: Vec<SourceRange> = group.sites.iter().skip(1).cloned().collect();
            self.accept(Finding {
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
            });
        }
    }

    /// The graph can only be judged once every module has been seen, so these are
    /// the findings that cannot exist until the walk is over.
    fn finish_modules(&mut self, root: &Path, config: &RqxConfig) {
        if !self.track_modules || self.modules.is_empty() {
            return;
        }
        let modules = std::mem::take(&mut self.modules);
        let report =
            rqx_ts::module_graph::analyze(modules, root, &config.entrypoints.paths);

        for dead in report.dead_exports {
            let Some(range) = source_range(&dead.path, root, dead.span) else { continue };
            self.accept_if_enabled(config, build(
                RuleId::UnusedExport,
                range,
                Evidence::UnusedExport { symbol: dead.name.clone(), kind: dead.kind.to_owned() },
                format!(
                    "delete `{}`, or add its module to `entrypoints` if something outside \
                     the project imports it",
                    dead.name
                ),
                Vec::new(),
            ));
        }

        for dead in report.dead_files {
            let Some(range) = source_range(&dead.path, root, oxc_span(0, 0)) else { continue };
            self.accept_if_enabled(config, build(
                RuleId::UnusedFile,
                range,
                Evidence::UnusedFile { exports: dead.exports },
                "delete the file, or add it to `entrypoints` if it is reached from outside \
                 the project"
                    .to_owned(),
                Vec::new(),
            ));
        }

        for (importer, targets) in &report.edges {
            self.import_edges.insert(
                relative_to(root, importer),
                targets.iter().map(|target| relative_to(root, target)).collect(),
            );
        }

        for (path, importers) in &report.importers {
            self.impact.set_importers(Arc::from(relative_to(root, path)), *importers);
        }

        for dead in report.unimplemented {
            let Some(range) = source_range(&dead.path, root, dead.span) else { continue };
            self.accept_if_enabled(config, build(
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
            ));
        }

        for cycle in report.cycles {
            let Some(anchor) = cycle.members.first() else { continue };
            let Some(range) = source_range(anchor, root, oxc_span(0, 0)) else { continue };
            let related = cycle
                .members
                .iter()
                .skip(1)
                .filter_map(|member| source_range(member, root, oxc_span(0, 0)))
                .collect();
            self.accept_if_enabled(config, build(
                RuleId::CircularDependency,
                range,
                Evidence::ImportCycle { modules: cycle.members.len() as u32 },
                "break the cycle by moving what they share into a module both can import, \
                 or invert the weaker of the two dependencies behind an interface"
                    .to_owned(),
                related,
            ));
        }
    }

    /// Churn is read once for the whole repository after the walk, because a file
    /// with no findings needs no history and most files have none.
    fn finish_history(&mut self, root: &Path, config: &RqxConfig) {
        if config.scan.churn_days == 0 {
            return;
        }
        let counts = crate::history::commits_per_file(root, config.scan.churn_days);
        if counts.is_empty() {
            return;
        }
        let prefix = crate::history::repository_prefix(root).unwrap_or_default();
        for (repo_path, commits) in counts {
            let Some(relative) = repo_path.strip_prefix(&prefix) else {
                continue;
            };
            self.impact.set_commits(Arc::from(relative.to_owned()), commits);
        }
    }

    /// Two files that almost always change together are one decision split across
    /// two places, which is what Shotgun Surgery describes. It needs history, so it
    /// runs last and stays silent outside a repository.
    fn finish_cochange(&mut self, root: &Path, config: &RqxConfig) {
        // Reads git history, not the import graph, so it must not ride on the gate
        // that builds one — enabling only this rule used to leave it silent.
        if !(config.rules.enabled(RuleId::ShotgunSurgery)
            || config.rules.enabled(RuleId::DivergentChange))
        {
            return;
        }
        let commits = crate::history::commits_by_file_set(root, config.scan.churn_days);
        if commits.len() < 5 {
            return;
        }
        let prefix = crate::history::repository_prefix(root).unwrap_or_default();

        let mut totals: HashMap<String, u32> = HashMap::new();
        let mut pairs: HashMap<(String, String), u32> = HashMap::new();
        for files in &commits {
            let mut relative: Vec<&str> = files
                .iter()
                .filter_map(|file| file.strip_prefix(&prefix))
                .filter(|file| self.analyzed.contains(*file))
                .collect();
            relative.sort_unstable();
            relative.dedup();
            for file in &relative {
                *totals.entry((*file).to_owned()).or_default() += 1;
            }
            for (index, left) in relative.iter().enumerate() {
                for right in &relative[index + 1..] {
                    *pairs
                        .entry(((*left).to_owned(), (*right).to_owned()))
                        .or_default() += 1;
                }
            }
        }

        // Divergent Change is the mirror image, read from the same data: a file that
        // changes often but with a different partner every time is changing for many
        // unrelated reasons.
        if config.rules.enabled(RuleId::DivergentChange) {
            let mut strongest: HashMap<&String, u32> = HashMap::new();
            for ((left, right), count) in &pairs {
                for file in [left, right] {
                    let entry = strongest.entry(file).or_default();
                    *entry = (*entry).max(*count);
                }
            }
            let mut diffuse: Vec<(&String, u32, u32)> = totals
                .iter()
                .filter(|(_, total)| **total >= 10)
                .map(|(file, total)| (file, *total, strongest.get(file).copied().unwrap_or(0)))
                .filter(|(_, total, peak)| peak * 10 < total * 3)
                .collect();
            diffuse.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            for (file, total, peak) in diffuse.into_iter().take(5) {
                let Some(range) = source_range(&root.join(file), root, oxc_span(0, 0)) else {
                    continue;
                };
                self.accept_if_enabled(config, build(
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
                ));
            }
        }

        let mut reported: HashSet<String> = HashSet::new();
        let mut ranked: Vec<((String, String), u32)> = pairs.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if !config.rules.enabled(RuleId::ShotgunSurgery) {
            return;
        }
        for ((left, right), together) in ranked {
            let total = totals.get(&left).copied().unwrap_or(0);
            // Both a floor and a ratio: three commits together is coincidence, and a
            // file that changes constantly co-occurs with everything.
            if total < 5 || together * 4 < total * 3 || !reported.insert(left.clone()) {
                continue;
            }
            let Some(range) = source_range(&root.join(&left), root, oxc_span(0, 0)) else {
                continue;
            };
            let Some(partner) = source_range(&root.join(&right), root, oxc_span(0, 0)) else {
                continue;
            };
            self.accept_if_enabled(config, build(
                RuleId::ShotgunSurgery,
                range,
                Evidence::CoChange { partner: right.clone(), together, total },
                format!(
                    "the decision these two share has no home; move it into one place so                      `{right}` stops needing an edit every time this file does"
                ),
                vec![partner],
            ));
        }
    }

    fn into_outcome(mut self, root: &Path, config: &RqxConfig) -> ScanOutcome {
        self.clear_progress();
        self.finish_clones();
        self.finish_modules(root, config);
        self.finish_history(root, config);
        self.finish_cochange(root, config);
        let retained = self.retained.len() as u32;
        let mut findings: Vec<Finding> =
            self.retained.into_iter().map(|Reverse(ranked)| ranked.0).collect();
        // The key has to be total. Two unused parameters of the same function share
        // a rule, a file and a line, so anything short of the byte offset lets the
        // order the heap happened to produce leak into the output — and that order
        // depends on how many threads ran.
        findings.sort_by(|a, b| {
            b.rank()
                .cmp(&a.rank())
                .then(a.location.file.cmp(&b.location.file))
                .then(a.location.line.cmp(&b.location.line))
                .then(a.location.start.cmp(&b.location.start))
                .then(a.location.end.cmp(&b.location.end))
                // Last resort: two findings can legitimately share a span, so the
                // ordering falls back to their text rather than to heap order.
                .then_with(|| a.fix_hint.cmp(&b.fix_hint))
        });
        self.skipped.sort();
        ScanOutcome {
            root: root.to_path_buf(),
            omitted: self.counts.total.saturating_sub(retained),
            counts: self.counts,
            analyzed: self.analyzed,
            impact: self.impact,
            import_edges: self.import_edges,
            findings,
            findings_by_file: self.findings_by_file,
            code_lines: self.code_lines,
            files_scanned: self.files_scanned,
            skipped: self.skipped,
            elapsed_ms: self.started.elapsed().as_millis(),
        }
    }
}

const fn oxc_span(start: u32, end: u32) -> oxc_span_alias::Span {
    oxc_span_alias::Span::new(start, end)
}

/// Graph and history findings are constructed here rather than through
/// `FindingBuilder`, so the rule filter has to be applied on this path too — without
/// it, disabling a rule or selecting a family would silently miss them.
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
    }
}

/// Graph findings are discovered after the file's text is gone, so the line and
/// column are resolved by re-reading it; a file that vanished mid-scan simply
/// drops its finding rather than reporting a position that no longer exists.
fn source_range(path: &Path, root: &Path, span: oxc_span_alias::Span) -> Option<SourceRange> {
    let text = std::fs::read_to_string(path).ok()?;
    let index = rqx_ts::LineIndex::new(&text);
    let (line, column) = index.locate(span.start);
    let (end_line, _) = index.locate(span.end);
    Some(SourceRange {
        file: Arc::from(relative_to(root, path)),
        start: span.start,
        end: span.end,
        line,
        column,
        end_line,
    })
}

/// Paths are shown relative to what was scanned, not to the shell's working
/// directory: `rqx scan ../other-repo` should still print short, clickable paths.
/// Keeps the tail, which is the part that identifies the file; a progress line that
/// wraps leaves its own remains on screen once the cursor returns.
fn elide(path: &str, width: usize) -> String {
    let count = path.chars().count();
    if count <= width {
        return path.to_owned();
    }
    let tail: String = path.chars().skip(count - width + 1).collect();
    format!("\u{2026}{tail}")
}

fn relative_to(root: &Path, path: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(root) {
        return relative.display().to_string();
    }
    // The graph works in canonical paths while the walker yields them as given, so
    // the two only line up after both are resolved.
    if let (Ok(canonical_root), Ok(canonical)) = (root.canonicalize(), path.canonicalize())
        && let Ok(relative) = canonical.strip_prefix(&canonical_root)
    {
        return relative.display().to_string();
    }
    path.display().to_string()
}

fn collect(
    receiver: Receiver<ScanMessage>,
    root: &Path,
    config: &RqxConfig,
    progress: bool,
    watcher: Option<&SyncSender<ScanProgress>>,
) -> ScanOutcome {
    let mut collector = Collector::new(
        config.scan.max_findings,
        config.rules.enabled(RuleId::DuplicateCode),
        config.rules.needs_modules(),
        progress,
    );
    for message in receiver {
        match message {
            ScanMessage::File { path, findings, clones, module, code_lines } => {
                collector.files_scanned += 1;
                collector.code_lines += u64::from(code_lines);
                for finding in findings {
                    collector.accept(finding);
                }
                for site in clones {
                    collector.record_clone(site);
                }
                let display = relative_to(root, &path);
                collector.report_progress(&display);
                collector.analyzed.insert(display);
                if collector.track_modules {
                    collector.modules.push(rqx_ts::module_graph::ModuleEntry {
                        path: path.canonicalize().unwrap_or(path),
                        facts: module,
                    });
                }
                if let Some(watcher) = watcher {
                    // A dropped update costs nothing: the reader only ever wants the
                    // latest count, so a full channel means it is already current.
                    let _ = watcher.try_send(ScanProgress {
                        files_scanned: collector.files_scanned,
                        findings: collector.counts.total,
                    });
                }
            }
            ScanMessage::Skipped { path, reason } => collector.skipped.push((path, reason)),
        }
    }
    collector.into_outcome(root, config)
}

/// Oversized files are rejected here rather than by the walker's own
/// `max_filesize`, which drops them before they are ever visible: a file that was
/// never analyzed has to be reported as such, or a clean score is a false one.
fn analyze_entry(
    path: &Path,
    root: &Path,
    size: Option<u64>,
    config: &RqxConfig,
    cache: Option<&crate::cache::Cache>,
    sender: &SyncSender<ScanMessage>,
) {
    if size.is_some_and(|bytes| bytes > config.scan.max_filesize) {
        let _ = sender.send(ScanMessage::Skipped {
            path: path.to_owned(),
            reason: format!(
                "{} KB exceeds the {} KB limit (raise `scan.max_filesize`)",
                size.unwrap_or_default() / 1024,
                config.scan.max_filesize / 1024
            ),
        });
        return;
    }
    let message = match std::fs::read_to_string(path) {
        Ok(source) => {
            let display = relative_to(root, path);
            let cached = cache.and_then(|cache| cache.get(&display, &source).cloned());
            let analysed = match cached {
                Some(analysis) => Ok(analysis),
                None => rqx_ts::analyze(path, &display, &source, config).inspect(|analysis| {
                    if let Some(cache) = cache {
                        cache.put(&display, &source, analysis);
                    }
                }),
            };
            match analysed {
                Ok(analysis) => ScanMessage::File {
                    path: path.to_owned(),
                    findings: analysis.findings,
                    clones: analysis.clones,
                    module: analysis.module,
                    code_lines: analysis.code_lines,
                },
                Err(error) => {
                    ScanMessage::Skipped { path: path.to_owned(), reason: error.to_string() }
                }
            }
        }
        Err(error) => ScanMessage::Skipped { path: path.to_owned(), reason: error.to_string() },
    };
    let _ = sender.send(message);
}

fn build_walker(root: &Path, config: &RqxConfig) -> Result<ignore::WalkParallel> {
    let mut types = TypesBuilder::new();
    for glob in ["*.ts", "*.tsx", "*.mts", "*.cts"] {
        types.add("typescript", glob).context("registering a TypeScript file glob")?;
    }
    types.select("typescript");

    let mut overrides = OverrideBuilder::new(root);
    for pattern in &config.ignore.patterns {
        overrides
            .add(&format!("!{pattern}"))
            .with_context(|| format!("ignore pattern `{pattern}` is not a valid glob"))?;
    }

    let pruned: Vec<String> = config.scan.prune_dirs.clone();
    let mut builder = WalkBuilder::new(root);
    builder
        .types(types.build().context("building the TypeScript file matcher")?)
        .overrides(overrides.build().context("building ignore overrides")?)
        .threads(config.scan.threads)
        .filter_entry(move |entry| {
            if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                return !pruned.iter().any(|dir| *dir == name);
            }
            true
        });
    Ok(builder.build_parallel())
}

pub fn scan(root: &Path, config: &RqxConfig) -> Result<ScanOutcome> {
    scan_watched(root, config, None)
}

pub fn scan_with_progress(
    root: &Path,
    config: &RqxConfig,
    watcher: SyncSender<ScanProgress>,
) -> Result<ScanOutcome> {
    scan_watched(root, config, Some(watcher))
}

fn scan_watched(
    root: &Path,
    config: &RqxConfig,
    watcher: Option<SyncSender<ScanProgress>>,
) -> Result<ScanOutcome> {
    let walker = build_walker(root, config)?;
    let cache = config.scan.cache.then(|| crate::cache::Cache::open(root, config));
    let (sender, receiver) = mpsc::sync_channel(CHANNEL_DEPTH);
    // A caller watching the channel is drawing its own progress on this terminal,
    // and a second writer scribbling over it corrupts both.
    let progress = watcher.is_none() && std::io::stderr().is_terminal();

    let collector = thread::scope(|scope| {
        let config_ref = &*config;
        let watcher = watcher.as_ref();
        let root_ref = root;
        let handle =
            scope.spawn(move || collect(receiver, root_ref, config_ref, progress, watcher));
        let cached = cache.as_ref();
        walker.run(|| {
            let sender = sender.clone();
            let config = &*config;
            Box::new(move |entry| {
                match entry {
                    Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => {
                        let size = entry.metadata().ok().map(|meta| meta.len());
                        analyze_entry(entry.path(), root, size, config, cached, &sender);
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
        drop(sender);
        handle.join()
    });

    if let Some(cache) = cache {
        cache.save();
    }

    collector.map_err(|_| anyhow::anyhow!("the collector thread panicked"))
}

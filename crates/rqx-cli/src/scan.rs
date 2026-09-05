use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result};
use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;
use ignore::{WalkBuilder, WalkState};
use rqx_core::Language;
use rqx_core::config::RqxConfig;
use rqx_core::finding::{Evidence, Finding, SourceRange};
use rqx_core::impact::ImpactMap;
use rqx_core::rule::RuleId;
use rqx_core::score::FindingCounts;
use rqx_core::span::Span;
use smallvec::SmallVec;

use crate::config::Resolved;

/// The walk analyzes files far faster than one collector drains them, so an
/// unbounded channel becomes the place a whole repo's findings pile up. Bounding
/// it makes producers wait instead, which is what keeps peak memory a function of
/// worker count rather than file count.
const CHANNEL_DEPTH: usize = 256;
const MAX_CLONE_SITES: usize = 8;

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
    advice: Vec<rqx_core::Advice>,
    clones: Vec<rqx_lang::CloneSite>,
    module: rqx_core::facts::ModuleFacts,
    code_lines: u32,
    shape: rqx_lang::FileShape,
    suppressions: Vec<rqx_core::suppress::Suppression>,
}

#[derive(Clone)]
pub(crate) enum ScanMessage {
    File(Box<Analyzed>),
    Skipped { path: PathBuf, reason: String },
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
    /// Code lines per file, so a file can be graded on its own density rather than
    /// on the project's.
    pub lines_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    /// What the analyser measured per function and per class, whether or not any
    /// threshold was crossed — the distribution a reader wants is the same set of
    /// numbers the rules only see the tail of.
    pub shape_by_file: std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
    /// Findings a waiver in the source asked rqx to leave out of the count. Kept so
    /// every waiver can be audited in one place rather than only being obeyed.
    pub suppressed: Vec<rqx_core::suppress::Suppressed>,
    /// How many findings the baseline recorded, when there is one. The ratchet
    /// compares the total against it.
    pub baseline_size: Option<u32>,
    /// Wall time per stage, so `--timings` can say where a slow scan went rather
    /// than leaving a reader to guess.
    pub timings: Vec<(&'static str, u128)>,
    pub omitted: u32,
    pub code_lines: u64,
    pub files_scanned: u32,
    pub skipped: Vec<(PathBuf, String)>,
    pub elapsed_ms: u128,
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
    /// The same total key the findings are finally sorted by, and total for the same
    /// reason: this order decides which findings the display cap keeps, so anything
    /// that leaves two findings equal lets the order the workers happened to finish
    /// in choose between them.
    fn cmp(&self, other: &Self) -> Ordering {
        let (mine, theirs) = (&self.0, &other.0);
        mine.rank()
            .cmp(&theirs.rank())
            .then_with(|| theirs.location.file.cmp(&mine.location.file))
            .then(theirs.location.line.cmp(&mine.location.line))
            .then(theirs.location.start.cmp(&mine.location.start))
            .then(theirs.location.end.cmp(&mine.location.end))
            .then_with(|| theirs.fix_hint.cmp(&mine.fix_hint))
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

/// Findings are capped for display while `counts` stays exact, because a health
/// score that moved with the display cap would not mean anything.
struct Collector {
    counts: FindingCounts,
    retained: BinaryHeap<Reverse<Ranked>>,
    max_findings: usize,
    findings_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    lines_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    suppressions: HashMap<Arc<str>, Vec<rqx_core::suppress::Suppression>>,
    cycles: Vec<Vec<String>>,
    baseline: Option<Arc<std::collections::HashSet<rqx_core::Identity>>>,
    baseline_findings: Option<u32>,
    /// Only the files a project-scoped finding names, read once each.
    texts: HashMap<Arc<str>, String>,
    root: PathBuf,
    suppressed: Vec<rqx_core::suppress::Suppressed>,
    today: String,
    phase: crate::progress::Phase,
    phase_began: std::time::Instant,
    timings: Vec<(&'static str, u128)>,
    shape_by_file: std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
    advice: Vec<rqx_core::Advice>,
    clone_groups: HashMap<u64, CloneGroup>,
    /// The same blocks keyed by a shape no grammar owns, so logic ported between two
    /// languages can be found. Kept apart from `clone_groups` because a group here is
    /// only interesting when it spans languages.
    ported: HashMap<u64, PortedGroup>,
    modules: Vec<rqx_lang::module_graph::ModuleEntry>,
    analyzed: std::collections::BTreeSet<String>,
    impact: ImpactMap,
    import_edges: std::collections::BTreeMap<String, Vec<String>>,
    track_clones: bool,
    track_ported: bool,
    track_modules: bool,
    /// What a test run reached, when the project pointed at a coverage file.
    coverage: Option<rqx_core::Hits>,
    code_lines: u64,
    files_scanned: u32,
    skipped: Vec<(PathBuf, String)>,
    bar: crate::progress::Bar,
    total: Option<u32>,
    started: Instant,
}

impl Collector {
    fn new(
        max_findings: usize,
        track_clones: bool,
        track_ported: bool,
        track_modules: bool,
        bar: crate::progress::Bar,
        total: Option<u32>,
    ) -> Self {
        Self {
            counts: FindingCounts::default(),
            retained: BinaryHeap::new(),
            max_findings,
            findings_by_file: std::collections::BTreeMap::new(),
            lines_by_file: std::collections::BTreeMap::new(),
            suppressions: HashMap::new(),
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
            shape_by_file: std::collections::BTreeMap::new(),
            advice: Vec::new(),
            clone_groups: HashMap::new(),
            ported: HashMap::new(),
            modules: Vec::new(),
            analyzed: std::collections::BTreeSet::new(),
            impact: ImpactMap::default(),
            import_edges: std::collections::BTreeMap::new(),
            track_clones,
            track_ported,
            track_modules,
            coverage: None,
            code_lines: 0,
            files_scanned: 0,
            skipped: Vec::new(),
            bar,
            total,
            started: Instant::now(),
        }
    }

    /// Advice never passes through `accept`: it must not reach `counts`, and the way
    /// to guarantee that is for it to have no route there at all.
    fn advise_if_enabled(&mut self, config: &RqxConfig, advice: rqx_core::Advice) {
        if config.patterns.enabled(advice.advisory) {
            self.advice.push(advice);
        }
    }

    fn accept_if_enabled(&mut self, config: &RqxConfig, mut finding: Finding) {
        if !config.rules.enabled(finding.rule) {
            return;
        }
        // A run that reached the code contradicts the graph that called it dead. The
        // finding is kept and shown — the graph may still be right about the import —
        // but at a confidence that keeps it out of the health score.
        if let Some(lowered) = rqx_core::runtime::contradicted(finding.rule)
            && ran(&finding.evidence)
        {
            finding.confidence = finding.confidence.min(lowered);
        }
        self.accept(finding);
    }

    /// The waiver that covers this finding, if its file wrote one. Cross-file
    /// findings anchor at line 1, which only a whole-file waiver can reach.
    fn waived(&self, finding: &Finding) -> Option<&rqx_core::suppress::Suppression> {
        self.suppressions
            .get(&finding.location.file)?
            .iter()
            .find(|waiver| waiver.covers(finding, &self.today))
    }

    fn accept(&mut self, mut finding: Finding) {
        // A cycle or a dead export is built after the walk drained, so no worker saw
        // it and nothing has tagged it yet. There are a handful of these against a
        // file per worker, so reading their text here costs almost nothing.
        if let Some(baseline) = &self.baseline
            && finding.rule.is_project_scoped()
        {
            let text = self
                .texts
                .entry(Arc::clone(&finding.location.file))
                .or_insert_with(|| {
                    std::fs::read_to_string(self.root.join(&*finding.location.file))
                        .unwrap_or_default()
                });
            finding.known = baseline.contains(&rqx_core::Identity::resolved(&finding, text));
        }
        // Every finding arrives here — the per-file ones off the channel and the
        // graph, clone and history ones built after it drained — so this is the one
        // place a waiver can reach all of them.
        if let Some(waiver) = self.waived(&finding) {
            self.suppressed.push(rqx_core::suppress::Suppressed {
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
            });
            return;
        }
        self.counts.record(&finding);
        *self
            .findings_by_file
            .entry(Arc::clone(&finding.location.file))
            .or_default() += 1;
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

    fn report_progress(&mut self, file: &str) {
        self.bar.update(
            crate::progress::Phase::Scanning,
            self.files_scanned,
            self.total,
            self.counts.total,
            file,
        );
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
        self.bar
            .update(phase, self.files_scanned, None, self.counts.total, "");
    }

    /// Blocks whose shape appears in more than one language. Same-language repeats
    /// are `duplicate-code`'s job and are already reported with a fingerprint that
    /// knows about identifiers; this only claims the narrower, louder thing.
    fn finish_ported(&mut self, root: &Path, settings: &Resolved) {
        // Ported code keeps its constants even when its syntax changes completely, so
        // a shared literal is what separates the same rule written twice from two
        // blocks that merely branch and return in the same order.
        const MIN_STATEMENTS: u32 = 8;
        const MIN_NODES: u32 = 40;

        let mut groups: Vec<PortedGroup> = self
            .ported
            .drain()
            .map(|(_, group)| group)
            .filter(|group| {
                group.languages.len() > 1
                    && group.sites.len() > 1
                    && group.statements >= MIN_STATEMENTS
                    && group.nodes >= MIN_NODES
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
            self.accept_if_enabled(
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
                },
            );
        }
    }

    fn finish_clones(&mut self, root: &Path, settings: &Resolved) {
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
            self.accept_if_enabled(config, Finding {
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
            });
        }
    }

    /// The patterns built on inheritance, which cannot be seen from one file in a
    /// language that puts one class in each.
    fn advise_hierarchy(
        &mut self,
        modules: &[rqx_lang::module_graph::ModuleEntry],
        config: &RqxConfig,
        at: &mut Positions,
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
            self.advise_if_enabled(
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
    fn finish_architecture(
        &mut self,
        root: &Path,
        config: &RqxConfig,
        report: &rqx_lang::module_graph::GraphReport,
        at: &mut Positions,
    ) {
        let plan = &config.architecture;
        if !plan.is_declared() {
            return;
        }
        let Ok(assignments) = Assignments::build(plan) else {
            return;
        };

        for (importer, imported) in &report.edges {
            let from = relative_to(root, importer);
            let Some(rules) = assignments.of(&from) else {
                continue;
            };
            for target in imported {
                let to = relative_to(root, target);
                let span = report
                    .edge_spans
                    .get(&(importer.clone(), target.clone()))
                    .copied()
                    .unwrap_or(WHOLE_FILE);
                let Some(range) = at.range(importer, span) else {
                    continue;
                };

                if let Some(glob) = assignments.forbids(rules, &to) {
                    self.accept_if_enabled(
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
                    rules.layer.as_deref().and_then(|name| plan.rank(name)),
                    assignments
                        .of(&to)
                        .and_then(|other| other.layer.as_deref())
                        .and_then(|name| plan.rank(name)),
                ) else {
                    continue;
                };
                if outer > inner {
                    self.accept_if_enabled(
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

    fn finish_modules(&mut self, root: &Path, config: &RqxConfig, at: &mut Positions) {
        if !self.track_modules || self.modules.is_empty() {
            return;
        }
        let modules = std::mem::take(&mut self.modules);
        self.advise_hierarchy(&modules, config, at);
        let report = graph_report(modules, root, config);
        self.finish_architecture(root, config, &report, at);

        for dead in report.dead_exports {
            let Some(range) = at.range(&dead.path, dead.span) else {
                continue;
            };
            // A run reaching code the graph called dead means a reader the graph
            // cannot see — a dynamic import, a container wiring by name, reflection.
            // The finding stays, at a confidence that keeps it out of the score.
            let ran = self
                .coverage
                .as_ref()
                .map(|hits| hits.covered(&range.file, range.line, range.end_line));
            self.accept_if_enabled(
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
            let ran = self
                .coverage
                .as_ref()
                .map(|hits| u32::from(hits.ran(&range.file)));
            self.accept_if_enabled(
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
            self.import_edges.insert(
                at.relative(importer),
                targets.iter().map(|target| at.relative(target)).collect(),
            );
        }

        for (path, importers) in &report.importers {
            self.impact
                .set_importers(Arc::from(at.relative(path)), *importers);
        }

        for dead in report.unimplemented {
            let Some(range) = at.range(&dead.path, dead.span) else {
                continue;
            };
            self.accept_if_enabled(
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
            self.cycles.push(ring.clone());
            // Two modules importing each other is a cycle, which the rule above
            // reports; three or more is a group no member can be read apart from,
            // which is a different thing to say about the same fact.
            if cycle.members.len() >= MEDIATOR_MEMBERS {
                let names: Vec<String> = cycle
                    .members
                    .iter()
                    .map(|member| at.relative(member))
                    .collect();
                self.advise_if_enabled(
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
            self.accept_if_enabled(
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
            self.advise_if_enabled(
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
            self.accept_if_enabled(
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
    }

    /// Churn is read once for the whole repository after the walk, because a file
    /// with no findings needs no history and most files have none.
    fn finish_history(&mut self, config: &RqxConfig, history: &crate::history::History) {
        if config.scan.churn_days == 0 {
            return;
        }
        let prefix = history.prefix();
        for (repo_path, commits) in history.commits_per_file() {
            let Some(relative) = repo_path.strip_prefix(prefix) else {
                continue;
            };
            self.impact.set_commits(Arc::from(relative), commits);
        }
    }

    /// Two files that almost always change together are one decision split across
    /// two places, which is what Shotgun Surgery describes. It needs history, so it
    /// runs last and stays silent outside a repository.
    fn finish_cochange(
        &mut self,
        root: &Path,
        config: &RqxConfig,
        at: &mut Positions,
        history: &crate::history::History,
    ) {
        // Reads git history, not the import graph, so it must not ride on the gate
        // that builds one — enabling only this rule used to leave it silent.
        if !(config.rules.enabled(RuleId::ShotgunSurgery)
            || config.rules.enabled(RuleId::DivergentChange))
        {
            return;
        }
        let commits: Vec<&Vec<String>> = history.co_changes().collect();
        if commits.len() < 5 {
            return;
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
                .filter(|file| self.analyzed.contains(*file))
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
                self.accept_if_enabled(
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
            return;
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
            self.accept_if_enabled(
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
    }

    fn into_outcome(
        mut self,
        root: &Path,
        settings: &Resolved,
        history: impl FnOnce() -> Arc<crate::history::History>,
    ) -> ScanOutcome {
        use crate::progress::Phase;
        let config = &settings.config;
        self.report_phase(Phase::Duplicates);
        self.finish_clones(root, settings);
        self.finish_ported(root, settings);
        let mut at = Positions::new(root);
        self.report_phase(Phase::Imports);
        self.finish_modules(root, config, &mut at);
        self.report_phase(Phase::History);
        let history = history();
        self.finish_history(config, &history);
        self.report_phase(Phase::CoChange);
        self.finish_cochange(root, config, &mut at, &history);
        self.bar.clear();
        let retained = self.retained.len() as u32;
        let mut findings: Vec<Finding> = self
            .retained
            .into_iter()
            .map(|Reverse(ranked)| ranked.0)
            .collect();
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
        });
        self.skipped.sort();
        // The last stage has no successor to close it, so it is closed here.
        self.timings
            .push((self.phase.label(), self.phase_began.elapsed().as_millis()));
        ScanOutcome {
            root: root.to_path_buf(),
            omitted: self.counts.total.saturating_sub(retained),
            counts: self.counts,
            analyzed: self.analyzed,
            impact: self.impact,
            import_edges: self.import_edges,
            cycles: self.cycles,
            findings,
            advice: self.advice,
            findings_by_file: self.findings_by_file,
            lines_by_file: self.lines_by_file,
            shape_by_file: self.shape_by_file,
            suppressed: self.suppressed,
            baseline_size: self.baseline_findings,
            timings: self.timings,
            code_lines: self.code_lines,
            files_scanned: self.files_scanned,
            skipped: self.skipped,
            elapsed_ms: self.started.elapsed().as_millis(),
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
    order: Vec<(&'plan String, &'plan rqx_core::config::Assignment)>,
}

impl<'plan> Assignments<'plan> {
    fn build(plan: &'plan rqx_core::config::Architecture) -> Result<Self, globset::Error> {
        let mut builder = globset::GlobSetBuilder::new();
        let mut order = Vec::new();
        for (glob, assignment) in &plan.paths {
            builder.add(globset::Glob::new(glob)?);
            order.push((glob, assignment));
        }
        Ok(Self {
            matcher: builder.build()?,
            order,
        })
    }

    /// The last declaration wins, the way the most specific rule a project wrote is
    /// the one that answers elsewhere in the config.
    fn of(&self, path: &str) -> Option<&'plan rqx_core::config::Assignment> {
        self.matcher
            .matches(path)
            .last()
            .map(|index| self.order[*index].1)
    }

    fn forbids(&self, rules: &rqx_core::config::Assignment, target: &str) -> Option<String> {
        fn matches<'a>(patterns: &'a [String], target: &str) -> Option<&'a String> {
            patterns.iter().find(|pattern| {
                globset::Glob::new(pattern)
                    .map(|glob| glob.compile_matcher().is_match(target))
                    .unwrap_or(false)
            })
        }
        if let Some(glob) = matches(&rules.may_not_import, target) {
            return Some(glob.clone());
        }
        if !rules.may_only_import.is_empty() && matches(&rules.may_only_import, target).is_none() {
            return Some(format!("may_only_import {:?}", rules.may_only_import));
        }
        None
    }
}

/// Every coverage file the project named, merged. A file that cannot be read is
/// skipped rather than fatal: a scan that failed because yesterday's coverage was
/// missing would be a scan nobody runs.
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
    files: HashMap<PathBuf, Option<String>>,
    root: PathBuf,
    /// The walker yields paths as given while the module graph works in canonical ones,
    /// so `strip_prefix` fails for every graph path and the fallback resolved *both*
    /// sides again — two syscalls per import edge. Resolving the root once is the whole
    /// difference.
    canonical_root: Option<PathBuf>,
}

impl Positions {
    fn new(root: &Path) -> Self {
        Self {
            files: HashMap::new(),
            root: root.to_path_buf(),
            canonical_root: root.canonicalize().ok(),
        }
    }

    fn relative(&self, path: &Path) -> String {
        if let Ok(relative) = path.strip_prefix(&self.root) {
            return relative.display().to_string();
        }
        if let Some(root) = &self.canonical_root
            && let Ok(relative) = path.strip_prefix(root)
        {
            return relative.display().to_string();
        }
        if let (Some(root), Ok(canonical)) = (&self.canonical_root, path.canonicalize())
            && let Ok(relative) = canonical.strip_prefix(root)
        {
            return relative.display().to_string();
        }
        path.display().to_string()
    }

    fn range(&mut self, path: &Path, span: Span) -> Option<SourceRange> {
        let file: Arc<str> = Arc::from(self.relative(path));
        let text = self
            .files
            .entry(path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(path).ok())
            .as_deref()?;
        let index = rqx_core::LineIndex::new(text);
        let (line, column) = index.locate(span.start);
        let (end_line, _) = index.locate(span.end);
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

/// The cross-file half: clones, the module graph and the churn rules, which no
/// single file's analysis can reach. Takes a receiver rather than a slice so the
/// parallel walk feeds it as it goes and a caller replaying a cached map can feed it
/// all at once.
pub(crate) fn collect(
    receiver: Receiver<ScanMessage>,
    root: &Path,
    settings: &Resolved,
    bar: crate::progress::Bar,
    total: Option<u32>,
    watcher: Option<&SyncSender<ScanProgress>>,
    // A thunk rather than the history itself: reading git is the one part of a scan
    // that does not depend on the walk, and waiting for it before draining the
    // channel stalls every worker behind it once the buffer fills.
    history: impl FnOnce() -> Arc<crate::history::History>,
) -> ScanOutcome {
    let mut collector = Collector::new(
        settings.config.scan.max_findings,
        settings.enabled_anywhere(RuleId::DuplicateCode),
        settings.enabled_anywhere(RuleId::CrossLanguageDuplicate),
        settings.needs_modules(),
        bar,
        total,
    );
    collector.coverage = read_coverage(root, &settings.config.scan.coverage);
    collector.baseline = settings.baseline.clone();
    collector.baseline_findings = settings.baseline_findings;
    collector.root = root.to_path_buf();
    for message in receiver {
        match message {
            ScanMessage::File(file) => {
                let Analyzed {
                    display,
                    canonical,
                    language,
                    findings,
                    advice,
                    clones,
                    module,
                    code_lines,
                    shape,
                    suppressions,
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
                for finding in findings {
                    collector.accept(finding);
                }
                collector.advice.extend(advice);
                for site in clones {
                    collector.record_clone(site, language);
                }
                collector
                    .lines_by_file
                    .insert(Arc::clone(&shared), code_lines);
                collector.shape_by_file.insert(Arc::clone(&shared), shape);
                collector.report_progress(&display);
                if let Some(path) = canonical {
                    collector.modules.push(rqx_lang::module_graph::ModuleEntry {
                        language,
                        path,
                        facts: module,
                    });
                }
                collector.analyzed.insert(display);
                if let Some(watcher) = watcher {
                    // A dropped update costs nothing: the reader only ever wants the
                    // latest count, so a full channel means it is already current.
                    let _ = watcher.try_send(ScanProgress {
                        files_scanned: collector.files_scanned,
                        findings: collector.counts.total,
                        total: collector.total,
                        phase: crate::progress::Phase::Scanning,
                        file: Some(Arc::clone(&shared)),
                    });
                }
            }
            ScanMessage::Skipped { path, reason } => collector.skipped.push((path, reason)),
        }
    }
    collector.into_outcome(root, settings, history)
}

/// Oversized files are rejected here rather than by the walker's own
/// `max_filesize`, which drops them before they are ever visible: a file that was
/// never analyzed has to be reported as such, or a clean score is a false one.
fn analyze_entry(
    path: &Path,
    root: &Path,
    size: Option<u64>,
    settings: &Resolved,
    cache: Option<&crate::cache::Cache>,
    sender: &SyncSender<ScanMessage>,
) {
    let _ = sender.send(analyze_file(path, root, size, settings, cache));
}

/// One file, analysed. Split from the send so a caller holding its own map of results
/// — the server behind `rqx serve` — can re-analyse one file without a channel and a
/// walk around it.
pub(crate) fn analyze_file(
    path: &Path,
    root: &Path,
    size: Option<u64>,
    settings: &Resolved,
    cache: Option<&crate::cache::Cache>,
) -> ScanMessage {
    let config = settings.for_file(path);
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
    match std::fs::read_to_string(path) {
        Ok(source) => {
            if config.scan.skip_generated
                && let Some(tool) = generated_by(&source)
            {
                return ScanMessage::Skipped {
                    path: path.to_owned(),
                    reason: format!("{tool} (set `scan.skip_generated = false` to include it)"),
                };
            }
            let display = relative_to(root, path);
            let cached = cache.and_then(|cache| cache.get(&display, &source).cloned());
            let analysed = match cached {
                Some(analysis) => Ok(analysis),
                None => rqx_lang::analyze(path, &display, &source, config).inspect(|analysis| {
                    if let Some(cache) = cache {
                        cache.put(&display, &source, analysis);
                    }
                }),
            };
            match analysed {
                Ok(mut analysis) => {
                    // Tagged here rather than on the collector: resolving an identity
                    // needs the file's text, and this is the one thread that already
                    // has it.
                    if let Some(baseline) = &settings.baseline {
                        for finding in &mut analysis.findings {
                            finding.known =
                                baseline.contains(&rqx_core::Identity::resolved(finding, &source));
                        }
                    }
                    ScanMessage::File(Box::new(Analyzed {
                        canonical: settings
                            .needs_modules()
                            .then(|| path.canonicalize().unwrap_or_else(|_| path.to_owned())),
                        display,
                        language: analysis.language,
                        findings: analysis.findings,
                        advice: analysis.advice,
                        clones: analysis.clones,
                        module: analysis.module,
                        code_lines: analysis.code_lines,
                        shape: analysis.shape,
                        suppressions: analysis.suppressions,
                    }))
                }
                Err(error) => ScanMessage::Skipped {
                    path: path.to_owned(),
                    reason: error.to_string(),
                },
            }
        }
        Err(error) => ScanMessage::Skipped {
            path: path.to_owned(),
            reason: error.to_string(),
        },
    }
}

/// One graph per language, merged. Import syntax means different things in different
/// languages, so resolving a Python module against a Java classpath would invent
/// edges — and a missing edge is a dead-code finding about live code.
fn graph_report(
    modules: Vec<rqx_lang::module_graph::ModuleEntry>,
    root: &Path,
    config: &RqxConfig,
) -> rqx_lang::module_graph::GraphReport {
    let mut by_language: HashMap<Language, Vec<rqx_lang::module_graph::ModuleEntry>> =
        HashMap::new();
    for module in modules {
        by_language.entry(module.language).or_default().push(module);
    }

    let mut languages: Vec<Language> = by_language.keys().copied().collect();
    languages.sort();

    let mut merged = rqx_lang::module_graph::GraphReport::default();
    for language in languages {
        let support = rqx_lang::support(language);
        let entries = by_language.remove(&language).unwrap_or_default();
        let resolver = (support.resolver)(root);
        let report = rqx_lang::module_graph::analyze(
            entries,
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

pub fn scan(root: &Path, settings: &Resolved) -> Result<ScanOutcome> {
    scan_watched(root, settings, None)
}

pub fn scan_with_progress(
    root: &Path,
    settings: &Resolved,
    watcher: SyncSender<ScanProgress>,
) -> Result<ScanOutcome> {
    scan_watched(root, settings, Some(watcher))
}

/// The same walker the scan uses, so the total it reports cannot disagree with what is
/// then scanned. Only directory entries are touched: nothing is opened or parsed.
fn count_files(root: &Path, config: &rqx_core::config::RqxConfig) -> Result<Option<u32>> {
    let counted = std::sync::atomic::AtomicU32::new(0);
    build_walker(root, config)?.run(|| {
        Box::new(|entry| {
            if entry.is_ok_and(|entry| entry.file_type().is_some_and(|kind| kind.is_file())) {
                counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            ignore::WalkState::Continue
        })
    });
    Ok(Some(counted.into_inner()).filter(|total| *total > 0))
}

fn scan_watched(
    root: &Path,
    settings: &Resolved,
    watcher: Option<SyncSender<ScanProgress>>,
) -> Result<ScanOutcome> {
    let config = &settings.config;
    // A caller watching the channel is drawing its own progress on this terminal,
    // and a second writer scribbling over it corrupts both.
    let drawing = watcher.is_none();
    // Git depends on nothing the walk produces, so it reads while the files are being
    // parsed rather than after — four processes on the critical path became one, off it.
    let history = {
        let root = root.to_path_buf();
        let days = config.scan.churn_days;
        thread::spawn(move || crate::history::History::read(&root, days))
    };
    let mut bar = if drawing {
        crate::progress::Bar::new(crate::brand::Palette::detect())
    } else {
        crate::progress::Bar::hidden()
    };

    // Counted before scanning so the bar can show a real proportion. This is directory
    // traversal with no file opened and nothing parsed, against a scan measured in
    // seconds — and a bar that claims a percentage it cannot know is worse than none.
    bar.update(crate::progress::Phase::Counting, 0, None, 0, "");
    let total = count_files(root, config)?;

    let walker = build_walker(root, config)?;
    let cache = config
        .scan
        .cache
        .then(|| crate::cache::Cache::open(root, settings));
    let (sender, receiver) = mpsc::sync_channel(CHANNEL_DEPTH);

    let collector = thread::scope(|scope| {
        let settings_ref = &*settings;
        let watcher = watcher.as_ref();
        let root_ref = root;
        let handle = scope.spawn(move || {
            collect(
                receiver,
                root_ref,
                settings_ref,
                bar,
                total,
                watcher,
                || Arc::new(history.join().unwrap_or_default()),
            )
        });
        let cached = cache.as_ref();
        walker.run(|| {
            let sender = sender.clone();
            let settings = &*settings;
            Box::new(move |entry| {
                match entry {
                    Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => {
                        let size = entry.metadata().ok().map(|meta| meta.len());
                        analyze_entry(entry.path(), root, size, settings, cached, &sender);
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

#[cfg(test)]
mod tests {
    use super::*;

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

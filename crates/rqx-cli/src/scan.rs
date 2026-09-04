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
struct Analyzed {
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
}

enum ScanMessage {
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

/// Findings are capped for display while `counts` stays exact, because a health
/// score that moved with the display cap would not mean anything.
struct Collector {
    counts: FindingCounts,
    retained: BinaryHeap<Reverse<Ranked>>,
    max_findings: usize,
    findings_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    lines_by_file: std::collections::BTreeMap<Arc<str>, u32>,
    advice: Vec<rqx_core::Advice>,
    clone_groups: HashMap<u64, CloneGroup>,
    modules: Vec<rqx_lang::module_graph::ModuleEntry>,
    analyzed: std::collections::BTreeSet<String>,
    impact: ImpactMap,
    import_edges: std::collections::BTreeMap<String, Vec<String>>,
    track_clones: bool,
    track_modules: bool,
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
            advice: Vec::new(),
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

    fn accept_if_enabled(&mut self, config: &RqxConfig, finding: Finding) {
        if config.rules.enabled(finding.rule) {
            self.accept(finding);
        }
    }

    fn accept(&mut self, finding: Finding) {
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

    fn record_clone(&mut self, site: rqx_lang::CloneSite) {
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
        insert_bounded(&mut group.sites, site.range);
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
    fn report_phase(&mut self, phase: crate::progress::Phase) {
        self.bar
            .update(phase, self.files_scanned, None, self.counts.total, "");
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
    fn finish_modules(&mut self, root: &Path, config: &RqxConfig, at: &mut Positions) {
        if !self.track_modules || self.modules.is_empty() {
            return;
        }
        let modules = std::mem::take(&mut self.modules);
        self.advise_hierarchy(&modules, config, at);
        let report = graph_report(modules, root, config);

        for dead in report.dead_exports {
            let Some(range) = at.range(&dead.path, dead.span) else {
                continue;
            };
            self.accept_if_enabled(
                config,
                build(
                    RuleId::UnusedExport,
                    range,
                    Evidence::UnusedExport {
                        symbol: dead.name.clone(),
                        kind: dead.kind.to_owned(),
                    },
                    format!(
                        "delete `{}`, or add its module to `entrypoints` if something outside \
                     the project imports it",
                        dead.name
                    ),
                    Vec::new(),
                ),
            );
        }

        for dead in report.dead_files {
            let Some(range) = at.range(&dead.path, WHOLE_FILE) else {
                continue;
            };
            self.accept_if_enabled(
                config,
                build(
                    RuleId::UnusedFile,
                    range,
                    Evidence::UnusedFile {
                        exports: dead.exports,
                    },
                    "delete the file, or add it to `entrypoints` if it is reached from outside \
                 the project"
                        .to_owned(),
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
            let Some(range) = at.range(anchor, WHOLE_FILE) else {
                continue;
            };
            let related: Vec<SourceRange> = cycle
                .members
                .iter()
                .skip(1)
                .filter_map(|member| at.range(member, WHOLE_FILE))
                .collect();
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
        history: &crate::history::History,
    ) -> ScanOutcome {
        use crate::progress::Phase;
        let config = &settings.config;
        self.report_phase(Phase::Duplicates);
        self.finish_clones(root, settings);
        let mut at = Positions::new(root);
        self.report_phase(Phase::Imports);
        self.finish_modules(root, config, &mut at);
        self.report_phase(Phase::History);
        self.finish_history(config, history);
        self.report_phase(Phase::CoChange);
        self.finish_cochange(root, config, &mut at, history);
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
        self.skipped.sort();
        ScanOutcome {
            root: root.to_path_buf(),
            omitted: self.counts.total.saturating_sub(retained),
            counts: self.counts,
            analyzed: self.analyzed,
            impact: self.impact,
            import_edges: self.import_edges,
            findings,
            advice: self.advice,
            findings_by_file: self.findings_by_file,
            lines_by_file: self.lines_by_file,
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
    settings: &Resolved,
    bar: crate::progress::Bar,
    total: Option<u32>,
    watcher: Option<&SyncSender<ScanProgress>>,
    history: &crate::history::History,
) -> ScanOutcome {
    let mut collector = Collector::new(
        settings.config.scan.max_findings,
        settings.enabled_anywhere(RuleId::DuplicateCode),
        settings.needs_modules(),
        bar,
        total,
    );
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
                } = *file;
                collector.files_scanned += 1;
                collector.code_lines += u64::from(code_lines);
                for finding in findings {
                    collector.accept(finding);
                }
                collector.advice.extend(advice);
                for site in clones {
                    collector.record_clone(site);
                }
                let shared: Arc<str> = Arc::from(display.as_str());
                collector
                    .lines_by_file
                    .insert(Arc::clone(&shared), code_lines);
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
    let config = settings.for_file(path);
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
            if config.scan.skip_generated
                && let Some(tool) = generated_by(&source)
            {
                let _ = sender.send(ScanMessage::Skipped {
                    path: path.to_owned(),
                    reason: format!("{tool} (set `scan.skip_generated = false` to include it)"),
                });
                return;
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
                Ok(analysis) => ScanMessage::File(Box::new(Analyzed {
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
                })),
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
    };
    let _ = sender.send(message);
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

fn build_walker(root: &Path, config: &RqxConfig) -> Result<ignore::WalkParallel> {
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
            // Joined only once the walk has drained, which is the first moment its
            // answer is needed and usually long after git finished.
            let history = history.join().unwrap_or_default();
            collect(
                receiver,
                root_ref,
                settings_ref,
                bar,
                total,
                watcher,
                &history,
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

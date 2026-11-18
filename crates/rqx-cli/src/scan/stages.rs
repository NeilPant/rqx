//! The stages that need every file: clones, the module graph, history and cost.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use anyhow::Result;
use rqx_core::Language;
use rqx_core::config::RqxConfig;
use rqx_core::finding::{Evidence, Finding, SourceRange};
use rqx_core::rule::RuleId;
use rqx_core::span::Span;

use super::cost_rules;
use crate::config::Resolved;

use super::analyze::relative_to;
use super::collect::{CloneGroup, Collector, PortedGroup};
use super::{CostFacts, Positions, Staged, build};

/// What a block needs before sharing its shape and constants with a block in another
/// language says more than sharing a few branches and returns.
pub(crate) const PORT_MIN_STATEMENTS: u32 = 8;

pub(crate) const PORT_MIN_NODES: u32 = 40;

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
pub(super) fn offered(config: &RqxConfig, mut finding: Finding) -> Option<Finding> {
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

impl Collector {
    /// Blocks whose shape appears in more than one language. Same-language repeats
    /// are `duplicate-code`'s job and are already reported with a fingerprint that
    /// knows about identifiers; this only claims the narrower, louder thing.
    fn ported_findings(ported: HashMap<u64, PortedGroup>) -> Staged {
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
            staged.propose(Finding {
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
            });
        }
        staged
    }

    fn clone_findings(clone_groups: HashMap<u64, CloneGroup>) -> Staged {
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
            staged.propose(Finding {
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
        settings: &Resolved,
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
        let mut deep: Vec<(PathBuf, Span, String, u32, u32)> = Vec::new();
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
            let limit = settings
                .for_file(declared.path)
                .thresholds
                .inheritance_depth;
            if depth > limit {
                deep.push((
                    declared.path.to_path_buf(),
                    declared.facts.span,
                    (*name).to_owned(),
                    depth,
                    limit,
                ));
            }
        }
        for (path, span, name, depth, limit) in deep {
            let Some(range) = at.range(&path, span) else {
                continue;
            };
            staged.propose(build(
                RuleId::DeepInheritance,
                range,
                Evidence::Breadth {
                    counted: format!("types above `{name}`"),
                    found: depth,
                    limit,
                },
                "give it what it needs as a collaborator, so the answer is one \
                     object away rather than four types up"
                    .to_owned(),
                Vec::new(),
            ));
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
            staged.suggest(rqx_core::Advice {
                advisory,
                location: range,
                signal: rqx_core::Signal::Family {
                    base: found.name.to_owned(),
                    detail: found.detail,
                },
                note,
                related: Vec::new(),
            });
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
                    staged.propose(build(
                        RuleId::ForbiddenImport,
                        range.clone(),
                        Evidence::ForbiddenImport {
                            target: to.clone(),
                            by: glob,
                        },
                        format!("`{to}` is outside what this subtree may reach for"),
                        Vec::new(),
                    ));
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
                    staged.propose(build(
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
                    ));
                }
            }
        }
    }

    fn module_findings(
        modules: &[rqx_lang::module_graph::ModuleEntry],
        root: &Path,
        settings: &Resolved,
        coverage: Option<&rqx_core::Hits>,
        at: &mut Positions,
    ) -> Staged {
        let mut staged = Staged::default();
        if modules.is_empty() {
            return staged;
        }
        // What the graph is and which architecture it is held to are the project's,
        // so they come from the root configuration.
        let config = &settings.config;
        Self::advise_hierarchy(modules, settings, at, &mut staged);
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
            staged.propose(build(
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
            ));
        }

        for dead in report.dead_files {
            let Some(range) = at.range(&dead.path, WHOLE_FILE) else {
                continue;
            };
            let ran = coverage.map(|hits| u32::from(hits.ran(&range.file)));
            staged.propose(build(
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
            ));
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
            let limit = settings.for_file(path).thresholds.fan_in;
            if *importers > limit
                && let Some(range) = at.range(path, WHOLE_FILE)
            {
                staged.propose(build(
                    RuleId::WideFanIn,
                    range,
                    Evidence::Breadth {
                        counted: "modules import this one".to_owned(),
                        found: *importers,
                        limit,
                    },
                    "split it along the seams its importers actually use, so a \
                         change reaches only the ones that asked for it"
                        .to_owned(),
                    Vec::new(),
                ));
            }
        }

        for (importer, targets) in &report.edges {
            let reaches = targets.len() as u32;
            let limit = settings.for_file(importer).thresholds.fan_out;
            if reaches > limit
                && let Some(range) = at.range(importer, WHOLE_FILE)
            {
                staged.propose(build(
                    RuleId::WideFanOut,
                    range,
                    Evidence::Breadth {
                        counted: "modules imported".to_owned(),
                        found: reaches,
                        limit,
                    },
                    "give it one collaborator that knows the rest, or move what it \
                         is orchestrating to the things being orchestrated"
                        .to_owned(),
                    Vec::new(),
                ));
            }
        }

        for dead in report.unimplemented {
            let Some(range) = at.range(&dead.path, dead.span) else {
                continue;
            };
            staged.propose(build(
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
                staged.suggest(rqx_core::Advice {
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
                });
            }
            staged.propose(build(
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
            ));
        }

        for external in &report.external {
            if external.importers.len() < FACADE_IMPORTERS {
                continue;
            }
            let Some(range) = at.range(&external.path, external.span) else {
                continue;
            };
            staged.suggest(rqx_core::Advice {
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
            });
        }

        for pair in report.parallel {
            let Some(range) = at.range(&pair.path, pair.span) else {
                continue;
            };
            staged.propose(build(
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
            ));
        }
        staged
    }

    fn size_findings(
        shape_by_file: &std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
        root: &Path,
        settings: &Resolved,
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
            let limit = settings.for_file(&path).thresholds.module_statements;
            if statements <= limit {
                continue;
            }
            let Some(range) = at.range(&path, opens) else {
                continue;
            };
            staged.propose(build(
                RuleId::OversizedModule,
                range,
                Evidence::Breadth {
                    counted: "statements in this file".to_owned(),
                    found: statements,
                    limit,
                },
                "split it along whatever its functions already group into".to_owned(),
                Vec::new(),
            ));
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
        settings: &Resolved,
        at: &mut Positions,
    ) -> Staged {
        let mut staged = Staged::default();
        if settings.config.scan.churn_days == 0 {
            return staged;
        }
        let prefix = history.prefix();
        for (repo_path, commits) in history.commits_per_file() {
            let Some(relative) = repo_path.strip_prefix(prefix) else {
                continue;
            };
            staged.commits.push((Arc::from(relative), commits));
        }
        Self::stale_todo_findings(todos, root, settings, at, &mut staged);
        staged
    }

    /// Churn is read once for the whole repository after the walk, because a file
    /// with no findings needs no history and most files have none. It counts the
    /// findings each file already has, so it runs once every other stage has been taken.
    fn churn_findings(
        &self,
        analyzed: &std::collections::BTreeSet<String>,
        settings: &Resolved,
        at: &mut Positions,
        history: &crate::history::History,
    ) -> Staged {
        let mut staged = Staged::default();
        if settings.config.scan.churn_days == 0 {
            return staged;
        }
        let prefix = history.prefix();
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
            let thresholds = &settings.for_file(&path).thresholds;
            if commits >= thresholds.hotspot_commits
                && findings > 0
                && let Some(range) = at.range(&path, WHOLE_FILE)
            {
                staged.propose(build(
                    RuleId::Hotspot,
                    range,
                    Evidence::Breadth {
                        counted: format!("commits, with {findings} findings still in it"),
                        found: commits,
                        limit: thresholds.hotspot_commits,
                    },
                    "clean this one first: whatever is wrong here is paid for on \
                         every change"
                        .to_owned(),
                    Vec::new(),
                ));
            }
            if !sole.is_empty()
                && commits >= thresholds.sole_author_commits
                && let Some(range) = at.range(&path, WHOLE_FILE)
            {
                staged.propose(build(
                    RuleId::SingleAuthorFile,
                    range,
                    Evidence::Breadth {
                        counted: "commits, all by one person".to_owned(),
                        found: commits,
                        limit: thresholds.sole_author_commits,
                    },
                    "get a second reader onto it before the first one moves on".to_owned(),
                    Vec::new(),
                ));
            }
        }
        staged
    }

    /// Blame is a process per file, so it runs only when its answer can become a
    /// finding: a project that switched `stale-todo` off pays nothing for it.
    fn stale_todo_findings(
        todos: HashMap<Arc<str>, Vec<rqx_core::facts::Spot>>,
        root: &Path,
        settings: &Resolved,
        at: &mut Positions,
        staged: &mut Staged,
    ) {
        if !settings.enabled_anywhere(RuleId::StaleTodo) {
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
            let limit = settings.for_file(&path).thresholds.todo_age_days;
            for spot in spots {
                let Some(range) = at.range(&path, spot.span) else {
                    continue;
                };
                let Some(written) = dates.get(&range.line) else {
                    continue;
                };
                let age = rqx_core::suppress::days_between(written, &today);
                if age < i64::from(limit) {
                    continue;
                }
                staged.propose(build(
                    RuleId::StaleTodo,
                    range,
                    Evidence::Breadth {
                        counted: format!("days since this {} was written", spot.detail),
                        found: age.unsigned_abs() as u32,
                        limit,
                    },
                    "do it, ticket it, or delete it".to_owned(),
                    Vec::new(),
                ));
            }
        }
    }

    /// Two files that almost always change together are one decision split across
    /// two places, which is what Shotgun Surgery describes. It needs history, so it
    /// stays silent outside a repository.
    fn cochange_findings(
        analyzed: &std::collections::BTreeSet<String>,
        root: &Path,
        settings: &Resolved,
        at: &mut Positions,
        history: &crate::history::History,
    ) -> Staged {
        let mut staged = Staged::default();
        // Reads git history, not the import graph, so it must not ride on the gate
        // that builds one — enabling only this rule used to leave it silent.
        if !(settings.enabled_anywhere(RuleId::ShotgunSurgery)
            || settings.enabled_anywhere(RuleId::DivergentChange))
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
        if settings.enabled_anywhere(RuleId::DivergentChange) {
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
                // Before the cap, so a subtree that switched the rule off does not use
                // up places the rest of the project would have had.
                .filter(|(file, _, _)| {
                    settings
                        .for_file(&root.join(file))
                        .rules
                        .admits(RuleId::DivergentChange, file)
                })
                .collect();
            diffuse.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            for (file, total, peak) in diffuse.into_iter().take(5) {
                let Some(range) = at.range(&root.join(file), WHOLE_FILE) else {
                    continue;
                };
                staged.propose(build(
                    RuleId::DivergentChange,
                    range,
                    Evidence::CoChange {
                        partner: "no single file".to_owned(),
                        together: peak,
                        total,
                    },
                    "split this file along the reasons it changes, so each part has one".to_owned(),
                    Vec::new(),
                ));
            }
        }

        let mut reported: HashSet<&str> = HashSet::new();
        let mut ranked: Vec<((&str, &str), u32)> = pairs.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if !settings.enabled_anywhere(RuleId::ShotgunSurgery) {
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
            staged.propose(build(
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
            ));
        }
        staged
    }

    /// The functions this change added that repeat one the base revision had. Nothing
    /// when the scan has no base to compare with.
    fn reimplemented_findings(
        shape_by_file: &std::collections::BTreeMap<Arc<str>, rqx_lang::FileShape>,
        reference: Option<&crate::reimplemented::Reference>,
    ) -> Staged {
        let mut staged = Staged::default();
        let Some(reference) = reference else {
            return staged;
        };
        for finding in crate::reimplemented::findings(shape_by_file, reference) {
            staged.propose(finding);
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
        let mut staged = cost_rules::staged(composed.sites, root);
        staged.summaries = Some(composed.summaries);
        if keep {
            staged.call_links = Some(composed.links);
        }
        staged
    }

    /// What other tools reported, read from the SARIF files the project named.
    /// Not admitted like a stage's findings: no rule in the catalogue produced them, so
    /// no rule's settings apply to them.
    fn accept_external(&mut self, root: &Path, config: &RqxConfig) {
        let findings = crate::external::read_all(root, &config.scan.sarif_in);
        self.accept_all(
            findings
                .into_iter()
                .map(|finding| (finding, None))
                .collect(),
        );
    }

    /// The stages that need every file, run side by side: duplicates, the import graph,
    /// and what git knows touch nothing but what the walk collected, so none waits for
    /// another. What they found is taken in the order the stages are written in, which
    /// is the order they used to run in, so the output does not depend on which
    /// finished first. Only churn waits, because it counts the findings the others
    /// produced. The phases report what the scan is waiting on, so a stage that
    /// finished while an earlier one was still running shows no time of its own.
    pub(super) fn finish_cross_file(
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
                    Self::clone_findings(clone_groups),
                    Self::ported_findings(ported),
                ]
            });
            let imports = scope.spawn(move || {
                let mut at = Positions::new(root);
                [
                    Self::module_findings(graph_modules, root, settings, coverage_ref, &mut at),
                    Self::size_findings(shapes_ref, root, settings, &mut at),
                    Self::reimplemented_findings(shapes_ref, reference_ref),
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
                let noted = Self::history_findings(todos, &history, root, settings, &mut at);
                let together =
                    Self::cochange_findings(analyzed_ref, root, settings, &mut at, &history);
                (history, noted, together)
            });

            self.report_phase(Phase::Duplicates);
            for staged in joined(duplicates) {
                self.absorb(staged, settings);
            }
            self.report_phase(Phase::Imports);
            for staged in joined(imports) {
                self.absorb(staged, settings);
            }
            self.report_phase(Phase::Cost);
            self.absorb(joined(costed), settings);
            self.report_phase(Phase::History);
            let (history, noted, together) = joined(git);
            self.stats.git_processes += history.git_processes();
            self.git_commands.extend(history.commands().iter().cloned());
            self.absorb(noted, settings);
            let churned =
                self.churn_findings(analyzed_ref, settings, &mut Positions::new(root), &history);
            self.absorb(churned, settings);
            self.report_phase(Phase::CoChange);
            self.absorb(together, settings);
            self.accept_external(root, config);
            history.versioned()
        });

        self.shape_by_file = shapes;
        self.analyzed = analyzed;
        self.coverage = coverage;
        inputs.links = std::mem::take(&mut self.call_links);
        self.cost_facts = self.keep_costs.then_some(inputs);
        versioned
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
pub(super) fn read_profile(root: &Path, paths: &[String]) -> Option<rqx_core::runtime::Samples> {
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

pub(super) fn read_coverage(root: &Path, paths: &[String]) -> Option<rqx_core::Hits> {
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

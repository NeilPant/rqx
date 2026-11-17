//! What each function costs once the functions it calls are folded in. The per-file
//! extraction stops at a call, and the cache holds what it found; this stage reads those
//! facts for every file together, follows each call to the functions it may reach, and
//! solves the call graph callees first, so that a function is costed after everything it
//! can call. Nothing here is cached: a callee edited in another file changes the answer
//! for a caller that did not change, so the answers are worked out again on every scan.

mod fold;
mod graph;
mod solve;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use rqx_core::cost::{
    Conf, FileCost, Order, Recurrence, Shrink, SizeId, SizeSrc, SizeVar, Summaries, Summary,
    TraceStep,
};
use rqx_core::facts::{ImportRequest, ModuleFacts};

use self::fold::{Done, Fold, TRACE_STEPS, Work, bind};
use self::graph::{Graph, NodeId};
use self::solve::{Level, solve};
use super::sizes::Sizes;
use super::tables::CostTables;
use crate::module_graph::{ModuleEntry, ResolvedRequests, resolve_requests_where};

/// One analysed file, as the composition reads it.
pub struct Unit<'a> {
    /// The file as findings name it.
    pub name: &'a str,
    /// The file as imports resolve to it.
    pub path: &'a Path,
    pub facts: &'a ModuleFacts,
    pub cost: &'a FileCost,
}

/// A file the scan analysed: its module, which says what it imports and exports, and the
/// cost facts extracted from it.
pub struct Analysed<'a> {
    pub name: &'a str,
    pub module: &'a ModuleEntry,
    pub cost: &'a FileCost,
}

/// The summary of every function the extraction recorded, from the files of one scan.
/// `complete` says whether those were all the files of the project.
pub fn summarise(
    files: &[Analysed<'_>],
    root: &Path,
    tables: &CostTables,
    complete: bool,
) -> Summaries {
    if files.iter().all(|file| file.cost.is_empty()) {
        return Summaries::new(complete);
    }
    let units: Vec<Unit<'_>> = files
        .iter()
        .map(|file| Unit {
            name: file.name,
            path: &file.module.path,
            facts: &file.module.facts,
            cost: file.cost,
        })
        .collect();
    let requests = requests_of(files, &units, root);
    compose(&units, &requests, tables, complete)
}

/// The import requests the composition may follow, resolved per language family because
/// a resolver only understands the imports of its own language.
fn requests_of(files: &[Analysed<'_>], units: &[Unit<'_>], root: &Path) -> ResolvedRequests {
    let wanted = Wanted::of(units);
    let mut families: std::collections::BTreeMap<rqx_core::Language, Vec<&ModuleEntry>> =
        std::collections::BTreeMap::new();
    for file in files {
        families
            .entry(file.module.language.family())
            .or_default()
            .push(file.module);
    }
    let mut requests = ResolvedRequests::new();
    for (language, modules) in families {
        let resolver = (crate::support(language).resolver)(root);
        requests.extend(resolve_requests_where(
            &modules,
            resolver.as_ref(),
            &|module, position| wanted.accepts(module, position),
        ));
    }
    requests
}

/// The import requests worth resolving: the ones a call goes through, and the ones that
/// pass a name on, because a call can arrive at a function by way of a barrel.
struct Wanted<'a> {
    called_through: HashMap<&'a Path, HashSet<usize>>,
}

impl<'a> Wanted<'a> {
    fn of(units: &[Unit<'a>]) -> Self {
        let mut called_through: HashMap<&Path, HashSet<usize>> = HashMap::new();
        for unit in units {
            let requests: HashSet<usize> = unit
                .cost
                .functions
                .iter()
                .flat_map(|function| &function.calls)
                .filter_map(|call| match &call.callee {
                    rqx_core::cost::CalleeRef::Imported { req, .. } => Some(usize::from(*req)),
                    _ => None,
                })
                .collect();
            if !requests.is_empty() {
                called_through.insert(unit.path, requests);
            }
        }
        Self { called_through }
    }

    fn accepts(&self, module: &ModuleEntry, position: usize) -> bool {
        if self
            .called_through
            .get(module.path.as_path())
            .is_some_and(|requests| requests.contains(&position))
        {
            return true;
        }
        module
            .facts
            .imports
            .get(position)
            .is_some_and(|request| passes_on(&module.facts, request))
    }
}

/// Whether a request hands what it imports on to whoever imports the file: a re-export,
/// or a name the file imports and then exports as well.
pub fn passes_on(facts: &ModuleFacts, request: &ImportRequest) -> bool {
    request.re_export
        || request
            .locals
            .iter()
            .any(|local| facts.exports.iter().any(|export| export.name == *local))
}

/// Whether other files may reach functions through this one without it defining them.
pub fn is_barrel(facts: &ModuleFacts) -> bool {
    facts
        .imports
        .iter()
        .any(|request| passes_on(facts, request))
}

/// `units` composed over the requests they make. The order the units are given in does
/// not matter, and neither does how many threads read them.
pub fn compose(
    units: &[Unit<'_>],
    requests: &ResolvedRequests,
    tables: &CostTables,
    complete: bool,
) -> Summaries {
    let mut ordered: Vec<&Unit<'_>> = units.iter().collect();
    ordered.sort_by(|left, right| left.name.cmp(right.name));
    let mut graph = Graph::new(ordered, requests, tables);
    let mut done: Vec<Option<Done>> = (0..graph.nodes.len()).map(|_| None).collect();

    let adjacency: Vec<Vec<usize>> = graph
        .calls
        .iter()
        .map(|calls| {
            let mut reached: Vec<usize> =
                calls.iter().flat_map(|c| c.nodes.iter().copied()).collect();
            reached.sort_unstable();
            reached.dedup();
            reached
        })
        .collect();
    for mut members in crate::module_graph::components(&adjacency) {
        members.sort_unstable();
        let solved = solve_component(&mut graph, &done, &members, tables);
        for (node, answer) in members.into_iter().zip(solved) {
            done[node] = Some(answer);
        }
    }

    let mut summaries = Summaries::new(complete);
    for (node, answer) in done.into_iter().enumerate() {
        let Some(answer) = answer else { continue };
        summaries.insert(
            graph.file_of(node),
            graph.nodes[node].function.start,
            Summary {
                time: answer.time,
                space: answer.space,
                io: answer.io.unwrap_or_default(),
                stack: answer.stack,
                conf: answer.conf,
                trace: answer.trace,
            },
        );
    }
    summaries
}

/// The answer for each member of a strongly connected component, in the members' order.
fn solve_component(
    graph: &mut Graph<'_>,
    done: &[Option<Done>],
    members: &[NodeId],
    tables: &CostTables,
) -> Vec<Done> {
    match members {
        [node] if recurses(graph, *node) => vec![recursion(graph, done, *node, tables)],
        [node] => vec![plain(graph, done, *node, tables)],
        _ => mutual_recursion(graph, done, members, tables),
    }
}

fn recurses(graph: &Graph<'_>, node: NodeId) -> bool {
    graph.nodes[node].function.rec.is_some() || self_calls(graph, node) > 0
}

fn self_calls(graph: &Graph<'_>, node: NodeId) -> usize {
    graph.calls[node]
        .iter()
        .filter(|call| call.nodes.contains(&node))
        .count()
}

fn plain(graph: &Graph<'_>, done: &[Option<Done>], node: NodeId, tables: &CostTables) -> Done {
    let mut fold = Fold::new(graph, node, done, &[], tables);
    let work = fold.work();
    Done {
        space: work.space.max(&work.callee_space),
        stack: work.below,
        io: work.io,
        conf: fold.conf,
        trace: work.time.lead.steps(graph, node, done),
        time: work.time.order,
    }
}

fn recursion(
    graph: &mut Graph<'_>,
    done: &[Option<Done>],
    node: NodeId,
    tables: &CostTables,
) -> Done {
    let function = graph.nodes[node].function;
    let stated = function.rec.clone();
    let rec = stated.clone().unwrap_or_else(|| Recurrence {
        calls: u8::try_from(self_calls(graph, node)).unwrap_or(u8::MAX),
        shrink: Shrink::Other,
        ..Recurrence::default()
    });
    let depth = graph.depth_variable(node, rec.param);
    let graph = &*graph;
    let cycle = [node];
    let mut fold = Fold::new(graph, node, done, &cycle, tables);
    let work = fold.work();
    // A halving recursion's work at each level is on what the level was handed, so a
    // loop over something the body built or never names, such as the two halves merged
    // back together, is a loop over that level's input.
    let level_input = |order: &Order| {
        if rec.shrink == Shrink::Half {
            level_sizes_are(order, &graph.nodes[node].vars, depth)
        } else {
            order.clone()
        }
    };
    let solved = solve(
        &rec,
        depth,
        &Level {
            time: level_input(&work.time.order),
            space: level_input(&work.space),
            below: work.below.clone(),
            io: work.io.as_ref().map(level_input),
        },
    );
    let mut conf = fold.conf;
    if solved.guessed || stated.is_none() {
        conf = conf.weaker(Conf::Heuristic);
    }
    let function = graph.nodes[node].function;
    let mut trace = vec![TraceStep {
        file: graph.file_of(node).to_owned(),
        line: function.line,
        what: "recursion".to_owned(),
        order: solved.time.clone(),
    }];
    trace.extend(work.time.lead.steps(graph, node, done));
    trace.truncate(TRACE_STEPS);
    Done {
        space: solved.space.max(&work.callee_space).max(&solved.stack),
        stack: solved.stack,
        io: solved.io,
        time: solved.time,
        conf,
        trace,
    }
}

/// `order` with every size the body built itself or could not name replaced by the size
/// `depth` stands for.
fn level_sizes_are(order: &Order, vars: &[SizeVar], depth: SizeId) -> Order {
    let built_here = |variable: SizeId| {
        vars.get(usize::from(variable))
            .is_none_or(|var| var.src == SizeSrc::Local)
    };
    let replaced: BTreeSet<SizeId> = order
        .terms()
        .iter()
        .flat_map(|term| term.factors())
        .map(|(variable, _, _)| *variable)
        .filter(|variable| built_here(*variable))
        .collect();
    replaced.into_iter().fold(order.clone(), |all, variable| {
        all.subst(variable, &Order::var(depth))
    })
}

/// Functions that call one another in a circle: how deep the circle goes is not known,
/// so each costs what all of them cost together, once for every level, and that is said
/// to be a guess.
fn mutual_recursion(
    graph: &mut Graph<'_>,
    done: &[Option<Done>],
    members: &[NodeId],
    tables: &CostTables,
) -> Vec<Done> {
    let depths: Vec<_> = members
        .iter()
        .map(|member| graph.depth_variable(*member, 0))
        .collect();
    let graph = &*graph;
    let mut folds: Vec<Fold<'_, '_>> = members
        .iter()
        .map(|member| Fold::new(graph, *member, done, members, tables))
        .collect();
    let works: Vec<Work> = folds.iter_mut().map(Fold::work).collect();

    let mut answers = Vec::new();
    for (position, member) in members.iter().copied().enumerate() {
        let depth = Order::var(depths[position]);
        let mut together = Together::default();
        for (other_position, other) in members.iter().copied().enumerate() {
            let work = &works[other_position];
            let carried = if other == member {
                Carried::own(work)
            } else {
                Carried::bound(work, graph, other, folds[position].sizes())
            };
            together.add(carried);
        }
        let time = depth.mul(&together.time);
        let mut trace = vec![TraceStep {
            file: graph.file_of(member).to_owned(),
            line: graph.nodes[member].function.line,
            what: "mutual recursion".to_owned(),
            order: time.clone(),
        }];
        trace.extend(works[position].time.lead.steps(graph, member, done));
        trace.truncate(TRACE_STEPS);
        let conf = folds
            .iter()
            .fold(Conf::Heuristic, |all, fold| all.weaker(fold.conf));
        answers.push(Done {
            space: depth.mul(&together.space),
            stack: depth.mul(&together.below),
            io: together.io.map(|io| depth.mul(&io)),
            time,
            conf,
            trace,
        });
    }
    answers
}

/// What one function of a circle costs in the terms of another.
struct Carried {
    time: Order,
    space: Order,
    below: Order,
    io: Option<Order>,
}

impl Carried {
    fn own(work: &Work) -> Self {
        Self {
            time: work.time.order.clone(),
            space: work.space.max(&work.callee_space),
            below: work.below.clone(),
            io: work.io.clone(),
        }
    }

    /// `work`, of the function `other`, in the sizes of the function `sizes` belongs to,
    /// the circle passing each function's parameters on to the next in the same places.
    fn bound(work: &Work, graph: &Graph<'_>, other: NodeId, sizes: &mut Sizes<'_>) -> Self {
        let node = &graph.nodes[other];
        let arguments: Vec<Order> = (0..node.function.params.len())
            .map(|position| sizes.param(u8::try_from(position).unwrap_or(u8::MAX)))
            .collect();
        let mut put = |order: &Order| bind(order, &node.vars, &arguments, sizes);
        Self {
            time: put(&work.time.order),
            space: put(&work.space.max(&work.callee_space)),
            below: put(&work.below),
            io: work.io.as_ref().map(&mut put),
        }
    }
}

#[derive(Default)]
struct Together {
    time: Order,
    space: Order,
    below: Order,
    io: Option<Order>,
}

impl Together {
    fn add(&mut self, carried: Carried) {
        self.time = self.time.max(&carried.time);
        self.space = self.space.max(&carried.space);
        self.below = self.below.max(&carried.below);
        self.io = match (self.io.take(), carried.io) {
            (Some(left), Some(right)) => Some(left.max(&right)),
            (left, right) => left.or(right),
        };
    }
}

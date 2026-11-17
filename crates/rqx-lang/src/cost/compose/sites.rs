//! The places the composed costs say a performance rule applies: a loop around a call
//! that is itself as long as the loop, a recursion as deep as its input, a call a loop
//! repeats unchanged, and two loops that compare their elements. Each place is found
//! from the facts the extraction cached and the summaries just composed, so a rule here
//! sees an edit made to a callee in another file.

use rqx_core::RuleId;
use rqx_core::cost::{Bound, Conf, Cost, FunctionCost, Invoked, Order, SizeId, TraceStep};
use rqx_core::cost_sites::{CrossSite, trace_text};

use super::fold::{Done, Fold, Reached, TRACE_STEPS};
use super::graph::{Graph, NodeId};
use crate::cost::local::{Dimension, entry_order};
use crate::cost::tables::CostTables;

/// Every place of every function, in the order the functions are in the graph.
pub fn find(graph: &Graph<'_>, done: &[Option<Done>], tables: &CostTables) -> Vec<CrossSite> {
    let mut sites = Vec::new();
    for node in 0..graph.nodes.len() {
        if done[node].is_none() {
            continue;
        }
        let mut place = Place {
            graph,
            done,
            tables,
            node,
            fold: Fold::new(graph, node, done, &[], tables),
        };
        sites.extend(place.sites());
    }
    sites
}

/// One function, read for the places in it.
struct Place<'g, 'a> {
    graph: &'g Graph<'a>,
    done: &'g [Option<Done>],
    tables: &'g CostTables,
    node: NodeId,
    fold: Fold<'g, 'a>,
}

/// A loop the visit is inside: how many times it runs, and where it is.
#[derive(Clone, Copy)]
struct Enclosing {
    bound: Bound,
    line: u32,
}

impl Place<'_, '_> {
    fn function(&self) -> &FunctionCost {
        self.graph.nodes[self.node].function
    }

    fn sites(&mut self) -> Vec<CrossSite> {
        let mut sites = self.repeated();
        let mut covered: Vec<u32> = sites.iter().map(|site| site.line).collect();
        for site in self.quadratic() {
            if !covered.contains(&site.line) {
                covered.push(site.line);
                sites.push(site);
            }
        }
        sites.extend(self.deep_recursion());
        sites.extend(self.pair_scans());
        sites
    }

    fn site(
        &self,
        rule: RuleId,
        line: u32,
        subject: &str,
        conf: Conf,
        message: String,
    ) -> CrossSite {
        CrossSite {
            rule,
            file: self.graph.file_of(self.node).to_owned(),
            function: self.function().start,
            line,
            subject: subject.to_owned(),
            depth: 1,
            conf,
            message,
        }
    }

    // -- a loop around a call that grows with the loop ---------------------------------

    fn quadratic(&mut self) -> Vec<CrossSite> {
        let mut calls = Vec::new();
        let time = self.function().time.clone();
        collect_calls(&time, &mut Vec::new(), &mut calls);
        let mut sites = Vec::new();
        for (call, loops) in calls {
            sites.extend(self.quadratic_at(call, &loops));
        }
        sites
    }

    fn quadratic_at(&mut self, call: usize, loops: &[Enclosing]) -> Option<CrossSite> {
        let line = self.function().calls.get(call)?.line;
        for reached in self.fold.reached(call) {
            if reached.node == self.node {
                continue;
            }
            let Some(enclosing) = self.loop_it_grows_with(&reached, loops) else {
                continue;
            };
            return self.quadratic_site(line, &reached, enclosing, loops);
        }
        None
    }

    /// The innermost loop over a size the callee's time grows with.
    fn loop_it_grows_with(
        &self,
        reached: &Reached,
        loops: &[Enclosing],
    ) -> Option<(Enclosing, SizeId)> {
        loops
            .iter()
            .rev()
            .find_map(|enclosing| match enclosing.bound {
                Bound::Size(variable) if grows_with(&reached.time, variable) => {
                    Some((*enclosing, variable))
                }
                _ => None,
            })
    }

    fn quadratic_site(
        &mut self,
        line: u32,
        reached: &Reached,
        (enclosing, variable): (Enclosing, SizeId),
        loops: &[Enclosing],
    ) -> Option<CrossSite> {
        let callee = self.done[reached.node].as_ref()?;
        let mut conf = self.function().conf.weaker(callee.conf);
        if reached.guessed {
            conf = conf.weaker(Conf::Heuristic);
        }
        if conf == Conf::Unknown {
            return None;
        }
        let mut total = reached.time.clone();
        for each in loops {
            total = total.mul(&self.fold.sizes().passes(each.bound));
        }
        let called = self.graph.nodes[reached.node].function;
        let mut steps = vec![
            TraceStep {
                file: self.graph.file_of(self.node).to_owned(),
                line: enclosing.line,
                what: "loop".to_owned(),
                order: total.clone(),
            },
            TraceStep {
                file: self.graph.file_of(reached.node).to_owned(),
                line: called.line,
                what: called.qual.clone(),
                order: reached.time.clone(),
            },
        ];
        steps.extend(callee.trace.iter().cloned());
        steps.truncate(TRACE_STEPS);
        let size = &self.function().vars[usize::from(variable)].name;
        let message = format!(
            "is called on every pass of a loop over `{size}` and itself takes time in `{size}`. {}",
            trace_text(&total, &steps, self.graph.file_of(self.node))
        );
        let name = called.name.as_deref().unwrap_or(&called.qual);
        let mut site = self.site(RuleId::QuadraticViaCall, line, name, conf, message);
        site.depth = loops.len() as u32;
        Some(site)
    }

    // -- a recursion as deep as its input ------------------------------------------------

    fn deep_recursion(&self) -> Option<CrossSite> {
        let function = self.function();
        let rec = function.rec.as_ref()?;
        if rec.calls != 1 || rec.shrink != rqx_core::cost::Shrink::Dec1 {
            return None;
        }
        let answer = self.done[self.node].as_ref()?;
        if !at_least_linear(&answer.stack) || answer.conf == Conf::Unknown {
            return None;
        }
        let line = function
            .calls
            .iter()
            .zip(&self.graph.calls[self.node])
            .find(|(_, reached)| reached.nodes.contains(&self.node))
            .map_or(function.line, |(call, _)| call.line);
        let shrinks = function
            .params
            .get(usize::from(rec.param))
            .map_or("its argument", |param| param.name.as_str());
        let message = format!(
            "calls itself once with `{shrinks}` one smaller each time, so the stack it needs is {} deep",
            answer.stack
        );
        let name = function.name.as_deref().unwrap_or(&function.qual);
        Some(self.site(RuleId::DeepRecursion, line, name, answer.conf, message))
    }

    // -- a call a loop repeats unchanged -------------------------------------------------

    fn repeated(&mut self) -> Vec<CrossSite> {
        let repeats = self.function().repeats.clone();
        let mut sites = Vec::new();
        for repeat in repeats {
            let conf = match repeat.of {
                Invoked::Call(call) => self.repeated_call(usize::from(call)),
                Invoked::Lib { entry, size, arg } => self.repeated_library(entry, size, arg),
            };
            let Some(conf) = conf else { continue };
            let mut site = self.site(
                RuleId::RepeatedComputationInLoop,
                repeat.line,
                &repeat.subject,
                conf,
                String::new(),
            );
            if let Invoked::Call(call) = repeat.of {
                site.depth = u32::from(
                    self.function()
                        .calls
                        .get(usize::from(call))
                        .map_or(1, |site| site.in_loop.max(1)),
                );
            }
            sites.push(site);
        }
        sites
    }

    /// Whether the function called costs at least linear time. A call's purity is not
    /// something the callee's cost can show, so the finding is only ever a heuristic.
    fn repeated_call(&mut self, call: usize) -> Option<Conf> {
        let callee_costly = self
            .fold
            .reached(call)
            .iter()
            .any(|reached| reached.node != self.node && at_least_linear(&reached.time));
        callee_costly.then_some(Conf::Heuristic)
    }

    fn repeated_library(
        &mut self,
        entry: u16,
        size: rqx_core::cost::ArgSize,
        arg: rqx_core::cost::ArgSize,
    ) -> Option<Conf> {
        let found = self.tables.entry(entry)?;
        if !found.pure || matches!(size, rqx_core::cost::ArgSize::Unknown) {
            return None;
        }
        let sizes = self.fold.sizes();
        let order = entry_order(found, Dimension::Time, &sizes.of(size), &sizes.of(arg));
        at_least_linear(&order).then_some(self.function().conf)
    }

    // -- two loops that compare their elements -------------------------------------------

    fn pair_scans(&self) -> Vec<CrossSite> {
        self.function()
            .pair_scans
            .iter()
            .map(|pair| {
                let message = format!(
                    "compares each element of `{}` with each element of `{}`, so the pair costs a \
                     comparison per combination where a map built from one of them costs one \
                     lookup per element of the other",
                    pair.inner, pair.outer
                );
                let mut site = self.site(
                    RuleId::PairScanCouldHash,
                    pair.line,
                    pair.comparison.trim(),
                    Conf::Heuristic,
                    message,
                );
                site.depth = 2;
                site
            })
            .collect()
    }
}

/// Every call in a cost tree with the loops around it, outermost first.
fn collect_calls(cost: &Cost, loops: &mut Vec<Enclosing>, into: &mut Vec<(usize, Vec<Enclosing>)>) {
    match cost {
        Cost::One | Cost::Lib { .. } => {}
        Cost::Loop { bound, body, line } => {
            loops.push(Enclosing {
                bound: *bound,
                line: *line,
            });
            collect_calls(body, loops, into);
            loops.pop();
        }
        Cost::Seq(items) | Cost::Branch(items) => {
            for item in items {
                collect_calls(item, loops, into);
            }
        }
        Cost::Call(index) if !loops.is_empty() => into.push((usize::from(*index), loops.clone())),
        Cost::Call(_) => {}
    }
}

/// Whether the order grows at least as fast as one size does, or faster.
fn at_least_linear(order: &Order) -> bool {
    order.is_exponential()
        || order
            .terms()
            .iter()
            .flat_map(|term| term.factors())
            .any(|(_, power, _)| *power >= 1)
}

/// Whether the order has the size in it to at least the first power.
fn grows_with(order: &Order, size: SizeId) -> bool {
    order
        .terms()
        .iter()
        .flat_map(|term| term.factors())
        .any(|(variable, power, _)| *variable == size && *power >= 1)
}

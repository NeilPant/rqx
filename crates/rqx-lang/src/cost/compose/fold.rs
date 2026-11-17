//! A function's cost trees turned into orders, with what its callees cost put in the
//! caller's own sizes.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use rqx_core::cost::{Conf, Cost, Order, SizeId, SizeSrc, SizeVar, TraceStep};

use super::graph::{Graph, NodeId};
use crate::cost::local::{Dimension, entry_order};
use crate::cost::sizes::Sizes;
use crate::cost::tables::CostTables;

/// How many steps of the way to a function's cost are kept.
pub const TRACE_STEPS: usize = 6;

/// A function once its callees are folded in, in the function's own size variables.
pub struct Done {
    pub time: Order,
    pub space: Order,
    /// `None` when the function makes no call that leaves the process, which is not the
    /// same as making one: a loop over a function that makes one is as long as the loop.
    pub io: Option<Order>,
    pub stack: Order,
    pub conf: Conf,
    pub trace: Vec<TraceStep>,
}

impl Done {
    fn is_constant(&self) -> bool {
        self.time.is_one() && self.space.is_one() && self.io.is_none() && self.stack.is_one()
    }
}

/// What one function's body costs, before the calls that recurse are accounted for.
pub struct Work {
    pub time: Led,
    pub space: Order,
    /// What the callees hold while they run, beyond what the body holds itself.
    pub callee_space: Order,
    /// The deepest stack any callee needs.
    pub below: Order,
    pub io: Option<Order>,
}

/// An order together with the part of the body it comes from.
pub struct Led {
    pub order: Order,
    pub io: Option<Order>,
    pub lead: Lead,
}

impl Led {
    fn constant() -> Self {
        Self {
            order: Order::one(),
            io: None,
            lead: Lead::Nothing,
        }
    }

    /// Whichever of two alternatives or successive parts costs more: the larger order, and
    /// the lead of the one that is heavier.
    fn or(self, other: Self) -> Self {
        Self {
            order: self.order.max(&other.order),
            io: match (self.io, other.io) {
                (Some(left), Some(right)) => Some(left.max(&right)),
                (left, right) => left.or(right),
            },
            lead: self.lead.heavier(other.lead),
        }
    }
}

/// The part of a function's time that dominates it, kept as what it is rather than as
/// the steps it will be written as, because most of them are thrown away unseen.
#[derive(Clone)]
pub enum Lead {
    Nothing,
    Loop {
        line: u32,
        order: Order,
        inner: Box<Lead>,
    },
    Call {
        line: u32,
        callee: NodeId,
        order: Order,
    },
    Lib {
        line: u32,
        entry: u16,
        order: Order,
    },
}

impl Lead {
    fn rank(&self) -> u32 {
        match self {
            Self::Nothing => 0,
            Self::Loop { order, .. } | Self::Call { order, .. } | Self::Lib { order, .. } => {
                order.rank()
            }
        }
    }

    fn line(&self) -> u32 {
        match self {
            Self::Nothing => u32::MAX,
            Self::Loop { line, .. } | Self::Call { line, .. } | Self::Lib { line, .. } => *line,
        }
    }

    /// The heavier of two leads, and on a tie the one nearer the top of the file.
    fn heavier(self, other: Self) -> Self {
        let wins = other.rank() > self.rank()
            || (other.rank() == self.rank() && other.line() < self.line());
        if wins { other } else { self }
    }

    /// The steps from this lead down, a callee's own steps following the call into it.
    pub fn steps(&self, graph: &Graph<'_>, node: NodeId, done: &[Option<Done>]) -> Vec<TraceStep> {
        let file = graph.file_of(node);
        let mut steps = Vec::new();
        let mut at = self;
        loop {
            match at {
                Self::Nothing => break,
                Self::Loop {
                    line, order, inner, ..
                } => {
                    steps.push(TraceStep {
                        file: file.to_owned(),
                        line: *line,
                        what: "loop".to_owned(),
                        order: order.clone(),
                    });
                    at = inner;
                }
                Self::Call { callee, order, .. } => {
                    let function = graph.nodes[*callee].function;
                    steps.push(TraceStep {
                        file: graph.file_of(*callee).to_owned(),
                        line: function.line,
                        what: function.qual.clone(),
                        order: order.clone(),
                    });
                    if let Some(below) = done[*callee].as_ref() {
                        steps.extend(below.trace.iter().cloned());
                    }
                    break;
                }
                Self::Lib { line, entry, order } => {
                    steps.push(TraceStep {
                        file: file.to_owned(),
                        line: *line,
                        what: graph.entry_id(*entry),
                        order: order.clone(),
                    });
                    break;
                }
            }
        }
        steps.truncate(TRACE_STEPS);
        steps
    }
}

/// What a callee costs, in the sizes of the function that calls it.
struct Callee {
    node: NodeId,
    time: Order,
    space: Order,
    io: Option<Order>,
    stack: Order,
}

pub struct Fold<'g, 'a> {
    graph: &'g Graph<'a>,
    node: NodeId,
    done: &'g [Option<Done>],
    /// The functions that recurse into one another with this one. Calls among them are
    /// what the recursion accounts for, so a fold counts them as constant.
    cycle: &'g [NodeId],
    tables: &'g CostTables,
    sizes: Sizes<'g>,
    /// The variable of each loop whose bound is not known, by the line it is on, so the
    /// loop is one quantity whether its time or its space is being read.
    unknown_bounds: HashMap<u32, Order>,
    callees: Vec<Option<Rc<Vec<Callee>>>>,
    /// How sure the fold is, which is no more than the least sure thing it used.
    pub conf: Conf,
}

impl<'g, 'a> Fold<'g, 'a> {
    pub fn new(
        graph: &'g Graph<'a>,
        node: NodeId,
        done: &'g [Option<Done>],
        cycle: &'g [NodeId],
        tables: &'g CostTables,
    ) -> Self {
        let function = graph.nodes[node].function;
        Self {
            graph,
            node,
            done,
            cycle,
            tables,
            sizes: Sizes::new(&graph.nodes[node].vars),
            unknown_bounds: HashMap::new(),
            callees: vec![None; function.calls.len()],
            conf: function.conf,
        }
    }

    pub fn work(&mut self) -> Work {
        let function = self.graph.nodes[self.node].function;
        let time = self.fold(&function.time, Dimension::Time);
        let space = self.fold(&function.space, Dimension::Space).order;
        let mut callee_space = Order::one();
        let mut below = Order::one();
        for index in 0..function.calls.len() {
            for callee in self.callees(index).iter() {
                callee_space = callee_space.max(&callee.space);
                below = below.max(&callee.stack);
            }
        }
        let direct = self.direct_round_trips();
        let io = match (time.io.clone(), direct) {
            (Some(through_calls), Some(direct)) => Some(through_calls.max(&direct)),
            (through_calls, direct) => through_calls.or(direct),
        };
        Work {
            time,
            space,
            callee_space,
            below,
            io,
        }
    }

    pub fn sizes(&mut self) -> &mut Sizes<'g> {
        &mut self.sizes
    }

    /// The round trips the extraction counted in the body itself.
    fn direct_round_trips(&mut self) -> Option<Order> {
        let indicators = &self.graph.nodes[self.node].function.ind;
        (indicators.round_trip || !indicators.round_trips.is_one())
            .then(|| self.fold(&indicators.round_trips, Dimension::Time).order)
    }

    fn fold(&mut self, cost: &Cost, dimension: Dimension) -> Led {
        match cost {
            Cost::One => Led::constant(),
            Cost::Loop { bound, body, line } => {
                let passes = self.passes(*bound, *line);
                let inner = self.fold(body, dimension);
                let order = passes.mul(&inner.order);
                let lead = if passes.is_one() {
                    inner.lead
                } else {
                    Lead::Loop {
                        line: *line,
                        order: order.clone(),
                        inner: Box::new(inner.lead),
                    }
                };
                Led {
                    order,
                    io: inner.io.map(|io| passes.mul(&io)),
                    lead,
                }
            }
            Cost::Seq(items) | Cost::Branch(items) => items
                .iter()
                .map(|item| self.fold(item, dimension))
                .fold(Led::constant(), Led::or),
            Cost::Call(index) => self.call(usize::from(*index), dimension),
            Cost::Lib {
                entry,
                size,
                line,
                arg,
            } => {
                let Some(found) = self.tables.entry(*entry) else {
                    return Led::constant();
                };
                let (size, arg) = (self.sizes.of(*size), self.sizes.of(*arg));
                let order = entry_order(found, dimension, &size, &arg);
                let lead = if order.is_one() {
                    Lead::Nothing
                } else {
                    Lead::Lib {
                        line: *line,
                        entry: *entry,
                        order: order.clone(),
                    }
                };
                Led {
                    order,
                    io: None,
                    lead,
                }
            }
        }
    }

    fn passes(&mut self, bound: rqx_core::cost::Bound, line: u32) -> Order {
        if bound != rqx_core::cost::Bound::Unknown {
            return self.sizes.passes(bound);
        }
        if let Some(known) = self.unknown_bounds.get(&line) {
            return known.clone();
        }
        let fresh = self.sizes.fresh();
        self.unknown_bounds.insert(line, fresh.clone());
        fresh
    }

    /// A call is as costly as the costliest function it may reach.
    fn call(&mut self, index: usize, dimension: Dimension) -> Led {
        let line = self.graph.nodes[self.node]
            .function
            .calls
            .get(index)
            .map_or(0, |call| call.line);
        let mut led = Led::constant();
        for callee in self.callees(index).iter() {
            let order = match dimension {
                Dimension::Time => &callee.time,
                Dimension::Space => &callee.space,
            };
            let lead = if order.is_one() {
                Lead::Nothing
            } else {
                Lead::Call {
                    line,
                    callee: callee.node,
                    order: order.clone(),
                }
            };
            led = led.or(Led {
                order: order.clone(),
                io: callee.io.clone(),
                lead,
            });
        }
        led
    }

    /// What each function the call may reach costs here. Worked out the first time it is
    /// asked for, because an argument nothing is known about takes a variable of its
    /// own, and the variables are best numbered in the order the body is read.
    fn callees(&mut self, index: usize) -> Rc<Vec<Callee>> {
        if let Some(known) = self.callees.get(index).and_then(Option::as_ref) {
            return Rc::clone(known);
        }
        let graph = self.graph;
        let site = graph.nodes[self.node].function.calls.get(index);
        let reached = graph.calls[self.node].get(index);
        let mut bound = Vec::new();
        if let (Some(site), Some(reached)) = (site, reached) {
            let live: Vec<NodeId> = reached
                .nodes
                .iter()
                .copied()
                .filter(|callee| !self.cycle.contains(callee) && self.done[*callee].is_some())
                .collect();
            if !live.is_empty() {
                let arguments: Vec<Order> =
                    site.args.iter().map(|size| self.sizes.of(*size)).collect();
                for callee in live {
                    if let Some(found) = self.bind(callee, &arguments, reached.guessed) {
                        bound.push(found);
                    }
                }
            }
        }
        let bound = Rc::new(bound);
        if let Some(slot) = self.callees.get_mut(index) {
            *slot = Some(Rc::clone(&bound));
        }
        bound
    }

    fn bind(&mut self, callee: NodeId, arguments: &[Order], guessed: bool) -> Option<Callee> {
        let done = self.done[callee].as_ref()?;
        if done.is_constant() {
            return None;
        }
        self.conf = self.conf.weaker(done.conf);
        if guessed {
            self.conf = self.conf.weaker(Conf::Heuristic);
        }
        let vars = &self.graph.nodes[callee].vars;
        let sizes = &mut self.sizes;
        let mut put = |order: &Order| bind(order, vars, arguments, sizes);
        Some(Callee {
            node: callee,
            time: put(&done.time),
            space: put(&done.space),
            io: done.io.as_ref().map(&mut put),
            stack: put(&done.stack),
        })
    }
}

/// The variables a callee's temporaries are renamed to while the caller's sizes are put
/// in, kept clear of any a function has so a replacement cannot be replaced again.
const TEMPORARY: SizeId = SizeId::MAX / 2;

/// `order`, written in the variables of the function it is the cost of, written in
/// the caller's: a variable that stands for a parameter becomes the size of the
/// argument passed for it, and any other is a size the caller knows nothing about.
pub fn bind(
    order: &Order,
    callee: &[SizeVar],
    arguments: &[Order],
    sizes: &mut Sizes<'_>,
) -> Order {
    if order.is_one() || order.is_exponential() {
        return order.clone();
    }
    let used: BTreeSet<SizeId> = order
        .terms()
        .iter()
        .flat_map(|term| term.factors())
        .map(|(variable, _, _)| *variable)
        .collect();
    let mut bound = order.clone();
    for variable in &used {
        bound = bound.subst(*variable, &Order::var(TEMPORARY.saturating_add(*variable)));
    }
    for variable in used {
        let replacement = match callee.get(usize::from(variable)).map(|var| &var.src) {
            Some(SizeSrc::Param(position)) => arguments
                .get(usize::from(*position))
                .cloned()
                .unwrap_or_else(|| sizes.fresh()),
            _ => sizes.fresh(),
        };
        bound = bound.subst(TEMPORARY.saturating_add(variable), &replacement);
    }
    bound
}

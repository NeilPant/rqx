//! Renumbering a function's size variables once its body has been read. Two names can
//! turn out to be one size (a list that is only added to inside a loop over `values`
//! is no longer than `values`), and that is only known after the passes that used them
//! have been written down.

use std::collections::{BTreeSet, HashMap};

use rqx_core::cost::{ArgSize, Bound, CallSite, Cost, SizeSrc, SizeVar};

use super::shape::Parts;

/// Which variable each one stands for, and where the survivors move to.
pub struct Remap {
    /// For every old variable, the variable it is the same size as.
    same_as: Vec<u16>,
    /// For every survivor, its new position.
    renumbered: HashMap<u16, u16>,
    kept: Vec<SizeVar>,
    /// The parameter position each survivor comes from, if it is one.
    source: Vec<SizeSrc>,
}

impl Remap {
    /// `same_as[v]` is the variable `v` was found to be the same size as, or `v`.
    /// Variables that nothing refers to are dropped, except parameters, which a
    /// call's argument names by position.
    pub fn new(vars: &[SizeVar], same_as: Vec<u16>, referenced: &BTreeSet<u16>) -> Self {
        let live: BTreeSet<u16> = referenced
            .iter()
            .map(|var| same_as[usize::from(*var)])
            .collect();
        let mut renumbered = HashMap::new();
        let mut kept = Vec::new();
        for (index, var) in vars.iter().enumerate() {
            let index = index as u16;
            let survives = same_as[usize::from(index)] == index
                && (live.contains(&index) || matches!(var.src, SizeSrc::Param(_)));
            if survives {
                renumbered.insert(index, kept.len() as u16);
                kept.push(var.clone());
            }
        }
        let source = kept.iter().map(|var| var.src.clone()).collect();
        Self {
            same_as,
            renumbered,
            kept,
            source,
        }
    }

    pub fn into_vars(self) -> Vec<SizeVar> {
        self.kept
    }

    fn variable(&self, old: u16) -> u16 {
        let same = self.same_as.get(usize::from(old)).copied().unwrap_or(old);
        self.renumbered.get(&same).copied().unwrap_or(0)
    }

    fn bound(&self, bound: Bound) -> Bound {
        match bound {
            Bound::Size(var) => Bound::Size(self.variable(var)),
            Bound::Log(var) => Bound::Log(self.variable(var)),
            other => other,
        }
    }

    pub fn arg(&self, size: ArgSize) -> ArgSize {
        match size {
            ArgSize::Local(var) => {
                let new = self.variable(var);
                match self.source.get(usize::from(new)) {
                    Some(SizeSrc::Param(position)) => ArgSize::Param(*position),
                    _ => ArgSize::Local(new),
                }
            }
            other => other,
        }
    }

    pub fn cost(&self, cost: &Cost) -> Cost {
        match cost {
            Cost::One => Cost::One,
            Cost::Call(index) => Cost::Call(*index),
            Cost::Loop { bound, body, line } => Cost::Loop {
                bound: self.bound(*bound),
                body: Box::new(self.cost(body)),
                line: *line,
            },
            Cost::Seq(items) => Cost::Seq(items.iter().map(|item| self.cost(item)).collect()),
            Cost::Branch(items) => Cost::Branch(items.iter().map(|item| self.cost(item)).collect()),
            Cost::Lib {
                entry,
                size,
                line,
                arg,
            } => Cost::Lib {
                entry: *entry,
                size: self.arg(*size),
                line: *line,
                arg: self.arg(*arg),
            },
        }
    }

    pub fn parts(&self, parts: Parts) -> Parts {
        Parts {
            time: self.cost(&parts.time),
            grow: parts.grow.map(|grow| self.cost(&grow)),
            temp: self.cost(&parts.temp),
            io: parts.io.map(|io| self.cost(&io)),
        }
    }

    pub fn call(&self, call: CallSite) -> CallSite {
        CallSite {
            args: call.args.into_iter().map(|arg| self.arg(arg)).collect(),
            ..call
        }
    }
}

/// Every variable a cost refers to by index.
pub fn referenced(cost: &Cost, into: &mut BTreeSet<u16>) {
    match cost {
        Cost::One | Cost::Call(_) => {}
        Cost::Loop { bound, body, .. } => {
            if let Bound::Size(var) | Bound::Log(var) = bound {
                into.insert(*var);
            }
            referenced(body, into);
        }
        Cost::Seq(items) | Cost::Branch(items) => {
            items.iter().for_each(|item| referenced(item, into));
        }
        Cost::Lib { size, arg, .. } => {
            for size in [size, arg] {
                if let ArgSize::Local(var) = size {
                    into.insert(*var);
                }
            }
        }
    }
}

pub fn referenced_by_calls(calls: &[CallSite], into: &mut BTreeSet<u16>) {
    for size in calls.iter().flat_map(|call| &call.args) {
        if let ArgSize::Local(var) = size {
            into.insert(*var);
        }
    }
}

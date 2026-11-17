//! A function's cost on its own: its loops and the library calls it makes, with a call
//! to another of its functions counted as constant. The order it gives is what the
//! function costs before anything it calls is taken into account.

use rqx_core::cost::{Cost, FunctionCost, Order, SizeId};

use super::sizes::Sizes;
use super::tables::{CostTables, Entry};

/// The size variable an entry's `n` stands for while the caller's sizes are put in, kept
/// clear of the function's own so the two substitutions cannot meet.
const ENTRY_N: SizeId = SizeId::MAX;
const ENTRY_M: SizeId = SizeId::MAX - 1;

pub fn time(function: &FunctionCost, tables: &CostTables) -> Order {
    Fold::new(function, tables, Dimension::Time).fold(&function.time)
}

pub fn space(function: &FunctionCost, tables: &CostTables) -> Order {
    Fold::new(function, tables, Dimension::Space).fold(&function.space)
}

#[derive(Clone, Copy)]
pub enum Dimension {
    Time,
    Space,
}

/// What a library entry costs when its `n` is `size` and its `m` is `arg`.
pub fn entry_order(entry: &Entry, dimension: Dimension, size: &Order, arg: &Order) -> Order {
    let order = match dimension {
        Dimension::Time => &entry.cost,
        Dimension::Space => &entry.space,
    };
    order
        .subst(0, &Order::var(ENTRY_N))
        .subst(1, &Order::var(ENTRY_M))
        .subst(ENTRY_N, size)
        .subst(ENTRY_M, arg)
}

struct Fold<'f> {
    tables: &'f CostTables,
    dimension: Dimension,
    sizes: Sizes<'f>,
}

impl<'f> Fold<'f> {
    fn new(function: &'f FunctionCost, tables: &'f CostTables, dimension: Dimension) -> Self {
        Self {
            tables,
            dimension,
            sizes: Sizes::new(&function.vars),
        }
    }

    fn fold(&mut self, cost: &Cost) -> Order {
        match cost {
            Cost::One | Cost::Call(_) => Order::one(),
            Cost::Loop { bound, body, .. } => {
                let passes = self.sizes.passes(*bound);
                passes.mul(&self.fold(body))
            }
            Cost::Seq(items) | Cost::Branch(items) => items
                .iter()
                .fold(Order::one(), |all, item| all.max(&self.fold(item))),
            Cost::Lib {
                entry, size, arg, ..
            } => {
                let Some(entry) = self.tables.entry(*entry) else {
                    return Order::one();
                };
                let (size, arg) = (self.sizes.of(*size), self.sizes.of(*arg));
                entry_order(entry, self.dimension, &size, &arg)
            }
        }
    }
}

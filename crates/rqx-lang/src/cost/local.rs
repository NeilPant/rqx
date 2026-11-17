//! A function's cost on its own: its loops and the library calls it makes, with a call
//! to another of its functions counted as constant. The order it gives is what the
//! function costs before anything it calls is taken into account.

use rqx_core::cost::{ArgSize, Bound, Cost, FunctionCost, Order, SizeId, SizeSrc};

use super::tables::CostTables;

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
enum Dimension {
    Time,
    Space,
}

struct Fold<'f> {
    function: &'f FunctionCost,
    tables: &'f CostTables,
    dimension: Dimension,
    /// The next variable for a size nothing is known about. Each such size is its own
    /// quantity, so two unknown loops are not taken to run over the same thing.
    unknowns: SizeId,
}

impl<'f> Fold<'f> {
    fn new(function: &'f FunctionCost, tables: &'f CostTables, dimension: Dimension) -> Self {
        Self {
            function,
            tables,
            dimension,
            unknowns: SizeId::try_from(function.vars.len()).unwrap_or(SizeId::MAX - 8),
        }
    }

    fn fresh(&mut self) -> Order {
        let variable = self.unknowns;
        self.unknowns += 1;
        Order::var(variable)
    }

    fn fold(&mut self, cost: &Cost) -> Order {
        match cost {
            Cost::One | Cost::Call(_) => Order::one(),
            Cost::Loop { bound, body, .. } => {
                let passes = self.passes(*bound);
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
                let order = match self.dimension {
                    Dimension::Time => &entry.cost,
                    Dimension::Space => &entry.space,
                };
                let (size, arg) = (self.size(*size), self.size(*arg));
                order
                    .subst(0, &Order::var(ENTRY_N))
                    .subst(1, &Order::var(ENTRY_M))
                    .subst(ENTRY_N, &size)
                    .subst(ENTRY_M, &arg)
            }
        }
    }

    fn passes(&mut self, bound: Bound) -> Order {
        match bound {
            Bound::Const => Order::one(),
            Bound::Size(var) => Order::var(var),
            Bound::Log(var) => Order::log_var(var),
            Bound::Unknown => self.fresh(),
        }
    }

    fn size(&mut self, size: ArgSize) -> Order {
        match size {
            ArgSize::Const => Order::one(),
            ArgSize::Local(var) => Order::var(var),
            ArgSize::Param(position) => {
                let found = self
                    .function
                    .vars
                    .iter()
                    .position(|var| var.src == SizeSrc::Param(position));
                match found {
                    Some(var) => Order::var(var as SizeId),
                    None => self.fresh(),
                }
            }
            ArgSize::Unknown => self.fresh(),
        }
    }
}

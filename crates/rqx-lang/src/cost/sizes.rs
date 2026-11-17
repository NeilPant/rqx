//! Turning the sizes a function's cost is written in into orders of the function's own
//! variables. A size nothing is known about is a variable of its own, so that two loops
//! over unknown collections are not taken to run over the same one.

use rqx_core::cost::{ArgSize, Bound, Order, SizeId, SizeSrc, SizeVar};

pub struct Sizes<'v> {
    vars: &'v [SizeVar],
    /// The next variable for a size nothing is known about.
    unknowns: SizeId,
}

impl<'v> Sizes<'v> {
    pub fn new(vars: &'v [SizeVar]) -> Self {
        Self {
            vars,
            unknowns: SizeId::try_from(vars.len()).unwrap_or(SizeId::MAX - 8),
        }
    }

    pub fn fresh(&mut self) -> Order {
        let variable = self.unknowns;
        self.unknowns += 1;
        Order::var(variable)
    }

    pub fn passes(&mut self, bound: Bound) -> Order {
        match bound {
            Bound::Const => Order::one(),
            Bound::Size(var) => Order::var(var),
            Bound::Log(var) => Order::log_var(var),
            Bound::Unknown => self.fresh(),
        }
    }

    pub fn of(&mut self, size: ArgSize) -> Order {
        match size {
            ArgSize::Const => Order::one(),
            ArgSize::Local(var) => Order::var(var),
            ArgSize::Param(position) => self.param(position),
            ArgSize::Unknown => self.fresh(),
        }
    }

    /// The size of the parameter at `position`, or a size of its own where the function
    /// never measures that parameter.
    pub fn param(&mut self, position: u8) -> Order {
        match self.variable_of(position) {
            Some(var) => Order::var(var),
            None => self.fresh(),
        }
    }

    pub fn variable_of(&self, position: u8) -> Option<SizeId> {
        self.vars
            .iter()
            .position(|var| var.src == SizeSrc::Param(position))
            .and_then(|var| SizeId::try_from(var).ok())
    }
}

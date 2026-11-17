//! Building cost trees without the noise: a constant step in a sequence adds nothing,
//! so it is not written down, and a sequence of one is the one.

use rqx_core::cost::{Bound, Cost};

/// The cost of statements one after the other.
pub fn seq(items: impl IntoIterator<Item = Cost>) -> Cost {
    collapse(items, Cost::Seq)
}

/// The cost of alternative paths.
pub fn branch(items: impl IntoIterator<Item = Cost>) -> Cost {
    collapse(items, Cost::Branch)
}

fn collapse(items: impl IntoIterator<Item = Cost>, join: fn(Vec<Cost>) -> Cost) -> Cost {
    let mut kept: Vec<Cost> = items.into_iter().filter(|item| !item.is_one()).collect();
    match kept.len() {
        0 => Cost::One,
        1 => kept.remove(0),
        _ => join(kept),
    }
}

/// A body run once per pass of a loop.
pub fn looped(bound: Bound, body: Cost, line: u32) -> Cost {
    Cost::Loop {
        bound,
        body: Box::new(body),
        line,
    }
}

/// What a stretch of a function does, in each of the ways it can cost.
#[derive(Default)]
pub struct Parts {
    pub time: Cost,
    /// What the stretch adds to the space it holds on to: one element per append, and
    /// so one per pass of every loop around it. `None` where it adds nothing, which is
    /// not the same as adding one thing.
    pub grow: Option<Cost>,
    /// Space allocated and let go within the stretch, which a loop does not pile up.
    pub temp: Cost,
    /// Calls that leave the process, counted the way growth is.
    pub io: Option<Cost>,
}

impl Parts {
    pub fn timed(time: Cost) -> Self {
        Self {
            time,
            ..Self::default()
        }
    }

    /// Stretches one after the other.
    pub fn sequence(parts: Vec<Self>) -> Self {
        let each = Each::of(parts);
        Self {
            time: seq(each.time),
            grow: join_present(each.grow, Cost::Seq),
            temp: seq(each.temp),
            io: join_present(each.io, Cost::Seq),
        }
    }

    /// Alternative stretches.
    pub fn alternatives(parts: Vec<Self>) -> Self {
        let each = Each::of(parts);
        Self {
            time: branch(each.time),
            grow: join_present(each.grow, Cost::Branch),
            temp: branch(each.temp),
            io: join_present(each.io, Cost::Branch),
        }
    }

    /// This stretch run once per pass of a loop. The loop itself costs its bound even
    /// when the pass does nothing; growth and calls out are only worth saying when a
    /// pass has some.
    pub fn repeated(self, bound: Bound, line: u32) -> Self {
        Self {
            time: looped(bound, self.time, line),
            grow: self.grow.map(|grow| looped(bound, grow, line)),
            temp: self.temp,
            io: self.io.map(|io| looped(bound, io, line)),
        }
    }
}

/// The stretches taken apart by what they cost.
struct Each {
    time: Vec<Cost>,
    grow: Vec<Cost>,
    temp: Vec<Cost>,
    io: Vec<Cost>,
}

impl Each {
    fn of(parts: Vec<Parts>) -> Self {
        let mut each = Self {
            time: Vec::new(),
            grow: Vec::new(),
            temp: Vec::new(),
            io: Vec::new(),
        };
        for part in parts {
            each.time.push(part.time);
            each.grow.extend(part.grow);
            each.temp.push(part.temp);
            each.io.extend(part.io);
        }
        each
    }
}

/// The stretches that have something, keeping the fact that one had even when it is
/// constant.
fn join_present(mut items: Vec<Cost>, join: fn(Vec<Cost>) -> Cost) -> Option<Cost> {
    match items.len() {
        0 => None,
        1 => Some(items.remove(0)),
        _ => Some(join(items)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loop_over(var: u16, body: Cost, line: u32) -> Cost {
        looped(Bound::Size(var), body, line)
    }

    #[test]
    fn constant_steps_are_not_written_down() {
        assert_eq!(seq([Cost::One, Cost::One]), Cost::One);
        assert_eq!(seq([Cost::One, Cost::Call(0)]), Cost::Call(0));
        assert_eq!(
            seq([Cost::Call(0), Cost::One, Cost::Call(1)]),
            Cost::Seq(vec![Cost::Call(0), Cost::Call(1)])
        );
        assert_eq!(branch([Cost::One]), Cost::One);
    }

    #[test]
    fn a_loop_that_does_nothing_still_costs_its_bound() {
        let pass = Parts::timed(Cost::One).repeated(Bound::Size(0), 3);
        assert_eq!(pass.time, loop_over(0, Cost::One, 3));
        assert!(pass.grow.is_none());
        assert!(pass.io.is_none());
    }

    #[test]
    fn growth_in_a_loop_is_one_element_per_pass() {
        let append = Parts {
            grow: Some(Cost::One),
            ..Parts::default()
        };
        let looped = append.repeated(Bound::Size(0), 5);
        assert_eq!(looped.grow, Some(loop_over(0, Cost::One, 5)));
    }

    #[test]
    fn a_temporary_is_not_piled_up_by_the_loop_around_it() {
        let temporary = Parts {
            temp: Cost::Call(7),
            ..Parts::default()
        };
        assert_eq!(temporary.repeated(Bound::Size(0), 1).temp, Cost::Call(7));
    }
}

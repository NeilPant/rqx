//! What a function that calls itself costs, from what one call costs without the
//! recursion and how the argument shrinks from one call to the next.

use rqx_core::cost::{Order, Recurrence, Shrink, SizeId};

/// One level of the recursion, with the recursive calls left out.
pub struct Level {
    pub time: Order,
    /// The space one level holds while its callees run.
    pub space: Order,
    /// The deepest stack the level's other callees need.
    pub below: Order,
    /// The round trips one level makes, if it makes any.
    pub io: Option<Order>,
}

pub struct Solved {
    pub time: Order,
    pub space: Order,
    pub stack: Order,
    pub io: Option<Order>,
    /// The answer rests on a guess about how the argument shrinks.
    pub guessed: bool,
}

/// `depth` is the variable the recursion is measured in.
pub fn solve(rec: &Recurrence, depth: SizeId, level: &Level) -> Solved {
    let size = Order::var(depth);
    match rec.shrink {
        Shrink::Dec1 => linear(rec, &size, level, false),
        Shrink::Half => halving(rec, depth, level),
        Shrink::Other => linear(rec, &size, level, true),
    }
}

/// An argument that is one smaller each call, or one that is not known to shrink in any
/// way: a walk of `size` levels, each costing what a level costs.
fn linear(rec: &Recurrence, size: &Order, level: &Level, guessed: bool) -> Solved {
    let per_call = if rec.copies_arg {
        level.time.max(size)
    } else {
        level.time.clone()
    };
    let branches = rec.calls >= 2 && !rec.memo && !guessed;
    let time = if branches {
        Order::exponential()
    } else {
        size.mul(&per_call)
    };
    let frame = if rec.copies_arg {
        level.space.max(size)
    } else {
        level.space.clone()
    };
    // Every level is still on the stack when the last one runs, so the frames add up.
    let stack = size.mul(&level.below);
    Solved {
        time,
        space: size.mul(&frame),
        stack,
        io: level.io.as_ref().map(|io| {
            if branches {
                Order::exponential()
            } else {
                size.mul(io)
            }
        }),
        guessed,
    }
}

/// An argument that is half as large each call: the master theorem, with `calls`
/// recursive calls of half the size and a level that costs `time`.
fn halving(rec: &Recurrence, depth: SizeId, level: &Level) -> Solved {
    let size = Order::var(depth);
    let per_call = if rec.copies_arg {
        level.time.max(&size)
    } else {
        level.time.clone()
    };
    let calls = u32::from(rec.calls.max(1));
    let stack = Order::log_var(depth).mul(&level.below);
    let copied = if rec.copies_arg {
        size.clone()
    } else {
        Order::one()
    };
    // The levels get smaller as fast as they get deeper, so the space of all of them is
    // that of the first, and the stack is what is left to add.
    let space = level.space.max(&copied).max(&stack);
    Solved {
        time: master(calls, &per_call, depth),
        space,
        stack,
        io: level.io.as_ref().map(|io| master(calls, io, depth)),
        // Three calls on halves is n^1.58, which an order cannot say; it is rounded up.
        guessed: !calls.is_power_of_two(),
    }
}

/// T(n) = a·T(n/2) + f(n), solved in `depth`. The three cases compare the power of `n` in
/// `f` with the exponent log2(a) that the leaves of the recursion tree grow with.
fn master(calls: u32, work: &Order, depth: SizeId) -> Order {
    let critical = u8::try_from(u32::BITS - (calls - 1).leading_zeros()).unwrap_or(u8::MAX);
    let power = power_in(work, depth);
    match power.cmp(&critical) {
        std::cmp::Ordering::Greater => work.clone(),
        std::cmp::Ordering::Equal => work.mul(&Order::log_var(depth)),
        std::cmp::Ordering::Less => work.max(&Order::power(depth, critical)),
    }
}

/// The largest power `depth` is raised to in any term of the order.
fn power_in(order: &Order, depth: SizeId) -> u8 {
    order
        .terms()
        .iter()
        .flat_map(|term| term.factors())
        .filter(|(variable, _, _)| *variable == depth)
        .map(|(_, power, _)| *power)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: SizeId = 0;

    fn order(text: &str) -> Order {
        text.parse().unwrap()
    }

    fn level(time: &str) -> Level {
        Level {
            time: order(time),
            space: Order::one(),
            below: Order::one(),
            io: None,
        }
    }

    fn rec(calls: u8, shrink: Shrink) -> Recurrence {
        Recurrence {
            calls,
            shrink,
            ..Recurrence::default()
        }
    }

    #[test]
    fn one_call_on_one_less_walks_the_input() {
        let solved = solve(&rec(1, Shrink::Dec1), N, &level("O(1)"));
        assert_eq!(solved.time, order("O(n)"));
        assert_eq!(solved.stack, order("O(n)"));
        assert_eq!(solved.space, order("O(n)"));
        assert!(!solved.guessed);
        let solved = solve(&rec(1, Shrink::Dec1), N, &level("O(n)"));
        assert_eq!(solved.time, order("O(n²)"));
    }

    #[test]
    fn a_copy_of_what_is_left_makes_every_level_cost_the_input() {
        let copying = Recurrence {
            copies_arg: true,
            ..rec(1, Shrink::Dec1)
        };
        let solved = solve(&copying, N, &level("O(1)"));
        assert_eq!(solved.time, order("O(n²)"));
        assert_eq!(solved.space, order("O(n²)"));
        assert_eq!(solved.stack, order("O(n)"));
    }

    #[test]
    fn two_calls_on_one_less_are_exponential_unless_the_answers_are_kept() {
        assert!(
            solve(&rec(2, Shrink::Dec1), N, &level("O(1)"))
                .time
                .is_exponential()
        );
        let kept = Recurrence {
            memo: true,
            ..rec(2, Shrink::Dec1)
        };
        assert_eq!(solve(&kept, N, &level("O(1)")).time, order("O(n)"));
    }

    #[test]
    fn halving_follows_the_master_theorem() {
        let cases = [
            (1, "O(1)", "O(log n)"),
            (1, "O(n)", "O(n)"),
            (2, "O(1)", "O(n)"),
            (2, "O(n)", "O(n log n)"),
            (2, "O(n²)", "O(n²)"),
            (4, "O(n)", "O(n²)"),
        ];
        for (calls, work, expected) in cases {
            let solved = solve(&rec(calls, Shrink::Half), N, &level(work));
            assert_eq!(solved.time, order(expected), "{calls} calls, {work}");
        }
    }

    #[test]
    fn a_halving_stack_is_logarithmic_and_its_frames_do_not_add_up() {
        let solved = solve(&rec(1, Shrink::Half), N, &level("O(1)"));
        assert_eq!(solved.stack, order("O(log n)"));
        assert_eq!(solved.space, order("O(log n)"));
        let solved = solve(
            &rec(2, Shrink::Half),
            N,
            &Level {
                space: order("O(n)"),
                ..level("O(n)")
            },
        );
        assert_eq!(solved.space, order("O(n)"));
    }

    #[test]
    fn three_calls_on_halves_are_rounded_up_and_said_to_be_a_guess() {
        let solved = solve(&rec(3, Shrink::Half), N, &level("O(1)"));
        assert_eq!(solved.time, order("O(n²)"));
        assert!(solved.guessed);
    }

    #[test]
    fn an_argument_that_shrinks_in_no_known_way_is_a_guess_at_a_walk() {
        let solved = solve(&rec(2, Shrink::Other), N, &level("O(1)"));
        assert_eq!(solved.time, order("O(n)"));
        assert!(solved.guessed);
    }

    #[test]
    fn the_work_outside_the_depth_variable_stays_a_factor() {
        let solved = solve(&rec(1, Shrink::Dec1), N, &level("O(m)"));
        assert_eq!(solved.time, order("O(n·m)"));
    }

    #[test]
    fn round_trips_repeat_with_the_recursion() {
        let level = Level {
            io: Some(Order::one()),
            ..level("O(1)")
        };
        assert_eq!(
            solve(&rec(1, Shrink::Dec1), N, &level).io,
            Some(order("O(n)"))
        );
        assert_eq!(
            solve(&rec(1, Shrink::Half), N, &level).io,
            Some(order("O(log n)"))
        );
    }
}

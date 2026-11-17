//! Rules about locks and about starting work: a lock taken and let go on every pass of
//! a loop when the same lock could be held across it, and work started once per element
//! of a collection of any size with nothing to limit how much runs at once.

use rqx_core::cost::Bound;
use rqx_core::facts::CostSite;
use rqx_core::{Language, RuleId};
use tree_sitter::Node;

use super::{Visitor, tail_of};
use crate::cost::written::{read_in, written_in};
use crate::tree::{descendants, kind_of, span_of};
use crate::vocabulary::mentions_any;

/// Names that say work is being held to a number at a time.
const LIMITER_HINTS: [&str; 11] = [
    "limit",
    "semaphore",
    "throttle",
    "bottleneck",
    "workers",
    "concurrency",
    "batch",
    "chunk",
    "errgroup",
    "pool",
    "executor",
];

/// The name Go programs give to a channel used as a semaphore.
const SEMAPHORE_NAMES: [&str; 2] = ["sem", "tokens"];

/// Calls that hand work to an executor, which is itself the limit on how much runs.
const HANDS_TO_AN_EXECUTOR: [&str; 2] = ["submit", "execute"];

impl<'a> Visitor<'a, '_> {
    /// Whether the lock a loop takes is the same lock on every pass: nothing in the
    /// loop assigns, introduces or changes what it names.
    fn lock_is_invariant(&self, operand: Node<'a>) -> bool {
        let Some(outermost) = self.loop_nodes.first().copied() else {
            return false;
        };
        let written = written_in(&self.names, outermost);
        read_in(&self.names, operand)
            .iter()
            .all(|name| !written.contains(name))
    }

    fn note_lock_site(&mut self, at: Node<'a>, operand: Node<'a>, subject: &str) {
        if self.loops_can_grow() && self.lock_is_invariant(operand) {
            self.note_site(RuleId::LockInLoop, at, subject);
        }
    }

    /// A call, in a loop, that takes a lock on something the loop does not change, or
    /// that starts work.
    pub(super) fn note_call_sites(&mut self, call: Node<'a>, dotted: &str, arguments: &[Node<'a>]) {
        let signals = &self.names.profile.signals;
        let tail = tail_of(dotted);
        if self.depth > 0
            && signals.lock_calls.contains(&tail)
            && let Some(receiver) = self.bounds().receiver_expression(call)
        {
            self.note_lock_site(call, receiver, dotted);
        }
        let fans_out = signals
            .fanout_calls
            .iter()
            .any(|entry| self.names.answers_to(dotted, entry));
        let spawns = self.depth > 0
            && !HANDS_TO_AN_EXECUTOR.contains(&tail)
            && signals
                .spawn_calls
                .iter()
                .any(|entry| self.names.answers_to(dotted, entry));
        if fans_out {
            self.note_fan_out(call, dotted, arguments);
        } else if spawns {
            self.note_spawn(call, dotted);
        }
    }

    /// A statement that holds a lock for its body or starts work of its own.
    pub(super) fn note_statement_sites(&mut self, node: Node<'a>, kind: &str) {
        let profile = self.names.profile;
        let signals = &profile.signals;
        if signals.lock_kinds.contains(&kind)
            && let Some(operand) = node.named_child(0)
        {
            let subject = self.names.text(operand).to_owned();
            self.note_lock_site(node, operand, &subject);
        }
        if self.is_lock_declaration(node)
            && let Some(guarded) = node
                .child_by_field_name("declarator")
                .and_then(|declarator| {
                    declarator
                        .child_by_field_name("value")
                        .or_else(|| declarator.child_by_field_name("parameters"))
                })
        {
            let subject = self.names.text(node).chars().take(40).collect::<String>();
            self.note_lock_site(node, guarded, &subject);
        }
        if profile.language == Language::Python
            && kind == "with_statement"
            && let Some(context) = self.lock_context(node)
        {
            let subject = self.names.text(context).to_owned();
            self.note_lock_site(node, context, &subject);
        }
        if signals.spawn_kinds.contains(&kind) {
            self.note_spawn(node, "go");
        }
    }

    /// C++ takes a lock by declaring a guard: `std::lock_guard<std::mutex> guard(mutex_)`.
    fn is_lock_declaration(&self, node: Node<'a>) -> bool {
        node.child_by_field_name("type").is_some_and(|declared| {
            self.names
                .profile
                .signals
                .lock_types
                .iter()
                .any(|lock| self.names.text(declared).contains(lock))
        })
    }

    /// The expression a Python `with` holds, when it is named like a lock.
    fn lock_context(&self, with: Node<'a>) -> Option<Node<'a>> {
        descendants(with)
            .into_iter()
            .filter(|node| kind_of(node) == "with_item")
            .filter_map(|item| item.named_child(0))
            .find(|context| mentions_any(self.names.text(*context), &["lock", "mutex"]))
    }

    /// Whether anything inside `node` leaves the process.
    fn performs_io(&self, node: Node<'a>) -> bool {
        descendants(node)
            .into_iter()
            .filter(|inner| self.names.profile.is_call(*inner))
            .filter_map(|call| self.names.called_name(call))
            .any(|dotted| {
                let (receiver, tail) = match dotted.rsplit_once('.') {
                    Some((receiver, tail)) => (Some(receiver), tail),
                    None => (None, dotted.as_str()),
                };
                self.is_round_trip(receiver, tail)
            })
    }

    /// The size of what a fan-out argument spreads over: the receiver of its `map`, or
    /// what its comprehension runs over.
    fn fan_out_bound(&mut self, argument: Node<'a>) -> Option<Bound> {
        let profile = self.names.profile;
        for node in descendants(argument) {
            if profile.comprehension_kinds.contains(&kind_of(node)) {
                let clause = profile.loops.clauses.iter().find_map(|(clause, _)| {
                    descendants(node)
                        .into_iter()
                        .find(|inner| kind_of(inner) == *clause)
                });
                if let Some(clause) = clause {
                    return Some(self.clause_bound(clause));
                }
            } else if profile.is_call(node)
                && self
                    .names
                    .called_name(node)
                    .is_some_and(|called| profile.hof_loops.contains(&tail_of(&called)))
                && let Some(source) = self.bounds().receiver_expression(node)
            {
                return Some(self.bounds().iterable_bound(source));
            }
        }
        None
    }

    fn note_fan_out(&mut self, call: Node<'a>, dotted: &str, arguments: &[Node<'a>]) {
        let mut bound = None;
        for argument in arguments {
            bound = bound.or_else(|| self.fan_out_bound(*argument));
        }
        if bound.is_some_and(|bound| bound != Bound::Const) && self.performs_io(call) {
            self.unbounded.push(self.site_of(call, dotted));
        }
    }

    fn note_spawn(&mut self, at: Node<'a>, subject: &str) {
        if self.loops_can_grow() && self.performs_io(at) {
            self.unbounded.push(self.site_of(at, subject));
        }
    }

    fn site_of(&self, node: Node<'a>, subject: &str) -> CostSite {
        CostSite {
            rule: RuleId::UnboundedConcurrency,
            span: span_of(node),
            subject: subject.to_owned(),
            depth: u32::from(self.depth),
        }
    }

    /// Reports the work started without a limit, unless the function says anywhere that
    /// it holds the number running to something.
    pub(super) fn settle_concurrency(&mut self, function: Node<'a>) {
        if self.unbounded.is_empty() {
            return;
        }
        let limited = descendants(function).into_iter().any(|node| {
            let profile = self.names.profile;
            profile.is_identifier(node) && {
                let name = self.names.text(node);
                mentions_any(name, &LIMITER_HINTS) || SEMAPHORE_NAMES.contains(&name)
            }
        });
        if !limited {
            self.sites.append(&mut self.unbounded);
        }
    }
}

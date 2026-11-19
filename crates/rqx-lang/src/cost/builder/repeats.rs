//! Calls in a loop that give the same answer on every pass. Nothing is decided here
//! beyond the syntax: a call is written down when the loop cannot change what it reads
//! and its value is used. Whether the callee is costly is for the composition to say.

use rqx_core::cost::{Cost, Invoked, RepeatedCall};
use tree_sitter::Node;

use super::Visitor;
use crate::cost::written::{read_in, written_in};
use crate::tree::kind_of;

/// Statements that run a call for what it does, not for what it returns. A `with`
/// enters what the call returns, which is a fresh context each time.
const EFFECT_STATEMENTS: [&str; 5] = [
    "expression_statement",
    "go_statement",
    "defer_statement",
    "with_item",
    "with_clause",
];

/// Wrappers a value passes through on its way to being used or discarded.
const TRANSPARENT: [&str; 2] = ["await", "parenthesized"];

impl<'a> Visitor<'a, '_> {
    /// A call to the project's own code, recorded as the call at `index`.
    pub(super) fn note_repeated_call(
        &mut self,
        call: Node<'a>,
        index: u16,
        dotted: &str,
        arguments: &[Node<'a>],
    ) {
        // A method answers from its object, and the call may be what changes the
        // object: `scanner.Next()`, `pubsub.Receive(ctx)`. Nothing here says the
        // project's method leaves its receiver alone, so a call on an object the
        // function holds is not judged; one on a type, a package or `this` is.
        let asks_its_object = self
            .bounds()
            .receiver_expression(call)
            .is_some_and(|receiver| self.holds_an_object(receiver));
        if !asks_its_object && self.loop_leaves_unchanged(call, arguments) {
            self.repeats.push(RepeatedCall {
                line: super::line_of(call),
                subject: dotted.to_owned(),
                of: Invoked::Call(index),
            });
        }
    }

    /// A library call a table marks as pure and as costing something that grows.
    pub(super) fn note_repeated_library(
        &mut self,
        call: Node<'a>,
        charged: &Cost,
        dotted: &str,
        arguments: &[Node<'a>],
    ) {
        let Cost::Lib {
            entry, size, arg, ..
        } = charged
        else {
            return;
        };
        let pure = self
            .shared
            .tables
            .entry(*entry)
            .is_some_and(|found| found.pure);
        if pure && !size.is_const() && self.loop_leaves_unchanged(call, arguments) {
            self.repeats.push(RepeatedCall {
                line: super::line_of(call),
                subject: dotted.to_owned(),
                of: Invoked::Lib {
                    entry: *entry,
                    size: *size,
                    arg: *arg,
                },
            });
        }
    }

    /// Whether the innermost loop around the call leaves everything the call reads
    /// alone, which makes the call a repeat of itself on every pass.
    fn loop_leaves_unchanged(&mut self, call: Node<'a>, arguments: &[Node<'a>]) -> bool {
        let Some(innermost) = self.loop_nodes.last().copied() else {
            return false;
        };
        // A callback names its own parameters, which are not written anywhere in it.
        if self.names.profile.is_call(innermost) || !self.value_is_used(call) {
            return false;
        }
        if within(call, iterable_of(innermost)) || self.leaves_the_loop(call, innermost) {
            return false;
        }
        let mut read: Vec<&'a str> = arguments
            .iter()
            .flat_map(|argument| read_in(&self.names, *argument))
            .collect();
        if let Some(receiver) = self.bounds().receiver_expression(call) {
            // `b.WriteString(x)` changes `b` with no assignment to it, so `b.String()`
            // answers differently on the next pass.
            if self.another_call_on(innermost, call, receiver) {
                return false;
            }
            read.extend(read_in(&self.names, receiver));
        }
        if read.is_empty() {
            return false;
        }
        let names = self.names;
        let written = self
            .written_in_loop
            .entry(innermost.id())
            .or_insert_with(|| written_in(&names, innermost));
        read.iter().all(|name| !written.contains(name))
    }

    /// Whether the call is in a statement that leaves the loop, so it runs at most once
    /// however many passes come before it.
    fn leaves_the_loop(&self, call: Node<'a>, innermost: Node<'a>) -> bool {
        let exits = self.names.profile.exit_kinds;
        let mut node = call;
        while let Some(parent) = node.parent() {
            if parent.id() == innermost.id() {
                return false;
            }
            let kind = kind_of(parent);
            if exits
                .iter()
                .any(|(exit, _)| *exit == kind && *exit != "continue_statement")
            {
                return true;
            }
            node = parent;
        }
        false
    }

    /// Whether a receiver is an object the function holds: anything but a plain name
    /// of a type, a package or the object itself, which the function never declares.
    fn holds_an_object(&self, receiver: Node<'a>) -> bool {
        let profile = self.names.profile;
        if !profile.is_identifier(receiver) {
            return !profile.self_names.contains(&self.names.text(receiver));
        }
        let name = self.names.text(receiver);
        if profile.self_names.contains(&name) {
            return false;
        }
        self.params.iter().any(|param| param.name == name)
            || written_in(&self.names, self.function).contains(name)
    }

    /// Whether the loop calls anything else on the same receiver for its effect, which
    /// may change what this call answers.
    fn another_call_on(&mut self, innermost: Node<'a>, call: Node<'a>, receiver: Node<'a>) -> bool {
        let source = self.names.source;
        let written = crate::tree::text_of(receiver, source);
        for other in crate::tree::descendants(innermost) {
            if other.id() == call.id()
                || !self.names.profile.is_call(other)
                || self.value_is_used(other)
            {
                continue;
            }
            if self
                .bounds()
                .receiver_expression(other)
                .is_some_and(|theirs| crate::tree::text_of(theirs, source) == written)
            {
                return true;
            }
        }
        false
    }

    fn value_is_used(&self, call: Node<'a>) -> bool {
        let mut node = call;
        while let Some(parent) = node.parent() {
            let kind = kind_of(parent);
            if TRANSPARENT.iter().any(|wrapper| kind.contains(wrapper)) {
                node = parent;
                continue;
            }
            return !EFFECT_STATEMENTS.contains(&kind);
        }
        true
    }
}

/// What a for-each loop walks: it is evaluated once, before the first pass, so a call
/// written there is not repeated by that loop.
fn iterable_of(loop_node: Node<'_>) -> Option<Node<'_>> {
    // Go keeps it in a range clause, and a comprehension in its first `for` clause.
    ["right", "value"]
        .iter()
        .find_map(|field| loop_node.child_by_field_name(field))
        .or_else(|| {
            let mut cursor = loop_node.walk();
            loop_node
                .named_children(&mut cursor)
                .find(|child| matches!(kind_of(*child), "range_clause" | "for_in_clause"))
                .and_then(|clause| clause.child_by_field_name("right"))
        })
}

fn within(node: Node<'_>, outer: Option<Node<'_>>) -> bool {
    outer.is_some_and(|outer| {
        outer.start_byte() <= node.start_byte() && node.end_byte() <= outer.end_byte()
    })
}

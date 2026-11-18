//! Calls in a loop that give the same answer on every pass. Nothing is decided here
//! beyond the syntax: a call is written down when the loop cannot change what it reads
//! and its value is used. Whether the callee is costly is for the composition to say.

use rqx_core::cost::{Cost, Invoked, RepeatedCall};
use tree_sitter::Node;

use super::Visitor;
use crate::cost::written::{read_in, written_in};
use crate::tree::kind_of;

/// Statements that run a call for what it does, not for what it returns.
const EFFECT_STATEMENTS: [&str; 3] = ["expression_statement", "go_statement", "defer_statement"];

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
        if self.loop_leaves_unchanged(call, arguments) {
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
        let mut read: Vec<&'a str> = arguments
            .iter()
            .flat_map(|argument| read_in(&self.names, *argument))
            .collect();
        if let Some(receiver) = self.bounds().receiver_expression(call) {
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

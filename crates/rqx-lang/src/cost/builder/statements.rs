//! Rules about a single statement in a loop that the language makes costly by its
//! meaning: Go's `defer`, which waits for the function to end, and Java's boxed
//! numbers, which allocate when they are changed.

use rqx_core::{Language, RuleId};
use tree_sitter::Node;

use super::Visitor;
use crate::cost::types::declarations;
use crate::tree::operator_of;

/// Java's wrappers around a number, which are objects.
const BOXED_TYPES: [&str; 7] = [
    "Integer",
    "Long",
    "Double",
    "Float",
    "Short",
    "Byte",
    "Character",
];

/// Statements that change what a name holds by an amount rather than replacing it.
const STEPS: [&str; 6] = ["+=", "-=", "*=", "/=", "%=", "++"];

impl<'a> Visitor<'a, '_> {
    /// A `defer` written inside a loop of the same function runs when the function ends,
    /// so every pass adds one to a stack that nothing unwinds until then.
    pub(super) fn note_deferred_in_loop(&mut self, node: Node<'a>, kind: &str) {
        if kind == "defer_statement" {
            let written: String = self.names.text(node).chars().take(40).collect();
            self.note_site(RuleId::DeferInLoop, node, &written);
        }
    }

    /// A local declared as a boxed number, by the loop depth it was made at.
    pub(super) fn note_boxed_declaration(&mut self, node: Node<'a>) {
        if self.names.profile.language != Language::Java {
            return;
        }
        for declaration in declarations(&self.names, node) {
            if declaration
                .spelled_type
                .is_some_and(|spelled| BOXED_TYPES.contains(&spelled.trim()))
            {
                self.boxed.insert(declaration.name, self.depth);
            }
        }
    }

    /// A boxed local stepped inside a loop that is deeper than the declaration: each
    /// step unboxes the value and boxes the result as a new object.
    pub(super) fn note_boxed_step(&mut self, node: Node<'a>, kind: &str) {
        if self.boxed.is_empty() || self.depth == 0 {
            return;
        }
        let target = if kind == "update_expression" {
            node.named_child(0)
        } else {
            let steps = operator_of(node, self.names.source).is_some_and(|op| STEPS.contains(&op));
            node.child_by_field_name("left").filter(|_| steps)
        };
        let Some(target) = target.filter(|target| self.names.profile.is_identifier(*target)) else {
            return;
        };
        let name = self.names.text(target);
        if self.boxed.get(name).is_some_and(|at| self.depth > *at) {
            self.boxed.remove(name);
            self.note_site(RuleId::BoxingInLoop, node, name);
        }
    }
}

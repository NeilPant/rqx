//! The places in a function body that a performance rule reports. The visit already
//! knows the loops, the receivers' types and the table entries each call matched, so
//! the rules are read off it as it goes rather than from a walk of their own.

use rqx_core::RuleId;
use rqx_core::cost::{ArgSize, Bound};
use rqx_core::facts::CostSite;
use tree_sitter::Node;

use super::Visitor;
use crate::cost::tables::Entry;
use crate::cost::types::TypeFact;
use crate::cost::written::{read_in, written_in};
use crate::tree::span_of;

/// Calls that give the start of a collection, which a free function takes in place of
/// the collection itself.
const RANGE_STARTS: [&str; 4] = ["begin", "cbegin", "rbegin", "data"];

impl<'a> Visitor<'a, '_> {
    pub(super) fn note_site(&mut self, rule: RuleId, node: Node<'a>, subject: &str) {
        self.sites.push(CostSite {
            rule,
            span: span_of(node),
            subject: subject.to_owned(),
            depth: u32::from(self.depth),
        });
    }

    /// Whether the loops around the visit can run more than a fixed number of times.
    pub(super) fn loops_can_grow(&self) -> bool {
        self.depth > 0
            && (self.loop_bounds.is_empty()
                || self.loop_bounds.iter().any(|bound| *bound != Bound::Const))
    }

    /// What the collection a call works on is, where the call is given the collection
    /// directly or through the start of its range.
    fn operand_fact(&mut self, operand: Node<'a>) -> Option<TypeFact> {
        let source = self.range_source(operand).unwrap_or(operand);
        self.fact_of(source)
    }

    fn range_source(&self, operand: Node<'a>) -> Option<Node<'a>> {
        let profile = self.names.profile;
        let node = self.names.see_through(operand);
        if !profile.is_call(node) {
            return None;
        }
        let callee = node.child_by_field_name(profile.callee_field)?;
        let property = self.names.member_property(callee)?;
        RANGE_STARTS
            .contains(&self.names.text(property))
            .then(|| callee.child_by_field_name(profile.object_field))
            .flatten()
    }

    fn holds_text(&self, fact: &TypeFact) -> bool {
        self.names
            .profile
            .perf
            .string_types
            .contains(&fact.head.as_str())
    }

    /// A call the table says grows with the collection it works on, made on a collection
    /// whose type the source states, inside a loop that can grow. Whether `scan-in-loop`
    /// reports the same call is settled where the findings are made.
    pub(super) fn note_library_site(
        &mut self,
        call: Node<'a>,
        dotted: &str,
        entry: &Entry,
        operand: Option<Node<'a>>,
        size: ArgSize,
    ) {
        if !entry.at_least_linear() || !self.loops_can_grow() || size.is_const() {
            return;
        }
        let Some(operand) = operand else { return };
        let Some(fact) = self.operand_fact(operand) else {
            return;
        };
        if !fact.declared || self.holds_text(&fact) {
            return;
        }
        if entry.sorts() {
            self.note_sort(call, dotted, operand);
        } else if entry.makes_no_copy() && size != ArgSize::Unknown {
            self.note_site(RuleId::LinearOpInLoop, call, dotted);
        }
    }

    /// Sorting something the loop never changes sorts the same thing on every pass.
    fn note_sort(&mut self, call: Node<'a>, dotted: &str, operand: Node<'a>) {
        let Some(outermost) = self.loop_nodes.first().copied() else {
            return;
        };
        let read = read_in(&self.names, operand);
        if read.is_empty() {
            return;
        }
        let written = written_in(&self.names, outermost);
        if read.iter().any(|name| written.contains(name)) {
            return;
        }
        self.note_site(RuleId::SortInLoop, call, dotted);
    }
}

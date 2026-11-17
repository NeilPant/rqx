//! Two loops over different collections whose inner body compares something of each
//! element with `==`. Read off the syntax while the visit is in the inner loop.

use rqx_core::cost::{Bound, PairScan};
use tree_sitter::Node;

use super::Visitor;
use crate::cost::written::read_in;
use crate::tree::{descendants, kind_of, operands_of, operator_of};

/// A `for` over a collection: the names it gives each element, and the collection.
struct EachLoop<'a> {
    elements: Vec<&'a str>,
    collection: Node<'a>,
}

impl<'a> Visitor<'a, '_> {
    /// Called for every node inside a loop.
    pub(super) fn note_pair_scan(&mut self, node: Node<'a>, kind: &str) {
        let is_equality = self.names.profile.binary_kinds.contains(&kind)
            && matches!(operator_of(node, self.names.source), Some("==" | "==="));
        if !is_equality {
            return;
        }
        let [.., outer, inner] = self.loop_nodes.as_slice() else {
            return;
        };
        let [.., outer_bound, inner_bound] = self.loop_bounds.as_slice() else {
            return;
        };
        if *outer_bound == Bound::Const || *inner_bound == Bound::Const {
            return;
        }
        let (outer, inner) = (*outer, *inner);
        let (Some(outer), Some(inner)) = (self.each_loop(outer), self.each_loop(inner)) else {
            return;
        };
        let Some((left, right)) = operands_of(node) else {
            return;
        };
        let (outer_text, inner_text) = (
            self.names.text(outer.collection),
            self.names.text(inner.collection),
        );
        if outer_text == inner_text || self.reads_any(inner.collection, &outer.elements) {
            return;
        }
        let compares = |first: Node<'a>, second: Node<'a>| {
            self.reads_only(first, &outer.elements, &inner.elements)
                && self.reads_only(second, &inner.elements, &outer.elements)
        };
        if !(compares(left, right) || compares(right, left)) {
            return;
        }
        let already = self
            .pair_scans
            .iter()
            .any(|found| found.outer == outer_text && found.inner == inner_text);
        if !already {
            self.pair_scans.push(PairScan {
                line: super::line_of(node),
                outer: outer_text.to_owned(),
                inner: inner_text.to_owned(),
                comparison: self.names.text(node).to_owned(),
            });
        }
    }

    fn each_loop(&mut self, node: Node<'a>) -> Option<EachLoop<'a>> {
        let vocabulary = &self.names.profile.loops;
        let (header, field) = vocabulary.each.iter().find_map(|(kind, field)| {
            self.bounds()
                .header(node, kind)
                .map(|header| (header, *field))
        })?;
        let collection = header.child_by_field_name(field)?;
        let mut elements = Vec::new();
        for field in ["left", "name", "declarator"] {
            if let Some(holder) = header.child_by_field_name(field) {
                elements.extend(
                    std::iter::once(holder)
                        .chain(descendants(holder))
                        .filter(|node| self.names.profile.is_identifier(*node))
                        .map(|node| self.names.text(node)),
                );
            }
        }
        (!elements.is_empty()).then_some(EachLoop {
            elements,
            collection,
        })
    }

    fn reads_any(&self, expression: Node<'a>, names: &[&str]) -> bool {
        read_in(&self.names, expression)
            .iter()
            .any(|read| names.contains(read))
    }

    /// Whether the expression is a function of `own` elements alone.
    fn reads_only(&self, expression: Node<'a>, own: &[&str], other: &[&str]) -> bool {
        self.reads_any(expression, own)
            && !self.reads_any(expression, other)
            && !kind_of(expression).contains("literal")
    }
}

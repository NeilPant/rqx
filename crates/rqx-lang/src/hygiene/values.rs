//! Rules about literal numbers and the arithmetic done with them.

use rqx_core::facts::Spot;
use tree_sitter::Node;

use crate::tree::{kind_of, operands_of, operator_of, span_of};
use crate::vocabulary::CONVENTIONAL;

use super::{Walker, squashed};

impl<'a> Walker<'a> {
    pub(super) fn note_number(&mut self, node: Node<'a>) {
        let raw = self.text(node);
        let Ok(value) = raw.parse::<f64>() else {
            return;
        };
        if CONVENTIONAL
            .iter()
            .any(|known| (known - value).abs() < f64::EPSILON)
            || self.is_declared_value(node)
        {
            return;
        }
        self.numbers
            .entry(raw.to_owned())
            .or_default()
            .push(span_of(node));
    }

    /// A literal that is the whole value of a declaration, as in `const MAX_RETRIES = 3`,
    /// already has the name the rule asks for. A sign, parentheses or a list of values
    /// around it do not change that; an expression does.
    fn is_declared_value(&self, literal: Node<'a>) -> bool {
        let profile = self.profile;
        let mut value = literal;
        let mut owner = literal.parent();
        while let Some(parent) = owner {
            let kind = kind_of(parent);
            let wraps = profile.negation_kinds.contains(&kind)
                || profile.transparent_kinds.contains(&kind)
                || profile.expression_list_kinds.contains(&kind);
            if !wraps {
                break;
            }
            value = parent;
            owner = parent.parent();
        }
        owner.is_some_and(|owner| {
            let kind = kind_of(owner);
            let declares = profile.declarator_kinds.contains(&kind)
                || profile.local_declaring_kinds.contains(&kind)
                || profile.field_kinds.contains(&kind);
            declares
                && ["value", "right", "default_value"]
                    .iter()
                    .any(|field| owner.child_by_field_name(field) == Some(value))
        })
    }

    /// `n + 1` and `n - 1` written about the same subject several times is where
    /// off-by-one lives.
    pub(super) fn note_boundary(&mut self, node: Node<'a>) {
        let Some(operator) = operator_of(node, self.source) else {
            return;
        };
        if !matches!(operator, "+" | "-") {
            return;
        }
        let Some((left, right)) = operands_of(node) else {
            return;
        };
        if self.text(right) != "1" {
            return;
        }
        if let Some(subject) = self.callee_name(left) {
            self.boundaries
                .entry(subject)
                .or_default()
                .push(span_of(node));
        }
    }

    /// Dividing one whole number by another. The remainder goes and nothing in the
    /// line says it was meant to.
    pub(super) fn note_integer_division(&mut self, node: Node<'a>) {
        let profile = self.profile;
        if operator_of(node, self.source) != Some("/") {
            return;
        }
        let Some((left, right)) = operands_of(node) else {
            return;
        };
        let whole = |side: Node<'a>| {
            let side = self.see_through(side);
            profile.number_kinds.contains(&kind_of(side))
                && !self.text(side).contains(['.', 'e', 'E'])
        };
        if !whole(left) || !whole(right) {
            return;
        }
        self.out.integer_divisions.push(Spot {
            span: span_of(node),
            detail: squashed(self.text(node)),
            count: 1,
        });
    }
}

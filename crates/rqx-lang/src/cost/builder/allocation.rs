//! Rules about what a loop allocates: a pass that is mostly allocating, a large buffer
//! made on every pass, and a collection that grows one append at a time when the loop
//! says how large it will be.

use rqx_core::cost::Bound;
use rqx_core::facts::CostSite;
use rqx_core::{Language, RuleId};
use tree_sitter::Node;

use super::{LoopFrame, Visitor, tail_of};
use crate::cost::types::{constructed_fact, declarations, fact_of_declaration};
use crate::tree::{descendants, kind_of, span_of};

/// Allocations in one pass of a loop from which the pass is mostly allocating.
const MANY_ALLOCATIONS: u32 = 3;

/// A size, in elements or bytes, that is worth setting aside once rather than on every pass.
const LARGE_ALLOCATION: u64 = 1024;

/// Calls that allocate as much as the number they are given says.
const SIZED_ALLOCATORS: [&str; 6] = [
    "make",
    "alloc",
    "allocUnsafe",
    "bytearray",
    "Array",
    "zeros",
];

/// Calls that give a collection room after it was made.
const RESERVING_CALLS: [&str; 3] = ["reserve", "ensureCapacity", "resize"];

/// Types of each language whose construction can be told how much room to set aside.
fn can_reserve(language: Language, head: &str) -> bool {
    let heads: &[&str] = match language {
        Language::Go => &["slice"],
        Language::Java => &[
            "ArrayList",
            "HashMap",
            "HashSet",
            "LinkedHashMap",
            "LinkedHashSet",
            "ArrayDeque",
            "Vector",
            "PriorityQueue",
        ],
        Language::Cpp => &["vector", "string", "unordered_map", "unordered_set"],
        Language::JavaScript | Language::TypeScript | Language::Python => &[],
    };
    heads.contains(&head)
}

/// The value of a written integer, for the notations the languages use.
fn integer_value(text: &str) -> Option<u64> {
    let digits: String = text.chars().filter(|c| !matches!(c, '_' | '\'')).collect();
    let digits = digits.trim_end_matches(['u', 'U', 'l', 'L', 'n']);
    match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => digits.parse().ok(),
    }
}

impl<'a> Visitor<'a, '_> {
    /// A fixed, large allocation, which a pass could have been given once.
    pub(super) fn note_large_allocation(&mut self, node: Node<'a>, kind: &str) {
        let profile = self.names.profile;
        let creates = kind.contains("new_") || kind.contains("creation");
        let sized_call = profile.is_call(node)
            && self
                .names
                .called_name(node)
                .is_some_and(|called| SIZED_ALLOCATORS.contains(&tail_of(&called)));
        let sized_declaration = kind == "init_declarator"
            && node
                .child_by_field_name("value")
                .is_some_and(|value| profile.argument_list_kinds.contains(&kind_of(value)));
        if !(creates || sized_call || sized_declaration) {
            return;
        }
        let largest = descendants(node)
            .into_iter()
            .filter(|inner| profile.number_kinds.contains(&kind_of(inner)))
            .filter_map(|inner| integer_value(self.names.text(inner)))
            .max();
        if largest.is_some_and(|size| size >= LARGE_ALLOCATION)
            && let Some(frame) = self.frames.last_mut()
            && frame.large_allocation.is_none()
        {
            let written: String = self.names.text(node).chars().take(40).collect();
            frame.large_allocation = Some((span_of(node), written));
        }
    }

    /// Where an innermost loop allocates more than a pass should: a large allocation of
    /// a fixed size, or several small ones.
    pub(super) fn note_allocation_site(&mut self, frame: &LoopFrame) {
        let (span, subject) = if let Some((span, written)) = &frame.large_allocation {
            (*span, format!("an allocation of `{written}`"))
        } else if frame.allocations >= MANY_ALLOCATIONS
            && let Some(first) = frame.first_allocation
        {
            (first, format!("{} allocations", frame.allocations))
        } else {
            return;
        };
        self.sites.push(CostSite {
            rule: RuleId::AllocationInLoop,
            span,
            subject,
            // The loop has already been left, so the visit is one level out of it.
            depth: u32::from(self.depth) + 1,
        });
    }

    /// A collection made empty here, of a type that could have been given a capacity.
    pub(super) fn note_empty_collection(&mut self, node: Node<'a>) {
        let language = self.names.profile.language;
        for declaration in declarations(&self.names, node) {
            let made = declaration
                .value
                .and_then(|value| constructed_fact(&self.names, value))
                .or_else(|| fact_of_declaration(&self.names, &declaration));
            if made.is_some_and(|fact| can_reserve(language, &fact.head))
                && self.starts_empty(declaration.value)
            {
                self.empty_collections.insert(declaration.name, self.depth);
            }
        }
    }

    /// Whether a collection is made with nothing in it and no size given.
    fn starts_empty(&self, value: Option<Node<'a>>) -> bool {
        let Some(value) = value else { return true };
        let value = self.names.see_through(value);
        let arguments = self.names.arguments(value);
        if kind_of(value) == "composite_literal" {
            return value
                .child_by_field_name("body")
                .is_some_and(|body| body.named_child_count() == 0);
        }
        if kind_of(value) == "object_creation_expression" {
            return arguments.is_empty();
        }
        if self.names.profile.is_call(value) {
            // `make([]T, 0)`: a type, and at most a length of nothing.
            let sized = arguments
                .get(1)
                .is_some_and(|length| self.names.text(*length) != "0");
            return arguments.len() < 3 && !sized;
        }
        false
    }

    /// Room set aside after the collection was made is the capacity this rule asks for.
    pub(super) fn note_reservation(&mut self, receiver: Option<&str>, tail: &str) {
        if RESERVING_CALLS.contains(&tail)
            && let Some(name) = receiver
        {
            self.empty_collections.remove(name);
        }
    }

    /// An append, in a loop that runs once per element of something, to a collection
    /// made empty above the loop: how large it will get is known before the loop starts.
    pub(super) fn note_append(&mut self, name: &str, call: Node<'a>) {
        let Some(&declared_at) = self.empty_collections.get(name) else {
            return;
        };
        if self.depth <= declared_at || !matches!(self.loop_bounds.last(), Some(Bound::Size(_))) {
            return;
        }
        self.empty_collections.remove(name);
        self.note_site(RuleId::MissingCapacity, call, name);
    }
}

//! Rules about values copied where a reference would do, which C++ makes the default:
//! a range-for variable declared by value, and a parameter taken by value that the
//! function only reads.

use rqx_core::{Language, RuleId};
use tree_sitter::Node;

use super::Visitor;
use crate::cost::types::{canonical_head, declarations};
use crate::cost::written::{read_in, written_in};
use crate::tree::{descendants, find_within, kind_of};

/// Types whose copy allocates, or copies as many elements as the value holds.
const COSTLY_TO_COPY: [&str; 15] = [
    "string",
    "wstring",
    "vector",
    "deque",
    "list",
    "map",
    "unordered_map",
    "set",
    "unordered_set",
    "multimap",
    "multiset",
    "function",
    "shared_ptr",
    "any",
    "valarray",
];

/// Whether a written type is one of those, held by value.
fn copied_type(spelled: &str) -> Option<String> {
    if spelled.contains(['&', '*']) {
        return None;
    }
    canonical_head(Language::Cpp, spelled).filter(|head| COSTLY_TO_COPY.contains(&head.as_str()))
}

impl<'a> Visitor<'a, '_> {
    /// `for (std::string name : names)` copies each element into `name`, unless the loop
    /// changes `name` and so needs a copy of its own.
    pub(super) fn note_range_for_copy(&mut self, node: Node<'a>, kind: &str) {
        if kind != "for_range_loop" {
            return;
        }
        let (Some(declared), Some(variable), Some(body)) = (
            node.child_by_field_name("type"),
            node.child_by_field_name("declarator"),
            node.child_by_field_name("body"),
        ) else {
            return;
        };
        if !self.names.profile.is_identifier(variable)
            || copied_type(self.names.text(declared)).is_none()
        {
            return;
        }
        let name = self.names.text(variable);
        if written_in(&self.names, body).contains(name) {
            return;
        }
        let subject = format!("{} {name}", self.names.text(declared));
        self.note_site(RuleId::CopyInRangeFor, node, &subject);
    }

    /// A parameter of a costly type taken by value that the function neither changes nor
    /// hands on with `std::move`: the caller's copy was made for nothing.
    pub(super) fn note_large_parameters(&mut self, function: Node<'a>, body: Option<Node<'a>>) {
        let profile = self.names.profile;
        let Some(body) = body.filter(|_| profile.language == Language::Cpp) else {
            return;
        };
        let Some(list) = find_within(function, profile.parameter_list_kinds, profile.block_kinds)
            .into_iter()
            .next_back()
        else {
            return;
        };
        let written = written_in(&self.names, body);
        let moved = self.moved_names(function);
        let mut cursor = list.walk();
        for parameter in list.named_children(&mut cursor) {
            if !profile.parameter_kinds.contains(&kind_of(parameter)) {
                continue;
            }
            let Some(spelled) = parameter.child_by_field_name("type") else {
                continue;
            };
            let by_value = parameter
                .child_by_field_name("declarator")
                .is_some_and(|declarator| profile.is_identifier(declarator));
            if !by_value || copied_type(self.names.text(spelled)).is_none() {
                continue;
            }
            for declaration in declarations(&self.names, parameter) {
                if !written.contains(declaration.name) && !moved.contains(&declaration.name) {
                    let subject = format!("{} {}", self.names.text(spelled), declaration.name);
                    self.note_site(RuleId::LargeByValue, parameter, &subject);
                }
            }
        }
    }

    /// The names handed on with `std::move` anywhere in a function, which includes a
    /// constructor's initializer list.
    fn moved_names(&self, function: Node<'a>) -> Vec<&'a str> {
        descendants(function)
            .into_iter()
            .filter(|node| self.names.profile.is_call(*node))
            .filter(|call| {
                self.names
                    .called_name(*call)
                    .is_some_and(|called| called.rsplit('.').next() == Some("move"))
            })
            .flat_map(|call| {
                self.names
                    .arguments(call)
                    .into_iter()
                    .flat_map(|argument| read_in(&self.names, argument))
            })
            .collect()
    }
}

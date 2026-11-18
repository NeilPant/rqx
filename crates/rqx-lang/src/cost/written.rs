//! Which names a stretch of code changes or introduces, so a rule can tell what a loop
//! leaves alone. A name that is only read in a loop is the same value on every pass.

use std::collections::HashSet;

use tree_sitter::Node;

use crate::names::Names;
use crate::tree::{descendants, kind_of};

/// Methods that add to, remove from or replace the contents of what they are called
/// on. Sorting and reversing are left out: they reorder what is there and so do not
/// change what a second sort would find.
const MUTATORS: [&str; 30] = [
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "append",
    "extend",
    "insert",
    "remove",
    "clear",
    "add",
    "put",
    "putAll",
    "addAll",
    "removeAll",
    "retainAll",
    "delete",
    "discard",
    "popitem",
    "update",
    "setdefault",
    "erase",
    "emplace",
    "emplace_back",
    "push_back",
    "pop_back",
    "push_front",
    "pop_front",
    "offer",
    "poll",
];

/// Fields whose identifiers a node introduces rather than reads.
const INTRODUCING_FIELDS: [&str; 3] = ["declarator", "name", "pattern"];

/// Every name that `scope` assigns to, introduces, increments or calls a mutating
/// method on. Reading a name does not count.
pub fn written_in<'a>(names: &Names<'a>, scope: Node<'a>) -> HashSet<&'a str> {
    let profile = names.profile;
    let mut written = HashSet::new();
    for (node, field) in crate::tree::descendants_with_fields(scope) {
        match field {
            Some("left") => collect_identifiers(names, node, &mut written),
            Some(field) if INTRODUCING_FIELDS.contains(&field) => {
                introduced(names, node, &mut written);
            }
            _ => {}
        }
        let kind = kind_of(node);
        if kind.contains("update") || kind.contains("inc_dec") {
            collect_identifiers(names, node, &mut written);
        }
        if profile.is_call(node)
            && let Some(dotted) = names.called_name(node)
            && let Some((receiver, tail)) = dotted.rsplit_once('.')
            && MUTATORS.contains(&tail)
        {
            let head = receiver.split('.').next().unwrap_or(receiver);
            if let Some(identifier) = descendants(node)
                .into_iter()
                .find(|inner| profile.is_identifier(*inner) && names.text(*inner) == head)
            {
                written.insert(names.text(identifier));
            }
        }
    }
    written
}

fn collect_identifiers<'a>(names: &Names<'a>, node: Node<'a>, into: &mut HashSet<&'a str>) {
    for inner in descendants(node) {
        if names.profile.is_identifier(inner) {
            into.insert(names.text(inner));
        }
    }
}

/// The name a declarator introduces, without the value it is set to.
fn introduced<'a>(names: &Names<'a>, node: Node<'a>, into: &mut HashSet<&'a str>) {
    if names.profile.is_identifier(node) {
        into.insert(names.text(node));
        return;
    }
    if let Some(inner) = INTRODUCING_FIELDS
        .iter()
        .find_map(|field| node.child_by_field_name(field))
    {
        introduced(names, inner, into);
    } else if kind_of(node).ends_with("declarator") {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            introduced(names, child, into);
        }
    }
}

/// The names an expression reads, which is all of it for a name or a spread of one. A
/// grammar that reads `hold(guard)` as a declaration puts `guard` where a type goes, so
/// any leaf that is a kind of identifier counts.
pub fn read_in<'a>(names: &Names<'a>, expression: Node<'a>) -> Vec<&'a str> {
    descendants(expression)
        .into_iter()
        .filter(|node| {
            names.profile.is_identifier(*node)
                || (node.named_child_count() == 0 && kind_of(node).ends_with("identifier"))
        })
        .map(|node| names.text(node))
        .collect()
}

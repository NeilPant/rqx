//! Caches that only grow. A collection shared by the whole program that functions add
//! entries to under keys they were given, and that nothing in the file ever removes
//! from or measures, keeps an entry for every distinct key the program has ever seen.

use std::collections::{HashMap, HashSet};

use rqx_core::RuleId;
use rqx_core::facts::CostSite;
use tree_sitter::Node;

use super::file::FileFacts;
use super::types::declarations;
use super::written::read_in;
use crate::names::Names;
use crate::tree::{descendants, find_within, kind_of, span_of};
use crate::vocabulary::mentions_any;

/// Methods that add an entry, or an element, to what they are called on.
const GROWERS: [&str; 14] = [
    "push",
    "add",
    "append",
    "set",
    "put",
    "insert",
    "emplace",
    "push_back",
    "emplace_back",
    "extend",
    "update",
    "setdefault",
    "putIfAbsent",
    "computeIfAbsent",
];

/// Methods that take entries out, or empty the collection.
const REMOVERS: [&str; 20] = [
    "delete",
    "remove",
    "pop",
    "popitem",
    "clear",
    "discard",
    "evict",
    "shift",
    "splice",
    "erase",
    "poll",
    "removeIf",
    "removeAll",
    "retainAll",
    "expire",
    "trim",
    "truncate",
    "pop_back",
    "pop_front",
    "popleft",
];

/// Properties and calls that measure a collection, which is how a bound is enforced.
const MEASURES: [&str; 5] = ["size", "length", "len", "count", "Count"];

/// Words that say a cache is bounded by what it is, in its name or where it is made.
const BOUND_HINTS: [&str; 10] = [
    "weak",
    "lru",
    "ttl",
    "bounded",
    "expir",
    "evict",
    "capacity",
    "maxsize",
    "caffeine",
    "cachetools",
];

/// What the file does to each shared collection it declares.
#[derive(Default)]
struct Use<'a> {
    /// The first place a function adds to it under a key it was given.
    keyed_growth: Option<Node<'a>>,
    shrinks: bool,
    measured: bool,
}

/// The places in a file where a shared collection only ever gains entries, and a
/// memoizing decorator that keeps every result.
pub fn unbounded_caches<'a>(names: Names<'a>, root: Node<'a>, facts: &FileFacts) -> Vec<CostSite> {
    let mut sites = Vec::new();
    if !facts.collections.is_empty() {
        sites.extend(growing_collections(names, root, facts));
    }
    sites.extend(unbounded_memoizers(names, root));
    sites
}

fn growing_collections<'a>(names: Names<'a>, root: Node<'a>, facts: &FileFacts) -> Vec<CostSite> {
    let profile = names.profile;
    let mut uses: HashMap<String, Use<'a>> = HashMap::new();
    for node in descendants(root) {
        if profile.is_call(node)
            && let Some(dotted) = names.called_name(node)
            && !dotted.contains('.')
        {
            // `delete(cache, key)` and `len(cache)` take the collection as an argument.
            if REMOVERS.contains(&dotted.as_str()) {
                note_all(&names, node, facts, &mut uses, |used| used.shrinks = true);
            } else if MEASURES.contains(&dotted.as_str()) {
                note_all(&names, node, facts, &mut uses, |used| used.measured = true);
            }
        } else if profile.is_call(node)
            && let Some(dotted) = names.called_name(node)
            && let Some((receiver, tail)) = dotted.rsplit_once('.')
        {
            let owner = receiver.rsplit('.').next().unwrap_or(receiver);
            if let Some((name, _)) = facts.collections.get_key_value(owner) {
                let used = uses.entry(name.clone()).or_default();
                if REMOVERS.contains(&tail) {
                    used.shrinks = true;
                } else if MEASURES.contains(&tail) {
                    used.measured = true;
                } else if GROWERS.contains(&tail) && used.keyed_growth.is_none() {
                    used.keyed_growth =
                        keyed_by_a_parameter(names, node, &names.arguments(node)).then_some(node);
                }
            }
        } else if kind_of(node).contains("delete") {
            note_all(&names, node, facts, &mut uses, |used| used.shrinks = true);
        } else if profile.assignment_kinds.contains(&kind_of(node)) {
            note_assignment(names, node, facts, &mut uses);
        } else if profile.is_member_access(node)
            && let Some(property) = names.member_property(node)
            && MEASURES.contains(&names.text(property))
        {
            note_all(&names, node, facts, &mut uses, |used| used.measured = true);
        }
    }
    let mut sites: Vec<CostSite> = uses
        .into_iter()
        .filter(|(name, used)| !used.shrinks && !used.measured && !bounded_by_name(name, facts))
        .filter_map(|(name, used)| {
            used.keyed_growth.map(|at| CostSite {
                rule: RuleId::UnboundedCache,
                span: span_of(at),
                subject: name,
                depth: 0,
            })
        })
        .collect();
    sites.sort_by_key(|site| site.span.start);
    sites
}

/// Applies `mark` to each shared collection named anywhere inside `node`.
fn note_all<'a>(
    names: &Names<'a>,
    node: Node<'a>,
    facts: &FileFacts,
    uses: &mut HashMap<String, Use<'a>>,
    mark: impl Fn(&mut Use<'a>),
) {
    for name in read_in(names, node) {
        if facts.collections.contains_key(name) {
            mark(uses.entry(name.to_owned()).or_default());
        }
    }
}

/// `cache[key] = value` adds an entry; `cache = {}` inside a function empties it.
fn note_assignment<'a>(
    names: Names<'a>,
    assignment: Node<'a>,
    facts: &FileFacts,
    uses: &mut HashMap<String, Use<'a>>,
) {
    let Some(target) = assignment.child_by_field_name("left") else {
        return;
    };
    let target = names.see_through(target);
    let target = descendants(target)
        .into_iter()
        .find(|inner| kind_of(inner).contains("subscript") || kind_of(inner).contains("index"))
        .unwrap_or(target);
    if names.profile.is_identifier(target) {
        let name = names.text(target);
        if facts.collections.contains_key(name) && enclosing_function(names, target).is_some() {
            uses.entry(name.to_owned()).or_default().shrinks = true;
        }
        return;
    }
    let Some(base) = ["value", "object", "operand", "argument"]
        .iter()
        .find_map(|field| target.child_by_field_name(field))
    else {
        return;
    };
    let name = names.text(names.see_through(base));
    let Some((name, _)) = facts.collections.get_key_value(name) else {
        return;
    };
    let used = uses.entry(name.clone()).or_default();
    if used.keyed_growth.is_none() {
        let key: Vec<Node<'a>> = ["subscript", "index", "indices"]
            .iter()
            .find_map(|field| target.child_by_field_name(field))
            .into_iter()
            .collect();
        used.keyed_growth = keyed_by_a_parameter(names, assignment, &key).then_some(assignment);
    }
}

/// Whether the key, or the first thing added, is something the enclosing function was
/// given, so that the entries follow what callers pass in.
fn keyed_by_a_parameter<'a>(names: Names<'a>, at: Node<'a>, keys: &[Node<'a>]) -> bool {
    let Some(function) = enclosing_function(names, at) else {
        return false;
    };
    let parameters = parameter_names(names, function);
    keys.iter().any(|key| {
        read_in(&names, *key)
            .iter()
            .any(|name| parameters.contains(name))
    })
}

fn enclosing_function<'a>(names: Names<'a>, node: Node<'a>) -> Option<Node<'a>> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if names.profile.is_function(parent) {
            return Some(parent);
        }
        current = parent.parent();
    }
    None
}

fn parameter_names<'a>(names: Names<'a>, function: Node<'a>) -> HashSet<&'a str> {
    let profile = names.profile;
    let mut found = HashSet::new();
    if let Some(list) = find_within(function, profile.parameter_list_kinds, profile.block_kinds)
        .into_iter()
        .next_back()
    {
        let mut cursor = list.walk();
        for parameter in list.named_children(&mut cursor) {
            if profile.parameter_kinds.contains(&kind_of(parameter)) {
                found.extend(
                    declarations(&names, parameter)
                        .into_iter()
                        .map(|declaration| declaration.name),
                );
            } else if profile.is_identifier(parameter) {
                found.insert(names.text(parameter));
            }
        }
    } else if let Some(only) = function
        .child_by_field_name("parameter")
        .filter(|node| profile.is_identifier(*node))
    {
        found.insert(names.text(only));
    }
    found
}

/// A cache named for being bounded, or made by something that bounds it.
fn bounded_by_name(name: &str, facts: &FileFacts) -> bool {
    mentions_any(name, &BOUND_HINTS)
        || facts
            .collections
            .get(name)
            .is_some_and(|declaration| mentions_any(declaration, &BOUND_HINTS))
}

/// `@lru_cache(maxsize=None)`, which remembers every result it has ever computed.
fn unbounded_memoizers<'a>(names: Names<'a>, root: Node<'a>) -> Vec<CostSite> {
    if names.profile.decorated_kinds.is_empty() || !names.source.contains("lru_cache") {
        return Vec::new();
    }
    descendants(root)
        .into_iter()
        .filter(|node| kind_of(node) == "decorator")
        .filter(|decorator| {
            let written: String = names
                .text(*decorator)
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            written.contains("lru_cache(maxsize=None)") || written.contains("lru_cache(None)")
        })
        .map(|decorator| CostSite {
            rule: RuleId::UnboundedCache,
            span: span_of(decorator),
            subject: "lru_cache(maxsize=None)".to_owned(),
            depth: 0,
        })
        .collect()
}

//! Signals that are not an order of growth but still say a function costs more than it
//! looks: waiting in series, taking a lock in a loop, starting work with no limit.

use tree_sitter::Node;

use crate::names::Names;
use crate::tree::{descendants, is_binding_position, kind_of};

/// What a language writes for the things the signals look for. Names are exact, like
/// the performance vocabulary beside them.
pub struct SignalVocabulary {
    /// Nodes that allocate: a construction, a collection written out.
    pub allocating: &'static [&'static str],
    /// Calls that take a lock, matched on their last segment.
    pub lock_calls: &'static [&'static str],
    /// Statements that hold a lock for their body.
    pub lock_kinds: &'static [&'static str],
    /// Types whose declaration takes a lock, which C++ spells as a variable.
    pub lock_types: &'static [&'static str],
    /// Calls that start many pieces of work at once, matched as dotted names.
    pub fanout_calls: &'static [&'static str],
    /// Calls that start one piece of work, which is unbounded only inside a loop.
    pub spawn_calls: &'static [&'static str],
    /// Statements that start work of their own: `go f()`.
    pub spawn_kinds: &'static [&'static str],
}

/// Pairs of consecutive awaits where the second does not read what the first
/// produced, so they could have run together. Each pair is the two statements.
pub fn independent_awaits<'a>(names: &Names<'a>, holder: Node<'a>) -> Vec<(Node<'a>, Node<'a>)> {
    let profile = names.profile;
    let mut cursor = holder.walk();
    let statements: Vec<Node<'a>> = holder.named_children(&mut cursor).collect();
    let introduces = |node: &Node<'a>| {
        profile.is_identifier(*node)
            && is_binding_position(
                *node,
                &[],
                profile.borrowed_name_kinds,
                profile.local_declaring_kinds,
                profile.expression_list_kinds,
            )
    };
    let takes_an_answer = |statement: Node<'a>| descendants(statement).iter().any(introduces);
    let waits = |statement: Node<'a>| {
        descendants(statement)
            .into_iter()
            .any(|node| profile.await_kinds.contains(&kind_of(node)))
    };

    let mut pairs = Vec::new();
    for window in statements.windows(2) {
        let [first, second] = window else { continue };
        // Both have to be taking an answer: two awaited calls written for their
        // effects are in that order because the author put them there.
        if !waits(*first) || !waits(*second) {
            continue;
        }
        if !takes_an_answer(*first) || !takes_an_answer(*second) {
            continue;
        }
        // The second reading a name the first introduced is the reason they are in
        // series.
        let introduced: Vec<&str> = descendants(*first)
            .iter()
            .filter(|node| introduces(node))
            .map(|node| names.text(*node))
            .collect();
        let depends = descendants(*second)
            .into_iter()
            .any(|node| profile.is_identifier(node) && introduced.contains(&names.text(node)));
        if !depends {
            pairs.push((*first, *second));
        }
    }
    pairs
}

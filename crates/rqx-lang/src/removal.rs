//! Code after an exit that no path runs, but whose removal would still change the
//! program, or that the analysis cannot prove dead. `unreachable-code` does not report
//! it, and its fix refuses it, so the two cannot disagree.

use std::collections::HashSet;

use rqx_core::Language;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{descendants, descendants_outside, kind_of, text_of};

const LABELED: &[&str] = &["labeled_statement"];
const JUMPS: &[&str] = &["goto_statement", "break_statement", "continue_statement"];
const LABEL_NAMES: &[&str] = &["label_name", "statement_identifier", "identifier"];

/// Whether deleting `dead`, a statement that follows an exit, would change what the
/// program does or could break its build.
pub fn still_matters(profile: &LanguageProfile, source: &str, dead: Node<'_>) -> bool {
    // A block the parser could not read whole is usually a macro it took for
    // statements, as in `ASSERT_DEATH(f(); return, "")`.
    if dead.parent().is_some_and(|block| block.has_error()) || dead.has_error() {
        return true;
    }
    let function = enclosing_function(profile, dead);
    if is_jumped_into(source, dead, function) {
        return true;
    }
    match profile.language {
        Language::Cpp => holds_macro(profile, source, dead),
        Language::Python => {
            makes_a_generator(profile, dead, function)
                || binds_a_name_read_elsewhere(profile, source, dead, function)
        }
        Language::TypeScript | Language::JavaScript => hoists(profile, dead),
        Language::Go | Language::Java => false,
    }
}

/// The function holding `node`, or the file when it sits at the top level.
fn enclosing_function<'tree>(profile: &LanguageProfile, node: Node<'tree>) -> Node<'tree> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if profile.is_function(parent) {
            return parent;
        }
        current = parent;
    }
    current
}

/// A label inside `dead` that a `goto`, `break` or `continue` names. Control reaches a
/// label from its jump, whatever comes before it.
fn is_jumped_into(source: &str, dead: Node<'_>, function: Node<'_>) -> bool {
    let labels: HashSet<&str> = labels(source, dead).into_iter().collect();
    !labels.is_empty()
        && jump_targets(source, function)
            .iter()
            .any(|target| labels.contains(target))
}

/// The label every jump in `scope` names.
pub(crate) fn jump_targets<'src>(source: &'src str, scope: Node<'_>) -> Vec<&'src str> {
    descendants(scope)
        .into_iter()
        .filter(|node| JUMPS.contains(&kind_of(*node)))
        .filter_map(|jump| {
            let mut cursor = jump.walk();
            jump.named_children(&mut cursor)
                .find(|child| LABEL_NAMES.contains(&kind_of(*child)))
        })
        .map(|label| text_of(label, source))
        .collect()
}

/// The labels `scope` defines.
pub(crate) fn labels<'src>(source: &'src str, scope: Node<'_>) -> Vec<&'src str> {
    descendants(scope)
        .into_iter()
        .filter(|node| LABELED.contains(&kind_of(*node)))
        .filter_map(|node| node.named_child(0))
        .map(|label| text_of(label, source))
        .collect()
}

/// A macro can expand to anything, a definition or a jump included, so the analysis
/// does not know what one after an exit does. The preprocessor gives no names, so a
/// call spelled in capitals stands for one, as does any preprocessor line.
fn holds_macro(profile: &LanguageProfile, source: &str, dead: Node<'_>) -> bool {
    descendants(dead).into_iter().any(|node| {
        kind_of(node).starts_with("preproc_")
            || (profile.is_call(node)
                && node
                    .child_by_field_name(profile.callee_field)
                    .is_some_and(|callee| is_macro_name(text_of(callee, source))))
    })
}

/// `ASSERT_EQ`, `NOTREACHED`, `CHECK`: the convention for a name the preprocessor
/// replaces.
pub(crate) fn is_macro_name(name: &str) -> bool {
    name.len() > 1
        && name.chars().any(|character| character.is_ascii_uppercase())
        && name.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

/// A `yield` anywhere in a Python function makes it a generator, reached or not. The
/// one after a `return` is how a function that yields nothing is written.
fn makes_a_generator(profile: &LanguageProfile, dead: Node<'_>, function: Node<'_>) -> bool {
    let yields = |scope: Node<'_>| -> Vec<usize> {
        descendants_outside(scope, |node| node != scope && opens_a_scope(profile, node))
            .into_iter()
            .filter(|node| kind_of(*node) == "yield")
            .map(|node| node.id())
            .collect()
    };
    let inside = yields(dead);
    !inside.is_empty() && yields(function).iter().all(|id| inside.contains(id))
}

/// Python decides at compile time which names a function binds, from every
/// statement in it, reached or not: `global`, `nonlocal`, or the only assignment,
/// import or definition of a name after a `return` still makes the name local where
/// the rest of the function reads it.
fn binds_a_name_read_elsewhere(
    profile: &LanguageProfile,
    source: &str,
    dead: Node<'_>,
    function: Node<'_>,
) -> bool {
    let inside = descendants_outside(dead, |node| node != dead && opens_a_scope(profile, node));
    if inside
        .iter()
        .any(|node| matches!(kind_of(*node), "global_statement" | "nonlocal_statement"))
    {
        return true;
    }
    let outside_dead = |node: &Node<'_>| {
        node.end_byte() <= dead.start_byte() || node.start_byte() >= dead.end_byte()
    };
    let bound_in = |nodes: &[Node<'_>]| -> HashSet<&str> {
        nodes
            .iter()
            .filter_map(|node| bound_target(*node))
            .flat_map(descendants)
            .filter(|node| profile.is_identifier(*node))
            .map(|node| text_of(node, source))
            .collect()
    };
    let rest: Vec<Node<'_>> = descendants_outside(function, |node| {
        node != function && opens_a_scope(profile, node)
    })
    .into_iter()
    .filter(outside_dead)
    .collect();
    let parameters: HashSet<&str> = function
        .child_by_field_name("parameters")
        .map(descendants)
        .unwrap_or_default()
        .into_iter()
        .filter(|node| profile.is_identifier(*node))
        .map(|node| text_of(node, source))
        .collect();
    let bound_elsewhere = bound_in(&rest);
    let only_bound_here: HashSet<&str> = bound_in(&inside)
        .into_iter()
        .filter(|name| !bound_elsewhere.contains(name) && !parameters.contains(name))
        .collect();
    // Everything after the exit in the same block is as dead as `dead`, so a read
    // there runs no more than the binding does.
    let dead_end = dead
        .parent()
        .map_or(dead.end_byte(), |block| block.end_byte());
    let reached =
        |node: &Node<'_>| node.end_byte() <= dead.start_byte() || node.start_byte() >= dead_end;
    !only_bound_here.is_empty()
        && descendants(function)
            .into_iter()
            .filter(reached)
            .any(|node| {
                profile.is_identifier(node) && only_bound_here.contains(text_of(node, source))
            })
}

/// The part of a Python statement that names what it binds.
fn bound_target(node: Node<'_>) -> Option<Node<'_>> {
    let field = match kind_of(node) {
        "assignment" | "augmented_assignment" | "for_statement" => "left",
        "named_expression" | "function_definition" | "class_definition" => "name",
        "aliased_import" | "as_pattern" => "alias",
        "import_statement" | "import_from_statement" => "name",
        "delete_statement" => return node.named_child(0),
        _ => return None,
    };
    node.child_by_field_name(field)
}

/// A `var` or a function declaration is hoisted to the top of its function, so one
/// after a `return` still decides what the name means everywhere in it.
fn hoists(profile: &LanguageProfile, dead: Node<'_>) -> bool {
    descendants_outside(dead, |node| node != dead && opens_a_scope(profile, node))
        .into_iter()
        .any(|node| {
            matches!(
                kind_of(node),
                "variable_declaration" | "function_declaration" | "generator_function_declaration"
            )
        })
}

fn opens_a_scope(profile: &LanguageProfile, node: Node<'_>) -> bool {
    profile.is_function(node) || profile.is_class(node)
}

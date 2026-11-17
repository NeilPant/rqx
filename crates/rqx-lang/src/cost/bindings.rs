//! Introducing names into a function's environment as the body is read: parameters
//! first, then each declaration and assignment as it is met. What a name was set from
//! decides what it stands for later, so a loop over `copy` is a loop over whatever
//! `copy` was copied from.

use rqx_core::cost::Param;
use tree_sitter::Node;

use super::bounds::Bounds;
use super::env::Binding;
use super::types::{TypeFact, canonical_head, constructed_fact, declarations, is_integer};
use crate::tree::{find_within, kind_of};

/// What a declaration's type and initial value say about the name it introduces.
fn binding_for<'a>(
    bounds: &mut Bounds<'_, 'a>,
    spelled_type: Option<&str>,
    value: Option<Node<'a>>,
) -> Binding {
    let language = bounds.names().profile.language;
    let declared = spelled_type
        .and_then(|spelled| canonical_head(language, spelled))
        .map(TypeFact::declared);
    let fact =
        declared.or_else(|| value.and_then(|value| constructed_fact(&bounds.names(), value)));
    let counts = fact
        .as_ref()
        .is_some_and(|fact| is_integer(language, &fact.head));
    let size = value.and_then(|value| {
        if counts {
            return bounds.count(value);
        }
        // A collection copies the size of what it was made from; a count measures one.
        let collection = fact.is_some() && !counts;
        if collection {
            bounds.collection(value).or_else(|| bounds.count(value))
        } else {
            bounds.count(value).or_else(|| bounds.collection(value))
        }
    });
    Binding {
        fact,
        size,
        parameter: None,
    }
}

/// Introduces the names a declaring node introduces. Where the language declares by
/// assigning, a second write to the same name re-introduces it.
pub fn bind_declaration<'a>(bounds: &mut Bounds<'_, 'a>, node: Node<'a>) {
    for declaration in declarations(&bounds.names(), node) {
        let binding = binding_for(bounds, declaration.spelled_type, declaration.value);
        bounds.env().bind(declaration.name, binding);
    }
}

/// A write to a name that is already introduced. `+=` and `++` change it by an
/// unknown amount, so what the name was known to hold is forgotten; a plain write
/// replaces it.
pub fn bind_assignment<'a>(bounds: &mut Bounds<'_, 'a>, node: Node<'a>) {
    let profile = bounds.names().profile;
    let compound = profile.compound_assignment_kinds.contains(&kind_of(node))
        && crate::tree::operator_of(node, bounds.names().source) != Some("=");
    for declaration in declarations(&bounds.names(), node) {
        let Some(previous) = bounds.env().binding(declaration.name).cloned() else {
            continue;
        };
        let mut binding = if compound {
            Binding::default()
        } else {
            binding_for(bounds, declaration.spelled_type, declaration.value)
        };
        binding.parameter = previous.parameter;
        if binding.fact.is_none() {
            binding.fact = previous.fact;
        }
        bounds.env().bind(declaration.name, binding);
    }
}

/// Introduces a function's parameters, numbered as a caller counts its arguments: a
/// receiver such as `self` is not one of them. Returns them for the function's
/// record.
pub fn bind_parameters<'a>(bounds: &mut Bounds<'_, 'a>, function: Node<'a>) -> Vec<Param> {
    let names = bounds.names();
    let profile = names.profile;
    let mut params = Vec::new();
    let mut introduced: Vec<(String, Option<String>)> = Vec::new();

    if let Some(receiver) = function.child_by_field_name("receiver") {
        for node in find_within(receiver, profile.parameter_kinds, &[]) {
            for declaration in declarations(&names, node) {
                bounds.env().set_receiver(declaration.name);
            }
        }
    }
    let list = find_within(function, profile.parameter_list_kinds, profile.block_kinds)
        .into_iter()
        .next_back();
    if let Some(list) = list {
        let mut cursor = list.walk();
        for node in list.named_children(&mut cursor) {
            if !profile.parameter_kinds.contains(&kind_of(node)) {
                continue;
            }
            for declaration in declarations(&names, node) {
                if profile.self_names.contains(&declaration.name) {
                    continue;
                }
                let head = declaration
                    .spelled_type
                    .and_then(|spelled| canonical_head(profile.language, spelled));
                introduced.push((declaration.name.to_owned(), head));
            }
        }
    } else if let Some(only) = function
        .child_by_field_name("parameter")
        .or_else(|| function.child_by_field_name("parameters"))
        .filter(|node| profile.is_identifier(*node))
    {
        introduced.push((names.text(only).to_owned(), None));
    }

    for (position, (name, head)) in introduced.into_iter().enumerate() {
        let position = u8::try_from(position).unwrap_or(u8::MAX);
        let mut binding = binding_for(bounds, head.as_deref(), None);
        binding.parameter = Some(position);
        bounds.env().bind(&name, binding);
        params.push(Param {
            name,
            type_head: head,
        });
    }
    params
}

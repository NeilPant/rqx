//! What the whole file says that a single function body cannot: which functions it
//! defines, what its classes hold, which collections live at module level, and what it
//! imports. Read in one pass over everything that is not inside a function, and so the
//! functions found on the way are handed back to be read afterwards.

use std::collections::{HashMap, HashSet};

use rqx_core::facts::ModuleFacts;
use tree_sitter::Node;

use super::types::{TypeFact, declarations, declares, fact_of_declaration};
use crate::names::Names;
use crate::tree::{descendants, kind_of};

/// A name this file brings in from another.
pub struct Import {
    /// The name as the target exports it. For a namespace import it is empty, and the
    /// member is whatever is reached through the namespace.
    pub member: Option<String>,
    pub namespace: bool,
    /// The position of the import among the file's import requests.
    pub request: u16,
}

/// A function waiting to be read, with the class it was written in.
pub struct Site<'a> {
    pub node: Node<'a>,
    pub class: Option<String>,
}

#[derive(Default)]
pub struct FileFacts {
    /// Every name a function or method of this file is defined under.
    pub functions: HashSet<String>,
    /// The members each class holds, by class and member name.
    pub fields: HashMap<String, HashMap<String, TypeFact>>,
    /// Collections shared by the whole program: module-level, or `static`.
    pub globals: HashSet<String>,
    /// Imported names, by the name this file uses.
    pub imports: HashMap<String, Import>,
    /// The names this file makes available to others.
    pub exports: HashSet<String>,
}

impl FileFacts {
    pub fn fields_of(&self, class: Option<&str>) -> HashMap<String, TypeFact> {
        class
            .and_then(|class| self.fields.get(class))
            .cloned()
            .unwrap_or_default()
    }
}

/// How a function is known: the name it is declared with, or the name it is bound to
/// where it is written as a value.
pub fn identity<'a>(names: &Names<'a>, function: Node<'a>) -> Option<&'a str> {
    let profile = names.profile;
    if let Some(name) = names.function_name(function) {
        return Some(names.text(name));
    }
    let parent = function.parent()?;
    let kind = kind_of(parent);
    let named = if profile.declarator_kinds.contains(&kind) {
        parent.child_by_field_name("name")
    } else if profile.assignment_kinds.contains(&kind) {
        parent.child_by_field_name("left")
    } else if profile.property_definition_kinds.contains(&kind)
        || profile.field_kinds.contains(&kind)
    {
        parent
            .child_by_field_name("key")
            .or_else(|| parent.child_by_field_name("name"))
    } else {
        None
    }?;
    let text = names.text(named);
    text.rsplit(['.', ':'])
        .next()
        .filter(|last| !last.is_empty())
}

/// The type a Go method is declared on, which is the class it belongs to.
fn receiver_type<'a>(names: &Names<'a>, function: Node<'a>) -> Option<String> {
    let receiver = function.child_by_field_name("receiver")?;
    let parameter = descendants(receiver)
        .into_iter()
        .find(|node| names.profile.parameter_kinds.contains(&kind_of(*node)))?;
    let spelled = names.text(parameter.child_by_field_name("type")?);
    super::types::canonical_head(names.profile.language, spelled)
}

pub fn read<'a>(
    names: Names<'a>,
    root: Node<'a>,
    module: Option<&ModuleFacts>,
) -> (FileFacts, Vec<Site<'a>>) {
    let mut facts = FileFacts::default();
    if let Some(module) = module {
        facts
            .exports
            .extend(module.exports.iter().map(|export| export.name.clone()));
        for (position, request) in module.imports.iter().enumerate() {
            for (index, local) in request.locals.iter().enumerate() {
                facts.imports.insert(
                    local.clone(),
                    Import {
                        member: request.names.get(index).cloned(),
                        namespace: request.takes_everything,
                        request: u16::try_from(position).unwrap_or(u16::MAX),
                    },
                );
            }
        }
    }
    let mut sites = Vec::new();
    scan(names, root, None, true, &mut facts, &mut sites);
    (facts, sites)
}

/// Walks what is not inside a function, handing each function found to `sites`.
fn scan<'a>(
    names: Names<'a>,
    node: Node<'a>,
    class: Option<&str>,
    at_module_level: bool,
    facts: &mut FileFacts,
    sites: &mut Vec<Site<'a>>,
) {
    let profile = names.profile;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if profile.is_function(child) {
            register(names, child, class, facts, sites);
        } else if profile.is_class(child) {
            let named = child
                .child_by_field_name("name")
                .map(|name| names.text(name).to_owned());
            if let Some(name) = &named {
                let fields = class_fields(names, child, facts);
                facts.fields.insert(name.clone(), fields);
            }
            scan(
                names,
                child,
                named.as_deref().or(class),
                false,
                facts,
                sites,
            );
        } else {
            if at_module_level && declares(profile, child) {
                note_global(names, child, facts);
            }
            scan(names, child, class, at_module_level, facts, sites);
        }
    }
}

fn register<'a>(
    names: Names<'a>,
    function: Node<'a>,
    class: Option<&str>,
    facts: &mut FileFacts,
    sites: &mut Vec<Site<'a>>,
) {
    if let Some(name) = identity(&names, function) {
        facts.functions.insert(name.to_owned());
    }
    let class = receiver_type(&names, function).or_else(|| class.map(str::to_owned));
    sites.push(Site {
        node: function,
        class,
    });
}

/// A module-level name that holds a collection is shared by every caller.
fn note_global(names: Names<'_>, node: Node<'_>, facts: &mut FileFacts) {
    for declaration in declarations(&names, node) {
        if declaration
            .value
            .and_then(|value| super::types::constructed_fact(&names, value))
            .is_some_and(|fact| fact.head != "string" && fact.head != "str")
        {
            facts.globals.insert(declaration.name.to_owned());
        }
    }
}

/// The members a class declares, and the ones its constructor sets. A `static` member
/// that holds a collection is shared like a module-level one.
fn class_fields<'a>(
    names: Names<'a>,
    class: Node<'a>,
    facts: &mut FileFacts,
) -> HashMap<String, TypeFact> {
    let profile = names.profile;
    let mut fields = HashMap::new();
    let body = class
        .child_by_field_name("body")
        .or_else(|| {
            class
                .child_by_field_name("type")
                .and_then(|kind| kind.named_child(0))
        })
        .unwrap_or(class);
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        let kind = kind_of(member);
        let declaring = if profile.assigned_field_kinds.contains(&kind) {
            member.named_child(0).unwrap_or(member)
        } else if profile.field_kinds.contains(&kind) {
            member
        } else {
            if is_constructor(&names, member) {
                constructor_fields(names, member, &mut fields);
            }
            continue;
        };
        let is_static = names
            .text(member)
            .split(['=', ';'])
            .next()
            .is_some_and(|head| {
                head.split_whitespace()
                    .any(|word| profile.static_markers.contains(&word))
            });
        for declaration in declarations(&names, declaring) {
            let fact = fact_of_declaration(&names, &declaration);
            if is_static && fact.is_some() {
                facts.globals.insert(declaration.name.to_owned());
            }
            if let Some(fact) = fact {
                fields.insert(declaration.name.to_owned(), fact);
            }
        }
    }
    fields
}

fn is_constructor(names: &Names<'_>, member: Node<'_>) -> bool {
    let profile = names.profile;
    profile.constructor_kinds.contains(&kind_of(member))
        || names
            .function_name(member)
            .is_some_and(|name| profile.constructor_names.contains(&names.text(name)))
}

/// `this.items = []` in a constructor declares a member where the class does not.
fn constructor_fields<'a>(
    names: Names<'a>,
    constructor: Node<'a>,
    fields: &mut HashMap<String, TypeFact>,
) {
    let profile = names.profile;
    for node in descendants(constructor) {
        if !profile.assignment_kinds.contains(&kind_of(node)) {
            continue;
        }
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) else {
            continue;
        };
        let Some(own) = names.callee_name(left).and_then(|dotted| {
            dotted
                .split_once('.')
                .map(|(head, rest)| (head.to_owned(), rest.to_owned()))
        }) else {
            continue;
        };
        if profile.self_names.contains(&own.0.as_str())
            && !own.1.contains('.')
            && let Some(fact) = super::types::constructed_fact(&names, right)
        {
            fields.entry(own.1).or_insert(fact);
        }
    }
}

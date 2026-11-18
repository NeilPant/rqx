use rqx_core::facts::UnusedParameter;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::scope::{BindingKind, Scopes};
use crate::tree::{find_within, kind_of, span_of, text_of};

/// Who decides the shape of a parameter list. Only a signature the author owns can
/// be shortened: a callback's arity is fixed by whatever calls it, and an override's
/// by the thing it overrides, so an unused parameter there is not a deletion anyone
/// can make locally.
#[derive(Copy, Clone, PartialEq, Eq)]
enum SignatureOwner {
    Author,
    Caller,
    Supertype,
}

struct Grader<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    scopes: &'a Scopes,
    owner: Vec<SignatureOwner>,
    enclosing: Vec<Option<String>>,
    exported_depth: u32,
    out: Vec<UnusedParameter>,
}

impl<'a> Grader<'a> {
    fn owner(&self) -> SignatureOwner {
        self.owner.last().copied().unwrap_or(SignatureOwner::Author)
    }

    fn name_of(&self, parameter: Node<'a>) -> Option<(&'a str, Node<'a>)> {
        if self.profile.is_identifier(parameter) {
            return Some((text_of(parameter, self.source), parameter));
        }
        // Named field first: a grammar that counts type names as identifiers would
        // otherwise answer `String` for `String subject`, and then find that name
        // used nowhere.
        let declared = self
            .profile
            .binding_fields
            .iter()
            .find_map(|field| parameter.child_by_field_name(field));
        if let Some(declared) = declared.filter(|node| self.profile.is_identifier(*node)) {
            return Some((text_of(declared, self.source), declared));
        }
        // A parameter written as a bare type, Go's `func(int)` or C++'s `f(Row*)`, has
        // no name at all, and the identifier inside its type is not one.
        let type_span = declared
            .is_none()
            .then(|| parameter.child_by_field_name("type"))
            .flatten()
            .map(span_of);
        crate::tree::descendants(parameter)
            .into_iter()
            .filter(|child| {
                type_span.is_none_or(|within| {
                    let span = span_of(*child);
                    span.start < within.start || span.end > within.end
                })
            })
            .find(|child| self.profile.is_identifier(*child))
            .map(|child| (text_of(child, self.source), child))
    }

    /// A parameter that also declares a field — TypeScript's
    /// `constructor(private readonly x: T)` — is read wherever the field is, which
    /// this file cannot see from the constructor body alone.
    fn declares_a_field(&self, parameter: Node<'a>) -> bool {
        let mut cursor = parameter.walk();
        parameter.children(&mut cursor).any(|child| {
            self.profile
                .field_declaring_modifiers
                .contains(&kind_of(child))
        })
    }

    fn is_used(&self, name: &str, node: Node<'a>) -> bool {
        let span = span_of(node);
        self.scopes
            .of_kind(BindingKind::Parameter)
            .find(|binding| binding.span == span && binding.name == name)
            .is_none_or(|binding| !binding.references.is_empty())
    }

    fn record(&mut self, function: Node<'a>) {
        if self.owner() != SignatureOwner::Author {
            return;
        }
        // An overload signature, `declare function`, or abstract method has no body,
        // so of course nothing reads its parameters — the implementation elsewhere
        // does. Reporting these is reporting the absence of a body.
        if function.child_by_field_name("body").is_none() {
            return;
        }
        let Some(list) = input_list(self.profile, function) else {
            return;
        };
        let mut cursor = list.walk();
        let parameters: Vec<Node<'a>> = list
            .named_children(&mut cursor)
            .filter(|child| self.profile.parameter_kinds.contains(&kind_of(child)))
            .collect();
        let total = parameters.len() as u32;

        let name = function
            .child_by_field_name("name")
            .or_else(|| {
                find_within(
                    function,
                    self.profile.declarator_wrapper_kinds,
                    self.profile.block_kinds,
                )
                .into_iter()
                .next()
                .and_then(|declarator| {
                    let mut cursor = declarator.walk();
                    declarator.named_children(&mut cursor).find(|child| {
                        self.profile.is_identifier(*child)
                            || self.profile.property_kinds.contains(&kind_of(child))
                    })
                })
            })
            .map(|node| text_of(node, self.source).to_owned())
            .or_else(|| self.enclosing.last().cloned().flatten());

        // Go's runner finds a case by name and calls it with a `*testing.T`. Delete
        // the parameter and the function is no longer a test the runner will call —
        // it stops running, silently, which `rqx fix --write` did to this project's
        // own Go fixtures.
        if name
            .as_deref()
            .is_some_and(|name| self.profile.test.owns_the_signature_of(name))
        {
            return;
        }

        let mut unused = Vec::new();
        for (index, parameter) in parameters.iter().enumerate() {
            if self.declares_a_field(*parameter) {
                continue;
            }
            let Some((identifier, node)) = self.name_of(*parameter) else {
                continue;
            };
            // A leading underscore is the ecosystem's way of saying "required by the
            // signature, deliberately ignored"; reporting it would be reporting the
            // author's own annotation back at them.
            if self.profile.is_ignored_name(identifier)
                || self.profile.self_names.contains(&identifier)
            {
                continue;
            }
            if self.is_used(identifier, node) {
                continue;
            }
            unused.push((index as u32, identifier.to_owned(), span_of(node)));
        }

        let last_used = parameters
            .iter()
            .enumerate()
            .rposition(|(index, _)| !unused.iter().any(|(dead, ..)| *dead == index as u32));
        for (index, parameter_name, span) in unused {
            let trailing = last_used.is_none_or(|last| index as usize > last);
            self.out.push(UnusedParameter {
                name: parameter_name,
                span,
                function: name.clone(),
                position: index + 1,
                total,
                exported: self.exported_depth > 0,
                trailing,
            });
        }
    }

    fn walk(&mut self, node: Node<'a>) {
        let kind = kind_of(node);
        let profile = self.profile;
        let mut owners = 0;
        let mut names = 0;

        if profile.export_kinds.contains(&kind) {
            self.exported_depth += 1;
        }
        if profile.argument_list_kinds.contains(&kind) {
            self.owner.push(SignatureOwner::Caller);
            owners += 1;
        }
        if profile.is_class(node) {
            let mut cursor = node.walk();
            let inherited = node
                .named_children(&mut cursor)
                .any(|child| profile.heritage_kinds.contains(&kind_of(child)));
            self.owner.push(if inherited {
                SignatureOwner::Supertype
            } else {
                SignatureOwner::Author
            });
            owners += 1;
        }
        if profile.declarator_kinds.contains(&kind)
            || profile.property_definition_kinds.contains(&kind)
        {
            self.enclosing.push(
                node.child_by_field_name("name")
                    .map(|name| text_of(name, self.source).to_owned()),
            );
            names += 1;
            // A binding with a declared type has its function shape imposed by that
            // type, so its arity is not the author's to change.
            let mut cursor = node.walk();
            let annotated = node
                .named_children(&mut cursor)
                .any(|child| profile.type_annotation_kinds.contains(&kind_of(child)))
                || profile.property_definition_kinds.contains(&kind);
            self.owner.push(if annotated {
                SignatureOwner::Caller
            } else {
                SignatureOwner::Author
            });
            owners += 1;
        }
        if profile.method_kinds.contains(&kind) {
            self.enclosing.push(
                node.child_by_field_name("name")
                    .map(|name| text_of(name, self.source).to_owned()),
            );
            names += 1;
        }
        if profile.is_function(node) {
            self.record(node);
            // A function declared inside a callback still owns its own signature.
            self.owner.push(SignatureOwner::Author);
            owners += 1;
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.walk(child);
        }

        for _ in 0..owners {
            self.owner.pop();
        }
        for _ in 0..names {
            self.enclosing.pop();
        }
        if profile.export_kinds.contains(&kind) {
            self.exported_depth -= 1;
        }
    }
}

/// The parameter list a function's callers fill. A Go signature can hold three lists,
/// receiver, parameters and results, and a function type written in a signature
/// brings its own, so the one the grammar files under `parameters` is preferred.
pub(crate) fn input_list<'tree>(
    profile: &LanguageProfile,
    function: Node<'tree>,
) -> Option<Node<'tree>> {
    let lists = find_within(function, profile.parameter_list_kinds, profile.block_kinds);
    lists
        .iter()
        .copied()
        .find(|list| crate::tree::field_name(*list) == Some("parameters"))
        .or_else(|| lists.last().copied())
}

/// Whether a parameter binding is one of its function's inputs, rather than a Go
/// receiver or named result, or a name inside a function type in the signature.
pub(crate) fn is_input_parameter(
    profile: &LanguageProfile,
    root: Node<'_>,
    name: rqx_core::span::Span,
) -> bool {
    let mut current = root.descendant_for_byte_range(name.start as usize, name.end as usize);
    let mut list = None;
    let mut below = None;
    while let Some(node) = current {
        if list.is_none() && profile.parameter_list_kinds.contains(&kind_of(node)) {
            list = Some(node);
        }
        if profile.is_function(node) {
            // `x => x + 1` and Java's `x -> x + 1` name their one parameter with no
            // list around it.
            let lone = below.is_some_and(|child| {
                matches!(
                    crate::tree::field_name_under(node, child),
                    Some("parameter" | "parameters")
                )
            });
            return lone || (list.is_some() && input_list(profile, node) == list);
        }
        below = Some(node);
        current = node.parent();
    }
    false
}

pub fn unused(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
    scopes: &Scopes,
) -> Vec<UnusedParameter> {
    let mut grader = Grader {
        profile,
        source,
        scopes,
        owner: Vec::new(),
        enclosing: Vec::new(),
        exported_depth: 0,
        out: Vec::new(),
    };
    grader.walk(root);
    grader.out
}

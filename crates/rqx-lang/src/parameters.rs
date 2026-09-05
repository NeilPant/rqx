use rqx_core::facts::UnusedParameter;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::scope::{BindingKind, Scopes};
use crate::tree::{find_within, span_of, text_of};

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
        if let Some(declared) = self
            .profile
            .binding_fields
            .iter()
            .find_map(|field| parameter.child_by_field_name(field))
            .filter(|node| self.profile.is_identifier(*node))
        {
            return Some((text_of(declared, self.source), declared));
        }
        crate::tree::descendants(parameter)
            .into_iter()
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
                .contains(&child.kind())
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
        let Some(list) = find_within(
            function,
            self.profile.parameter_list_kinds,
            self.profile.block_kinds,
        )
        .into_iter()
        .next_back() else {
            return;
        };
        let mut cursor = list.walk();
        let parameters: Vec<Node<'a>> = list
            .named_children(&mut cursor)
            .filter(|child| self.profile.parameter_kinds.contains(&child.kind()))
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
                            || self.profile.property_kinds.contains(&child.kind())
                    })
                })
            })
            .map(|node| text_of(node, self.source).to_owned())
            .or_else(|| self.enclosing.last().cloned().flatten());

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
        let kind = node.kind();
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
                .any(|child| profile.heritage_kinds.contains(&child.kind()));
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
                .any(|child| profile.type_annotation_kinds.contains(&child.kind()))
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

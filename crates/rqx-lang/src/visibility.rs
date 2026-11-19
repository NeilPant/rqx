//! Whether code outside a file can call one of its functions. A public function's
//! parameter list is a contract with callers no scan of the project sees, so neither
//! `unused-parameter` nor `rqx fix` treats it as the author's to shorten.

use std::collections::{BTreeMap, HashSet};

use rqx_core::Language;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{descendants, find_within, kind_of, text_of};

pub struct Visibility<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    /// Python's `__all__`, which makes a name public whatever its spelling.
    listed_in_all: HashSet<&'a str>,
}

impl<'a> Visibility<'a> {
    pub fn new(profile: &'static LanguageProfile, source: &'a str, root: Node<'_>) -> Self {
        let listed_in_all = if profile.language == Language::Python {
            listed_in_all(source, root)
        } else {
            HashSet::new()
        };
        Self {
            profile,
            source,
            listed_in_all,
        }
    }

    /// Python: a top-level function whose name has no leading underscore, or one
    /// `__all__` lists. TypeScript and JavaScript: an exported function, or a method
    /// that is not `private` or `#private`. Java: anything not `private`. C++: anything
    /// but a `static` free function, one in an unnamed namespace, or a lambda. Go: a
    /// capitalised name.
    pub fn is_public(&self, function: Node<'_>, name: Option<&str>) -> bool {
        let kind = kind_of(function);
        match self.profile.language {
            Language::Python => {
                !self.is_nested(function)
                    && name.is_some_and(|name| {
                        !name.starts_with('_') || self.listed_in_all.contains(name)
                    })
            }
            Language::TypeScript | Language::JavaScript => {
                self.is_exported(function)
                    || (self.profile.method_kinds.contains(&kind)
                        && !self.is_private_member(function))
            }
            Language::Java => {
                kind != "lambda_expression"
                    && !header_words(function, self.source).any(|word| word == "private")
            }
            Language::Cpp => kind != "lambda_expression" && !self.has_internal_linkage(function),
            Language::Go => name
                .and_then(|name| name.chars().next())
                .is_some_and(char::is_uppercase),
        }
    }

    fn is_nested(&self, function: Node<'_>) -> bool {
        std::iter::successors(function.parent(), Node::parent)
            .any(|ancestor| self.profile.is_function(ancestor))
    }

    fn is_exported(&self, function: Node<'_>) -> bool {
        std::iter::successors(function.parent(), Node::parent)
            .any(|ancestor| self.profile.export_kinds.contains(&kind_of(ancestor)))
    }

    /// `private go()` or `#go()`: a method only its own class can call.
    fn is_private_member(&self, function: Node<'_>) -> bool {
        let mut cursor = function.walk();
        let private_keyword = function.named_children(&mut cursor).any(|child| {
            kind_of(child) == "accessibility_modifier" && text_of(child, self.source) == "private"
        });
        private_keyword
            || function
                .child_by_field_name("name")
                .is_some_and(|name| kind_of(name) == "private_property_identifier")
    }

    /// A `static` free function or one in an unnamed namespace, which no other
    /// translation unit can call. A `static` member function is still a member.
    fn has_internal_linkage(&self, function: Node<'_>) -> bool {
        let ancestors: Vec<Node<'_>> =
            std::iter::successors(function.parent(), Node::parent).collect();
        let in_unnamed_namespace = ancestors.iter().any(|node| {
            kind_of(*node) == "namespace_definition" && node.child_by_field_name("name").is_none()
        });
        let in_class = ancestors
            .iter()
            .any(|node| kind_of(*node) == "field_declaration_list");
        in_unnamed_namespace
            || (!in_class && header_words(function, self.source).any(|word| word == "static"))
    }
}

/// The header of every public function in the file, by name. A list because
/// overloads share a name.
pub fn public_signatures(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
) -> BTreeMap<String, Vec<String>> {
    let visibility = Visibility::new(profile, source, root);
    let mut signatures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for function in descendants(root)
        .into_iter()
        .filter(|node| profile.is_function(*node))
    {
        let Some(name) = function_name(profile, source, function) else {
            continue;
        };
        if visibility.is_public(function, Some(name)) {
            signatures
                .entry(name.to_owned())
                .or_default()
                .push(header_words(function, source).collect::<Vec<_>>().join(" "));
        }
    }
    for headers in signatures.values_mut() {
        headers.sort();
    }
    signatures
}

/// A function's own name: the grammar's `name` field, or the identifier its C++
/// declarator wraps.
fn function_name<'s>(
    profile: &LanguageProfile,
    source: &'s str,
    function: Node<'_>,
) -> Option<&'s str> {
    if let Some(name) = function.child_by_field_name("name") {
        return Some(text_of(name, source));
    }
    let declarator = find_within(
        function,
        profile.declarator_wrapper_kinds,
        profile.block_kinds,
    )
    .into_iter()
    .next()?;
    let mut cursor = declarator.walk();
    let name = declarator.named_children(&mut cursor).find(|child| {
        profile.is_identifier(*child) || profile.property_kinds.contains(&kind_of(*child))
    })?;
    Some(text_of(name, source))
}

/// The words of a function's text before its body: modifiers, name, parameters.
pub fn header_words<'s>(function: Node<'_>, source: &'s str) -> impl Iterator<Item = &'s str> {
    let end = function
        .child_by_field_name("body")
        .map_or(function.end_byte(), |body| body.start_byte());
    source
        .get(function.start_byte()..end)
        .unwrap_or_default()
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .filter(|word| !word.is_empty())
}

/// The names a Python module lists in `__all__`.
fn listed_in_all<'s>(source: &'s str, root: Node<'_>) -> HashSet<&'s str> {
    descendants(root)
        .into_iter()
        .filter(|node| matches!(kind_of(*node), "assignment" | "augmented_assignment"))
        .filter(|node| {
            node.child_by_field_name("left")
                .is_some_and(|left| text_of(left, source) == "__all__")
        })
        .filter_map(|node| node.child_by_field_name("right"))
        .flat_map(descendants)
        .filter(|node| kind_of(*node) == "string_content")
        .map(|node| text_of(node, source))
        .collect()
}

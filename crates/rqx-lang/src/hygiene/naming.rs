//! Rules about the names a file introduces and how far they travel.

use rqx_core::facts::Spot;
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::scope::{Binding, BindingKind, Scopes};
use crate::tree::{descendants, kind_of, span_of};
use crate::vocabulary::convention_of;

use super::{Walker, split_words};

impl<'a> Walker<'a> {
    /// A name that says what a thing is not, and a name the language already owns.
    pub(super) fn note_declared_name(&mut self, node: Node<'a>) {
        let profile = self.profile;
        // The empty parameter list is deliberate: "any identifier under a parameter
        // node" would count the type beside the name, and a type is not a name this
        // file chose.
        if !profile.is_identifier(node)
            || !crate::tree::is_binding_position(
                node,
                &[],
                profile.borrowed_name_kinds,
                profile.local_declaring_kinds,
                profile.expression_list_kinds,
            )
        {
            return;
        }
        let name = self.text(node);
        if profile.builtin_names.contains(&name) {
            self.out.shadowed_builtins.push(Spot {
                span: span_of(node),
                detail: name.to_owned(),
                count: 1,
            });
        }
        if let Some(marker) = crate::vocabulary::negated_name(name) {
            self.out.negated_names.push(Spot {
                span: span_of(node),
                detail: format!("{name} ({marker})"),
                count: 1,
            });
        }
    }

    /// A name introduced on one line and returned on the next. The name is read
    /// once, by the line under it, and says nothing the expression did not.
    pub(super) fn note_fresh_local_return(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        let mut cursor = holder.walk();
        let statements: Vec<Node<'a>> = holder
            .named_children(&mut cursor)
            .filter(|child| !profile.is_comment(*child))
            .collect();
        let [.., declares, returns] = statements.as_slice() else {
            return;
        };
        if kind_of(returns) != "return_statement" {
            return;
        }
        let Some(mut answer) = returns.named_child(0).map(|node| self.see_through(node)) else {
            return;
        };
        while profile.expression_list_kinds.contains(&kind_of(answer))
            && answer.named_child_count() == 1
        {
            let Some(only) = answer.named_child(0) else {
                break;
            };
            answer = self.see_through(only);
        }
        if !profile.is_identifier(answer) {
            return;
        }
        let name = self.text(answer);
        let introduces = descendants(*declares).into_iter().any(|node| {
            profile.is_identifier(node)
                && self.text(node) == name
                && crate::tree::is_binding_position(
                    node,
                    &[],
                    profile.borrowed_name_kinds,
                    profile.local_declaring_kinds,
                    profile.expression_list_kinds,
                )
        });
        // Read anywhere else and the name is holding a value two lines need.
        let read_elsewhere = descendants(holder)
            .into_iter()
            .filter(|node| profile.is_identifier(*node) && self.text(*node) == name)
            .count()
            > 2;
        if introduces && !read_elsewhere {
            self.out.fresh_local_returns.push(Spot {
                span: span_of(*declares),
                detail: name.to_owned(),
                count: 1,
            });
        }
    }

    /// A name that answers true or false without asking anything.
    pub(super) fn note_unquestioning_name(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(name) = self.function_name(function) else {
            return;
        };
        let text = self.text(name);
        if crate::vocabulary::asks_a_question(text) {
            return;
        }
        // The answer's own type, not a parameter's: both are annotations under the
        // function, and only the one after the parameter list says what comes back.
        let Some(list) =
            crate::tree::find_within(function, profile.parameter_list_kinds, profile.block_kinds)
                .into_iter()
                .next_back()
        else {
            return;
        };
        let body_at = self
            .function_body(function)
            .map_or(usize::MAX, |body| body.start_byte());
        let declared = descendants(function)
            .into_iter()
            .filter(|node| node.start_byte() >= list.end_byte() && node.start_byte() < body_at)
            .find(|node| {
                profile.type_annotation_kinds.contains(&kind_of(node))
                    || profile.is_identifier(*node)
                    || profile.binding_identifier_kinds.contains(&kind_of(node))
            })
            .map(|node| self.text(node))
            .or_else(|| {
                // A language that writes the answer before the name has it nowhere
                // after the parameters.
                function
                    .child_by_field_name("type")
                    .map(|node| self.text(node))
            })
            .unwrap_or("");
        let answers_boolean = profile
            .boolean_type_names
            .iter()
            .any(|boolean| declared.split_whitespace().any(|word| word == *boolean));
        if !answers_boolean {
            return;
        }
        self.out.unquestioning_names.push(Spot {
            span: span_of(name),
            detail: text.to_owned(),
            count: 1,
        });
    }

    /// A name that ends in the word for what it holds, next to a declaration that
    /// already says so, and a name whose number disagrees with its value.
    pub(super) fn note_named_shape(&mut self, node: Node<'a>) {
        let profile = self.profile;
        // A field's name wears a kind of its own in some grammars, and a field is a
        // declaration like any other.
        let names_something =
            profile.is_identifier(node) || profile.property_kinds.contains(&kind_of(node));
        if !names_something
            || !crate::tree::is_binding_position(
                node,
                &[],
                profile.borrowed_name_kinds,
                profile.local_declaring_kinds,
                profile.expression_list_kinds,
            )
        {
            return;
        }
        let name = self.text(node);
        // A capital opens a type or a component, whose name is its own word; the
        // rule is about a value that spells its type into its name.
        // A capital opens a type or a component, whose name is its own word; and a
        // function named for what it answers with is named for the answer, not for
        // the type of a value it holds.
        let holds_a_function = node.parent().is_some_and(|parent| {
            profile.is_function(parent)
                || parent
                    .child_by_field_name("value")
                    .is_some_and(|value| profile.is_function(self.see_through(value)))
        });
        let names_a_value =
            name.chars().next().is_some_and(char::is_lowercase) && !holds_a_function;
        if names_a_value
            && let Some(suffix) = profile
                .type_suffixes
                .iter()
                .find(|suffix| name.len() > suffix.len() && name.ends_with(*suffix))
        {
            self.out.typed_names.push(Spot {
                span: span_of(node),
                detail: format!("{name} ({suffix})"),
                count: 1,
            });
        }
        let holder = node.parent().filter(|parent| {
            parent.child_by_field_name("value").is_some()
                || parent.child_by_field_name("right").is_some()
        });
        // C++ wraps the name in a declarator of its own, so the value it was given is
        // a level further out.
        let holder = holder.or_else(|| node.parent().and_then(|parent| parent.parent()));
        let Some(mut value) = holder
            .and_then(|parent| {
                parent
                    .child_by_field_name("value")
                    .or_else(|| parent.child_by_field_name("right"))
            })
            .map(|value| self.see_through(value))
        else {
            return;
        };
        // A list of one is how some grammars spell every initialiser, and it is not
        // the collection this rule is about.
        while profile.expression_list_kinds.contains(&kind_of(value))
            && value.named_child_count() == 1
        {
            let Some(only) = value.named_child(0) else {
                break;
            };
            value = self.see_through(only);
        }
        if profile.expression_list_kinds.contains(&kind_of(value)) {
            return;
        }
        // Only a literal can settle this: a call may answer with anything, and
        // guessing from its name is how the rule would start being wrong.
        let kind = kind_of(value);
        let literal_many = kind.contains("array")
            || kind.contains("list")
            || kind.contains("set")
            || kind.contains("dictionary")
            || kind.contains("composite_literal");
        // Only this direction: a plural name holding a scalar is usually a counter,
        // and `openSessions = 0` says how many there are rather than what it is.
        let reads_many = plural(name);
        // An empty collection under a singular name is an accumulator waiting to be
        // filled; one written with things in it is already many.
        if literal_many && !reads_many && value.named_child_count() > 0 {
            self.out.plural_mismatches.push(Spot {
                span: span_of(node),
                detail: name.to_owned(),
                count: 1,
            });
        }
    }
}

pub(super) fn note_conventions(bindings: &[Binding], out: &mut Vec<(String, Span)>) {
    for binding in bindings {
        if let Some(convention) = convention_of(&binding.name) {
            out.push((convention.to_owned(), binding.span));
        }
    }
}

/// Locals declared below the file's own scope. A name assigned at the top of a module
/// is the module's, read from anywhere in it, however the language spells it.
fn locals_inside_a_body(scopes: &Scopes) -> impl Iterator<Item = &Binding> {
    scopes
        .of_kind(BindingKind::Local)
        .filter(|binding| scopes.depth(binding.scope) > 1)
}

/// A one-letter name is perfectly readable while its whole life fits on screen, and
/// stops being readable the moment it does not — so the scope's size decides this,
/// not the name's length alone.
pub(super) fn short_names(scopes: &Scopes, source: &str, scope_lines: u32) -> Vec<Spot> {
    let mut spots: Vec<Spot> = locals_inside_a_body(scopes)
        .filter(|binding| binding.name.len() <= 2 && !binding.name.starts_with('_'))
        .filter_map(|binding| {
            let last = binding.references.iter().map(|span| span.end).max()?;
            let lines = source
                .get(binding.span.start as usize..last as usize)
                .map_or(0, |text| text.lines().count() as u32);
            (lines >= scope_lines).then(|| Spot {
                span: binding.span,
                detail: binding.name.clone(),
                count: lines,
            })
        })
        .collect();
    spots.sort_by_key(|spot| spot.span.start);
    spots
}

/// A binding declared far above the line that first reads it makes a reader hold it
/// in mind across everything in between. Only locals count: a module constant read at
/// the bottom of the file is where it belongs.
pub(super) fn distant_declarations(
    profile: &LanguageProfile,
    root: Node<'_>,
    scopes: &Scopes,
    source: &str,
    distance: u32,
) -> Vec<Spot> {
    let mut spots: Vec<Spot> = locals_inside_a_body(scopes)
        .filter(|binding| !binding.name.starts_with('_'))
        .filter_map(|binding| {
            let declared = declaration_end(profile, root, binding.span);
            let first = binding
                .references
                .iter()
                .map(|span| span.start)
                .filter(|start| *start > declared)
                .min()?;
            let lines = source
                .get(declared as usize..first as usize)
                .map_or(0, |text| text.lines().count() as u32);
            (lines > distance).then(|| Spot {
                span: binding.span,
                detail: binding.name.clone(),
                count: lines,
            })
        })
        .collect();
    spots.sort_by_key(|spot| spot.span.start);
    spots
}

/// Where the declaration of a name ends. A table or an object written across sixty
/// lines is read while it is declared, so the distance to the first use starts after
/// it. A name bound by a loop or a `with` has no declaration of its own to end.
fn declaration_end(profile: &LanguageProfile, root: Node<'_>, name: Span) -> u32 {
    let mut current = root.descendant_for_byte_range(name.start as usize, name.end as usize);
    for _ in 0..4 {
        let Some(node) = current else {
            break;
        };
        let kind = kind_of(node);
        if profile.declarator_kinds.contains(&kind) || profile.local_declaring_kinds.contains(&kind)
        {
            return node.end_byte() as u32;
        }
        current = node.parent();
    }
    name.end
}

/// Whether a name reads as more than one thing. English only, and only the endings
/// that are unambiguous: `status` is not a plural and neither is `data`.
fn plural(name: &str) -> bool {
    const NOT_PLURAL: [&str; 12] = [
        "status", "address", "class", "process", "access", "success", "business", "less", "cross",
        "press", "pass", "gas",
    ];
    let words = split_words(name);
    let Some(word) = words.last().map(String::as_str) else {
        return false;
    };
    if NOT_PLURAL.iter().any(|exception| word.ends_with(exception)) {
        return false;
    }
    word.len() > 2 && word.ends_with('s') && !word.ends_with("ss") && !word.ends_with("us")
}

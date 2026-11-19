//! Rules about parameters, locals and the values written to them.

use std::collections::{HashMap, HashSet};

use rqx_core::facts::Spot;
use rqx_core::rule::RuleId;
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::scope::{BindingKind, Scopes};
use crate::tree::{descendants, find_within, kind_of, span_of};

use super::Walker;

impl<'a> Walker<'a> {
    pub(super) fn note_flag_parameters(&mut self, function: Node<'a>) {
        if self.function_body(function).is_none() {
            return;
        }
        let Some(list) = crate::parameters::input_list(self.profile, function) else {
            return;
        };
        let mut cursor = list.walk();
        let parameters: Vec<Node<'a>> = list
            .named_children(&mut cursor)
            .filter(|child| self.profile.parameter_kinds.contains(&kind_of(child)))
            .collect();
        for (index, parameter) in parameters.iter().enumerate() {
            let declared = self.declared_type(*parameter);
            if !declared.is_some_and(|text| self.profile.boolean_type_names.contains(&text)) {
                continue;
            }
            let Some(name) = descendants(*parameter)
                .into_iter()
                .find(|child| self.profile.is_identifier(*child))
            else {
                continue;
            };
            self.out.flag_parameters.push(Spot {
                span: span_of(name),
                detail: self.text(name).to_owned(),
                count: index as u32 + 1,
            });
        }
    }

    /// A function that takes arguments only to hand them on unchanged. The caller
    /// could have called the thing itself.
    pub(super) fn note_passthrough(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        let mut cursor = holder.walk();
        let mut statements = holder
            .named_children(&mut cursor)
            .filter(|child| !profile.is_comment(*child));
        let Some(only) = statements.next() else {
            return;
        };
        if statements.next().is_some() {
            return;
        }
        let Some(call) = self.only_call(only) else {
            return;
        };
        let parameters = self.parameter_names(function);
        if parameters.is_empty() || self.argument_names(call) != parameters {
            return;
        }
        let Some(name) = self.function_name(function) else {
            return;
        };
        let Some(target) = self.called_name(call) else {
            return;
        };
        self.out.passthrough_functions.push(Spot {
            span: span_of(function),
            detail: format!("{} to {target}", self.text(name)),
            count: 1,
        });
        self.out.focus(
            RuleId::PassthroughFunction,
            span_of(function),
            span_of(name),
        );
    }

    fn parameter_names(&self, function: Node<'a>) -> Vec<String> {
        let profile = self.profile;
        let Some(list) = find_within(function, profile.parameter_list_kinds, profile.block_kinds)
            .into_iter()
            .next_back()
        else {
            return Vec::new();
        };
        list.named_children(&mut list.walk())
            .filter(|child| {
                profile.parameter_kinds.contains(&kind_of(child)) || profile.is_identifier(*child)
            })
            .filter_map(|parameter| {
                if profile.is_identifier(parameter) {
                    return Some(self.text(parameter).to_owned());
                }
                profile
                    .binding_fields
                    .iter()
                    .find_map(|field| parameter.child_by_field_name(field))
                    .filter(|node| profile.is_identifier(*node))
                    .or_else(|| {
                        descendants(parameter)
                            .into_iter()
                            .find(|child| profile.is_identifier(*child))
                    })
                    .map(|node| self.text(node).to_owned())
            })
            .collect()
    }

    /// The argument list, but only when every argument is a bare name. Anything
    /// computed on the way through means the function is doing something.
    fn argument_names(&self, call: Node<'a>) -> Vec<String> {
        let profile = self.profile;
        let Some(list) = call
            .named_children(&mut call.walk())
            .find(|child| profile.argument_list_kinds.contains(&kind_of(child)))
        else {
            return Vec::new();
        };
        list.named_children(&mut list.walk())
            .map(|argument| {
                if profile.is_identifier(argument) {
                    self.text(argument).to_owned()
                } else {
                    String::new()
                }
            })
            .collect()
    }

    /// A literal `true` at a call site says nothing about what it turns on; the
    /// reader has to open the callee to find out.
    pub(super) fn note_boolean_argument(&mut self, call: Node<'a>) {
        let profile = self.profile;
        if self.test_depth > 0 {
            return;
        }
        let Some(list) = call
            .named_children(&mut call.walk())
            .find(|child| profile.argument_list_kinds.contains(&kind_of(child)))
        else {
            return;
        };
        let arguments: Vec<Node<'a>> = list.named_children(&mut list.walk()).collect();
        // One argument reads as the whole of what the call does; it is the flag among
        // others that nobody can name from the call site.
        if arguments.len() < 2 {
            return;
        }
        let Some(flag) = arguments
            .into_iter()
            .find(|argument| profile.boolean_literals.contains(&self.text(*argument)))
        else {
            return;
        };
        self.out.boolean_arguments.push(Spot {
            span: span_of(flag),
            detail: self
                .called_name(call)
                .unwrap_or_else(|| "the call".to_owned()),
            count: 1,
        });
    }

    /// How many names one body asks a reader to hold at once.
    pub(super) fn note_many_locals(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let mut declared: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut cursor = body.walk();
        let mut stack: Vec<Node<'a>> = body.children(&mut cursor).collect();
        while let Some(node) = stack.pop() {
            // A callback's own names live and die inside it.
            if profile.is_function(node) {
                continue;
            }
            if profile.is_identifier(node)
                && crate::tree::is_binding_position(
                    node,
                    profile.parameter_kinds,
                    profile.borrowed_name_kinds,
                    profile.local_declaring_kinds,
                    profile.expression_list_kinds,
                )
            {
                declared.insert(self.text(node));
            }
            let mut inner = node.walk();
            stack.extend(node.children(&mut inner));
        }
        let held = declared.len() as u32;
        if held > self.limits.function_locals {
            self.out.crowded_functions.push(Spot {
                span: self.name_span(function),
                detail: held.to_string(),
                count: held,
            });
        }
    }

    /// A return that hands back several things at once, and a signature that
    /// declares it does.
    pub(super) fn note_many_results(&mut self, returns: Node<'a>) {
        let profile = self.profile;
        let Some(value) = returns.named_child(0).map(|node| self.see_through(node)) else {
            return;
        };
        // A list of one is how some grammars spell every return; a list of several is
        // the function answering with several things.
        let several = profile.expression_list_kinds.contains(&kind_of(value))
            || matches!(kind_of(value), "expression_list" | "tuple")
            || kind_of(value).contains("array");
        // A list of records is a collection the caller walks; a run of plain values
        // is several answers the caller has to know the order of.
        let positional = value.named_children(&mut value.walk()).all(|element| {
            let element = self.see_through(element);
            profile.is_identifier(element)
                || profile.is_call(element)
                || profile.is_member_access(element)
                || profile.number_kinds.contains(&kind_of(element))
                || profile.string_kinds.contains(&kind_of(element))
        });
        let handed = if several && positional {
            value.named_child_count() as u32
        } else {
            1
        };
        if handed > self.limits.max_results {
            self.out.many_results.push(Spot {
                span: span_of(returns),
                detail: handed.to_string(),
                count: handed,
            });
        }
    }

    /// A value written and written again with nothing reading the first one.
    pub(super) fn note_stores(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        // A callback can read the outer name at any time, so a second write is not
        // dead just because no line between the two mentions it.
        if descendants(body)
            .into_iter()
            .any(|node| profile.is_function(node))
        {
            return;
        }
        // A package, module or class level name can be read by anything the function
        // calls between the two writes, so only the function's own names are judged.
        let parameters: HashSet<&str> = descendants(function)
            .into_iter()
            .filter(|node| node.start_byte() < body.start_byte() && profile.is_identifier(*node))
            .map(|node| self.text(node))
            .collect();
        for (name, mentions) in self.touches(body) {
            if name.starts_with('_') {
                continue;
            }
            let reassigned = mentions.iter().filter(|touch| touch.wrote).count();
            let read = mentions.iter().filter(|touch| touch.read).count();
            let local = parameters.contains(name) || mentions.iter().any(|touch| touch.declared);
            if local
                && let Some(window) = mentions.windows(2).find(|pair| {
                    pair[0].stores
                        && !pair[0].read
                        && pair[1].stores
                        && !pair[1].read
                        && pair[0].block == pair[1].block
                })
            {
                self.out.dead_stores.push(Spot {
                    span: Span::new(
                        window[0].start as u32,
                        window[0].start as u32 + name.len() as u32,
                    ),
                    detail: name.to_owned(),
                    count: reassigned as u32,
                });
            }
            // A name written once and read after is a constant the language was not
            // told about — but only where the language has a word for that.
            if reassigned == 1
                && read > 0
                && profile.locals_can_be_fixed
                && let Some(first) = mentions.first()
                && first.declared
            {
                let opens = self.source[..first.at]
                    .rsplit('\n')
                    .next()
                    .unwrap_or_default()
                    .trim_start();
                // Split on punctuation as well as spaces: in `for (final String row :`
                // and `for (const auto& row :` the marker follows a parenthesis.
                let fixed = profile.immutable_markers.iter().any(|marker| {
                    opens
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .any(|word| word == *marker)
                });
                if !fixed {
                    self.out.never_reassigned.push(Spot {
                        span: Span::new(first.at as u32, first.at as u32 + name.len() as u32),
                        detail: name.to_owned(),
                        count: 1,
                    });
                }
            }
        }
    }
}

/// The assignments whose target is a parameter of a function around it, one per
/// parameter. A name is matched through the scopes rather than by spelling: a local
/// that happens to share its name with a parameter elsewhere in the file is not one,
/// and neither is a declaration that introduces a new name. A receiver (Go's, or
/// Python's `self`) or a Go named result is not an input, and filling a named result
/// is how a Go function answers.
pub(super) fn reassigned_parameters(
    profile: &LanguageProfile,
    root: Node<'_>,
    scopes: &Scopes,
    assigned: Vec<Spot>,
) -> Vec<Spot> {
    let mut parameter_of: HashMap<(u32, u32), usize> = HashMap::new();
    for (index, binding) in scopes.bindings.iter().enumerate() {
        if binding.kind == BindingKind::Parameter
            && !profile.self_names.contains(&binding.name.as_str())
            && crate::parameters::is_input_parameter(profile, root, binding.span)
        {
            for reference in &binding.references {
                parameter_of.insert((reference.start, reference.end), index);
            }
        }
    }
    let mut reported = HashSet::new();
    assigned
        .into_iter()
        .filter(|spot| {
            parameter_of
                .get(&(spot.span.start, spot.span.end))
                .is_some_and(|index| reported.insert(*index))
        })
        .collect()
}

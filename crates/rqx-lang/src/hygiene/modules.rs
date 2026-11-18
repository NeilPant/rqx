//! Rules about file-level state, imports and work done at import time.

use std::collections::{HashMap, HashSet};

use rqx_core::facts::{Hygiene, Spot};
use rqx_core::rule::RuleId;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{Nodes, descendants, kind_of, span_of, text_of};

use super::{Walker, squashed};

impl<'a> Walker<'a> {
    pub(super) fn note_wildcard_import(&mut self, node: Node<'a>) {
        let Some((_, required)) = self
            .profile
            .wildcard_import_kinds
            .iter()
            .find(|(kind, _)| *kind == kind_of(node))
        else {
            return;
        };
        let statement = node.parent().unwrap_or(node);
        let text = self.text(statement);
        if !required.is_empty() && !text.contains(required) {
            return;
        }
        self.out.wildcard_imports.push(Spot {
            span: span_of(statement),
            detail: self.snippet(statement),
            count: 1,
        });
    }

    /// A binding at file level that anything in the file can write to. Every function
    /// that reads it has an argument nobody passed.
    pub(super) fn note_module_state(&mut self, root: Node<'a>) {
        let profile = self.profile;
        let mut holders = vec![root];
        // A language that has no file-level binding spells the same thing as a static
        // member, so the class bodies are read too.
        holders.extend(
            self.nodes
                .under(root)
                .iter()
                .copied()
                .filter(|node| profile.is_class(*node))
                .filter_map(|class| self.class_body(class)),
        );
        for holder in holders {
            self.note_shared_bindings(holder, holder.id() != root.id());
        }
    }

    fn note_shared_bindings(&mut self, root: Node<'a>, members_only: bool) {
        let profile = self.profile;
        for statement in root.named_children(&mut root.walk()) {
            // `export let x = 1` is a declaration wearing a wrapper.
            let statement = if profile.export_kinds.contains(&kind_of(statement)) {
                statement.named_child(0).unwrap_or(statement)
            } else {
                statement
            };
            // Inside a type, only what belongs to the type rather than to an instance
            // is shared by the whole program.
            if members_only && !self.is_static(statement) {
                continue;
            }
            let declares = profile
                .variable_declaration_kinds
                .contains(&kind_of(statement))
                || profile.local_declaring_kinds.contains(&kind_of(statement))
                || profile.field_kinds.contains(&kind_of(statement));
            if !declares {
                continue;
            }
            // The keyword can sit behind other modifiers, so the words before the
            // type are all read rather than only the first.
            let declared = self.text(statement);
            let head = declared.split('=').next().unwrap_or(declared);
            if head
                .split_whitespace()
                .any(|word| profile.immutable_markers.contains(&word))
            {
                continue;
            }
            // A file-level statement that declares a function is not a binding, and
            // in C++ a bare prototype wears the same node kind as one.
            let inner = descendants(statement);
            if inner.iter().any(|node| {
                profile.is_function(*node)
                    || profile.declarator_wrapper_kinds.contains(&kind_of(node))
                    || profile.is_class(*node)
            }) {
                continue;
            }
            let Some(name) = inner.into_iter().find(|node| {
                profile.is_identifier(*node)
                    && crate::tree::is_binding_position(
                        *node,
                        &[],
                        profile.borrowed_name_kinds,
                        profile.local_declaring_kinds,
                        profile.expression_list_kinds,
                    )
            }) else {
                continue;
            };
            let name = self.text(name);
            // A name in capitals is the convention a language without a keyword uses
            // to say the same thing.
            let spelled_constant = name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if spelled_constant || name.starts_with('_') {
                continue;
            }
            self.out.module_state.push(Spot {
                span: span_of(statement),
                detail: name.to_owned(),
                count: 1,
            });
        }
    }

    /// A statement that runs work the moment the file is loaded, before any caller
    /// has asked for anything.
    pub(super) fn note_import_time_work(&mut self, root: Node<'a>) {
        let profile = self.profile;
        let functions: HashSet<&str> = self
            .nodes
            .iter()
            .filter(|node| profile.is_function(*node))
            .filter_map(|node| self.function_name(node))
            .map(|name| self.text(name))
            .collect();
        for statement in root.named_children(&mut root.walk()) {
            if kind_of(statement) != "expression_statement" {
                continue;
            }
            let Some(call) = statement
                .named_child(0)
                .map(|value| self.see_through(value))
                .filter(|value| profile.is_call(*value))
            else {
                continue;
            };
            let Some(name) = self.called_name(call) else {
                continue;
            };
            // A test file is a list of registrations, and registering is the whole of
            // what loading it is for.
            if profile.test.is_test(&name)
                || profile.test.is_assertion(&name)
                || self.registers(call, &functions)
            {
                continue;
            }
            self.out.import_time_work.push(Spot {
                span: span_of(statement),
                detail: name,
                count: 1,
            });
            if let Some(callee) = self.callee_span(call) {
                self.out
                    .focus(RuleId::ImportTimeWork, span_of(statement), callee);
            }
        }
    }

    /// A call that hands over something to be run later: a route, a listener, a
    /// command. The work happens when the handler is called, not when the file loads.
    /// A handler is a function written in place or a function this file declares. A
    /// method spelled like a registration (`router.use(auth)`,
    /// `router.get("/health", healthHandler())`) registers whatever it is handed, so
    /// any argument other than a plain literal counts.
    fn registers(&self, call: Node<'a>, functions: &HashSet<&str>) -> bool {
        const REGISTRATIONS: [&str; 17] = [
            "use",
            "get",
            "post",
            "put",
            "patch",
            "delete",
            "head",
            "options",
            "all",
            "route",
            "on",
            "once",
            "addListener",
            "command",
            "register",
            "subscribe",
            "handle",
        ];
        let profile = self.profile;
        let Some(arguments) = call
            .named_children(&mut call.walk())
            .find(|child| profile.argument_list_kinds.contains(&kind_of(child)))
        else {
            return false;
        };
        let registering_method = call
            .child_by_field_name(profile.callee_field)
            .is_some_and(|callee| profile.is_member_access(callee))
            && profile
                .call_tail(call, self.source)
                .is_some_and(|tail| REGISTRATIONS.contains(&tail));
        arguments
            .named_children(&mut arguments.walk())
            .map(|argument| self.see_through(argument))
            .any(|argument| {
                let named = profile.is_identifier(argument);
                profile.is_function(argument)
                    || (named && functions.contains(self.text(argument)))
                    || (registering_method && !self.is_plain_literal(argument))
            })
    }

    fn is_plain_literal(&self, node: Node<'a>) -> bool {
        let kind = kind_of(node);
        self.profile.string_kinds.contains(&kind)
            || self.profile.number_kinds.contains(&kind)
            || matches!(
                self.text(node),
                "true" | "false" | "null" | "undefined" | "None" | "True" | "False" | "nil"
            )
    }

    /// Reaching for the clock, the environment or a random source from inside a body
    /// nobody handed one to.
    pub(super) fn note_ambient_dependency(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(dotted) = self
            .callee_name(node)
            .or_else(|| self.called_name(node))
            .or_else(|| {
                profile
                    .qualified_name_kinds
                    .contains(&kind_of(node))
                    .then(|| self.text(node).replace("::", "."))
            })
        else {
            return;
        };
        if !profile.ambient_names.contains(&dotted.as_str()) || self.test_depth > 0 {
            return;
        }
        // The callee and the call both spell the same reach; the call is the one a
        // reader points at.
        if node
            .parent()
            .is_some_and(|parent| profile.is_call(parent) && !profile.is_call(node))
        {
            return;
        }
        // Only inside a body: a file-level constant reading the environment once is
        // configuration, and every caller can see it happen. A parameter's default
        // is the injection the rule asks for, with the ambient source as the fallback.
        if !self.inside_body(node) || self.inside_parameter(node) {
            return;
        }
        self.out.ambient_dependencies.push(Spot {
            span: span_of(node),
            detail: dotted,
            count: 1,
        });
        if let Some(callee) = self.callee_span(node) {
            self.out
                .focus(RuleId::AmbientDependency, span_of(node), callee);
        }
    }

    fn inside_body(&self, node: Node<'a>) -> bool {
        let mut parent = node.parent();
        while let Some(current) = parent {
            if self.profile.is_function(current) {
                return true;
            }
            parent = current.parent();
        }
        false
    }

    /// Whether the node is part of a parameter's declaration, which in practice means
    /// its default value: `(clock = () => Date.now())` is the fallback as much as
    /// `(now = Date.now)` is.
    fn inside_parameter(&self, node: Node<'a>) -> bool {
        let profile = self.profile;
        let mut parent = node.parent();
        while let Some(current) = parent {
            if profile.parameter_kinds.contains(&kind_of(current))
                || profile.parameter_list_kinds.contains(&kind_of(current))
            {
                return true;
            }
            parent = current.parent();
        }
        false
    }
}

/// Two functions in one file written the same way. One of them was copied, and the
/// next change has to find both.
pub(super) fn note_twin_functions(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    out: &mut Hygiene,
) {
    const WORTH_SAYING: usize = 3;
    let mut seen: HashMap<String, &str> = HashMap::new();
    for function in nodes.iter().filter(|node| profile.is_function(*node)) {
        let Some(body) = function.child_by_field_name("body") else {
            continue;
        };
        let statements = descendants(body)
            .into_iter()
            .filter(|node| profile.is_statement(*node))
            .count();
        if statements < WORTH_SAYING {
            continue;
        }
        // C++ keeps the name under a declarator, so the last name written before the
        // body is the one.
        let Some(name) = function
            .child_by_field_name("name")
            .or_else(|| {
                descendants(function).into_iter().rfind(|node| {
                    node.start_byte() < body.start_byte() && profile.is_identifier(*node)
                })
            })
            .map(|node| text_of(node, source))
        else {
            continue;
        };
        let shape = squashed(text_of(body, source));
        match seen.get(&shape) {
            Some(first) => out.duplicate_functions.push(Spot {
                span: span_of(function),
                detail: format!("{name} and {first}"),
                count: 2,
            }),
            None => {
                seen.insert(shape, name);
            }
        }
    }
}

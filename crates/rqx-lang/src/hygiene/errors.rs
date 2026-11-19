//! Rules about failure handling: catches, throws, exits and discarded errors.

use rqx_core::facts::Spot;
use rqx_core::rule::RuleId;
use tree_sitter::Node;

use crate::tree::{descendants, kind_of, span_of};

use super::Walker;

impl<'a> Walker<'a> {
    /// A comparison that has already decided: an operand against itself, or against
    /// the literal it is being asked whether it equals.
    /// A statement whose whole content is a value nobody keeps. Deliberately narrow:
    /// a call may be there for its effect and a string may be a docstring or a
    /// `"use strict"` directive, so only a number, a name, or a comparison counts.
    pub(super) fn note_discarded_expression(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let mut cursor = node.walk();
        let children: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
        let [only] = children.as_slice() else {
            return;
        };
        let kind = kind_of(only);
        let discarded = profile.number_kinds.contains(&kind)
            || profile.is_identifier(*only)
            || (profile.binary_kinds.contains(&kind) && self.compares(*only));
        if !discarded {
            return;
        }
        self.out.discarded_expressions.push(Spot {
            span: span_of(node),
            detail: self.text(*only).trim().to_owned(),
            count: 1,
        });
    }

    /// Whether a binary node is an equality test. Python writes one as a run of
    /// operands with the operator between them rather than as a left/right pair, so
    /// the field is absent there and the operator is read positionally.
    fn compares(&self, node: Node<'a>) -> bool {
        node.child_by_field_name("operator")
            .or_else(|| {
                (0..node.child_count())
                    .filter_map(|index| node.child(index))
                    .find(|child| !child.is_named())
            })
            .is_some_and(|operator| {
                self.profile
                    .equality_operators
                    .contains(&self.text(operator))
            })
    }

    /// A catch that catches only to raise the same thing again. The handler reads as
    /// a decision and makes none.
    pub(super) fn note_rethrow_only_catch(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(body) = node
            .child_by_field_name("body")
            .or_else(|| self.function_body(node))
        else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        let mut cursor = holder.walk();
        let mut statements = holder.named_children(&mut cursor);
        let Some(only) = statements.next() else {
            return;
        };
        if statements.next().is_some() || !profile.throw_kinds.contains(&kind_of(only)) {
            return;
        }
        // Throwing something built here is translation, not a rethrow: the handler
        // added what this level knows. Only handing the same value back is empty.
        let thrown = only.named_child(0).map(|node| self.see_through(node));
        let caught = self.caught_name(node);
        let bare = only.named_child_count() == 0;
        if !bare
            && !thrown.is_some_and(|value| {
                profile.is_identifier(value) && caught.as_deref() == Some(self.text(value))
            })
        {
            return;
        }
        self.out.rethrow_only_catches.push(Spot {
            span: span_of(node),
            detail: self.snippet(only),
            count: 1,
        });
    }

    /// The name a handler binds the failure to, where it binds one.
    fn caught_name(&self, catch: Node<'a>) -> Option<String> {
        let profile = self.profile;
        let bound = catch
            .child_by_field_name("parameter")
            .or_else(|| catch.child_by_field_name("name"))
            .or_else(|| {
                catch
                    .named_children(&mut catch.walk())
                    .find(|child| !profile.is_block(*child))
            })?;
        if profile.is_identifier(bound) {
            return Some(self.text(bound).to_owned());
        }
        descendants(bound)
            .into_iter()
            .rfind(|node| profile.is_identifier(*node))
            .map(|node| self.text(node).to_owned())
    }

    /// A body whose whole content is a refusal saying the work is still to come.
    pub(super) fn note_placeholder_body(&mut self, function: Node<'a>) {
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
        if statements.next().is_some() || !profile.throw_kinds.contains(&kind_of(only)) {
            return;
        }
        let Some(marker) = crate::vocabulary::unfinished_marker(self.text(only)) else {
            return;
        };
        let Some(name) = self.function_name(function) else {
            return;
        };
        let name = self.text(name).to_owned();
        self.out.placeholder_bodies.push(Spot {
            span: span_of(function),
            detail: format!("{name}: {marker}"),
            count: 1,
        });
    }

    /// A jump out of the clause that runs on the way out. It discards whatever the
    /// block was leaving with, including a failure nobody will ever see.
    pub(super) fn note_exit_from_finally(&mut self, clause: Node<'a>) {
        let profile = self.profile;
        // A `return` inside a callback leaves the callback, not the clause.
        for node in crate::tree::descendants_outside(clause, |node| profile.is_function(node)) {
            // Python chains a failure raised here to the one in flight, which stays
            // in `__context__` and in the traceback, so only a jump discards it.
            let chains = profile.language == rqx_core::Language::Python;
            let leaves = match kind_of(node) {
                "return_statement" => true,
                "break_statement" | "continue_statement" => !self.jumps_within(node, clause),
                kind => !chains && profile.throw_kinds.contains(&kind),
            };
            if !leaves {
                continue;
            }
            self.out.exits_from_finally.push(Spot {
                span: span_of(node),
                detail: kind_of(node).trim_end_matches("_statement").to_owned(),
                count: 1,
            });
            return;
        }
    }

    /// Whether a `break` or `continue` lands on a loop, switch or label that is itself
    /// inside the clause, so control stays in the clause.
    fn jumps_within(&self, jump: Node<'a>, clause: Node<'a>) -> bool {
        let profile = self.profile;
        let label = jump.named_child(0).map(|label| self.text(label));
        let mut current = jump.parent();
        while let Some(node) = current {
            if node.id() == clause.id() {
                return false;
            }
            let kind = kind_of(node);
            let lands = match label {
                Some(label) => {
                    kind == "labeled_statement"
                        && node
                            .named_child(0)
                            .is_some_and(|named| self.text(named) == label)
                }
                // Python's `match` takes no `break`; one inside it leaves the loop.
                None => {
                    profile.loop_kinds.contains(&kind)
                        || (kind_of(jump) == "break_statement"
                            && profile.switch_kinds.contains(&kind)
                            && kind != "match_statement")
                }
            };
            if lands {
                return true;
            }
            current = node.parent();
        }
        false
    }

    /// A handler that names the root of the error hierarchy, or names nothing, takes
    /// every failure including the ones it was never written for.
    pub(super) fn note_catch_all(&mut self, catch: Node<'a>) {
        let profile = self.profile;
        let Some(body) = catch.child_by_field_name("body").or_else(|| {
            catch
                .named_children(&mut catch.walk())
                .find(|child| profile.is_block(*child))
        }) else {
            return;
        };
        let header: Vec<Node<'a>> = descendants(catch)
            .into_iter()
            .filter(|node| node.start_byte() < body.start_byte())
            .collect();
        // A language that spells the type separately from the binding puts it in an
        // annotation, and the annotation is the half that says what is taken.
        let annotated = header
            .iter()
            .find(|node| profile.type_annotation_kinds.contains(&kind_of(*node)))
            .and_then(|node| node.named_child(0));
        let named = annotated.or_else(|| {
            header
                .iter()
                .copied()
                .find(|node| {
                    profile.is_identifier(*node)
                        || profile.binding_identifier_kinds.contains(&kind_of(node))
                        || profile.qualified_name_kinds.contains(&kind_of(node))
                })
                .filter(|_| profile.catch_names_a_type)
        });
        let caught = named.map(|node| self.text(node));
        let takes_everything = match caught {
            Some(name) => profile.catch_all_types.contains(&name),
            // Naming nothing is a catch-all only where the language offered a name.
            None => profile.catch_names_a_type,
        };
        if !takes_everything {
            return;
        }
        self.out.catch_all_handlers.push(Spot {
            span: span_of(catch),
            detail: caught.unwrap_or("everything").to_owned(),
            count: 1,
        });
    }

    /// Raising the root type tells a caller that something failed and nothing else,
    /// so the only way to react to it is to catch everything.
    pub(super) fn note_generic_throw(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(raised) = node.named_child(0) else {
            return;
        };
        let raised = self.see_through(raised);
        let Some(name) = self
            .constructed_name(raised)
            .or_else(|| self.called_name(raised))
            .or_else(|| {
                profile
                    .is_identifier(raised)
                    .then(|| self.text(raised).to_owned())
            })
        else {
            return;
        };
        if !profile.base_error_types.contains(&name.as_str()) {
            return;
        }
        self.out.generic_throws.push(Spot {
            span: span_of(node),
            detail: name,
            count: 1,
        });
    }

    /// Ending the process from somewhere that is not the entry point takes the
    /// decision away from every caller above it.
    pub(super) fn note_library_exit(&mut self, call: Node<'a>) {
        let profile = self.profile;
        let Some(name) = self.called_name(call) else {
            return;
        };
        if !profile.exit_calls.contains(&name.as_str()) || self.inside_entry_point(call) {
            return;
        }
        self.out.library_exits.push(Spot {
            span: span_of(call),
            detail: name,
            count: 1,
        });
    }

    fn inside_entry_point(&self, node: Node<'a>) -> bool {
        let profile = self.profile;
        let mut parent = node.parent();
        while let Some(current) = parent {
            if profile.is_function(current)
                && let Some(name) = self.function_name(current)
            {
                return self.text(name).eq_ignore_ascii_case("main");
            }
            parent = current.parent();
        }
        false
    }

    /// A guarded block big enough that no single line in it is the one being
    /// guarded.
    pub(super) fn note_wide_try(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(guarded) = node
            .named_children(&mut node.walk())
            .find(|child| profile.is_block(*child))
        else {
            return;
        };
        let holder = guarded
            .named_children(&mut guarded.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(guarded);
        let statements = holder
            .named_children(&mut holder.walk())
            .filter(|child| profile.is_statement(*child))
            .count() as u32;
        if statements > self.limits.try_statements {
            self.out.wide_tries.push(Spot {
                span: span_of(guarded),
                detail: statements.to_string(),
                count: statements,
            });
        }
    }

    /// A guarded block holding nearly the whole body: the handler is not answering a
    /// failure, it is a net under the function.
    pub(super) fn note_blanket_try(&mut self, node: Node<'a>) {
        // What a function is allowed to do outside the guard and still be reading as
        // one line guarded rather than a body inside a net.
        const OUTSIDE_ALLOWANCE: u32 = 1;
        const WORTH_SAYING: u32 = 3;
        let profile = self.profile;
        let Some(function) = self.enclosing_function(node) else {
            return;
        };
        let Some(guarded) = node
            .named_children(&mut node.walk())
            .find(|child| profile.is_block(*child))
        else {
            return;
        };
        let inside = self.statements_in(guarded);
        let whole = self
            .function_body(function)
            .map_or(0, |body| self.statements_in(body));
        let outside = whole.saturating_sub(inside + 1);
        if inside < WORTH_SAYING || outside > OUTSIDE_ALLOWANCE {
            return;
        }
        self.out.blanket_tries.push(Spot {
            span: span_of(guarded),
            detail: format!("{inside} of {whole}"),
            count: inside,
        });
    }

    fn statements_in(&self, holder: Node<'a>) -> u32 {
        descendants(holder)
            .into_iter()
            .filter(|node| self.profile.is_statement(*node))
            .count() as u32
    }

    /// A check that the language is free to compile away, standing where a real one
    /// was meant to be.
    pub(super) fn note_production_assert(&mut self, node: Node<'a>) {
        if self.test_depth > 0 {
            return;
        }
        let profile = self.profile;
        let is_assert = profile.assertion_statement_kinds.contains(&kind_of(node))
            || (profile.is_call(node)
                && self
                    .called_name(node)
                    .is_some_and(|name| name == "assert" || name.ends_with(".assert")));
        if !is_assert {
            return;
        }
        self.out.production_asserts.push(Spot {
            span: span_of(node),
            detail: self.snippet(node),
            count: 1,
        });
    }

    /// A handler written below one that already takes every failure it names. The
    /// second can never run. Only types whose ancestry is known count: the standard
    /// library's, and classes this file declares.
    pub(super) fn note_shadowed_handler(&mut self, guarded: Node<'a>) {
        let profile = self.profile;
        if !RuleId::ShadowedHandler.applies_to(profile.language) {
            return;
        }
        let mut cursor = guarded.walk();
        let handlers: Vec<Node<'a>> = guarded
            .named_children(&mut cursor)
            .filter(|child| profile.catch_kinds.contains(&kind_of(child)))
            .collect();
        for (index, later) in handlers.iter().enumerate() {
            let Some(narrow) = self.handled_types(*later) else {
                continue;
            };
            let Some(wide) = handlers[..index].iter().find(|earlier| {
                self.handled_types(**earlier)
                    .is_some_and(|wide| self.takes_all_of(&wide, &narrow))
            }) else {
                continue;
            };
            self.out.shadowed_handlers.push(Spot {
                span: span_of(*wide),
                detail: self.snippet(*wide),
                count: 1,
            });
            return;
        }
    }

    /// The error types a handler names, or nothing for one that names none and so
    /// takes everything: Python's bare `except:`, C++'s `catch (...)`.
    fn handled_types(&self, catch: Node<'a>) -> Option<Vec<String>> {
        let profile = self.profile;
        let named = catch
            .named_children(&mut catch.walk())
            .find(|child| !profile.is_block(*child) && !profile.is_comment(*child))?;
        let named = match kind_of(named) {
            "as_pattern" => named.named_child(0)?,
            "parameter_list" => return self.parameter_type(named).map(|name| vec![name]),
            "catch_formal_parameter" => named
                .named_children(&mut named.walk())
                .find(|child| kind_of(*child) == "catch_type")?,
            _ => named,
        };
        let listed = if matches!(kind_of(named), "tuple" | "catch_type") {
            named.named_children(&mut named.walk()).collect()
        } else {
            vec![named]
        };
        Some(
            listed
                .into_iter()
                .map(|node| self.error_type_name(node))
                .collect(),
        )
    }

    /// The type a C++ handler's parameter declares, marked when it is taken by
    /// pointer, since a pointer handler never takes what a reference one does.
    fn parameter_type(&self, parameters: Node<'a>) -> Option<String> {
        let parameter = parameters.named_child(0)?;
        let written = parameter.child_by_field_name("type")?;
        let by_pointer = descendants(parameter)
            .into_iter()
            .any(|node| kind_of(node) == "pointer_declarator");
        let suffix = if by_pointer { "*" } else { "" };
        Some(format!("{}{suffix}", self.error_type_name(written)))
    }

    /// A type name as the error tables spell it, whichever prefix the code wrote.
    fn error_type_name(&self, node: Node<'a>) -> String {
        error_type_spelling(self.text(node))
    }

    /// Whether a handler takes every failure or the root of the error hierarchy:
    /// a bare `except:`, `catch (...)`, `catch (Exception e)`, and any JavaScript
    /// handler, which cannot name a type at all.
    pub(super) fn takes_any_failure(&self, catch: Node<'a>) -> bool {
        let profile = self.profile;
        if !profile.catch_names_a_type {
            return true;
        }
        self.handled_types(catch).is_none_or(|types| {
            types.iter().any(|taken| {
                profile
                    .catch_all_types
                    .iter()
                    .any(|root| error_type_spelling(root) == *taken)
            })
        })
    }

    /// Whether every type in `narrow` is one of `wide`, or derives from one of them.
    fn takes_all_of(&self, wide: &[String], narrow: &[String]) -> bool {
        !narrow.is_empty()
            && narrow.iter().all(|taken| {
                self.error_ancestry(taken)
                    .iter()
                    .any(|ancestor| wide.contains(ancestor))
            })
    }

    /// A type and everything it is known to derive from, through the language's own
    /// error types and the classes this file declares.
    fn error_ancestry(&self, name: &str) -> Vec<String> {
        let mut ancestry = vec![name.to_owned()];
        let mut next = 0;
        while let Some(current) = ancestry.get(next).cloned() {
            next += 1;
            let known = self
                .profile
                .error_parents
                .iter()
                .filter(|(child, _)| *child == current)
                .map(|(_, parent)| (*parent).to_owned());
            let parents: Vec<String> = known.chain(self.declared_bases(&current)).collect();
            for parent in parents {
                if !ancestry.contains(&parent) {
                    ancestry.push(parent);
                }
            }
        }
        ancestry
    }

    /// The bases a class this file declares under `name` is written to extend.
    fn declared_bases(&self, name: &str) -> Vec<String> {
        let profile = self.profile;
        let classes = self.nodes.iter().filter(|node| {
            profile.is_class(*node)
                && node
                    .child_by_field_name("name")
                    .is_some_and(|named| self.text(named) == name)
        });
        let clauses = classes.flat_map(|class| {
            class
                .named_children(&mut class.walk())
                .filter(|clause| profile.heritage_kinds.contains(&kind_of(clause)))
                .collect::<Vec<_>>()
        });
        clauses
            .flat_map(|clause| {
                clause
                    .named_children(&mut clause.walk())
                    .collect::<Vec<_>>()
            })
            .filter(|base| {
                !matches!(
                    kind_of(base),
                    "access_specifier" | "keyword_argument" | "comment"
                )
            })
            .map(|base| self.error_type_name(base))
            .collect()
    }

    /// A call whose answer is put somewhere nobody reads. The name says the author
    /// saw the answer and decided not to look at it. A call with several results puts
    /// the error last, so only that position decides: `_, err := w.Write(b)` drops
    /// the count and keeps the error. The spot counts the results the call returned.
    pub(super) fn note_discarded_error(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(target) = node.child_by_field_name("left").or_else(|| {
            profile
                .binding_fields
                .iter()
                .find_map(|field| node.child_by_field_name(field))
        }) else {
            return;
        };
        let targets = self.listed(target);
        let Some(last) = targets.last() else {
            return;
        };
        if !profile.is_ignored_name(self.text(*last)) || self.text(*last).len() > 2 {
            return;
        }
        // `var _ Handler = (*server)(nil)` asks the compiler whether a type satisfies
        // an interface; nothing is called and nothing is thrown away.
        if node.child_by_field_name("type").is_some() {
            return;
        }
        let Some(value) = node
            .child_by_field_name("right")
            .or_else(|| node.child_by_field_name("value"))
            .map(|node| self.see_through(node))
        else {
            return;
        };
        let values = self.listed(value);
        let paired = values.len() == targets.len();
        let Some(value) = (if paired {
            values.last()
        } else {
            values.first()
        })
        .map(|value| self.see_through(*value)) else {
            return;
        };
        if !profile.is_call(value) || self.answers_without_an_error(value) {
            return;
        }
        self.out.discarded_errors.push(Spot {
            span: span_of(node),
            detail: self
                .called_name(value)
                .unwrap_or_else(|| "the call".to_owned()),
            count: if paired { 1 } else { targets.len() as u32 },
        });
        self.out
            .focus(RuleId::DiscardedError, span_of(node), span_of(target));
    }

    /// A call to a function this file declares, whose answer cannot carry a failure:
    /// in Go, results that include no `error`; elsewhere a plain number, which a
    /// failure would have had to be thrown past rather than returned in.
    fn answers_without_an_error(&self, call: Node<'a>) -> bool {
        if self.profile.language != rqx_core::Language::Go {
            return self
                .declared_callee(call)
                .is_some_and(|function| self.answers_a_plain_number(function));
        }
        let Some(callee) = call
            .child_by_field_name(self.profile.callee_field)
            .filter(|callee| self.profile.is_identifier(*callee))
        else {
            return false;
        };
        let callee = self.text(callee);
        self.nodes
            .iter()
            .filter(|node| kind_of(*node) == "function_declaration")
            .find(|function| {
                function
                    .child_by_field_name("name")
                    .is_some_and(|name| self.text(name) == callee)
            })
            .is_some_and(|function| {
                function
                    .child_by_field_name("result")
                    .is_some_and(|result| {
                        !descendants(result)
                            .into_iter()
                            .chain(std::iter::once(result))
                            .any(|node| self.text(node) == "error")
                    })
            })
    }
}

/// Return types that hold a number and nothing else.
const NUMBER_TYPES: [&str; 22] = [
    "int",
    "long",
    "short",
    "byte",
    "float",
    "double",
    "unsigned",
    "unsigned int",
    "unsigned long",
    "long long",
    "size_t",
    "std::size_t",
    "int32_t",
    "int64_t",
    "uint32_t",
    "uint64_t",
    "Integer",
    "Long",
    "Double",
    "number",
    "bigint",
    "Number",
];

/// Calls whose answer is a count or a measure, whatever they are given.
const NUMBER_CALLS: [&str; 6] = ["len", "abs", "round", "min", "max", "sum"];

impl<'a> Walker<'a> {
    /// The one function this file declares that a call reaches: `f()`, or
    /// `this.f()` / `self.f()` for a method. Two declarations of the name are two
    /// candidates, and neither is chosen.
    fn declared_callee(&self, call: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        let callee = call.child_by_field_name(profile.callee_field)?;
        let name = if profile.is_identifier(callee) {
            self.text(callee)
        } else {
            let object = callee.child_by_field_name(profile.object_field)?;
            if !profile.self_names.contains(&self.text(object)) {
                return None;
            }
            self.text(self.member_property(callee)?)
        };
        let mut declared = self.nodes.iter().filter(|node| {
            profile.is_function(*node)
                && self
                    .function_name(*node)
                    .is_some_and(|named| self.text(named) == name)
        });
        let only = declared.next()?;
        declared.next().is_none().then_some(only)
    }

    /// Whether a function answers with a number and has no way to answer with a
    /// failure: not async, declared to return a number or returning only numbers,
    /// and never a negative literal, which is how a failure is spelled as a number.
    fn answers_a_plain_number(&self, function: Node<'a>) -> bool {
        let profile = self.profile;
        if profile.is_async(function) {
            return false;
        }
        let declared = function
            .child_by_field_name("return_type")
            .or_else(|| function.child_by_field_name("type"))
            .map(|written| {
                self.text(written)
                    .trim_start_matches([':', ' '])
                    .trim()
                    .to_owned()
            });
        let returned: Vec<Node<'a>> = self
            .function_body(function)
            .map(|body| {
                crate::tree::descendants_outside(body, |node| profile.is_function(node))
                    .into_iter()
                    .filter(|node| kind_of(*node) == "return_statement")
                    .filter_map(|statement| statement.named_child(0))
                    .collect()
            })
            .unwrap_or_default();
        if returned
            .iter()
            .any(|value| self.text(*value).trim_start().starts_with('-'))
        {
            return false;
        }
        match declared {
            Some(written) => NUMBER_TYPES.contains(&written.as_str()),
            None => !returned.is_empty() && returned.iter().all(|value| self.is_a_number(*value)),
        }
    }

    /// An expression that can only be a number: a literal, a length, a count, or
    /// arithmetic over them.
    fn is_a_number(&self, value: Node<'a>) -> bool {
        let profile = self.profile;
        let value = self.see_through(value);
        if profile.number_kinds.contains(&kind_of(value)) {
            return true;
        }
        if profile.is_call(value) {
            return self
                .called_name(value)
                .is_some_and(|name| NUMBER_CALLS.contains(&name.as_str()));
        }
        if profile.is_member_access(value) {
            return self
                .member_property(value)
                .is_some_and(|property| matches!(self.text(property), "length" | "size"));
        }
        profile.binary_kinds.contains(&kind_of(value))
            && crate::tree::operands_of(value)
                .is_some_and(|(left, right)| self.is_a_number(left) && self.is_a_number(right))
    }
}

fn error_type_spelling(text: &str) -> String {
    let text = text.trim();
    let text = text.strip_prefix("::").unwrap_or(text);
    let text = text.strip_prefix("std::").unwrap_or(text);
    let text = text.strip_prefix("java.lang.").unwrap_or(text);
    text.strip_prefix("builtins.").unwrap_or(text).to_owned()
}

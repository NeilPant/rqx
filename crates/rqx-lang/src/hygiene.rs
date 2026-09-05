use std::collections::HashMap;

use rqx_core::facts::{Hygiene, Limits, Spot};
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::scope::{Binding, BindingKind, Scopes};
use crate::tree::{descendants, find_within, operands_of, operator_of, span_of, text_of};
use crate::vocabulary::{
    CONVENTIONAL, HASHED_HINTS, IO_BARE, KEYED_CONSTRUCTIONS, TestVocabulary, asks_for_data,
    configuration_literal, convention_of, foreign_language, journal_marker, looks_like_code,
    mentions_any, query_prefix, reaches_outside, redundant_subject, same_family, vague_verb,
    verb_and_object,
};

/// One function literal's view of the loops around it. A callback handed to
/// `forEach` runs per element, so it inherits the loops of its caller; anything else
/// starts clear of them.
#[derive(Default)]
struct PerfFrame {
    is_async: bool,
    loops: u32,
    /// Loops written as loops. A callback is repetition the caller chose, and
    /// awaiting inside one is how concurrency is spelled rather than a mistake.
    statement_loops: u32,
}

struct Walker<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    limits: Limits,
    extra: &'a rqx_core::config::Performance,
    out: Hygiene,
    numbers: HashMap<String, Vec<Span>>,
    boundaries: HashMap<String, Vec<Span>>,
    conventions: Vec<(String, Span)>,
    verbs: Vec<(String, String, Span)>,
    /// What each local name was introduced as, so a rule can ask whether a value is a
    /// list, a string, or something already keyed.
    declared: HashMap<String, String>,
    frames: Vec<PerfFrame>,
    test_depth: u32,
    block_depth: u32,
}

impl<'a> Walker<'a> {
    fn text(&self, node: Node<'a>) -> &'a str {
        text_of(node, self.source)
    }

    fn see_through(&self, node: Node<'a>) -> Node<'a> {
        let mut current = node;
        while self.profile.transparent_kinds.contains(&current.kind()) {
            match current.named_child(0) {
                Some(inner) => current = inner,
                None => break,
            }
        }
        current
    }

    /// The dotted name a call is written with, so `describe.skip` and `t.Skip` are
    /// one string a vocabulary can match.
    fn callee_name(&self, node: Node<'a>) -> Option<String> {
        let node = self.see_through(node);
        if self.profile.is_identifier(node) {
            return Some(self.text(node).to_owned());
        }
        if self.profile.is_member_access(node) {
            let object = self.callee_name(node.child_by_field_name(self.profile.object_field)?)?;
            let property = self.member_property(node)?;
            return Some(format!("{object}.{}", self.text(property)));
        }
        if self.profile.qualified_name_kinds.contains(&node.kind()) {
            return Some(self.text(node).replace("::", "."));
        }
        None
    }

    /// The member half of a reach: `a.b` and `a.b()` name it in different fields in
    /// some grammars, and both mean the same thing here.
    fn member_property(&self, node: Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name(self.profile.property_field)
            .or_else(|| node.child_by_field_name(self.profile.callee_field))
    }

    fn called_name(&self, call: Node<'a>) -> Option<String> {
        // A grammar that splits the receiver from the method name leaves the call
        // node itself as the only thing spelling the whole dotted name.
        let callee = call.child_by_field_name(self.profile.callee_field)?;
        if self.profile.is_member_access(call) {
            return self.callee_name(call);
        }
        self.callee_name(callee)
    }

    /// How many separate decisions one condition holds. A run of the same operator is
    /// still one decision per operand to a reader.
    fn operands(&self, node: Node<'a>) -> u32 {
        let node = self.see_through(node);
        if self.profile.binary_kinds.contains(&node.kind())
            && operator_of(node, self.source)
                .is_some_and(|operator| self.profile.logical_operators.contains(&operator))
        {
            let Some((left, right)) = operands_of(node) else {
                return 1;
            };
            return self.operands(left) + self.operands(right);
        }
        1
    }

    fn chain_depth(&self, node: Node<'a>) -> u32 {
        let node = self.see_through(node);
        if self.profile.is_member_access(node) {
            return 1 + node
                .child_by_field_name(self.profile.object_field)
                .map_or(0, |n| self.chain_depth(n));
        }
        if self.profile.is_call(node) {
            return node
                .child_by_field_name(self.profile.callee_field)
                .map_or(0, |n| self.chain_depth(n));
        }
        0
    }

    /// Counts the decisions packed into one expression. Nothing here is named, so each
    /// one is a step the reader has to hold rather than read.
    fn operations(&self, node: Node<'a>) -> u32 {
        descendants(node)
            .into_iter()
            .filter(|inner| {
                self.profile.binary_kinds.contains(&inner.kind())
                    || self.profile.is_call(*inner)
                    || inner.kind() == "ternary_expression"
                    || inner.kind() == "conditional_expression"
            })
            .count() as u32
    }

    /// Looks for an assertion anywhere in the test's body rather than tracking a
    /// stack, because a helper called from the test is still the test asserting.
    fn body_asserts(&self, body: Node<'a>) -> bool {
        descendants(body)
            .into_iter()
            .filter(|node| self.profile.is_call(*node))
            .any(|call| {
                self.called_name(call)
                    .is_some_and(|name| self.profile.test.is_assertion(&name))
            })
    }

    /// A query that writes is only interesting when the write outlives the call, so
    /// assignments to a local are not counted — a field or a captured name is.
    fn body_mutates(&self, body: Node<'a>) -> bool {
        descendants(body)
            .into_iter()
            .filter(|node| self.profile.assignment_kinds.contains(&node.kind()))
            .any(|assignment| {
                assignment
                    .child_by_field_name("left")
                    .is_some_and(|target| {
                        let target = if self.profile.expression_list_kinds.contains(&target.kind())
                        {
                            target.named_child(0).unwrap_or(target)
                        } else {
                            target
                        };
                        self.profile.is_member_access(target)
                    })
            })
    }

    /// Shared by plain functions and by methods, which in most grammars are a
    /// different node kind and would otherwise be invisible to both rules.
    fn name_the_body(&mut self, name: &str, span: Span, body: Option<Node<'a>>) {
        if vague_verb(name) {
            self.out.vague_names.push(Spot {
                span,
                detail: name.to_owned(),
                count: 1,
            });
        }
        if let Some((verb, object)) = verb_and_object(name) {
            self.verbs.push((verb.to_owned(), object, span));
        }
        if query_prefix(name) && body.is_some_and(|body| self.body_mutates(body)) {
            self.out.side_effecting_queries.push(Spot {
                span,
                detail: name.to_owned(),
                count: 1,
            });
        }
    }

    /// A function may name itself directly or through a declarator wrapper, and its
    /// body may be the braces themselves or a list inside them.
    fn function_name(&self, node: Node<'a>) -> Option<Node<'a>> {
        if let Some(name) = node.child_by_field_name("name") {
            return Some(name);
        }
        let declarator = find_within(
            node,
            self.profile.declarator_wrapper_kinds,
            self.profile.block_kinds,
        )
        .into_iter()
        .next()?;
        let mut cursor = declarator.walk();
        declarator.named_children(&mut cursor).find(|child| {
            self.profile.is_identifier(*child)
                || self.profile.property_kinds.contains(&child.kind())
        })
    }

    fn function_body(&self, node: Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body").or_else(|| {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find(|child| self.profile.is_block(*child))
        })
    }

    fn note_flag_parameters(&mut self, function: Node<'a>) {
        if self.function_body(function).is_none() {
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

    fn declared_type(&self, parameter: Node<'a>) -> Option<&'a str> {
        let annotation = descendants(parameter)
            .into_iter()
            .find(|child| self.profile.type_annotation_kinds.contains(&child.kind()))?;
        let inner = annotation.named_child(0).unwrap_or(annotation);
        Some(self.text(inner))
    }

    fn note_unreachable(&mut self, block: Node<'a>) {
        let mut cursor = block.walk();
        let statements: Vec<Node<'a>> = block
            .named_children(&mut cursor)
            .filter(|child| self.profile.is_statement(*child))
            .collect();
        for (index, statement) in statements.iter().enumerate() {
            let Some((_, word)) = self
                .profile
                .exit_kinds
                .iter()
                .find(|(kind, _)| *kind == statement.kind())
            else {
                continue;
            };
            if let Some(next) = statements.get(index + 1)
                && !self.profile.hoisted_kinds.contains(&next.kind())
            {
                self.out.unreachable.push(Spot {
                    span: span_of(*next),
                    detail: (*word).to_owned(),
                    count: 1,
                });
            }
            break;
        }
    }

    fn note_condition(&mut self, branch: Node<'a>) {
        let Some(condition) = branch.child_by_field_name(self.profile.condition_field) else {
            return;
        };
        let inner = self.see_through(condition);
        let operands = self.operands(condition);
        if operands > self.limits.condition_operands {
            self.out.complex_conditions.push(Spot {
                span: span_of(inner),
                detail: String::new(),
                count: operands,
            });
        }
        let negated = operator_of(inner, self.source)
            .is_some_and(|operator| self.profile.negation_operators.contains(&operator));
        if negated
            && branch
                .child_by_field_name(self.profile.else_field)
                .is_some()
        {
            self.out.negative_conditionals.push(Spot {
                span: span_of(inner),
                detail: String::new(),
                count: 1,
            });
        }
    }

    /// A framework that finds its cases by naming convention marks them with an
    /// annotation rather than a call, so both live outside `note_call`.
    fn note_declared_test(&mut self, function: Node<'a>) {
        let vocabulary: &TestVocabulary = &self.profile.test;
        let Some(name) = self.function_name(function) else {
            return;
        };
        let label = self.text(name).to_owned();
        if !vocabulary.names_a_case(&label) {
            return;
        }
        for annotation in self.annotations(function) {
            match vocabulary.annotation(&annotation) {
                Some("skipped") => self.out.skipped_tests.push(Spot {
                    span: span_of(name),
                    detail: annotation.clone(),
                    count: 1,
                }),
                Some("focused") => self.out.focused_tests.push(Spot {
                    span: span_of(name),
                    detail: annotation.clone(),
                    count: 1,
                }),
                _ => {}
            }
        }
        let marked = self
            .annotations(function)
            .iter()
            .any(|annotation| vocabulary.annotation(annotation).is_some());
        if marked {
            return;
        }
        if let Some(body) = self.function_body(function)
            && !self.body_asserts(body)
        {
            self.out.assertionless_tests.push(Spot {
                span: span_of(name),
                detail: label,
                count: 1,
            });
        }
    }

    fn annotations(&self, function: Node<'a>) -> Vec<String> {
        let mut found = Vec::new();
        if let Some(parent) = function.parent()
            && self.profile.decorated_kinds.contains(&parent.kind())
        {
            let mut cursor = parent.walk();
            found.extend(
                parent
                    .named_children(&mut cursor)
                    .filter(|child| self.profile.decorator_kinds.contains(&child.kind()))
                    .map(|child| self.text(child).to_owned()),
            );
        }
        let mut cursor = function.walk();
        for container in function.named_children(&mut cursor).filter(|child| {
            self.profile
                .modifier_container_kinds
                .contains(&child.kind())
        }) {
            found.extend(
                descendants(container)
                    .into_iter()
                    .filter(|child| self.profile.decorator_kinds.contains(&child.kind()))
                    .map(|child| self.text(child).to_owned()),
            );
        }
        found
    }

    fn note_call(&mut self, call: Node<'a>) {
        let vocabulary: &TestVocabulary = &self.profile.test;
        if let Some(name) = self.called_name(call) {
            match vocabulary.directive(&name) {
                Some(("skipped", directive)) => self.out.skipped_tests.push(Spot {
                    span: span_of(call),
                    detail: directive.to_owned(),
                    count: 1,
                }),
                Some(("focused", directive)) => self.out.focused_tests.push(Spot {
                    span: span_of(call),
                    detail: directive.to_owned(),
                    count: 1,
                }),
                _ => {}
            }

            // Only leaf tests are checked for assertions; a suite asserts through the
            // cases inside it.
            if vocabulary.is_case(&name) && vocabulary.directive(&name).is_none() {
                let arguments = call.child_by_field_name("arguments");
                let body = arguments.and_then(|list| list.named_child(1));
                if let Some(body) = body
                    && !self.body_asserts(body)
                {
                    let label = arguments
                        .and_then(|list| list.named_child(0))
                        .filter(|node| self.profile.string_kinds.contains(&node.kind()))
                        .map(|node| self.text(node).trim_matches(['"', '\'', '`']).to_owned())
                        .unwrap_or_else(|| name.clone());
                    self.out.assertionless_tests.push(Spot {
                        span: span_of(call),
                        detail: label,
                        count: 1,
                    });
                }
            }

            if self.test_depth > 0
                && let Some(verdict) = vocabulary.control(&name)
            {
                let spot = Spot {
                    span: span_of(call),
                    detail: name.clone(),
                    count: 1,
                };
                match verdict {
                    "skipped" => self.out.skipped_tests.push(spot),
                    "focused" => self.out.focused_tests.push(spot),
                    _ => {}
                }
            }

            if self.test_depth > 0 && vocabulary.is_delay(&name) {
                let pause = call
                    .child_by_field_name("arguments")
                    .map(descendants)
                    .unwrap_or_default()
                    .into_iter()
                    .find(|node| self.profile.number_kinds.contains(&node.kind()))
                    .and_then(|node| self.text(node).parse::<f64>().ok());
                if let Some(pause) = pause.filter(|ms| *ms >= f64::from(self.limits.slow_test_ms)) {
                    self.out.slow_tests.push(Spot {
                        span: span_of(call),
                        detail: name.clone(),
                        count: pause as u32,
                    });
                }
            }
        }

        let measured = if self.profile.is_member_access(call) {
            Some(call)
        } else {
            call.child_by_field_name(self.profile.callee_field)
        };
        if let Some(callee) = measured {
            let depth = self.chain_depth(callee);
            if depth > self.limits.message_chain_depth {
                self.out.message_chains.push(Spot {
                    span: span_of(call),
                    detail: String::new(),
                    count: depth,
                });
            }
        }
    }

    fn note_number(&mut self, node: Node<'a>) {
        let raw = self.text(node);
        let Ok(value) = raw.parse::<f64>() else {
            return;
        };
        if CONVENTIONAL
            .iter()
            .any(|known| (known - value).abs() < f64::EPSILON)
        {
            return;
        }
        self.numbers
            .entry(raw.to_owned())
            .or_default()
            .push(span_of(node));
    }

    /// `n + 1` and `n - 1` written about the same subject several times is where
    /// off-by-one lives.
    fn note_boundary(&mut self, node: Node<'a>) {
        let Some(operator) = operator_of(node, self.source) else {
            return;
        };
        if !matches!(operator, "+" | "-") {
            return;
        }
        let Some((left, right)) = operands_of(node) else {
            return;
        };
        if self.text(right) != "1" {
            return;
        }
        if let Some(subject) = self.callee_name(left) {
            self.boundaries
                .entry(subject)
                .or_default()
                .push(span_of(node));
        }
    }

    /// The shapes an `if` takes when it is saying less than it looks like: an `else`
    /// after a branch that already left, an `if` that is the only thing inside
    /// another, two arms that do the same thing, and a branch whose whole job is to
    /// turn a condition into the boolean it already is.
    fn note_branch_shape(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(consequence) = node
            .child_by_field_name("consequence")
            .map(|arm| self.body(arm))
        else {
            return;
        };
        let alternative = node.child_by_field_name(profile.else_field);

        if let Some(alternative) = alternative {
            if self.always_exits(consequence) {
                self.out.redundant_else.push(Spot {
                    span: span_of(alternative),
                    detail: "else".to_owned(),
                    count: 1,
                });
            }
            // Some languages make the alternative the block itself and others wrap it
            // in an `else` clause; only the wrapper is worth seeing through.
            let other = self.body(alternative);
            if other.kind() != profile.if_kind
                && !self.text(consequence).trim().is_empty()
                && squashed(self.text(consequence)) == squashed(self.text(other))
            {
                self.out.identical_branches.push(Spot {
                    span: span_of(node),
                    detail: "both arms".to_owned(),
                    count: 1,
                });
            }
            if let (Some(first), Some(second)) = (
                self.returned_boolean(consequence),
                self.returned_boolean(other),
            ) && first != second
            {
                self.out.boolean_from_branch.push(Spot {
                    span: span_of(node),
                    detail: "the condition itself".to_owned(),
                    count: 1,
                });
            }
        } else if let Some(inner) = self.sole_statement(consequence)
            && inner.kind() == profile.if_kind
            && inner.child_by_field_name(profile.else_field).is_none()
        {
            self.out.collapsible_ifs.push(Spot {
                span: span_of(inner),
                detail: "one condition".to_owned(),
                count: 1,
            });
        }
    }

    /// A block a reader was promised something in. The empty bodies a language needs
    /// — `pass`, an abstract method — are declared per language and skipped, and a
    /// catch is left to the rule that already reports it.
    fn note_empty_block(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(parent) = node.parent() else {
            return;
        };
        // A grammar that puts a statement list inside the braces makes the block's
        // parent the braces rather than the branch, so the decision is one level up.
        let governs = |candidate: Node<'a>| {
            candidate.kind() == profile.if_kind
                || profile.else_clause_kinds.contains(&candidate.kind())
                || profile.loop_kinds.contains(&candidate.kind())
        };
        let decides = governs(parent)
            || (profile.structural_block_parents.contains(&parent.kind())
                && parent.parent().is_some_and(governs));
        // Reported once: where both the braces and a list inside them are nodes, the
        // list is the one that carries the statements.
        if profile.structural_block_parents.contains(&node.kind())
            && node
                .named_children(&mut node.walk())
                .any(|child| profile.is_block(child))
        {
            return;
        }
        // A comment is a named child in these grammars, so a block holding only an
        // explanation is a decision somebody documented rather than an empty one.
        if !decides || node.named_child_count() > 0 {
            return;
        }
        self.out.empty_blocks.push(Spot {
            span: span_of(node),
            detail: parent.kind().replace('_', " "),
            count: 1,
        });
    }

    /// A comparison that has already decided: an operand against itself, or against
    /// the literal it is being asked whether it equals.
    fn note_comparison(&mut self, node: Node<'a>) {
        let profile = self.profile;
        // Python writes a comparison as a run of operands with the operators between
        // them rather than as a left/right pair, so the fields are absent there and
        // the operator has to be read positionally.
        let operator = node.child_by_field_name("operator").map_or_else(
            || {
                (0..node.child_count())
                    .filter_map(|index| node.child(index))
                    .find(|child| !child.is_named())
            },
            Some,
        );
        let Some(operator) = operator else {
            return;
        };
        if !profile.equality_operators.contains(&self.text(operator)) {
            return;
        }
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left")
                .or_else(|| node.named_child(0)),
            node.child_by_field_name("right")
                .or_else(|| node.named_child(1)),
        ) else {
            return;
        };
        let (left_text, right_text) = (self.text(left).trim(), self.text(right).trim());
        if left_text == right_text && !left_text.is_empty() {
            self.out.self_comparisons.push(Spot {
                span: span_of(node),
                detail: left_text.to_owned(),
                count: 1,
            });
            return;
        }
        for side in [left_text, right_text] {
            if profile.boolean_literals.contains(&side) {
                self.out.literal_comparisons.push(Spot {
                    span: span_of(node),
                    detail: side.to_owned(),
                    count: 1,
                });
                return;
            }
        }
    }

    /// Ways out of one body, past the guard clauses. A `return` in the first few
    /// statements is a precondition a reader expects; the ones written deep in the
    /// body are the ones they have to hunt for.
    /// A body with nothing in it. An abstract declaration has no body at all, so it
    /// never reaches here; a constructor is exempt because an empty one is how several
    /// languages spell "the default, but private".
    fn note_empty_method(&mut self, function: Node<'a>) {
        let profile = self.profile;
        if profile.constructor_kinds.contains(&function.kind()) {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        // `empty_body_kinds` is how each language spells nothing: a comment
        // everywhere, and `pass` or `...` in Python.
        let empty = body
            .named_children(&mut body.walk())
            .all(|child| profile.empty_body_kinds.contains(&child.kind()));
        if !empty {
            return;
        }
        // An empty function literal passed as a callback is a deliberate no-op, and
        // being empty is the whole of what it says.
        let Some(name) = self.function_name(function) else {
            return;
        };
        let name = self.text(name).to_owned();
        self.out.empty_methods.push(Spot {
            span: span_of(function),
            detail: name,
            count: 1,
        });
    }

    /// A `continue` or `break` that control was reaching anyway. The keyword makes a
    /// reader stop and look for the condition that is not there.
    fn note_redundant_jump(&mut self, loop_node: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(loop_node).or_else(|| {
            loop_node
                .named_children(&mut loop_node.walk())
                .find(|child| profile.is_block(*child))
        }) else {
            return;
        };
        // Go puts a `statement_list` inside the `block` that is the loop's body, so
        // the statements are one level further down than the body field suggests.
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        let mut cursor = holder.walk();
        let Some(last) = holder.named_children(&mut cursor).last() else {
            return;
        };
        // `break` ends a loop early and `continue` skips the rest, and at the end of
        // the body both do exactly what falling off the end does — unless they carry
        // a label, which is a jump out of somewhere else entirely.
        let bare = last.named_child_count() == 0;
        if bare && last.kind() == "continue_statement" {
            self.out.redundant_jumps.push(Spot {
                span: span_of(last),
                detail: "continue".to_owned(),
                count: 1,
            });
        }
    }

    fn note_exits(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let guards = (0..body.named_child_count())
            .filter_map(|index| body.named_child(index))
            .take_while(|child| {
                child.kind() == profile.if_kind
                    || profile.exit_kinds.iter().any(|(k, _)| *k == child.kind())
            })
            .count();

        let mut cursor = body.walk();
        let mut stack: Vec<Node<'a>> = body.children(&mut cursor).collect();
        let mut exits: u32 = 0;
        while let Some(node) = stack.pop() {
            // A nested function has its own ways out; counting them here would blame
            // this body for a callback's shape.
            if profile.is_function(node) {
                continue;
            }
            if profile.exit_kinds.iter().any(|(k, _)| *k == node.kind()) {
                exits += 1;
            }
            let mut inner = node.walk();
            stack.extend(node.children(&mut inner));
        }

        let past_guards = exits.saturating_sub(guards as u32);
        if past_guards > self.limits.max_exits {
            self.out.many_exits.push(Spot {
                span: self.name_span(function),
                detail: past_guards.to_string(),
                count: past_guards,
            });
        }
    }

    fn note_switch_width(&mut self, node: Node<'a>) {
        let profile = self.profile;
        // The arms sit inside the switch's body in most grammars and directly under
        // the statement in the rest, so both are counted rather than one guessed at.
        let mut cursor = node.walk();
        let mut stack: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
        let mut arms = 0_u32;
        while let Some(current) = stack.pop() {
            if profile.case_kinds.contains(&current.kind()) {
                arms += 1;
                continue;
            }
            if profile.switch_kinds.contains(&current.kind()) {
                continue;
            }
            let mut inner = current.walk();
            stack.extend(current.named_children(&mut inner));
        }
        if arms > 0 && arms < self.limits.min_cases {
            self.out.tiny_switches.push(Spot {
                span: span_of(node),
                detail: arms.to_string(),
                count: arms,
            });
        }
        if arms > self.limits.max_cases {
            self.out.wide_switches.push(Spot {
                span: span_of(node),
                detail: arms.to_string(),
                count: arms,
            });
        }
    }

    /// How deep a function literal sits inside other function literals. Reported on
    /// the innermost one, which is where a reader ends up.
    fn note_callback_depth(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let mut depth = 0;
        let mut parent = node.parent();
        while let Some(current) = parent {
            if profile.is_function(current) {
                depth += 1;
            }
            parent = current.parent();
        }
        if depth > self.limits.callback_depth {
            self.out.callback_nesting.push(Spot {
                span: self.name_span(node),
                detail: depth.to_string(),
                count: depth,
            });
        }
    }

    /// An assignment where a condition belongs. It reads as a comparison and behaves
    /// as a write, which is why it survives review.
    fn note_assignment_in_condition(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(condition) = node.child_by_field_name("condition") else {
            return;
        };
        let mut cursor = condition.walk();
        let mut stack: Vec<Node<'a>> = condition.children(&mut cursor).collect();
        stack.push(condition);
        while let Some(current) = stack.pop() {
            if profile.assignment_kinds.contains(&current.kind()) {
                self.out.assignments_in_conditions.push(Spot {
                    span: span_of(current),
                    detail: self.text(current).trim().to_owned(),
                    count: 1,
                });
                return;
            }
            let mut inner = current.walk();
            stack.extend(current.children(&mut inner));
        }
    }

    /// A function's name where it has one, so a finding lands on the identifier a
    /// reader searches for rather than on the whole body.
    fn name_span(&self, function: Node<'a>) -> Span {
        self.function_name(function)
            .map_or_else(|| span_of(function), span_of)
    }

    /// Whether every path out of a block leaves the function or the loop, which is
    /// what makes a following `else` a level of indentation nobody needs.
    fn always_exits(&self, block: Node<'a>) -> bool {
        let profile = self.profile;
        let exits = |node: Node<'a>| {
            profile
                .exit_kinds
                .iter()
                .any(|(kind, _)| *kind == node.kind())
        };
        let last = (0..block.named_child_count())
            .filter_map(|index| block.named_child(index))
            .rfind(|child| !profile.is_comment(*child));
        match last {
            Some(node) if exits(node) => true,
            Some(node) if node.kind() == profile.if_kind => {
                let consequence = node.child_by_field_name("consequence");
                let alternative = node.child_by_field_name(profile.else_field);
                match (consequence, alternative) {
                    (Some(one), Some(other)) => self.always_exits(one) && self.always_exits(other),
                    _ => false,
                }
            }
            Some(node) if profile.is_block(node) => self.always_exits(node),
            _ => exits(block),
        }
    }

    /// The node actually holding a body's statements. Some grammars make an arm the
    /// block itself, some wrap it in an `else` clause, and Go puts a statement list
    /// inside the braces — comparing two arms means comparing the same thing.
    fn body(&self, node: Node<'a>) -> Node<'a> {
        if self.profile.is_block(node) {
            return node;
        }
        let mut cursor = node.walk();
        let inner: Vec<Node<'a>> = node
            .named_children(&mut cursor)
            .filter(|child| self.profile.is_block(*child))
            .collect();
        match inner.as_slice() {
            [only] => *only,
            _ => node.named_child(0).map_or(node, |first| {
                if self.profile.is_block(first) {
                    first
                } else {
                    node
                }
            }),
        }
    }

    fn sole_statement(&self, block: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        let mut statements = (0..block.named_child_count())
            .filter_map(|index| block.named_child(index))
            .filter(|child| !profile.is_comment(*child));
        let first = statements.next()?;
        statements.next().is_none().then_some(first)
    }

    /// The boolean a block returns, when returning one is all it does.
    fn returned_boolean(&self, block: Node<'a>) -> Option<bool> {
        let profile = self.profile;
        let statement = self.sole_statement(block).unwrap_or(block);
        if !profile
            .exit_kinds
            .iter()
            .any(|(kind, _)| *kind == statement.kind())
        {
            return None;
        }
        let value = statement.named_child(0)?;
        let text = self.text(value).trim();
        profile
            .boolean_literals
            .contains(&text)
            .then_some(matches!(text, "true" | "True"))
    }

    fn loop_depth(&self) -> u32 {
        self.frames.last().map_or(0, |frame| frame.loops)
    }

    fn in_statement_loop(&self) -> bool {
        self.frames
            .last()
            .is_some_and(|frame| frame.statement_loops > 0)
    }

    fn in_async(&self) -> bool {
        self.frames.last().is_some_and(|frame| frame.is_async)
    }

    fn push_loop(&mut self, counts_as_written: bool) {
        if let Some(frame) = self.frames.last_mut() {
            frame.loops += 1;
            if counts_as_written {
                frame.statement_loops += 1;
            }
        }
    }

    fn pop_loop(&mut self, counted_as_written: bool) {
        if let Some(frame) = self.frames.last_mut() {
            frame.loops = frame.loops.saturating_sub(1);
            if counted_as_written {
                frame.statement_loops = frame.statement_loops.saturating_sub(1);
            }
        }
    }

    fn note(&mut self, into: fn(&mut Hygiene) -> &mut Vec<Spot>, span: Span, detail: String) {
        let count = self.loop_depth().max(1);
        into(&mut self.out).push(Spot {
            span,
            detail,
            count,
        });
    }

    /// What a call is written on: `orders.items.includes` answers `orders.items`.
    /// Go writes both sides of an assignment as a list even when it assigns one
    /// thing, so the single element is what the rules are about.
    fn unwrapped(&self, node: Node<'a>) -> Node<'a> {
        let node = self.see_through(node);
        if self.profile.expression_list_kinds.contains(&node.kind()) {
            return node.named_child(0).unwrap_or(node);
        }
        node
    }

    fn receiver_of<'d>(&self, dotted: &'d str) -> Option<&'d str> {
        dotted.rsplit_once('.').map(|(head, _)| head)
    }

    /// A vocabulary entry spelling a receiver has to match one; an entry that is a
    /// bare name matches any call whose last segment is it.
    fn answers_to(&self, dotted: &str, entry: &str) -> bool {
        if entry.contains('.') {
            return dotted == entry || dotted.ends_with(&format!(".{entry}"));
        }
        dotted.rsplit('.').next() == Some(entry)
    }

    fn matches_any(&self, dotted: &str, entries: &[&str]) -> bool {
        entries.iter().any(|entry| self.answers_to(dotted, entry))
    }

    /// Everything known about how a name was introduced: its declared type and the
    /// head of whatever it was first set to.
    fn declaration_of(&self, name: &str) -> &str {
        self.declared.get(name).map_or("", String::as_str)
    }

    /// A value that already answers by key. Searching one costs nothing, so a scan
    /// over it is not the shape this rule is about.
    fn already_keyed(&self, receiver: &str) -> bool {
        let last = receiver.rsplit('.').next().unwrap_or(receiver);
        mentions_any(last, &HASHED_HINTS)
            || mentions_any(self.declaration_of(last), &KEYED_CONSTRUCTIONS)
            || mentions_any(self.declaration_of(receiver), &KEYED_CONSTRUCTIONS)
    }

    /// Searching a string is not searching a collection: `line.indexOf(":")` walks a
    /// handful of characters and answers a parsing question.
    fn holds_text(&self, receiver: &str) -> bool {
        let last = receiver.rsplit('.').next().unwrap_or(receiver);
        let declaration = self.declaration_of(last);
        declaration.starts_with('"')
            || declaration.starts_with('\'')
            || declaration.starts_with('`')
            || self
                .profile
                .perf
                .string_types
                .iter()
                .any(|kind| declaration.split_whitespace().any(|word| word == *kind))
    }

    /// A name in capitals is a fixed table somebody wrote out, and a literal list is
    /// as long as the source says it is. Neither grows with the input.
    fn fixed_size(&self, receiver: &str) -> bool {
        let last = receiver.rsplit('.').next().unwrap_or(receiver);
        let capitals = last.chars().any(|c| c.is_ascii_uppercase())
            && !last.chars().any(|c| c.is_ascii_lowercase());
        capitals || self.declaration_of(last).starts_with('[')
    }

    fn worth_scanning(&self, receiver: &str) -> bool {
        !receiver.is_empty()
            && !self.already_keyed(receiver)
            && !self.holds_text(receiver)
            && !self.fixed_size(receiver)
    }

    fn arguments(&self, call: Node<'a>) -> Vec<Node<'a>> {
        let Some(list) = (0..call.named_child_count())
            .filter_map(|index| call.named_child(index))
            .find(|child| self.profile.argument_list_kinds.contains(&child.kind()))
        else {
            return Vec::new();
        };
        let mut cursor = list.walk();
        list.named_children(&mut cursor).collect()
    }

    fn is_literal(&self, node: Node<'a>) -> bool {
        let kind = node.kind();
        self.profile.string_kinds.contains(&kind)
            || self.profile.number_kinds.contains(&kind)
            || kind == "regex"
    }

    /// The collection a free-function scan is given, which is its first argument
    /// however many iterators it was spelled with.
    fn scanned_argument(&self, call: Node<'a>) -> Option<String> {
        let first = self.arguments(call).into_iter().next()?;
        let named = self.callee_name(first).or_else(|| {
            self.profile
                .is_call(first)
                .then(|| self.called_name(first))
                .flatten()
        })?;
        Some(
            named
                .rsplit_once('.')
                .map_or(named.clone(), |(head, _)| head.to_owned()),
        )
    }

    /// `new RegExp(…)` names what it builds in a field of its own, which the callee
    /// walk has no reason to look in.
    fn constructed_name(&self, call: Node<'a>) -> Option<String> {
        let field = self.profile.constructor_field?;
        self.callee_name(call.child_by_field_name(field)?)
    }

    /// Rules 1, 3, 5 and 8 all ask what a call is: one walk of the vocabulary rather
    /// than four.
    fn note_perf_call(&mut self, node: Node<'a>) {
        let Some(dotted) = self
            .called_name(node)
            .or_else(|| self.constructed_name(node))
        else {
            return;
        };
        let perf = &self.profile.perf;
        let tail = dotted.rsplit('.').next().unwrap_or(&dotted).to_owned();
        let receiver = self.receiver_of(&dotted).unwrap_or("").to_owned();
        let span = span_of(node);
        let inside_loop = self.loop_depth() > 0;

        if inside_loop
            && self.matches_any(&dotted, perf.regex_constructors)
            && !self.arguments(node).is_empty()
            && self
                .arguments(node)
                .iter()
                .all(|argument| self.is_literal(*argument))
        {
            self.note(|out| &mut out.invariants_in_loops, span, dotted.clone());
            return;
        }

        if inside_loop {
            let scanned = if perf.scans.contains(&tail.as_str()) && !receiver.is_empty() {
                Some(receiver.clone())
            } else if self.matches_any(&dotted, perf.scan_calls) {
                self.scanned_argument(node)
            } else {
                None
            };
            if let Some(collection) = scanned
                && self.worth_scanning(&collection)
                && !self
                    .arguments(node)
                    .first()
                    .is_some_and(|first| self.profile.string_kinds.contains(&first.kind()))
            {
                self.note(|out| &mut out.scans_in_loops, span, dotted.clone());
                return;
            }
        }

        if inside_loop
            && !self.already_keyed(&receiver)
            && ((reaches_outside(&receiver, &self.extra.receivers)
                && asks_for_data(&tail, &self.extra.methods))
                || (receiver.is_empty() && IO_BARE.contains(&tail.to_ascii_lowercase().as_str())))
        {
            self.note(|out| &mut out.queries_in_loops, span, dotted.clone());
            return;
        }

        if self.in_async()
            && perf.blocking.iter().any(|entry| {
                if entry.contains('.') {
                    self.answers_to(&dotted, entry)
                } else {
                    tail == *entry
                        && (receiver.is_empty()
                            || perf
                                .blocking_receivers
                                .contains(&receiver.rsplit('.').next().unwrap_or(&receiver)))
                }
            })
        {
            self.out.blocking_in_async.push(Spot {
                span,
                detail: dotted,
                count: 1,
            });
        }
    }

    /// Awaiting inside a loop makes the passes wait for each other. Awaiting inside a
    /// callback is how the same work is started together, so only written loops count.
    fn note_await(&mut self, node: Node<'a>) {
        if !self.in_statement_loop() {
            return;
        }
        let subject = node
            .named_child(0)
            .and_then(|inner| self.called_name(inner).or_else(|| self.callee_name(inner)))
            .unwrap_or_else(|| "await".to_owned());
        self.note(|out| &mut out.awaits_in_loops, span_of(node), subject);
    }

    /// `std::regex digits("…")` is a declaration rather than a call, so the call walk
    /// never sees the construction C++ spells this way.
    fn note_regex_declaration(&mut self, node: Node<'a>) {
        if self.loop_depth() == 0 {
            return;
        }
        let Some(kind) = node.child_by_field_name("type") else {
            return;
        };
        let named = self.text(kind).replace("::", ".");
        if !self
            .profile
            .perf
            .regex_types
            .iter()
            .any(|entry| named.split('.').next_back() == entry.split('.').next_back())
        {
            return;
        }
        let built = descendants(node).into_iter().any(|child| {
            self.profile.argument_list_kinds.contains(&child.kind())
                && child
                    .named_child(0)
                    .is_some_and(|first| self.is_literal(first))
        });
        if built {
            self.note(|out| &mut out.invariants_in_loops, span_of(node), named);
        }
    }

    /// `if row.id in wanted` is a walk of `wanted` where a set would answer at once,
    /// but only when `wanted` is a list: membership in a dict is already a lookup.
    fn note_membership_scan(&mut self, node: Node<'a>) {
        if self.loop_depth() == 0 || !self.text(node).contains(" in ") {
            return;
        }
        let Some(right) = node.named_child(node.named_child_count().saturating_sub(1)) else {
            return;
        };
        let Some(named) = self.callee_name(right) else {
            return;
        };
        let declaration = self.declaration_of(named.rsplit('.').next().unwrap_or(&named));
        if declaration.starts_with('[') || declaration.starts_with("list(") {
            self.note(|out| &mut out.scans_in_loops, span_of(node), named);
        }
    }

    /// An accumulator rebuilt from itself: every pass copies what the passes before
    /// it built.
    fn note_copy_accumulation(&mut self, node: Node<'a>) {
        if self.loop_depth() == 0 {
            return;
        }
        let Some(target) = node.child_by_field_name("left") else {
            return;
        };
        let target = self.unwrapped(target);
        if !self.profile.is_identifier(target) {
            return;
        }
        let name = self.text(target);
        let Some(value) = node.child_by_field_name("right") else {
            return;
        };
        let value = self.unwrapped(value);
        let text = self.text(value);
        let head = text.trim_start();
        let rebuilt = head.starts_with(&format!("[...{name}"))
            || head.starts_with(&format!("{{...{name}"))
            || head.starts_with(&format!("[*{name}"))
            || head.starts_with(&format!("{name}.concat("))
            || head.starts_with(&format!("{name} + "))
            || (self.text(node).contains("+=") && self.grows_by_copy(name, value));
        if rebuilt && self.grows_by_copy(name, value) {
            self.note(
                |out| &mut out.copied_accumulators,
                span_of(node),
                name.to_owned(),
            );
        }
    }

    /// Adding to a string or a list is a copy; adding to a number is not.
    fn grows_by_copy(&self, name: &str, value: Node<'a>) -> bool {
        let text = self.text(value).trim_start();
        self.holds_text(name)
            || self.declaration_of(name).starts_with('[')
            || text.starts_with('[')
            || text.starts_with('{')
            || descendants(value)
                .into_iter()
                .any(|child| self.profile.string_kinds.contains(&child.kind()))
    }

    /// What a name was introduced as. The first declaration wins: a later assignment
    /// is the name being used, and `total = total + part` would otherwise erase the
    /// `""` that says what `total` is.
    fn note_declaration(&mut self, node: Node<'a>) {
        let named = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("declarator"))
            .or_else(|| node.child_by_field_name("left"));
        let Some(named) = named else { return };
        let named = self.see_through(named);
        let name = if self.profile.is_identifier(named) {
            self.text(named).to_owned()
        } else {
            let Some(inner) = descendants(named)
                .into_iter()
                .find(|child| self.profile.is_identifier(*child))
            else {
                return;
            };
            self.text(inner).to_owned()
        };
        let declared = node
            .child_by_field_name("type")
            .or_else(|| {
                node.parent()
                    .and_then(|parent| parent.child_by_field_name("type"))
            })
            .map(|kind| self.text(kind).to_owned())
            .or_else(|| self.declared_type(node).map(str::to_owned))
            .unwrap_or_default();
        let value = node
            .child_by_field_name("value")
            .or_else(|| node.child_by_field_name("right"))
            .map(|value| self.text(value))
            .unwrap_or_default();
        let head: String = value.chars().take(40).collect();
        self.declared
            .entry(name)
            .or_insert_with(|| format!("{declared} {head}").trim().to_owned());
    }

    fn walk(&mut self, node: Node<'a>) {
        let kind = node.kind();
        let profile = self.profile;
        let mut entered_test = false;

        // A function's own braces are not a level of nesting a reader chose; the
        // blocks written inside them are.
        let nests = profile.is_block(node)
            && node
                .parent()
                .is_some_and(|parent| !profile.is_function(parent));
        if profile.is_block(node) {
            self.note_unreachable(node);
        }
        if nests {
            self.block_depth += 1;
        }
        let entered_frame = profile.is_function(node);
        if entered_frame {
            let inherited = if profile.is_loop_callback(node, self.source) {
                self.loop_depth() + 1
            } else {
                0
            };
            self.frames.push(PerfFrame {
                is_async: profile.is_async(node),
                loops: inherited,
                statement_loops: 0,
            });
        }
        let repeats = profile.loop_kinds.contains(&kind);
        if repeats {
            self.push_loop(true);
        }
        let clauses = if profile.comprehension_kinds.contains(&kind) {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .filter(|child| child.kind() == "for_in_clause")
                .count()
                .max(1) as u32
        } else {
            0
        };
        for _ in 0..clauses {
            self.push_loop(true);
        }
        if kind == "program"
            || kind == "module"
            || kind == "source_file"
            || kind == "translation_unit"
        {
            self.note_unreachable(node);
        }
        if profile.is_function(node) {
            self.note_flag_parameters(node);
            self.note_declared_test(node);
            if let Some(name) = self.function_name(node) {
                let text = self.text(name).to_owned();
                self.name_the_body(&text, span_of(name), self.function_body(node));
            }
        }
        if profile.catch_kinds.contains(&kind)
            && node
                .child_by_field_name("body")
                .or_else(|| node.named_child(node.named_child_count().saturating_sub(1)))
                .is_some_and(|body| {
                    body.named_children(&mut body.walk())
                        .all(|child| profile.empty_body_kinds.contains(&child.kind()))
                })
        {
            self.out.empty_catches.push(Spot {
                span: span_of(node),
                detail: "catch clause".to_owned(),
                count: 1,
            });
        }
        if kind == profile.if_kind || profile.else_clause_kinds.contains(&kind) {
            self.note_condition(node);
        }
        if kind == profile.if_kind {
            self.note_branch_shape(node);
            self.note_assignment_in_condition(node);
        }
        if profile.switch_kinds.contains(&kind) {
            self.note_switch_width(node);
        }
        if profile.loop_kinds.contains(&kind) {
            self.note_redundant_jump(node);
        }
        if profile.is_function(node) {
            self.note_empty_method(node);
            self.note_exits(node);
            self.note_callback_depth(node);
        }
        // The braces too, not only the statement list inside them: a grammar that
        // separates the two emits no list at all when the braces are empty.
        if profile.is_block(node) || profile.structural_block_parents.contains(&kind) {
            self.note_empty_block(node);
        }
        if profile.binary_kinds.contains(&kind) {
            self.note_comparison(node);
        }
        if profile.declarator_kinds.contains(&kind) || profile.local_declaring_kinds.contains(&kind)
        {
            self.note_declaration(node);
        }
        if profile.await_kinds.contains(&kind) {
            self.note_await(node);
        }
        if kind == "comparison_operator" {
            self.note_membership_scan(node);
        }
        if kind == "declaration" {
            self.note_declaration(node);
            self.note_regex_declaration(node);
        }
        if profile.assignment_kinds.contains(&kind) {
            self.note_copy_accumulation(node);
        }
        if profile.is_call(node) {
            self.note_perf_call(node);
            self.note_call(node);
            if self
                .called_name(node)
                .is_some_and(|name| profile.test.is_test(&name))
            {
                self.test_depth += 1;
                entered_test = true;
            }
        }
        if profile.is_function(node)
            && self
                .function_name(node)
                .is_some_and(|name| profile.test.names_a_case(self.text(name)))
        {
            self.test_depth += 1;
            entered_test = true;
        }
        if profile.annotation_kinds.contains(&kind)
            && let Some(directive) = profile
                .suppression_directives
                .iter()
                .find(|marker| self.text(node).contains(*marker))
        {
            self.out.suppressions.push(Spot {
                span: span_of(node),
                detail: (*directive).to_owned(),
                count: 1,
            });
        }
        if profile.number_kinds.contains(&kind) {
            self.note_number(node);
        }
        if profile.binary_kinds.contains(&kind) {
            self.note_boundary(node);
        }
        if profile.string_kinds.contains(&kind) {
            let text = self.text(node).trim_matches(['"', '\'', '`']);
            if let Some(language) = foreign_language(text) {
                self.out.mixed_languages.push(Spot {
                    span: span_of(node),
                    detail: language.to_owned(),
                    count: 1,
                });
            }
            if let Some(standin) = crate::vocabulary::placeholder_value(text) {
                self.out.example_values.push(Spot {
                    span: span_of(node),
                    detail: standin.to_owned(),
                    count: 1,
                });
            }
            if self.block_depth >= self.limits.configuration_depth
                && let Some(configuration) = configuration_literal(text)
            {
                self.out.buried_configuration.push(Spot {
                    span: span_of(node),
                    detail: configuration.to_owned(),
                    count: self.block_depth,
                });
            }
        }
        if kind == "return_statement"
            && let Some(value) = node.named_child(0)
        {
            let operations = self.operations(value);
            if operations > self.limits.expression_operations {
                self.out.obscured_expressions.push(Spot {
                    span: span_of(node),
                    detail: String::new(),
                    count: operations,
                });
            }
        }
        if let Some((_, label)) = profile
            .imprecise_type_kinds
            .iter()
            .find(|(candidate, _)| *candidate == kind)
        {
            let excepted = profile
                .imprecise_type_exceptions
                .iter()
                .any(|exception| self.text(node).trim_end().ends_with(exception));
            if !excepted {
                self.out.imprecise_types.push(Spot {
                    span: span_of(node),
                    detail: (*label).to_owned(),
                    count: 1,
                });
            }
        }
        if node.named_child_count() == 0 && profile.imprecise_type_names.contains(&self.text(node))
        {
            self.out.imprecise_types.push(Spot {
                span: span_of(node),
                detail: self.text(node).to_owned(),
                count: 1,
            });
        }
        if profile.assignment_kinds.contains(&kind)
            && let Some(target) = node.child_by_field_name("left")
        {
            let target = if profile.expression_list_kinds.contains(&target.kind()) {
                target.named_child(0).unwrap_or(target)
            } else {
                target
            };
            if profile.is_identifier(target) {
                self.out.mutated_parameters.push(Spot {
                    span: span_of(target),
                    detail: self.text(target).to_owned(),
                    count: 1,
                });
            }
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.walk(child);
        }

        for _ in 0..clauses {
            self.pop_loop(true);
        }
        if repeats {
            self.pop_loop(true);
        }
        if entered_frame {
            self.frames.pop();
        }
        if entered_test {
            self.test_depth -= 1;
        }
        if nests {
            self.block_depth -= 1;
        }
    }
}

fn note_conventions(bindings: &[Binding], out: &mut Vec<(String, Span)>) {
    for binding in bindings {
        if let Some(convention) = convention_of(&binding.name) {
            out.push((convention.to_owned(), binding.span));
        }
    }
}

/// A one-letter name is perfectly readable while its whole life fits on screen, and
/// stops being readable the moment it does not — so the scope's size decides this,
/// not the name's length alone.
fn short_names(scopes: &Scopes, source: &str, scope_lines: u32) -> Vec<Spot> {
    let mut spots: Vec<Spot> = scopes
        .of_kind(BindingKind::Local)
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
fn distant_declarations(scopes: &Scopes, source: &str, distance: u32) -> Vec<Spot> {
    let mut spots: Vec<Spot> = scopes
        .of_kind(BindingKind::Local)
        .filter(|binding| !binding.name.starts_with('_'))
        .filter_map(|binding| {
            let first = binding
                .references
                .iter()
                .map(|span| span.start)
                .filter(|start| *start > binding.span.end)
                .min()?;
            let lines = source
                .get(binding.span.end as usize..first as usize)
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

fn unused_locals(scopes: &Scopes) -> Vec<Spot> {
    let mut spots: Vec<Spot> = scopes
        .of_kind(BindingKind::Local)
        .filter(|binding| binding.references.is_empty() && !binding.name.starts_with('_'))
        .map(|binding| Spot {
            span: binding.span,
            detail: binding.name.clone(),
            count: 1,
        })
        .collect();
    spots.sort_by_key(|spot| spot.span.start);
    spots
}

/// Whitespace-insensitive text, so two branches written the same way compare equal
/// however they are indented.
fn squashed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The waivers alone, for a configuration that asks for no semantic analysis: a
/// developer's `rqx-ignore` has to be honoured whichever rules are switched on.
pub fn waivers(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
) -> Vec<rqx_core::suppress::Suppression> {
    descendants(root)
        .into_iter()
        .filter(|node| profile.is_comment(*node))
        .filter_map(|comment| {
            rqx_core::suppress::Suppression::parse(text_of(comment, source), span_of(comment))
        })
        .collect()
}

fn scan_comments(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
    limits: Limits,
    out: &mut Hygiene,
) {
    let mut runs: Vec<(Span, u32)> = Vec::new();
    for comment in descendants(root)
        .into_iter()
        .filter(|node| profile.is_comment(*node))
    {
        let span = span_of(comment);
        let text = text_of(comment, source);
        if crate::vocabulary::emoji(text) {
            out.emoji.push(Spot {
                span,
                detail: String::new(),
                count: 1,
            });
        }
        if let Some(standin) = crate::vocabulary::placeholder_value(text) {
            out.example_values.push(Spot {
                span,
                detail: standin.to_owned(),
                count: 1,
            });
        }
        // Checked before the suppression-directive branch: rqx's own waiver is not
        // one of the diagnostics `suppressed-diagnostic` is meant to report.
        if let Some(waiver) = rqx_core::suppress::Suppression::parse(text, span) {
            out.waivers.push(waiver);
        } else if let Some(directive) = profile
            .suppression_directives
            .iter()
            .find(|marker| text.contains(*marker))
        {
            out.suppressions.push(Spot {
                span,
                detail: (*directive).to_owned(),
                count: 1,
            });
        } else if let Some(opening) = crate::vocabulary::chat_residue(text) {
            out.chat_residue.push(Spot {
                span,
                detail: opening.to_owned(),
                count: 1,
            });
        } else if let Some(regret) = crate::vocabulary::apology(text) {
            out.apologies.push(Spot {
                span,
                detail: regret.to_owned(),
                count: 1,
            });
        } else if crate::vocabulary::banner(text) {
            out.banner_comments.push(Spot {
                span,
                detail: String::new(),
                count: 1,
            });
        } else if let Some(marker) = journal_marker(text) {
            out.journal_comments.push(Spot {
                span,
                detail: marker.to_owned(),
                count: 1,
            });
        } else if let Some(subject) = source
            .get(span.end as usize..)
            .and_then(|rest| rest.lines().nth(1))
            .and_then(|line| redundant_subject(text, line, profile.declaration_keywords))
        {
            out.redundant_comments.push(Spot {
                span,
                detail: subject,
                count: 1,
            });
        } else if looks_like_code(
            text,
            profile.code_line_endings,
            profile.code_markers,
            profile.code_operators,
        ) {
            runs.push((span, text.lines().count() as u32));
        }
    }

    // Consecutive commented-out lines are one decision, not one finding per line.
    let mut index = 0;
    while index < runs.len() {
        let start = runs[index].0;
        let mut end = runs[index].0;
        let mut lines = runs[index].1;
        while index + 1 < runs.len()
            && source[end.end as usize..runs[index + 1].0.start as usize]
                .chars()
                .all(char::is_whitespace)
        {
            index += 1;
            end = runs[index].0;
            lines += runs[index].1;
        }
        if lines >= limits.commented_code_lines {
            out.commented_code.push(Spot {
                span: Span::new(start.start, end.end),
                detail: String::new(),
                count: lines,
            });
        }
        index += 1;
    }
}

pub fn collect(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
    scopes: &Scopes,
    limits: Limits,
    extra: &rqx_core::config::Performance,
) -> Hygiene {
    let mut walker = Walker {
        profile,
        source,
        limits,
        extra,
        out: Hygiene::default(),
        numbers: HashMap::new(),
        boundaries: HashMap::new(),
        conventions: Vec::new(),
        verbs: Vec::new(),
        declared: HashMap::new(),
        frames: vec![PerfFrame::default()],
        test_depth: 0,
        block_depth: 0,
    };
    walker.walk(root);

    let mut out = std::mem::take(&mut walker.out);
    note_conventions(&scopes.bindings, &mut walker.conventions);

    // A literal used once is usually obvious from context; the same one written three
    // times is a decision somebody will have to change in three places.
    let mut repeated: Vec<(String, Vec<Span>)> = walker
        .numbers
        .into_iter()
        .filter(|(_, spans)| spans.len() as u32 >= limits.magic_number_repeats)
        .collect();
    repeated.sort_by(|a, b| a.1[0].start.cmp(&b.1[0].start));
    for (raw, spans) in repeated {
        out.magic_numbers.push(Spot {
            span: spans[0],
            detail: raw,
            count: spans.len() as u32,
        });
    }

    let mut boundaries: Vec<(String, Vec<Span>)> = walker
        .boundaries
        .into_iter()
        .filter(|(_, spans)| spans.len() as u32 >= limits.boundary_repeats)
        .collect();
    boundaries.sort_by(|a, b| a.1[0].start.cmp(&b.1[0].start));
    for (subject, spans) in boundaries {
        out.boundary_arithmetic.push(Spot {
            span: spans[0],
            detail: subject,
            count: spans.len() as u32,
        });
    }

    // One finding per file, at the first name that broke the majority convention.
    let camel = walker
        .conventions
        .iter()
        .filter(|(style, _)| style == "camelCase")
        .count();
    let snake = walker
        .conventions
        .iter()
        .filter(|(style, _)| style == "snake_case")
        .count();
    if camel > 0 && snake > 0 {
        let minority = if camel < snake {
            "camelCase"
        } else {
            "snake_case"
        };
        if let Some((_, span)) = walker
            .conventions
            .iter()
            .find(|(style, _)| style == minority)
        {
            out.mixed_conventions.push(Spot {
                span: *span,
                detail: minority.to_owned(),
                count: camel.min(snake) as u32,
            });
        }
    }

    // One finding per concept: the point is that a reader meets it twice, not that a
    // particular name is wrong.
    let mut by_object: HashMap<String, Vec<(String, Span)>> = HashMap::new();
    for (verb, object, span) in walker.verbs {
        by_object.entry(object).or_default().push((verb, span));
    }
    let mut inconsistent: Vec<Spot> = by_object
        .into_iter()
        .filter_map(|(object, uses)| {
            let (first_verb, first_span) = uses.first()?.clone();
            let clash = uses
                .iter()
                .find(|(verb, _)| *verb != first_verb && same_family(&first_verb, verb))?;
            Some(Spot {
                span: first_span,
                detail: format!("{first_verb}/{} for {object}", clash.0),
                count: uses.len() as u32,
            })
        })
        .collect();
    inconsistent.sort_by_key(|spot| spot.span.start);
    out.inconsistent_vocabulary = inconsistent;

    out.unused_locals = unused_locals(scopes);
    out.short_names = short_names(scopes, source, limits.short_name_scope_lines);
    out.distant_declarations =
        distant_declarations(scopes, source, limits.declaration_distance_lines);
    scan_comments(profile, source, root, limits, &mut out);

    // A parameter that is only ever reassigned is a different finding from a local
    // that is, so the list is narrowed to actual parameters here.
    let parameters: Vec<&str> = scopes
        .of_kind(BindingKind::Parameter)
        .map(|binding| binding.name.as_str())
        .collect();
    out.mutated_parameters
        .retain(|spot| parameters.contains(&spot.detail.as_str()));
    out.mutated_parameters.dedup_by(|a, b| a.detail == b.detail);
    out
}

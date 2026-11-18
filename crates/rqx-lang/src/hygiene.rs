use std::collections::{HashMap, HashSet};

use rqx_core::facts::{Hygiene, Limits, Spot};
use rqx_core::rule::RuleId;
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::names::Names;
use crate::profile::LanguageProfile;
use crate::scope::{Binding, BindingKind, Scopes};
use crate::tree::{
    Nodes, descendants, find_within, kind_of, operands_of, operator_of, span_of, text_of,
};
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

/// Functions in one file that assert, each with the stretch of source that can call
/// it: the whole file for one declared at the top or in a class, its enclosing
/// function for one declared inside a function.
#[derive(Default)]
struct Helpers {
    known: Vec<(String, Option<std::ops::Range<usize>>)>,
}

impl Helpers {
    fn reaches(&self, name: &str, call: Node<'_>) -> bool {
        self.known.iter().any(|(known, scope)| {
            known == name
                && scope
                    .as_ref()
                    .is_none_or(|scope| scope.contains(&call.start_byte()))
        })
    }

    fn is_empty(&self) -> bool {
        self.known.is_empty()
    }
}

/// What one statement in the arm of an `if` inside a test does to the test.
enum Ending {
    /// Fails or skips it.
    Fails,
    /// Leaves the arm without doing anything of its own: a `return`, or a `t.Logf`.
    Leaves,
    /// Carries on with work of its own, so the arm is a path the test takes.
    Continues,
}

struct Walker<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    nodes: &'a Nodes<'a>,
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
    /// The names whose declaration spells a text type, as opposed to one that merely
    /// started out holding a piece of text.
    text_declared: HashSet<String>,
    frames: Vec<PerfFrame>,
    test_depth: u32,
    /// Whether anything in this file turned out to be a test. Known only once the
    /// walk is over, so the rules that need it run after it.
    holds_tests: bool,
    block_depth: u32,
    /// Functions in this file that assert, so a test calling one asserts through it.
    asserting_helpers: Helpers,
}

impl<'a> Walker<'a> {
    fn names(&self) -> Names<'a> {
        Names::new(self.profile, self.source)
    }

    fn text(&self, node: Node<'a>) -> &'a str {
        text_of(node, self.source)
    }

    fn see_through(&self, node: Node<'a>) -> Node<'a> {
        self.names().see_through(node)
    }

    fn callee_name(&self, node: Node<'a>) -> Option<String> {
        self.names().callee_name(node)
    }

    fn member_property(&self, node: Node<'a>) -> Option<Node<'a>> {
        self.names().member_property(node)
    }

    fn called_name(&self, call: Node<'a>) -> Option<String> {
        self.names().called_name(call)
    }

    /// What a call is named by, receiver included and arguments left out: the part of
    /// a call a finding is about when the call is the whole statement.
    fn callee_span(&self, call: Node<'a>) -> Option<Span> {
        let callee = call.child_by_field_name(self.profile.callee_field)?;
        (self.profile.is_call(call) && callee.end_byte() > call.start_byte())
            .then(|| Span::new(call.start_byte() as u32, callee.end_byte() as u32))
    }

    /// The name a declaration is introduced by, as the part of it a finding is about.
    fn focus_on_name(&mut self, rule: RuleId, declaration: Node<'a>) {
        if let Some(name) = self.function_name(declaration) {
            self.out.focus(rule, span_of(declaration), span_of(name));
        }
    }

    /// How many separate decisions one condition holds. A run of the same operator is
    /// still one decision per operand to a reader.
    fn operands(&self, node: Node<'a>) -> u32 {
        let node = self.see_through(node);
        if self.profile.binary_kinds.contains(&kind_of(node))
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
                self.profile.binary_kinds.contains(&kind_of(inner))
                    || self.profile.is_call(*inner)
                    || kind_of(inner) == "ternary_expression"
                    || kind_of(inner) == "conditional_expression"
            })
            .count() as u32
    }

    /// One node that checks something, whether the language spells a check as a call
    /// or as a statement of its own.
    fn asserts(&self, node: Node<'a>) -> bool {
        if self
            .profile
            .assertion_statement_kinds
            .contains(&kind_of(node))
        {
            return true;
        }
        self.profile.is_call(node)
            && self
                .called_name(node)
                .is_some_and(|name| self.profile.test.is_assertion(&name))
    }

    /// The names a runner's handle goes by inside `scope`: the test's own `t`, and the
    /// `t` of every subtest written in it.
    fn test_handles(&self, scope: Node<'a>) -> Vec<&'a str> {
        let profile = self.profile;
        if profile.test.handle_types.is_empty() {
            return Vec::new();
        }
        let mut handles = Vec::new();
        for parameter in descendants(scope)
            .into_iter()
            .filter(|node| profile.parameter_kinds.contains(&kind_of(node)))
        {
            let typed_as_handle = parameter
                .child_by_field_name("type")
                .is_some_and(|written| profile.test.is_handle_type(self.text(written)));
            if typed_as_handle {
                let mut cursor = parameter.walk();
                handles.extend(
                    parameter
                        .children_by_field_name("name", &mut cursor)
                        .map(|name| self.text(name)),
                );
            }
        }
        handles
    }

    /// Looks for a check anywhere in the test's body rather than tracking a stack,
    /// because a subtest or a closure written in the test is still the test checking.
    fn body_verifies(&self, body: Node<'a>, handles: &[&str], helpers: &Helpers) -> bool {
        descendants(body)
            .into_iter()
            .any(|node| self.verifies(node, handles, helpers))
    }

    /// One node that checks something on the test's behalf: an assertion, a call that
    /// fails the test, a call that hands the runner's handle to a helper, or a call to
    /// a function in this file that asserts.
    fn verifies(&self, node: Node<'a>, handles: &[&str], helpers: &Helpers) -> bool {
        let profile = self.profile;
        let vocabulary = &profile.test;
        let kind = kind_of(node);
        if profile.assertion_statement_kinds.contains(&kind)
            || vocabulary.assertion_kinds.contains(&kind)
        {
            return true;
        }
        if vocabulary.misread_call_kinds.contains(&kind) {
            return node
                .child_by_field_name("declarator")
                .is_some_and(|macro_name| vocabulary.checks(self.text(macro_name)));
        }
        if !profile.is_call(node) {
            return false;
        }
        let named = self.called_name(node).is_some_and(|name| {
            vocabulary.checks(&name)
                || vocabulary.fails_on_handle(&name, handles)
                || self.names_a_helper(&name, node, helpers)
        });
        named || self.hands_on_a_handle(node, handles) || self.hands_on_a_helper(node, helpers)
    }

    /// `t.Run(name, tc.test)`: the subtest is a helper passed by name.
    fn hands_on_a_helper(&self, call: Node<'a>, helpers: &Helpers) -> bool {
        let profile = self.profile;
        !helpers.is_empty()
            && call
                .child_by_field_name("arguments")
                .is_some_and(|arguments| {
                    let mut cursor = arguments.walk();
                    arguments.named_children(&mut cursor).any(|argument| {
                        (profile.is_identifier(argument) || profile.is_member_access(argument))
                            && self
                                .text(argument)
                                .rsplit('.')
                                .next()
                                .is_some_and(|name| helpers.reaches(name, call))
                    })
                })
    }

    /// A helper is called bare or on the object it is a method of. Anything else
    /// named the same belongs to some other type.
    fn names_a_helper(&self, name: &str, call: Node<'a>, helpers: &Helpers) -> bool {
        match name.rsplit_once('.') {
            None => helpers.reaches(name, call),
            Some((receiver, method)) => {
                self.profile.self_names.contains(&receiver) && helpers.reaches(method, call)
            }
        }
    }

    fn hands_on_a_handle(&self, call: Node<'a>, handles: &[&str]) -> bool {
        !handles.is_empty()
            && call
                .child_by_field_name("arguments")
                .is_some_and(|arguments| {
                    let mut cursor = arguments.walk();
                    arguments.named_children(&mut cursor).any(|argument| {
                        self.profile.is_identifier(argument)
                            && handles.contains(&self.text(argument))
                    })
                })
    }

    /// The name a function is called by, whether it is declared with one or bound to
    /// one: `const expectSame = () => ...`.
    fn defined_name(&self, function: Node<'a>) -> Option<&'a str> {
        if let Some(name) = self.function_name(function) {
            return Some(self.text(name));
        }
        let parent = function.parent()?;
        self.profile
            .binding_fields
            .iter()
            .find_map(|field| parent.child_by_field_name(field))
            .filter(|name| self.profile.is_identifier(*name))
            .map(|name| self.text(name))
    }

    /// Functions in this file that assert. Settled over a few rounds, because one
    /// helper can assert through another.
    fn find_asserting_helpers(&self) -> Helpers {
        const ROUNDS: usize = 3;
        let profile = self.profile;
        let functions: Vec<(&'a str, Node<'a>, Node<'a>)> = self
            .nodes
            .iter()
            .filter(|node| profile.is_function(*node))
            .filter_map(|function| {
                Some((
                    self.defined_name(function)?,
                    function,
                    self.function_body(function)?,
                ))
            })
            .collect();
        let mut helpers = Helpers::default();
        let mut settled = vec![false; functions.len()];
        for _ in 0..ROUNDS {
            let found: Vec<usize> = functions
                .iter()
                .enumerate()
                .filter(|(at, _)| !settled[*at])
                .filter(|(_, (_, function, body))| {
                    self.body_verifies(*body, &self.test_handles(*function), &helpers)
                })
                .map(|(at, _)| at)
                .collect();
            if found.is_empty() {
                break;
            }
            for at in found {
                settled[at] = true;
                let (name, function, _) = functions[at];
                let scope = self
                    .enclosing_function(function)
                    .map(|outer| outer.start_byte()..outer.end_byte());
                helpers.known.push((name.to_owned(), scope));
            }
        }
        helpers
    }

    /// `if got != want { t.Errorf(...) }`: an `if` whose every arm only fails or skips
    /// the test is a check written out by hand, not a path the test takes.
    fn only_fails(&self, branch: Node<'a>, handles: &[&str]) -> bool {
        let profile = self.profile;
        let Some(consequence) = branch.child_by_field_name("consequence") else {
            return false;
        };
        let helpers = &self.asserting_helpers;
        // `if !assert.NoError(t, err) { return }` checks in the condition and leaves.
        let condition_checks = branch
            .child_by_field_name(profile.condition_field)
            .is_some_and(|condition| self.body_verifies(condition, handles, helpers));
        let mut arms = vec![consequence];
        let mut cursor = branch.walk();
        for alternative in branch.children_by_field_name("alternative", &mut cursor) {
            let arm = if profile.is_block(alternative)
                || !profile.else_clause_kinds.contains(&kind_of(alternative))
            {
                Some(alternative)
            } else {
                alternative
                    .child_by_field_name("consequence")
                    .or_else(|| alternative.child_by_field_name("body"))
                    .or_else(|| alternative.named_child(0))
            };
            // An `else if` is an `if` of its own and is judged on its own.
            match arm {
                Some(arm) if kind_of(arm) == profile.if_kind => {}
                Some(arm) => arms.push(arm),
                None => return false,
            }
        }
        arms.into_iter()
            .all(|arm| self.arm_only_fails(arm, condition_checks, handles))
    }

    fn arm_only_fails(&self, arm: Node<'a>, condition_checks: bool, handles: &[&str]) -> bool {
        let profile = self.profile;
        let statements: Vec<Node<'a>> = if profile.is_block(arm) {
            let mut cursor = arm.walk();
            arm.named_children(&mut cursor)
                .filter(|child| !profile.is_comment(*child))
                .collect()
        } else {
            vec![arm]
        };
        let mut fails = condition_checks;
        for statement in statements {
            match self.ending_of(statement, handles) {
                Ending::Fails => fails = true,
                Ending::Leaves => {}
                Ending::Continues => return false,
            }
        }
        fails
    }

    fn ending_of(&self, statement: Node<'a>, handles: &[&str]) -> Ending {
        let profile = self.profile;
        let kind = kind_of(statement);
        if profile.throw_kinds.contains(&kind) {
            return Ending::Fails;
        }
        if matches!(
            kind,
            "return_statement" | "continue_statement" | "break_statement"
        ) {
            return Ending::Leaves;
        }
        let names: Vec<String> = descendants(statement)
            .into_iter()
            .filter(|node| profile.is_call(*node))
            .filter_map(|call| self.called_name(call))
            .collect();
        let vocabulary = &profile.test;
        if names.iter().any(|name| {
            vocabulary.fails(name)
                || vocabulary.skips(name)
                || vocabulary.fails_on_handle(name, handles)
        }) {
            return Ending::Fails;
        }
        // `t.Logf` before `t.FailNow()` says why the test is about to stop.
        let on_handle = |name: &String| {
            name.rsplit_once('.')
                .is_some_and(|(receiver, _)| handles.contains(&receiver))
        };
        if !names.is_empty() && names.iter().all(on_handle) {
            return Ending::Leaves;
        }
        Ending::Continues
    }

    /// `for _, tt := range tests`: a loop over a table the test wrote out runs one
    /// case per row rather than choosing between paths. So does the loop a benchmark
    /// runner drives through its handle: `for i := 0; i < b.N; i++`.
    fn iterates_a_table(&self, loop_node: Node<'a>, scope: Node<'a>, handles: &[&str]) -> bool {
        let profile = self.profile;
        let body = loop_node.child_by_field_name("body");
        let mut cursor = loop_node.walk();
        let driven_by_runner = !handles.is_empty()
            && loop_node
                .named_children(&mut cursor)
                .filter(|child| Some(*child) != body)
                .flat_map(descendants)
                .any(|node| profile.is_identifier(node) && handles.contains(&self.text(node)));
        if driven_by_runner {
            return true;
        }
        let Some(collection) = self.iterated_collection(loop_node) else {
            return false;
        };
        if self.is_table(collection) {
            return true;
        }
        self.table_name(collection)
            .is_some_and(|name| self.bound_to_table(name, scope))
    }

    fn iterated_collection(&self, loop_node: Node<'a>) -> Option<Node<'a>> {
        self.profile.loops.each.iter().find_map(|(kind, field)| {
            let header = if kind_of(loop_node) == *kind {
                Some(loop_node)
            } else {
                let mut cursor = loop_node.walk();
                loop_node
                    .named_children(&mut cursor)
                    .find(|child| kind_of(child) == *kind)
            };
            header?
                .child_by_field_name(field)
                .map(|collection| self.see_through(collection))
        })
    }

    /// The name a loop reads its rows from: `cases`, or `cases` in `cases.items()`.
    fn table_name(&self, collection: Node<'a>) -> Option<&'a str> {
        let profile = self.profile;
        if profile.is_identifier(collection) {
            return Some(self.text(collection));
        }
        if !profile.is_call(collection) {
            return None;
        }
        let name = self.called_name(collection)?;
        let (receiver, method) = name.rsplit_once('.').unwrap_or(("", &name));
        let derived_from_receiver = !receiver.is_empty()
            && !receiver.contains('.')
            && profile.loops.derived_methods.contains(&method);
        if derived_from_receiver {
            return descendants(collection)
                .into_iter()
                .find(|node| profile.is_identifier(*node) && self.text(*node) == receiver)
                .map(|node| self.text(node));
        }
        if profile.loops.derived_functions.contains(&name.as_str()) {
            let arguments = collection.child_by_field_name("arguments")?;
            let first = arguments.named_child(0)?;
            return profile.is_identifier(first).then(|| self.text(first));
        }
        None
    }

    fn is_table(&self, node: Node<'a>) -> bool {
        let node = self.see_through(node);
        let vocabulary = &self.profile.test;
        vocabulary.table_kinds.contains(&kind_of(node))
            || (self.profile.is_call(node)
                && self
                    .called_name(node)
                    .is_some_and(|name| vocabulary.builds_table(&name)))
    }

    /// Whether `name` is given a table anywhere in `scope`, in any of the shapes a
    /// declaration or an assignment takes.
    fn bound_to_table(&self, name: &str, scope: Node<'a>) -> bool {
        let profile = self.profile;
        descendants(scope).into_iter().any(|node| {
            let target = ["left", "name", "declarator"]
                .iter()
                .find_map(|field| node.child_by_field_name(field));
            let value = ["right", "value"]
                .iter()
                .find_map(|field| node.child_by_field_name(field));
            let (Some(target), Some(value)) = (target, value) else {
                return false;
            };
            let targets = self.listed(target);
            let values = self.listed(value);
            targets
                .iter()
                .position(|target| {
                    let named = if profile.is_identifier(*target) {
                        Some(*target)
                    } else {
                        descendants(*target)
                            .into_iter()
                            .find(|inner| profile.is_identifier(*inner))
                    };
                    named.is_some_and(|named| self.text(named) == name)
                })
                .and_then(|at| values.get(at))
                .is_some_and(|value| self.is_table(*value))
        })
    }

    /// Both sides of `a, b := x, y` as lists, and either side of `a = x` as a list of one.
    fn listed(&self, node: Node<'a>) -> Vec<Node<'a>> {
        if self.profile.expression_list_kinds.contains(&kind_of(node)) {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).collect()
        } else {
            vec![node]
        }
    }

    /// A query that writes is only interesting when the write outlives the call, so
    /// assignments to a local are not counted — a field or a captured name is.
    fn body_mutates(&self, body: Node<'a>) -> bool {
        descendants(body)
            .into_iter()
            .filter(|node| self.profile.assignment_kinds.contains(&kind_of(node)))
            .any(|assignment| {
                assignment
                    .child_by_field_name("left")
                    .is_some_and(|target| {
                        let target = if self
                            .profile
                            .expression_list_kinds
                            .contains(&kind_of(target))
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

    fn function_name(&self, node: Node<'a>) -> Option<Node<'a>> {
        self.names().function_name(node)
    }

    fn function_body(&self, node: Node<'a>) -> Option<Node<'a>> {
        self.names().function_body(node)
    }

    fn note_flag_parameters(&mut self, function: Node<'a>) {
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

    fn declared_type(&self, parameter: Node<'a>) -> Option<&'a str> {
        let annotation = descendants(parameter)
            .into_iter()
            .find(|child| self.profile.type_annotation_kinds.contains(&kind_of(child)))?;
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
                .find(|(kind, _)| *kind == kind_of(statement))
            else {
                continue;
            };
            if let Some(next) = statements.get(index + 1)
                && !self.profile.hoisted_kinds.contains(&kind_of(next))
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
        let profile = self.profile;
        let vocabulary: &TestVocabulary = &profile.test;
        // A constructor of a class whose name starts like a case builds a value.
        if profile.constructor_kinds.contains(&kind_of(function)) {
            return;
        }
        let Some(name) = self.function_name(function) else {
            return;
        };
        let mut label = self.text(name).to_owned();
        if !vocabulary.names_a_case(&label) {
            return;
        }
        let measures = vocabulary.measures(&label);
        // A framework whose case is a macro puts the name in the arguments, so the
        // declaration itself is called nothing but the macro.
        if vocabulary.case_prefixes.contains(&label.as_str())
            && let Some(list) = crate::tree::find_within(
                function,
                profile.parameter_list_kinds,
                profile.block_kinds,
            )
            .into_iter()
            .next_back()
        {
            // The last one is the case; whatever comes before it is the suite.
            if let Some(spelled) = descendants(list)
                .into_iter()
                .rfind(|node| profile.is_identifier(*node))
            {
                label = self.text(spelled).to_owned();
            }
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
        let expects = self
            .annotations(function)
            .iter()
            .any(|annotation| vocabulary.expects_by_annotation(annotation));
        if let Some(body) = self.function_body(function) {
            let handles = self.test_handles(function);
            let checks =
                measures || expects || self.body_verifies(body, &handles, &self.asserting_helpers);
            if !checks {
                self.out.assertionless_tests.push(Spot {
                    span: span_of(name),
                    detail: label.clone(),
                    count: 1,
                });
            }
            self.note_test_body(body, &label, true, &handles);
        }
    }

    fn annotations(&self, function: Node<'a>) -> Vec<String> {
        let mut found = Vec::new();
        if let Some(parent) = function.parent()
            && self.profile.decorated_kinds.contains(&kind_of(parent))
        {
            let mut cursor = parent.walk();
            found.extend(
                parent
                    .named_children(&mut cursor)
                    .filter(|child| self.profile.decorator_kinds.contains(&kind_of(child)))
                    .map(|child| self.text(child).to_owned()),
            );
        }
        let mut cursor = function.walk();
        for container in function.named_children(&mut cursor).filter(|child| {
            self.profile
                .modifier_container_kinds
                .contains(&kind_of(child))
        }) {
            found.extend(
                descendants(container)
                    .into_iter()
                    .filter(|child| self.profile.decorator_kinds.contains(&kind_of(child)))
                    .map(|child| self.text(child).to_owned()),
            );
        }
        found
    }

    fn note_call(&mut self, call: Node<'a>) {
        let vocabulary: &TestVocabulary = &self.profile.test;
        if let Some(name) = self.called_name(call) {
            if let Some(writer) = self
                .profile
                .debug_writers
                .iter()
                .find(|writer| **writer == name)
            {
                let spot = Spot {
                    span: span_of(call),
                    detail: (*writer).to_owned(),
                    count: 1,
                };
                if self.test_depth == 0 {
                    self.out.debug_output.push(spot);
                } else {
                    self.out.test_debug_output.push(spot);
                }
            }
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
                // A registration hands the runner a function. Anything else written
                // on the same name, `test.assertEqual(...)`, is a call on a value.
                let body = arguments
                    .and_then(|list| list.named_child(1))
                    .filter(|body| self.profile.is_function(*body));
                if let Some(body) = body {
                    // A framework that takes the case's name as a string gives the
                    // reader a name; one that takes a bare identifier gives the call.
                    let written = arguments
                        .and_then(|list| list.named_child(0))
                        .filter(|node| self.profile.string_kinds.contains(&kind_of(node)))
                        .map(|node| self.text(node).trim_matches(['"', '\'', '`']).to_owned());
                    let named = written.is_some();
                    let label = written.unwrap_or_else(|| name.clone());
                    let handles = self.test_handles(body);
                    if !self.body_verifies(body, &handles, &self.asserting_helpers) {
                        self.out.assertionless_tests.push(Spot {
                            span: span_of(call),
                            detail: label.clone(),
                            count: 1,
                        });
                    }
                    self.note_test_body(body, &label, named, &handles);
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
                    .find(|node| self.profile.number_kinds.contains(&kind_of(node)))
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
            || self.is_declared_value(node)
        {
            return;
        }
        self.numbers
            .entry(raw.to_owned())
            .or_default()
            .push(span_of(node));
    }

    /// A literal that is the whole value of a declaration, as in `const MAX_RETRIES = 3`,
    /// already has the name the rule asks for. A sign, parentheses or a list of values
    /// around it do not change that; an expression does.
    fn is_declared_value(&self, literal: Node<'a>) -> bool {
        let profile = self.profile;
        let mut value = literal;
        let mut owner = literal.parent();
        while let Some(parent) = owner {
            let kind = kind_of(parent);
            let wraps = profile.negation_kinds.contains(&kind)
                || profile.transparent_kinds.contains(&kind)
                || profile.expression_list_kinds.contains(&kind);
            if !wraps {
                break;
            }
            value = parent;
            owner = parent.parent();
        }
        owner.is_some_and(|owner| {
            let kind = kind_of(owner);
            let declares = profile.declarator_kinds.contains(&kind)
                || profile.local_declaring_kinds.contains(&kind)
                || profile.field_kinds.contains(&kind);
            declares
                && ["value", "right", "default_value"]
                    .iter()
                    .any(|field| owner.child_by_field_name(field) == Some(value))
        })
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
                if self.text(alternative).starts_with("else") {
                    let keyword = alternative.start_byte() as u32;
                    self.out.focus(
                        RuleId::RedundantElse,
                        span_of(alternative),
                        Span::new(keyword, keyword + "else".len() as u32),
                    );
                }
            }
            // Some languages make the alternative the block itself and others wrap it
            // in an `else` clause; only the wrapper is worth seeing through.
            let other = self.body(alternative);
            if kind_of(other) != profile.if_kind
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
            && kind_of(inner) == profile.if_kind
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
            kind_of(candidate) == profile.if_kind
                || profile.else_clause_kinds.contains(&kind_of(candidate))
                || profile.loop_kinds.contains(&kind_of(candidate))
        };
        let decides = governs(parent)
            || (profile.structural_block_parents.contains(&kind_of(parent))
                && parent.parent().is_some_and(governs));
        // Reported once: where both the braces and a list inside them are nodes, the
        // list is the one that carries the statements.
        if profile.structural_block_parents.contains(&kind_of(node))
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
            detail: kind_of(parent).replace('_', " "),
            count: 1,
        });
    }

    /// A comparison that has already decided: an operand against itself, or against
    /// the literal it is being asked whether it equals.
    /// A statement whose whole content is a value nobody keeps. Deliberately narrow:
    /// a call may be there for its effect and a string may be a docstring or a
    /// `"use strict"` directive, so only a number, a name, or a comparison counts.
    fn note_discarded_expression(&mut self, node: Node<'a>) {
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
        // A literal with a fractional part: `0.1`, `1.5e3`. An integer literal
        // compares exactly whatever the variable's type is, so only the decimal point
        // makes this certain without knowing the types.
        for side in [left_text, right_text] {
            if is_fractional(side) {
                self.out.float_equalities.push(Spot {
                    span: span_of(node),
                    detail: side.to_owned(),
                    count: 1,
                });
                return;
            }
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
        if profile.constructor_kinds.contains(&kind_of(function))
            || self.initialises_outside_its_body(function)
        {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        // `empty_body_kinds` is how each language spells nothing: a comment
        // everywhere, and `pass` or `...` in Python.
        let empty = body
            .named_children(&mut body.walk())
            .all(|child| profile.empty_body_kinds.contains(&kind_of(child)));
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

    /// A constructor whose work is written outside the braces: a C++ member
    /// initialiser list, `Gauge(int scale) : scale(scale) {}`, or TypeScript parameter
    /// properties, `constructor(private readonly clock: Clock) {}`.
    fn initialises_outside_its_body(&self, function: Node<'a>) -> bool {
        let profile = self.profile;
        let mut cursor = function.walk();
        if function
            .named_children(&mut cursor)
            .any(|child| kind_of(child) == "field_initializer_list")
        {
            return true;
        }
        crate::parameters::input_list(profile, function).is_some_and(|list| {
            descendants(list)
                .into_iter()
                .any(|node| profile.field_declaring_modifiers.contains(&kind_of(node)))
        })
    }

    /// A `continue` or `break` that control was reaching anyway. The keyword makes a
    /// reader stop and look for the condition that is not there.
    fn note_redundant_jump(&mut self, loop_node: Node<'a>) {
        let Some(holder) = self.loop_statements(loop_node) else {
            return;
        };
        let mut cursor = holder.walk();
        let Some(last) = holder.named_children(&mut cursor).last() else {
            return;
        };
        // `break` ends a loop early and `continue` skips the rest, and at the end of
        // the body both do exactly what falling off the end does — unless they carry
        // a label, which is a jump out of somewhere else entirely.
        let bare = last.named_child_count() == 0;
        if bare && kind_of(last) == "continue_statement" {
            self.out.redundant_jumps.push(Spot {
                span: span_of(last),
                detail: "continue".to_owned(),
                count: 1,
            });
        }
    }

    /// The run of statements a loop repeats. Go puts a `statement_list` inside the
    /// `block` a loop's body field names, so the statements are one level deeper
    /// than that field suggests.
    fn loop_statements(&self, loop_node: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        let body = self.function_body(loop_node).or_else(|| {
            loop_node
                .named_children(&mut loop_node.walk())
                .find(|child| profile.is_block(*child))
        })?;
        Some(
            body.named_children(&mut body.walk())
                .find(|child| profile.is_block(*child))
                .unwrap_or(body),
        )
    }

    /// A loop that cannot reach a second iteration: the last thing it does is leave,
    /// and nothing inside it starts the next pass.
    fn note_single_pass_loop(&mut self, loop_node: Node<'a>) {
        let profile = self.profile;
        let Some(holder) = self.loop_statements(loop_node) else {
            return;
        };
        let Some(last) = holder.named_children(&mut holder.walk()).last() else {
            return;
        };
        let leaves = matches!(kind_of(last), "return_statement" | "break_statement");
        // A labelled break leaves an outer loop, so this one is not the loop that
        // stops.
        if !leaves || last.named_child_count() > 0 && kind_of(last) == "break_statement" {
            return;
        }
        let restarts = descendants(loop_node).into_iter().any(|node| {
            kind_of(node) == "continue_statement"
                && !Self::inside_nested_loop(node, loop_node, profile)
        });
        if restarts {
            return;
        }
        self.out.single_pass_loops.push(Spot {
            span: span_of(last),
            detail: kind_of(last).trim_end_matches("_statement").to_owned(),
            count: 1,
        });
    }

    fn inside_nested_loop(node: Node<'a>, outer: Node<'a>, profile: &LanguageProfile) -> bool {
        let mut parent = node.parent();
        while let Some(current) = parent {
            if current.id() == outer.id() {
                return false;
            }
            if profile.loop_kinds.contains(&kind_of(current)) || profile.is_function(current) {
                return true;
            }
            parent = current.parent();
        }
        false
    }

    /// Reported on the inner conditional, which is where a reader has already lost
    /// track of the first condition.
    fn note_nested_ternary(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let mut parent = node.parent();
        while let Some(current) = parent {
            if profile.ternary_kinds.contains(&kind_of(current)) {
                self.out.nested_ternaries.push(Spot {
                    span: span_of(node),
                    detail: self.snippet(node),
                    count: 1,
                });
                return;
            }
            if profile.is_statement(current) || profile.is_function(current) {
                return;
            }
            parent = current.parent();
        }
    }

    /// Two different logical operators in one condition with nothing written to say
    /// which binds tighter.
    fn note_ambiguous_precedence(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(outer) = operator_of(node, self.source) else {
            return;
        };
        if !profile.logical_operators.contains(&outer) {
            return;
        }
        let Some((left, right)) = operands_of(node) else {
            return;
        };
        for side in [left, right] {
            if !profile.binary_kinds.contains(&kind_of(side)) {
                continue;
            }
            let Some(inner) = operator_of(side, self.source) else {
                continue;
            };
            if profile.logical_operators.contains(&inner) && inner != outer {
                self.out.ambiguous_precedence.push(Spot {
                    span: span_of(node),
                    detail: format!("{inner} inside {outer}"),
                    count: 1,
                });
                return;
            }
        }
    }

    fn note_negated_comparison(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(operator) = operator_of(node, self.source) else {
            return;
        };
        if !profile.negation_operators.contains(&operator) {
            return;
        }
        let Some(operand) = node.named_child(0) else {
            return;
        };
        let inner = self.see_through(operand);
        if !profile.binary_kinds.contains(&kind_of(inner)) {
            return;
        }
        let Some(compared) = operator_of(inner, self.source) else {
            return;
        };
        if !profile.equality_operators.contains(&compared) {
            return;
        }
        self.out.negated_comparisons.push(Spot {
            span: span_of(node),
            detail: compared.to_owned(),
            count: 1,
        });
    }

    /// A switch wide enough to be worth writing as a switch, with nothing said about
    /// the values it does not name.
    fn note_missing_default(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let mut cursor = node.walk();
        let mut stack: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
        let mut arms = 0_u32;
        let mut catches_the_rest = false;
        while let Some(current) = stack.pop() {
            if profile.case_kinds.contains(&kind_of(current)) {
                arms += 1;
                let text = self.text(current).trim_start();
                if profile
                    .default_case_markers
                    .iter()
                    .any(|marker| text.starts_with(marker))
                {
                    catches_the_rest = true;
                }
                continue;
            }
            if profile.switch_kinds.contains(&kind_of(current)) {
                continue;
            }
            let mut inner = current.walk();
            stack.extend(current.named_children(&mut inner));
        }
        if arms >= self.limits.min_cases && !catches_the_rest {
            self.out.defaultless_switches.push(Spot {
                span: span_of(node),
                detail: arms.to_string(),
                count: arms,
            });
        }
    }

    /// One line of a node's text, short enough to read inside a finding.
    fn snippet(&self, node: Node<'a>) -> String {
        let line = self.text(node).lines().next().unwrap_or("").trim();
        match line.char_indices().nth(48) {
            Some((at, _)) => format!("{}…", &line[..at]),
            None => line.to_owned(),
        }
    }

    /// A catch that catches only to raise the same thing again. The handler reads as
    /// a decision and makes none.
    fn note_rethrow_only_catch(&mut self, node: Node<'a>) {
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
    fn note_placeholder_body(&mut self, function: Node<'a>) {
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

    /// A function that takes arguments only to hand them on unchanged. The caller
    /// could have called the thing itself.
    fn note_passthrough(&mut self, function: Node<'a>) {
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

    /// The call a statement is, whether it is returned or stands on its own. Only the
    /// statement's own value counts: a call buried inside a larger expression means
    /// the body is doing something with the answer.
    fn only_call(&self, statement: Node<'a>) -> Option<Node<'a>> {
        let mut value = self.see_through(statement.named_child(0)?);
        // Go wraps a returned value in a list of one, which is not a step the author
        // wrote.
        while self.profile.expression_list_kinds.contains(&kind_of(value))
            && value.named_child_count() == 1
        {
            value = self.see_through(value.named_child(0)?);
        }
        self.profile.is_call(value).then_some(value)
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

    fn note_wildcard_import(&mut self, node: Node<'a>) {
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

    /// A function that answers with a value on one path and with nothing on another.
    fn note_mixed_return(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let (mut valued, mut bare) = (0_u32, 0_u32);
        let mut cursor = body.walk();
        let mut stack: Vec<Node<'a>> = body.children(&mut cursor).collect();
        let mut first_bare = None;
        while let Some(node) = stack.pop() {
            // A callback answers for itself, not for the function it was written in.
            if profile.is_function(node) {
                continue;
            }
            if kind_of(node) == "return_statement" {
                if node.named_child_count() == 0 {
                    bare += 1;
                    first_bare = Some(node);
                } else {
                    valued += 1;
                }
            }
            let mut inner = node.walk();
            stack.extend(node.children(&mut inner));
        }
        if let Some(node) = first_bare
            && valued > 0
            && bare > 0
        {
            self.out.mixed_returns.push(Spot {
                span: span_of(node),
                detail: format!("{valued} with a value, {bare} without"),
                count: valued + bare,
            });
        }
    }

    /// A jump out of the clause that runs on the way out. It discards whatever the
    /// block was leaving with, including a failure nobody will ever see.
    fn note_exit_from_finally(&mut self, clause: Node<'a>) {
        let profile = self.profile;
        // A `return` inside a callback leaves the callback, not the clause.
        for node in crate::tree::descendants_outside(clause, |node| profile.is_function(node)) {
            let leaves = matches!(
                kind_of(node),
                "return_statement" | "break_statement" | "continue_statement"
            ) || profile.throw_kinds.contains(&kind_of(node));
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

    /// A handler that names the root of the error hierarchy, or names nothing, takes
    /// every failure including the ones it was never written for.
    fn note_catch_all(&mut self, catch: Node<'a>) {
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
    fn note_generic_throw(&mut self, node: Node<'a>) {
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
    fn note_library_exit(&mut self, call: Node<'a>) {
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

    /// A literal `true` at a call site says nothing about what it turns on; the
    /// reader has to open the callee to find out.
    fn note_boolean_argument(&mut self, call: Node<'a>) {
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
    fn note_many_locals(&mut self, function: Node<'a>) {
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

    /// A guarded block big enough that no single line in it is the one being
    /// guarded.
    fn note_wide_try(&mut self, node: Node<'a>) {
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

    /// A type that inherits and adds nothing, so every use of it could have used
    /// what it came from.
    fn note_class_shape(&mut self, class: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.class_body(class) else {
            return;
        };
        let members: Vec<Node<'a>> = body
            .named_children(&mut body.walk())
            .filter(|child| !profile.is_comment(*child))
            .map(|child| {
                // A decorated member is wrapped, and the wrapper is not the member.
                if profile.decorated_kinds.contains(&kind_of(child)) {
                    child
                        .named_child(child.named_child_count().saturating_sub(1))
                        .unwrap_or(child)
                } else {
                    child
                }
            })
            .collect();
        let inherits = descendants(class).into_iter().any(|node| {
            node.start_byte() < body.start_byte() && profile.heritage_kinds.contains(&kind_of(node))
        });
        let Some(name) = self.class_name(class) else {
            return;
        };
        if inherits
            && members
                .iter()
                .all(|member| profile.empty_body_kinds.contains(&kind_of(member)))
        {
            self.out.empty_subclasses.push(Spot {
                span: span_of(class),
                detail: name.clone(),
                count: 1,
            });
        }
        let behaviour: Vec<Node<'a>> = members
            .iter()
            .copied()
            .filter(|member| profile.method_kinds.contains(&kind_of(member)))
            .collect();
        if !inherits
            && behaviour.len() > 1
            && behaviour.iter().all(|member| self.is_static(*member))
        {
            self.out.static_only_classes.push(Spot {
                span: span_of(class),
                detail: name,
                count: behaviour.len() as u32,
            });
            self.focus_on_name(RuleId::StaticOnlyClass, class);
        }
    }

    fn enclosing_class_name(&self, node: Node<'a>) -> Option<String> {
        let profile = self.profile;
        let mut parent = node.parent();
        while let Some(current) = parent {
            if profile.is_class(current) {
                return self.class_name(current);
            }
            parent = current.parent();
        }
        None
    }

    fn class_body(&self, class: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        class.child_by_field_name("body").or_else(|| {
            class
                .named_children(&mut class.walk())
                .find(|child| profile.is_block(*child))
        })
    }

    fn class_name(&self, class: Node<'a>) -> Option<String> {
        let profile = self.profile;
        class
            .child_by_field_name("name")
            .filter(|node| {
                profile.is_identifier(*node)
                    || profile.binding_identifier_kinds.contains(&kind_of(node))
            })
            .map(|node| self.text(node).to_owned())
    }

    /// The marker is an anonymous keyword in most grammars and a decorator in
    /// Python, so the unnamed children count as well as the named ones.
    fn is_static(&self, member: Node<'a>) -> bool {
        let profile = self.profile;
        let holder = member
            .parent()
            .filter(|parent| profile.decorated_kinds.contains(&kind_of(parent)))
            .unwrap_or(member);
        let mut cursor = holder.walk();
        let mut direct: Vec<Node<'a>> = holder.children(&mut cursor).collect();
        let containers: Vec<Node<'a>> = direct
            .iter()
            .copied()
            .filter(|node| profile.modifier_container_kinds.contains(&kind_of(node)))
            .collect();
        for container in containers {
            let mut inner = container.walk();
            direct.extend(container.children(&mut inner));
        }
        direct
            .into_iter()
            .any(|node| profile.static_markers.contains(&self.text(node)))
            || self.annotations(member).iter().any(|annotation| {
                profile
                    .static_markers
                    .iter()
                    .any(|marker| annotation.contains(marker))
            })
    }

    /// Construction that does work rather than only recording what it was given: the
    /// object cannot be made without whatever that work needs.
    fn note_working_constructor(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let named = self.function_name(function).map(|node| self.text(node));
        let is_constructor = profile.constructor_kinds.contains(&kind_of(function))
            || named.is_some_and(|name| profile.constructor_names.contains(&name))
            || named
                .is_some_and(|name| self.enclosing_class_name(function).as_deref() == Some(name));
        if !is_constructor {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        let working = holder
            .named_children(&mut holder.walk())
            .filter(|statement| !profile.is_comment(*statement))
            .filter(|statement| {
                descendants(*statement).into_iter().any(|node| {
                    profile.is_call(node)
                        && !self
                            .callee_name(node)
                            .is_some_and(|name| profile.self_names.contains(&name.as_str()))
                })
            })
            .count() as u32;
        if working == 0 {
            return;
        }
        self.out.working_constructors.push(Spot {
            span: span_of(function),
            detail: working.to_string(),
            count: working,
        });
    }

    /// The four questions asked of one test body, wherever that body was found: a
    /// framework that registers a case with a callback and one that names a function
    /// give the same body two shapes and one meaning.
    fn note_test_body(&mut self, body: Node<'a>, label: &str, named: bool, handles: &[&str]) {
        let profile = self.profile;
        let mut branches = 0_u32;
        let mut guarded = None;
        // Keyed by where the statement starts, so two assertion calls chained into one
        // statement are the one assertion a reader sees.
        let mut assertions: HashMap<u32, String> = HashMap::new();
        // A decision inside a callback belongs to whatever the callback stands in
        // for — a mock, a render, a comparator — not to the test's own path.
        for node in crate::tree::descendants_outside(body, |node| profile.is_function(node)) {
            let kind = kind_of(node);
            // Deliberately not every branch kind: a ternary is an expression the test
            // evaluates, not a path the test can take. Nor is a hand-written check, or
            // a loop over a table of cases.
            let decides = (kind == profile.if_kind && !self.only_fails(node, handles))
                || (profile.loop_kinds.contains(&kind)
                    && !self.iterates_a_table(node, body, handles))
                || profile.switch_kinds.contains(&kind);
            if decides {
                branches += 1;
            }
            if kind == "try_statement" && !descendants(node).is_empty() {
                guarded = Some(node);
            }
            if self.asserts(node)
                && let Some(statement) = self.enclosing_statement(node, body)
            {
                assertions
                    .entry(statement.start_byte() as u32)
                    .or_insert_with(|| squashed(self.text(statement)));
            }
        }
        let mut seen: HashMap<String, u32> = HashMap::new();
        for text in assertions.into_values() {
            *seen.entry(text).or_default() += 1;
        }
        // A run of checks with nothing said about any of them: a failing run prints
        // two values and the reader has to find which line they came from.
        let checks: Vec<Node<'a>> = descendants(body)
            .into_iter()
            .filter(|node| self.asserts(*node))
            .collect();
        let labelled = checks.iter().any(|check| {
            descendants(*check)
                .into_iter()
                .any(|node| profile.string_kinds.contains(&kind_of(node)))
        });
        if checks.len() >= 3 && !labelled {
            self.out.unlabelled_assertions.push(Spot {
                span: span_of(body),
                detail: checks.len().to_string(),
                count: checks.len() as u32,
            });
        }
        // A stand-in built and never asked what it saw. It made the test run and
        // held it to nothing.
        let stubs: Vec<(String, Node<'a>)> = descendants(body)
            .into_iter()
            .filter(|node| profile.is_call(*node))
            .filter(|node| {
                self.called_name(*node).is_some_and(|dotted| {
                    let tail = dotted.rsplit('.').next().unwrap_or(&dotted).to_owned();
                    profile.stub_calls.contains(&tail.as_str())
                })
            })
            .filter_map(|node| {
                let parent = node.parent().map(|parent| {
                    // The value half of a declaration can sit in a list of one.
                    if profile.expression_list_kinds.contains(&kind_of(parent)) {
                        parent.parent().unwrap_or(parent)
                    } else {
                        parent
                    }
                })?;
                let held = profile
                    .binding_fields
                    .iter()
                    .find_map(|field| parent.child_by_field_name(field))
                    .or_else(|| parent.child_by_field_name("left"))
                    .map(|held| {
                        if profile.expression_list_kinds.contains(&kind_of(held)) {
                            held.named_child(0).unwrap_or(held)
                        } else {
                            held
                        }
                    })
                    .filter(|held| profile.is_identifier(*held))?;
                Some((self.text(held).to_owned(), node))
            })
            .collect();
        for (name, at) in stubs {
            let checked = checks.iter().any(|check| {
                descendants(*check)
                    .into_iter()
                    .any(|node| profile.is_identifier(node) && self.text(node) == name)
            });
            if !checked {
                self.out.unchecked_stubs.push(Spot {
                    span: span_of(at),
                    detail: name,
                    count: 1,
                });
            }
        }
        // A test that drives several units at once fails for any of them, and the run
        // says only that the test failed.
        let vocabulary = &profile.test;
        let driven: std::collections::BTreeSet<String> = descendants(body)
            .into_iter()
            .filter(|node| profile.is_call(*node))
            .filter_map(|node| self.called_name(node))
            .filter(|dotted| {
                !vocabulary.is_assertion(dotted)
                    && !vocabulary.is_test(dotted)
                    && vocabulary.control(dotted).is_none()
            })
            .map(|dotted| dotted.rsplit('.').next().unwrap_or(&dotted).to_owned())
            .collect();
        if driven.len() >= 5 {
            self.out.eager_tests.push(Spot {
                span: span_of(body),
                detail: driven.len().to_string(),
                count: driven.len() as u32,
            });
        }
        // A check against a serialised shape holds the code to how it prints rather
        // than to what it answered.
        if let Some(printed) = descendants(body).into_iter().find(|node| {
            profile.string_kinds.contains(&kind_of(node)) && {
                // A serialised shape opens with its own bracket; a sentence that
                // happens to hold one is prose.
                let text = self
                    .text(*node)
                    .trim_matches(['"', '\'', '`', 'r', 'f', 'b']);
                text.len() > 30 && text.starts_with(['{', '['])
            }
        }) {
            self.out.shape_assertions.push(Spot {
                span: span_of(printed),
                detail: self.snippet(printed),
                count: 1,
            });
        }
        // The whole statement, not the call that names the check: a fluent framework
        // puts the expected value on the far side of the call that does the asserting.
        let written: Vec<Node<'a>> = checks
            .iter()
            .filter_map(|check| self.enclosing_statement(*check, body))
            .collect();
        if let Some(circular) = written.iter().find(|check| self.compares_itself(**check)) {
            self.out.self_asserting_tests.push(Spot {
                span: span_of(*circular),
                detail: self.snippet(*circular),
                count: 1,
            });
        }
        if let Some(bare) = written
            .iter()
            .find_map(|check| self.unexplained_value(*check))
        {
            self.out.unexplained_values.push(Spot {
                span: span_of(bare),
                detail: self.text(bare).to_owned(),
                count: 1,
            });
        }
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        if holder.named_children(&mut holder.walk()).all(|child| {
            profile.is_comment(child) || profile.empty_body_kinds.contains(&kind_of(child))
        }) {
            self.out.empty_tests.push(Spot {
                span: span_of(body),
                detail: label.to_owned(),
                count: 1,
            });
        }
        if named && crate::vocabulary::says_nothing(label) {
            self.out.unnamed_tests.push(Spot {
                span: span_of(body),
                detail: label.to_owned(),
                count: 1,
            });
        }
        // A test that reaches a database or the network is not testing this code.
        if let Some(reach) = descendants(body).into_iter().find(|node| {
            profile.is_call(*node)
                && self.called_name(*node).is_some_and(|dotted| {
                    let mut parts = dotted.rsplitn(2, '.');
                    let tail = parts.next().unwrap_or_default().to_owned();
                    let receiver = parts.next().unwrap_or_default().to_owned();
                    (crate::vocabulary::reaches_outside(&receiver, &self.extra.receivers)
                        && crate::vocabulary::asks_for_data(&tail, &self.extra.methods))
                        || (receiver.is_empty()
                            && IO_BARE.contains(&tail.to_ascii_lowercase().as_str()))
                })
        }) {
            self.out.real_world_tests.push(Spot {
                span: span_of(reach),
                detail: label.to_owned(),
                count: 1,
            });
        }
        if branches > 0 {
            self.out.branching_tests.push(Spot {
                span: span_of(body),
                detail: label.to_owned(),
                count: branches,
            });
        }
        if let Some(node) = guarded
            && descendants(node)
                .into_iter()
                .any(|child| profile.catch_kinds.contains(&kind_of(child)))
        {
            self.out.hand_rolled_throws.push(Spot {
                span: span_of(node),
                detail: label.to_owned(),
                count: 1,
            });
        }
        let mut repeated: Vec<(String, u32)> =
            seen.into_iter().filter(|(_, times)| *times > 1).collect();
        repeated.sort();
        if let Some((text, seen)) = repeated.into_iter().next() {
            self.out.duplicate_assertions.push(Spot {
                span: span_of(body),
                detail: text,
                count: seen,
            });
        }
    }

    fn enclosing_statement(&self, node: Node<'a>, stop: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        let mut current = node;
        loop {
            if profile.is_statement(current) {
                return Some(current);
            }
            let parent = current.parent()?;
            if parent.id() == stop.id() {
                return Some(current);
            }
            current = parent;
        }
    }

    /// Whether a check has the same expression on both sides, so it holds the code
    /// to whatever the code already does.
    fn compares_itself(&self, check: Node<'a>) -> bool {
        let profile = self.profile;
        let calls: Vec<String> = descendants(check)
            .into_iter()
            .filter(|node| profile.is_call(*node))
            .filter(|node| {
                !self
                    .called_name(*node)
                    .is_some_and(|name| profile.test.is_assertion(&name))
            })
            .map(|node| squashed(self.text(node)))
            .collect();
        // Any two of them, not just two in a row: the tree walk interleaves a call
        // with whatever it was written on.
        let mut seen: std::collections::HashSet<&String> = std::collections::HashSet::new();
        calls.iter().any(|call| !seen.insert(call))
    }

    /// A number a check expects with nothing anywhere saying where it came from.
    fn unexplained_value(&self, check: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        descendants(check).into_iter().find(|node| {
            profile.number_kinds.contains(&kind_of(node))
                && self
                    .text(*node)
                    .parse::<f64>()
                    .is_ok_and(|value| !CONVENTIONAL.contains(&value) && value.abs() > 1.0)
        })
    }

    /// A name that says what a thing is not, and a name the language already owns.
    fn note_declared_name(&mut self, node: Node<'a>) {
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

    /// A binding at file level that anything in the file can write to. Every function
    /// that reads it has an argument nobody passed.
    fn note_module_state(&mut self, root: Node<'a>) {
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
    fn note_import_time_work(&mut self, root: Node<'a>) {
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
    fn note_ambient_dependency(&mut self, node: Node<'a>) {
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

    /// A field filled with something the object built itself. Nothing can be put in
    /// its place, so nothing can be tested without the real thing.
    fn note_self_built_collaborator(&mut self, assignment: Node<'a>) {
        let profile = self.profile;
        let Some(target) = assignment.child_by_field_name("left") else {
            return;
        };
        if !profile.is_member_access(target) {
            return;
        }
        let reaches_self = target
            .child_by_field_name(profile.object_field)
            .map(|object| self.text(object))
            .is_some_and(|object| profile.self_names.contains(&object));
        if !reaches_self {
            return;
        }
        let Some(value) = assignment.child_by_field_name("right") else {
            return;
        };
        let value = self.see_through(value);
        let Some(built) = self.constructed_type(value) else {
            return;
        };
        self.out.self_built_collaborators.push(Spot {
            span: span_of(assignment),
            detail: built,
            count: 1,
        });
    }

    /// What a `new` builds, whichever field the grammar keeps it in — or, in a
    /// language with no `new`, a call on a name spelled like a type.
    fn constructed_type(&self, node: Node<'a>) -> Option<String> {
        if let Some(built) = self
            .profile
            .constructor_field
            .and_then(|field| node.child_by_field_name(field))
        {
            return self
                .callee_name(built)
                .or_else(|| Some(self.text(built).to_owned()));
        }
        let name = self.called_name(node)?;
        let tail = name.rsplit('.').next().unwrap_or(&name);
        tail.chars()
            .next()
            .is_some_and(|first| first.is_uppercase())
            .then(|| tail.to_owned())
    }

    /// Reading a member the other type marked as its own business.
    fn note_private_reach(&mut self, access: Node<'a>) {
        let profile = self.profile;
        if profile.private_name_prefixes.is_empty() {
            return;
        }
        let Some(property) = self.member_property(access) else {
            return;
        };
        let name = self.text(property);
        if !profile
            .private_name_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            return;
        }
        let receiver = access
            .child_by_field_name(profile.object_field)
            .map(|object| self.text(object));
        // The receiver is the only one entitled to it, and so is a name that is
        // itself private.
        if receiver.is_none_or(|object| {
            profile.self_names.contains(&object)
                || profile
                    .private_name_prefixes
                    .iter()
                    .any(|prefix| object.starts_with(prefix))
        }) {
            return;
        }
        self.out.private_reaches.push(Spot {
            span: span_of(access),
            detail: name.to_owned(),
            count: 1,
        });
    }

    /// A run of parameter checks at the top of a body, each answering the same way.
    /// The body has not started yet and the reader has read four decisions.
    fn note_guard_cascade(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        let mut cursor = holder.walk();
        let guards = holder
            .named_children(&mut cursor)
            .take_while(|statement| self.is_bare_guard(*statement))
            .count() as u32;
        if guards >= self.limits.guard_cascade {
            self.out.guard_cascades.push(Spot {
                span: self.name_span(function),
                detail: guards.to_string(),
                count: guards,
            });
        }
    }

    /// An `if` whose whole body is one way out and nothing else.
    fn is_bare_guard(&self, statement: Node<'a>) -> bool {
        let profile = self.profile;
        if kind_of(statement) != profile.if_kind {
            return false;
        }
        let Some(taken) = statement.child_by_field_name("consequence") else {
            return false;
        };
        let leaves = |node: Node<'a>| {
            profile.throw_kinds.contains(&kind_of(node)) || kind_of(node) == "return_statement"
        };
        // A guard is often written without braces, in which case the consequence is
        // the way out rather than a block holding it.
        if leaves(taken) {
            return true;
        }
        let holder = taken
            .named_children(&mut taken.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(taken);
        let mut cursor = holder.walk();
        let mut inside = holder.named_children(&mut cursor);
        let Some(only) = inside.next() else {
            return false;
        };
        inside.next().is_none() && leaves(only)
    }

    /// A guarded block holding nearly the whole body: the handler is not answering a
    /// failure, it is a net under the function.
    fn note_blanket_try(&mut self, node: Node<'a>) {
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

    fn enclosing_function(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut parent = node.parent();
        while let Some(current) = parent {
            if self.profile.is_function(current) {
                return Some(current);
            }
            parent = current.parent();
        }
        None
    }

    /// A name introduced on one line and returned on the next. The name is read
    /// once, by the line under it, and says nothing the expression did not.
    fn note_fresh_local_return(&mut self, function: Node<'a>) {
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

    /// Two arms of one chain asking the same question. The second can never be
    /// reached, and a reader has to compare them to find that out.
    fn note_duplicate_condition(&mut self, branch: Node<'a>) {
        let profile = self.profile;
        // Only from the head of a chain, so a chain of four is read once.
        let is_tail = branch.parent().is_some_and(|parent| {
            kind_of(parent) == profile.if_kind
                && parent
                    .child_by_field_name(profile.else_field)
                    .is_some_and(|alternative| alternative.id() == branch.id())
        });
        if is_tail {
            return;
        }
        let mut asked: Vec<(String, Node<'a>)> = Vec::new();
        let ask = |node: Node<'a>, asked: &mut Vec<(String, Node<'a>)>| {
            if let Some(condition) = node.child_by_field_name(profile.condition_field) {
                asked.push((squashed(self.text(condition)), condition));
            }
        };
        ask(branch, &mut asked);
        if profile.else_is_sibling_list {
            // The clauses hang off the `if` as a list rather than nesting inside one
            // another, so the whole chain is one node's children.
            let mut cursor = branch.walk();
            let clauses: Vec<Node<'a>> = branch
                .named_children(&mut cursor)
                .filter(|child| profile.else_clause_kinds.contains(&kind_of(child)))
                .collect();
            for clause in clauses {
                ask(clause, &mut asked);
            }
        } else {
            let mut current = branch.child_by_field_name(profile.else_field);
            while let Some(node) = current {
                // Some grammars hang the next `if` straight off the field; others put
                // an `else` clause in between and nest the `if` inside that.
                let arm = if kind_of(node) == profile.if_kind {
                    node
                } else if profile.else_clause_kinds.contains(&kind_of(node)) {
                    match node
                        .named_children(&mut node.walk())
                        .find(|child| kind_of(child) == profile.if_kind)
                    {
                        Some(nested) => nested,
                        None => break,
                    }
                } else {
                    break;
                };
                ask(arm, &mut asked);
                current = arm.child_by_field_name(profile.else_field);
            }
        }
        for (at, (text, node)) in asked.iter().enumerate() {
            if asked[..at].iter().any(|(earlier, _)| earlier == text) {
                self.out.duplicate_conditions.push(Spot {
                    span: span_of(*node),
                    detail: text.clone(),
                    count: 1,
                });
                return;
            }
        }
    }

    /// Building a string one piece at a time inside a loop: every pass copies
    /// everything written so far.
    fn note_loop_concatenation(&mut self, assignment: Node<'a>) {
        if self.loop_depth() == 0 {
            return;
        }
        let profile = self.profile;
        let Some(operator) = operator_of(assignment, self.source) else {
            return;
        };
        if operator != "+=" {
            return;
        }
        let Some(target) = assignment.child_by_field_name("left") else {
            return;
        };
        let name = self.text(target);
        // Either the declaration named a text type, or the value it started with was
        // a piece of text — a language with no type annotation only offers the second.
        let holds_text = self.declared.get(name).is_some_and(|introduced| {
            let lowered = introduced.to_lowercase();
            lowered.contains("string")
                || lowered.contains("str")
                || introduced.trim_start().starts_with(['"', '\'', '`'])
        }) || assignment
            .child_by_field_name("right")
            .is_some_and(|value| profile.string_kinds.contains(&kind_of(value)));
        if !holds_text {
            return;
        }
        self.out.loop_concatenations.push(Spot {
            span: span_of(assignment),
            detail: name.to_owned(),
            count: self.loop_depth(),
        });
    }

    /// A type declared abstract that leaves nothing for anyone to fill in.
    fn note_empty_abstraction(&mut self, class: Node<'a>) {
        let profile = self.profile;
        if profile.abstract_markers.is_empty() {
            return;
        }
        let mut cursor = class.walk();
        let marked = class
            .children(&mut cursor)
            .any(|node| profile.abstract_markers.contains(&self.text(node)));
        if !marked {
            return;
        }
        let Some(body) = self.class_body(class) else {
            return;
        };
        let unfinished = descendants(body).into_iter().any(|node| {
            let mut inner = node.walk();
            profile.method_kinds.contains(&kind_of(node))
                && (node
                    .children(&mut inner)
                    .any(|child| profile.abstract_markers.contains(&self.text(child)))
                    || profile.signature_kinds.contains(&kind_of(node)))
        }) || descendants(body)
            .into_iter()
            .any(|node| profile.signature_kinds.contains(&kind_of(node)));
        if unfinished {
            return;
        }
        let Some(name) = self.class_name(class) else {
            return;
        };
        self.out.empty_abstractions.push(Spot {
            span: span_of(class),
            detail: name,
            count: 1,
        });
    }

    /// A switch arm that does work and then runs into the next one.
    fn note_silent_fallthrough(&mut self, switch: Node<'a>) {
        let profile = self.profile;
        let mut cursor = switch.walk();
        let mut stack: Vec<Node<'a>> = switch.named_children(&mut cursor).collect();
        let mut arms: Vec<Node<'a>> = Vec::new();
        while let Some(current) = stack.pop() {
            if profile.case_kinds.contains(&kind_of(current)) {
                arms.push(current);
                continue;
            }
            if profile.switch_kinds.contains(&kind_of(current)) {
                continue;
            }
            let mut inner = current.walk();
            stack.extend(current.named_children(&mut inner));
        }
        arms.sort_by_key(|arm| arm.start_byte());
        let Some((_, earlier)) = arms.split_last() else {
            return;
        };
        for arm in earlier {
            let mut inside = arm.walk();
            let statements: Vec<Node<'a>> = arm
                .named_children(&mut inside)
                .filter(|child| profile.is_statement(*child) || self.leaves_the_arm(*child))
                .collect();
            let Some(last) = statements.last() else {
                continue;
            };
            if statements.is_empty() || self.leaves_the_arm(*last) {
                continue;
            }
            self.out.silent_fallthroughs.push(Spot {
                span: span_of(*arm),
                detail: self.snippet(*arm),
                count: 1,
            });
            return;
        }
    }

    fn leaves_the_arm(&self, node: Node<'a>) -> bool {
        let profile = self.profile;
        matches!(
            kind_of(node),
            "break_statement" | "return_statement" | "continue_statement" | "goto_statement"
        ) || profile.throw_kinds.contains(&kind_of(node))
    }

    /// A function literal written inside a loop and reading the name the loop moves.
    /// Whatever runs it later sees whichever value the loop stopped on.
    fn note_loop_capture(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(loop_node) = self.enclosing_loop(function) else {
            return;
        };
        // A callback the loop itself calls, once per element, closes over nothing it
        // will outlive.
        if profile.is_loop_callback(function, self.source)
            || self.runs_in_place(function)
            || self.binds_per_iteration(loop_node)
        {
            return;
        }
        let moved: Vec<String> = self.loop_names(loop_node);
        if moved.is_empty() {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        let Some(captured) = descendants(body)
            .into_iter()
            .filter(|node| profile.is_identifier(*node))
            .map(|node| self.text(node))
            .find(|name| moved.iter().any(|moved| moved == name))
        else {
            return;
        };
        self.out.loop_variable_captures.push(Spot {
            span: span_of(function),
            detail: captured.to_owned(),
            count: 1,
        });
    }

    /// A function literal handed to a call that runs it before returning: a sort's
    /// comparison, `strings.Map`, a Go subtest. It is done with the loop variable
    /// before the loop moves on. A subtest that calls `t.Parallel()` is the exception:
    /// it waits until the loop has finished.
    fn runs_in_place(&self, function: Node<'a>) -> bool {
        let profile = self.profile;
        let callers: &[&str] = match profile.language {
            rqx_core::Language::Go => &[
                "Run",
                "Slice",
                "SliceStable",
                "SortFunc",
                "SortStableFunc",
                "BinarySearchFunc",
                "IndexFunc",
                "ContainsFunc",
                "DeleteFunc",
                "CompactFunc",
                "EqualFunc",
                "MinFunc",
                "MaxFunc",
                "Map",
                "FieldsFunc",
                "TrimFunc",
                "TrimLeftFunc",
                "TrimRightFunc",
                "Walk",
                "WalkDir",
                "Do",
            ],
            rqx_core::Language::TypeScript | rqx_core::Language::JavaScript => &[
                "sort",
                "toSorted",
                "findLastIndex",
                "replace",
                "replaceAll",
                "from",
            ],
            rqx_core::Language::Python => &[
                "sorted", "sort", "min", "max", "filter", "map", "any", "all", "reduce", "sub",
            ],
            rqx_core::Language::Java => &[
                "sort",
                "removeIf",
                "replaceAll",
                "computeIfAbsent",
                "computeIfPresent",
                "compute",
                "merge",
            ],
            rqx_core::Language::Cpp => &[
                "sort",
                "stable_sort",
                "find_if",
                "count_if",
                "any_of",
                "all_of",
                "none_of",
                "remove_if",
                "accumulate",
            ],
        };
        // Python passes a key function by keyword: `sorted(rows, key=lambda row: …)`.
        let mut list = function.parent();
        if list.is_some_and(|parent| kind_of(parent) == "keyword_argument") {
            list = list.and_then(|parent| parent.parent());
        }
        let Some(call) = list
            .filter(|list| profile.argument_list_kinds.contains(&kind_of(*list)))
            .and_then(|list| list.parent())
            .filter(|call| profile.is_call(*call))
        else {
            return false;
        };
        let Some(tail) = profile.call_tail(call, self.source) else {
            return false;
        };
        callers.contains(&tail) && !(tail == "Run" && self.text(function).contains(".Parallel()"))
    }

    /// Whether a name spelled like an imprecise type is used as one. `expect.any(…)`,
    /// a variable called `any` and `from typing import Any` name something else: a
    /// member name never counts, and a plain identifier counts only inside a type
    /// annotation, which is how Python writes its types.
    fn is_in_type_position(&self, node: Node<'a>) -> bool {
        const MEMBER_NAMES: [&str; 3] = [
            "property_identifier",
            "field_identifier",
            "shorthand_property_identifier",
        ];
        if MEMBER_NAMES.contains(&kind_of(node)) {
            return false;
        }
        if kind_of(node) != "identifier" {
            return true;
        }
        let mut current = node.parent();
        for _ in 0..3 {
            let Some(parent) = current else {
                return false;
            };
            let kind = kind_of(parent);
            if kind == "type" || kind.ends_with("_type") || kind.ends_with("type_annotation") {
                return true;
            }
            current = parent.parent();
        }
        false
    }

    /// A JavaScript loop that declares its variable with `let` or `const` gives every
    /// pass a binding of its own, so a closure keeps the value of its own pass. Only
    /// `var`, or a variable declared before the loop, is shared between passes.
    fn binds_per_iteration(&self, loop_node: Node<'a>) -> bool {
        if !matches!(
            self.profile.language,
            rqx_core::Language::TypeScript | rqx_core::Language::JavaScript
        ) {
            return false;
        }
        let lexical = |node: Node<'a>| matches!(self.text(node), "let" | "const");
        match kind_of(loop_node) {
            "for_in_statement" => loop_node.child_by_field_name("kind").is_some_and(lexical),
            "for_statement" => loop_node
                .child_by_field_name("initializer")
                .is_some_and(|initializer| kind_of(initializer) == "lexical_declaration"),
            _ => false,
        }
    }

    fn enclosing_loop(&self, node: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        let mut parent = node.parent();
        while let Some(current) = parent {
            if profile.loop_kinds.contains(&kind_of(current)) {
                return Some(current);
            }
            if profile.is_function(current) {
                return None;
            }
            parent = current.parent();
        }
        None
    }

    /// The names a loop moves from one pass to the next.
    fn loop_names(&self, loop_node: Node<'a>) -> Vec<String> {
        let profile = self.profile;
        let Some(body) = self.loop_statements(loop_node) else {
            return Vec::new();
        };
        // A `for … in` names what it moves in a field of its own; a counted loop
        // declares it in the header like any other local.
        let named = ["left", "name", "index"]
            .into_iter()
            .find_map(|field| self.loop_header(loop_node, field));
        if let Some(header) = named {
            return descendants(header)
                .into_iter()
                .chain(std::iter::once(header))
                .filter(|node| profile.is_identifier(*node))
                .map(|node| self.text(node).to_owned())
                .collect();
        }
        descendants(loop_node)
            .into_iter()
            .filter(|node| node.start_byte() < body.start_byte())
            .filter(|node| {
                profile.is_identifier(*node)
                    && crate::tree::is_binding_position(
                        *node,
                        profile.parameter_kinds,
                        profile.borrowed_name_kinds,
                        profile.local_declaring_kinds,
                        profile.expression_list_kinds,
                    )
            })
            .map(|node| self.text(node).to_owned())
            .collect()
    }

    /// A field of the loop's header, looking inside a range clause where the grammar
    /// puts one between the loop and its parts.
    fn loop_header(&self, loop_node: Node<'a>, field: &str) -> Option<Node<'a>> {
        loop_node.child_by_field_name(field).or_else(|| {
            loop_node
                .named_children(&mut loop_node.walk())
                .find(|child| kind_of(child) == "range_clause")
                .and_then(|clause| clause.child_by_field_name(field))
        })
    }

    /// Growing or shrinking the very collection a loop is walking.
    fn note_mutation_while_iterating(&mut self, loop_node: Node<'a>) {
        const CHANGERS: [&str; 16] = [
            "push",
            "pop",
            "shift",
            "unshift",
            "splice",
            "append",
            "remove",
            "add",
            "insert",
            "delete",
            "push_back",
            "pop_back",
            "emplace_back",
            "erase",
            "clear",
            "removeIf",
        ];
        let profile = self.profile;
        let Some(walked) = self.iterated_name(loop_node) else {
            return;
        };
        let Some(body) = self.loop_statements(loop_node) else {
            return;
        };
        let Some(changed) = descendants(body).into_iter().find(|node| {
            let calls_a_changer = profile.is_call(*node)
                && self.called_name(*node).is_some_and(|dotted| {
                    let mut parts = dotted.rsplitn(2, '.');
                    let tail = parts.next().unwrap_or_default();
                    let receiver = parts.next().unwrap_or_default();
                    receiver == walked && CHANGERS.contains(&tail)
                });
            // A language whose collections are values rewrites the name instead of
            // calling a method on it.
            let rewrites = (profile.assignment_kinds.contains(&kind_of(node))
                || profile.local_declaring_kinds.contains(&kind_of(node)))
                && node
                    .child_by_field_name("left")
                    .is_some_and(|target| self.text(target) == walked);
            calls_a_changer || rewrites
        }) else {
            return;
        };
        self.out.mutated_while_iterated.push(Spot {
            span: span_of(changed),
            detail: walked,
            count: 1,
        });
    }

    /// What a loop is walking, where the loop names it.
    fn iterated_name(&self, loop_node: Node<'a>) -> Option<String> {
        let over = self
            .loop_header(loop_node, "right")
            .or_else(|| self.loop_header(loop_node, "value"))?;
        let over = self.see_through(over);
        self.profile
            .is_identifier(over)
            .then(|| self.text(over).to_owned())
    }

    /// Two waits in a row on calls that do not read each other's answers. They could
    /// have run together and the reader is paying for both in series.
    fn note_sequential_await(&mut self, function: Node<'a>) {
        let profile = self.profile;
        // A test waits in order on purpose: each wait is the step it is checking.
        if !profile.is_async(function) || self.test_depth > 0 {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        if let Some((_, second)) =
            crate::cost::indicators::independent_awaits(&self.names(), holder).first()
        {
            self.out.sequential_awaits.push(Spot {
                span: span_of(*second),
                detail: self.snippet(*second),
                count: 2,
            });
        }
    }

    /// A check that the language is free to compile away, standing where a real one
    /// was meant to be.
    fn note_production_assert(&mut self, node: Node<'a>) {
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

    /// A return that hands back several things at once, and a signature that
    /// declares it does.
    fn note_many_results(&mut self, returns: Node<'a>) {
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

    /// A name that answers true or false without asking anything.
    fn note_unquestioning_name(&mut self, function: Node<'a>) {
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
    fn note_named_shape(&mut self, node: Node<'a>) {
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

    /// A field only one method ever reads. It is that method's local, kept where
    /// every other method can see it.
    fn note_lonely_fields(&mut self, class: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.class_body(class) else {
            return;
        };
        let members: Vec<Node<'a>> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.method_kinds.contains(&kind_of(child)))
            .collect();
        // A type whose methods are declared here and written elsewhere cannot say who
        // reads a field: most of its behaviour is in another file.
        if members.len() < 2
            || members
                .iter()
                .any(|member| self.function_body(*member).is_none())
        {
            return;
        }
        let declared: Vec<(String, Node<'a>)> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.field_kinds.contains(&kind_of(child)))
            .filter_map(|field| {
                // The declared name, not the type beside it: both are identifiers
                // under the same node in most of these grammars.
                let holder = field
                    .named_children(&mut field.walk())
                    .find(|child| profile.declarator_kinds.contains(&kind_of(child)))
                    .unwrap_or(field);
                let spells_a_name = |node: &Node<'a>| {
                    profile.is_identifier(*node) || profile.property_kinds.contains(&kind_of(node))
                };
                let name = profile
                    .binding_fields
                    .iter()
                    .find_map(|binding| holder.child_by_field_name(binding))
                    .filter(spells_a_name)
                    .or_else(|| descendants(holder).into_iter().find(spells_a_name))?;
                Some((self.text(name).to_owned(), name))
            })
            .collect();
        for (name, at) in declared {
            let readers = members
                .iter()
                .filter(|member| {
                    descendants(**member)
                        .into_iter()
                        .any(|node| profile.is_identifier(node) && self.text(node) == name)
                })
                .count();
            if readers == 1 {
                self.out.lonely_fields.push(Spot {
                    span: span_of(at),
                    detail: name,
                    count: 1,
                });
            }
        }
    }

    /// How many other types one type has to know about to do its job.
    fn note_many_collaborators(&mut self, class: Node<'a>) {
        let profile = self.profile;
        let Some(name) = self.class_name(class) else {
            return;
        };
        let mut known: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for node in descendants(class) {
            if let Some(built) = self.constructed_type(node) {
                known.insert(built);
                continue;
            }
            if profile.type_annotation_kinds.contains(&kind_of(node))
                && let Some(inner) = node.named_child(0)
                && profile.binding_identifier_kinds.contains(&kind_of(inner))
            {
                known.insert(self.text(inner).to_owned());
            }
        }
        known.remove(&name);
        let count = known.len() as u32;
        if count > self.limits.class_collaborators {
            self.out.many_collaborators.push(Spot {
                span: span_of(class),
                detail: name,
                count,
            });
            self.focus_on_name(RuleId::ManyCollaborators, class);
        }
    }

    /// A body that says more about what it is doing than it does.
    fn note_over_logging(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let inside = descendants(body);
        let statements = inside
            .iter()
            .filter(|node| profile.is_statement(**node))
            .count() as u32;
        if statements < 4 {
            return;
        }
        let logged = inside
            .iter()
            .filter(|node| profile.is_call(**node))
            .filter(|node| {
                self.called_name(**node).is_some_and(|dotted| {
                    let tail = dotted.rsplit('.').next().unwrap_or(&dotted).to_owned();
                    dotted.contains('.') && profile.log_calls.contains(&tail.as_str())
                })
            })
            .count() as u32;
        if logged * 100 >= statements * self.limits.log_percent && logged > 1 {
            self.out.over_logging.push(Spot {
                span: self.name_span(function),
                detail: format!("{logged} of {statements}"),
                count: logged,
            });
        }
    }

    /// Stopping the thread for a fixed time, outside a test. Whatever it is waiting
    /// for, it is not waiting for that.
    fn note_sleep(&mut self, call: Node<'a>) {
        if self.test_depth > 0 {
            return;
        }
        let profile = self.profile;
        let Some(dotted) = self.called_name(call) else {
            return;
        };
        let tail = dotted.rsplit('.').next().unwrap_or(&dotted);
        if !profile.perf.sleep_calls.contains(&tail) {
            return;
        }
        self.out.sleeps.push(Spot {
            span: span_of(call),
            detail: dotted.clone(),
            count: 1,
        });
    }

    /// A function whose whole body hands back something the file keeps at module
    /// level.
    fn note_global_accessor(&mut self, function: Node<'a>, shared: &[String]) {
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
        if statements.next().is_some() || kind_of(only) != "return_statement" {
            return;
        }
        let Some(mut answer) = only.named_child(0).map(|node| self.see_through(node)) else {
            return;
        };
        while profile.expression_list_kinds.contains(&kind_of(answer))
            && answer.named_child_count() == 1
        {
            let Some(inner) = answer.named_child(0) else {
                break;
            };
            answer = self.see_through(inner);
        }
        if !profile.is_identifier(answer) {
            return;
        }
        let name = self.text(answer).to_owned();
        if !shared.contains(&name) {
            return;
        }
        self.out.global_accessors.push(Spot {
            span: self.name_span(function),
            detail: name,
            count: 1,
        });
    }

    /// A member that overrides something and then calls the thing it overrode,
    /// changing nothing on the way.
    fn note_passthrough_override(&mut self, function: Node<'a>) {
        let profile = self.profile;
        if profile.super_names.is_empty() {
            return;
        }
        let Some(name) = self.function_name(function).map(|node| self.text(node)) else {
            return;
        };
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
        let Some(dotted) = self.called_name(call) else {
            return;
        };
        let mut parts = dotted.rsplitn(2, '.');
        let tail = parts.next().unwrap_or_default();
        let receiver = parts.next().unwrap_or_default();
        if tail != name || !profile.super_names.contains(&receiver) {
            return;
        }
        self.out.passthrough_overrides.push(Spot {
            span: self.name_span(function),
            detail: name.to_owned(),
            count: 1,
        });
    }

    /// A handler that takes everything, written before one that takes less. The
    /// second can never run.
    fn note_shadowed_handler(&mut self, guarded: Node<'a>) {
        let profile = self.profile;
        let mut cursor = guarded.walk();
        let handlers: Vec<Node<'a>> = guarded
            .named_children(&mut cursor)
            .filter(|child| profile.catch_kinds.contains(&kind_of(child)))
            .collect();
        if handlers.len() < 2 {
            return;
        }
        let Some((_, earlier)) = handlers.split_last() else {
            return;
        };
        let Some(wide) = earlier
            .iter()
            .find(|handler| self.takes_everything(**handler))
        else {
            return;
        };
        self.out.shadowed_handlers.push(Spot {
            span: span_of(*wide),
            detail: self.snippet(*wide),
            count: 1,
        });
    }

    /// Whether a handler names the root of the hierarchy, or names nothing.
    fn takes_everything(&self, catch: Node<'a>) -> bool {
        let profile = self.profile;
        let Some(body) = catch.child_by_field_name("body").or_else(|| {
            catch
                .named_children(&mut catch.walk())
                .find(|child| profile.is_block(*child))
        }) else {
            return false;
        };
        let named = descendants(catch).into_iter().find(|node| {
            node.start_byte() < body.start_byte()
                && (profile.is_identifier(*node)
                    || profile.binding_identifier_kinds.contains(&kind_of(node))
                    || profile.qualified_name_kinds.contains(&kind_of(node)))
        });
        match named.map(|node| self.text(node)) {
            Some(name) => profile.catch_all_types.contains(&name),
            None => true,
        }
    }

    /// A callback that starts another wait. The order is real and none of it is
    /// written as an order.
    fn note_nested_continuation(&mut self, call: Node<'a>) {
        let profile = self.profile;
        if profile.continuation_calls.is_empty() {
            return;
        }
        let Some(dotted) = self.called_name(call) else {
            return;
        };
        let tail = dotted.rsplit('.').next().unwrap_or(&dotted);
        if !profile.continuation_calls.contains(&tail) {
            return;
        }
        let inside = descendants(call).into_iter().any(|node| {
            node.id() != call.id()
                && profile.is_call(node)
                && self.called_name(node).is_some_and(|inner| {
                    let tail = inner.rsplit('.').next().unwrap_or(&inner).to_owned();
                    profile.continuation_calls.contains(&tail.as_str())
                })
        });
        if !inside {
            return;
        }
        self.out.nested_continuations.push(Spot {
            span: span_of(call),
            detail: tail.to_owned(),
            count: 1,
        });
    }

    /// A type whose methods fall into groups that share no state. Each group is a
    /// type somebody has not separated yet.
    fn note_split_cohesion(&mut self, class: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.class_body(class) else {
            return;
        };
        let fields: Vec<String> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.field_kinds.contains(&kind_of(child)))
            .filter_map(|field| self.declared_field_name(field))
            .collect();
        if fields.len() < 2 {
            return;
        }
        let touched: Vec<std::collections::BTreeSet<&str>> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.method_kinds.contains(&kind_of(child)))
            .map(|member| {
                let names: Vec<&str> = descendants(member)
                    .into_iter()
                    .filter(|node| {
                        profile.is_identifier(*node)
                            || profile.property_kinds.contains(&kind_of(node))
                    })
                    .map(|node| self.text(node))
                    .collect();
                fields
                    .iter()
                    .filter(|field| names.contains(&field.as_str()))
                    .map(String::as_str)
                    .collect()
            })
            .filter(|used: &std::collections::BTreeSet<&str>| !used.is_empty())
            .collect();
        if touched.len() < 2 {
            return;
        }
        // Two methods belong together when they share a field, directly or through a
        // third; the groups left over are the types inside this one.
        let mut groups: Vec<std::collections::BTreeSet<&str>> = Vec::new();
        for used in touched {
            let (mut joined, apart): (Vec<_>, Vec<_>) = groups
                .into_iter()
                .partition(|group| group.intersection(&used).next().is_some());
            let mut merged = used;
            for group in joined.drain(..) {
                merged.extend(group);
            }
            groups = apart;
            groups.push(merged);
        }
        if groups.len() < 2 {
            return;
        }
        let Some(name) = self.class_name(class) else {
            return;
        };
        self.out.split_cohesion.push(Spot {
            span: span_of(class),
            detail: name,
            count: groups.len() as u32,
        });
    }

    fn declared_field_name(&self, field: Node<'a>) -> Option<String> {
        let profile = self.profile;
        let spells_a_name = |node: &Node<'a>| {
            profile.is_identifier(*node) || profile.property_kinds.contains(&kind_of(node))
        };
        let holder = field
            .named_children(&mut field.walk())
            .find(|child| profile.declarator_kinds.contains(&kind_of(child)))
            .unwrap_or(field);
        profile
            .binding_fields
            .iter()
            .find_map(|binding| holder.child_by_field_name(binding))
            .filter(spells_a_name)
            .or_else(|| descendants(holder).into_iter().find(spells_a_name))
            .map(|node| self.text(node).to_owned())
    }

    /// A method that never mentions the instance it was written on.
    fn note_instance_free_method(&mut self, function: Node<'a>) {
        let profile = self.profile;
        if profile.self_names.is_empty() || self.is_static(function) {
            return;
        }
        let inside_class = function
            .parent()
            .and_then(|parent| parent.parent())
            .is_some_and(|grand| profile.is_class(grand));
        if !inside_class || profile.constructor_kinds.contains(&kind_of(function)) {
            return;
        }
        let Some(name) = self
            .function_name(function)
            .map(|node| self.text(node).to_owned())
        else {
            return;
        };
        if profile.constructor_names.contains(&name.as_str()) {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        let mentions_self = descendants(body)
            .into_iter()
            .any(|node| profile.self_names.contains(&self.text(node)));
        if mentions_self {
            return;
        }
        self.out.instance_free_methods.push(Spot {
            span: self.name_span(function),
            detail: name,
            count: 1,
        });
    }

    /// Starting work that answers later and throwing the answer away. Nothing waits
    /// for it and nothing hears if it fails.
    fn note_dropped_promise(&mut self, statement: Node<'a>, awaited: &[String]) {
        let profile = self.profile;
        let Some(call) = self.only_call(statement) else {
            return;
        };
        if descendants(statement)
            .into_iter()
            .any(|node| profile.await_kinds.contains(&kind_of(node)))
        {
            return;
        }
        let Some(dotted) = self.called_name(call) else {
            return;
        };
        let tail = dotted.rsplit('.').next().unwrap_or(&dotted).to_owned();
        if !awaited.contains(&tail) {
            return;
        }
        self.out.dropped_promises.push(Spot {
            span: span_of(statement),
            detail: dotted,
            count: 1,
        });
    }

    /// Every mention of a name inside one body, in order, and whether the mention
    /// wrote to it. Three rules ask the same question of this and get different
    /// answers out of it.
    fn touches(&self, body: Node<'a>) -> HashMap<&'a str, Vec<Touch>> {
        let profile = self.profile;
        let mut seen: HashMap<&'a str, Vec<Touch>> = HashMap::new();
        for node in descendants(body) {
            let names_something =
                profile.is_identifier(node) || profile.property_kinds.contains(&kind_of(node));
            if !names_something {
                continue;
            }
            // Asked once and passed along: `Node::parent` walks down from the root
            // of the tree to find it, and this loop wanted it five times per node.
            let above = node.parent();
            let declares = above.is_some_and(|parent| {
                crate::tree::is_binding_position_under(
                    parent,
                    node,
                    &[],
                    profile.borrowed_name_kinds,
                    profile.local_declaring_kinds,
                    profile.expression_list_kinds,
                )
            });
            // The left of an assignment is a write even where it declares nothing —
            // and `+=` is a read as well, which is what stops it looking like a
            // second write over a value nobody looked at.
            // A language that writes `a, b = x, y` puts the target inside a list even
            // when there is only one of them.
            let holder = above.filter(|parent| {
                profile.expression_list_kinds.contains(&kind_of(parent))
                    && parent.named_child_count() == 1
            });
            let statement = holder.and_then(|list| list.parent()).or(above);
            let target = |kinds: &[&str]| {
                statement.is_some_and(|parent| {
                    kinds.contains(&kind_of(parent))
                        && parent.child_by_field_name("left").is_some_and(|left| {
                            left.id() == node.id() || Some(left.id()) == holder.map(|l| l.id())
                        })
                })
            };
            // A method's name sits in the same field a declaration's does in some
            // grammars, so `rows.add(x)` would read as a write to `add`.
            let reaching = above.is_some_and(|parent| {
                profile.is_call(parent)
                    || profile.is_member_access(parent)
                    || profile.markup_kinds.contains(&kind_of(parent))
            });
            let assigns = target(profile.assignment_kinds);
            let updates = target(profile.compound_assignment_kinds)
                && operator_of(above.unwrap_or(node), self.source)
                    .is_some_and(|operator| operator.len() > 1 && operator.ends_with('='));
            let wrote = (declares || assigns || updates) && !reaching;
            // A write lands after everything the right-hand side read, even though
            // the name being written is the first thing on the line.
            let at = if assigns || updates {
                statement.map_or(node.end_byte(), |parent| parent.end_byte())
            } else {
                node.start_byte()
            };
            let mut block = body.id();
            let mut parent = above;
            while let Some(current) = parent {
                if current.id() == body.id() {
                    break;
                }
                if profile.is_block(current) {
                    block = current.id();
                    break;
                }
                parent = current.parent();
            }
            seen.entry(self.text(node)).or_default().push(Touch {
                at,
                block,
                wrote,
                read: !wrote || updates,
                declared: declares && !reaching,
            });
        }
        for mentions in seen.values_mut() {
            mentions.sort_unstable_by_key(|touch| touch.at);
        }
        seen
    }

    /// A value written and written again with nothing reading the first one.
    fn note_stores(&mut self, function: Node<'a>) {
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
                    pair[0].wrote
                        && !pair[0].read
                        && pair[1].wrote
                        && !pair[1].read
                        && pair[0].block == pair[1].block
                })
            {
                self.out.dead_stores.push(Spot {
                    span: Span::new(window[0].at as u32, window[0].at as u32 + name.len() as u32),
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

    /// A field the constructor sets and nothing else ever writes.
    fn note_assign_once_fields(&mut self, class: Node<'a>) {
        let profile = self.profile;
        if profile.immutable_markers.is_empty() {
            return;
        }
        let Some(body) = self.class_body(class) else {
            return;
        };
        let fields: Vec<(String, Node<'a>)> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.field_kinds.contains(&kind_of(child)))
            .filter(|child| {
                let head = self.text(*child);
                !profile
                    .immutable_markers
                    .iter()
                    .any(|marker| head.split_whitespace().any(|word| word == *marker))
            })
            .filter_map(|field| Some((self.declared_field_name(field)?, field)))
            .collect();
        if fields.is_empty() {
            return;
        }
        let members: Vec<Node<'a>> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.method_kinds.contains(&kind_of(child)))
            .collect();
        for (name, at) in fields {
            let written_outside = members.iter().any(|member| {
                let is_constructor = self
                    .function_name(*member)
                    .map(|node| self.text(node))
                    .is_some_and(|named| {
                        profile.constructor_names.contains(&named)
                            || self.enclosing_class_name(*member).as_deref() == Some(named)
                    })
                    || profile.constructor_kinds.contains(&kind_of(member));
                if is_constructor {
                    return false;
                }
                self.function_body(*member).is_some_and(|inner| {
                    self.touches(inner)
                        .get(name.as_str())
                        .is_some_and(|mentions| mentions.iter().any(|touch| touch.wrote))
                })
            });
            if !written_outside {
                self.out.assign_once_fields.push(Spot {
                    span: span_of(at),
                    detail: name,
                    count: 1,
                });
            }
        }
    }

    /// Dividing one whole number by another. The remainder goes and nothing in the
    /// line says it was meant to.
    fn note_integer_division(&mut self, node: Node<'a>) {
        let profile = self.profile;
        if operator_of(node, self.source) != Some("/") {
            return;
        }
        let Some((left, right)) = operands_of(node) else {
            return;
        };
        let whole = |side: Node<'a>| {
            let side = self.see_through(side);
            profile.number_kinds.contains(&kind_of(side))
                && !self.text(side).contains(['.', 'e', 'E'])
        };
        if !whole(left) || !whole(right) {
            return;
        }
        self.out.integer_divisions.push(Spot {
            span: span_of(node),
            detail: squashed(self.text(node)),
            count: 1,
        });
    }

    /// A call whose answer is put somewhere nobody reads. The name says the author
    /// saw the answer and decided not to look at it. A call with several results puts
    /// the error last, so only that position decides: `_, err := w.Write(b)` drops
    /// the count and keeps the error. The spot counts the results the call returned.
    fn note_discarded_error(&mut self, node: Node<'a>) {
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

    /// A Go call to a function this file declares, whose results do not include an
    /// error: discarding one of them drops a value, not a failure.
    fn answers_without_an_error(&self, call: Node<'a>) -> bool {
        if self.profile.language != rqx_core::Language::Go {
            return false;
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

    fn note_exits(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let guards = (0..body.named_child_count())
            .filter_map(|index| body.named_child(index))
            .take_while(|child| {
                kind_of(child) == profile.if_kind
                    || profile.exit_kinds.iter().any(|(k, _)| *k == kind_of(child))
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
            if profile.exit_kinds.iter().any(|(k, _)| *k == kind_of(node)) {
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
            if profile.case_kinds.contains(&kind_of(current)) {
                arms += 1;
                continue;
            }
            if profile.switch_kinds.contains(&kind_of(current)) {
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
            if profile.assignment_kinds.contains(&kind_of(current)) {
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
                .any(|(kind, _)| *kind == kind_of(node))
        };
        let last = (0..block.named_child_count())
            .filter_map(|index| block.named_child(index))
            .rfind(|child| !profile.is_comment(*child));
        match last {
            Some(node) if exits(node) => true,
            Some(node) if kind_of(node) == profile.if_kind => {
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
            .any(|(kind, _)| *kind == kind_of(statement))
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
        if self.profile.expression_list_kinds.contains(&kind_of(node)) {
            return node.named_child(0).unwrap_or(node);
        }
        node
    }

    fn receiver_of<'d>(&self, dotted: &'d str) -> Option<&'d str> {
        self.names().receiver_of(dotted)
    }

    fn matches_any(&self, dotted: &str, entries: &[&str]) -> bool {
        self.names().matches_any(dotted, entries)
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

    /// `Buffer.concat` and `Array.from` are called on a type rather than on something
    /// holding elements, so they walk their argument once instead of once per pass.
    fn names_a_type(&self, receiver: &str) -> bool {
        receiver
            .rsplit('.')
            .next()
            .unwrap_or(receiver)
            .starts_with(|first: char| first.is_ascii_uppercase())
    }

    fn worth_scanning(&self, receiver: &str) -> bool {
        !receiver.is_empty()
            && !self.already_keyed(receiver)
            && !self.holds_text(receiver)
            && !self.fixed_size(receiver)
            && !self.names_a_type(receiver)
    }

    fn arguments(&self, call: Node<'a>) -> Vec<Node<'a>> {
        self.names().arguments(call)
    }

    fn is_literal(&self, node: Node<'a>) -> bool {
        let kind = kind_of(node);
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
                    .is_some_and(|first| self.profile.string_kinds.contains(&kind_of(first)))
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

        if self.in_async() && self.names().is_blocking(&dotted) {
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
            self.profile.argument_list_kinds.contains(&kind_of(child))
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
            if self.text_declared.contains(name) {
                self.note(
                    |out| &mut out.typed_text_accumulators,
                    span_of(node),
                    name.to_owned(),
                );
            } else {
                self.note(
                    |out| &mut out.copied_accumulators,
                    span_of(node),
                    name.to_owned(),
                );
            }
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
                .any(|child| self.profile.string_kinds.contains(&kind_of(child)))
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
        if self
            .profile
            .perf
            .string_types
            .contains(&declared.trim_start_matches("final ").trim())
        {
            self.text_declared.insert(name.clone());
        }
        self.declared
            .entry(name)
            .or_insert_with(|| format!("{declared} {head}").trim().to_owned());
    }

    fn walk(&mut self, node: Node<'a>) {
        let kind = kind_of(node);
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
                .filter(|child| kind_of(child) == "for_in_clause")
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
            self.note_module_state(node);
        }
        if profile.is_function(node) {
            self.note_flag_parameters(node);
            self.note_unquestioning_name(node);
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
                        .all(|child| profile.empty_body_kinds.contains(&kind_of(child)))
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
            self.note_duplicate_condition(node);
        }
        if profile.switch_kinds.contains(&kind) {
            self.note_switch_width(node);
            self.note_missing_default(node);
            self.note_silent_fallthrough(node);
        }
        if profile.catch_kinds.contains(&kind) {
            self.note_rethrow_only_catch(node);
            self.note_catch_all(node);
        }
        if profile.finally_kinds.contains(&kind) {
            self.note_exit_from_finally(node);
        }
        if kind == "try_statement" {
            self.note_wide_try(node);
            self.note_blanket_try(node);
            self.note_shadowed_handler(node);
        }
        if profile.throw_kinds.contains(&kind) {
            self.note_generic_throw(node);
        }
        self.note_wildcard_import(node);
        if profile.qualified_name_kinds.contains(&kind)
            && self.test_depth == 0
            && let Some(writer) = profile
                .debug_writers
                .iter()
                .find(|writer| **writer == self.text(node).replace("::", "."))
        {
            self.out.debug_output.push(Spot {
                span: span_of(node),
                detail: (*writer).to_owned(),
                count: 1,
            });
        }
        if profile.loop_kinds.contains(&kind) {
            self.note_redundant_jump(node);
        }
        if profile.loop_kinds.contains(&kind) {
            self.note_single_pass_loop(node);
            self.note_mutation_while_iterating(node);
        }
        self.note_production_assert(node);
        if profile.ternary_kinds.contains(&kind) {
            self.note_nested_ternary(node);
        }
        if profile.negation_kinds.contains(&kind) {
            self.note_negated_comparison(node);
        }
        if profile.is_class(node) {
            self.note_class_shape(node);
            self.note_empty_abstraction(node);
            self.note_split_cohesion(node);
            self.note_assign_once_fields(node);
            self.note_lonely_fields(node);
            self.note_many_collaborators(node);
        }
        self.note_declared_name(node);
        self.note_named_shape(node);
        self.note_ambient_dependency(node);
        if profile.is_member_access(node) {
            self.note_private_reach(node);
        }
        if profile.is_function(node) {
            self.note_empty_method(node);
            self.note_loop_capture(node);
            self.note_passthrough_override(node);
            self.note_stores(node);
            self.note_instance_free_method(node);
            self.note_over_logging(node);
            self.note_sequential_await(node);
            self.note_guard_cascade(node);
            self.note_fresh_local_return(node);
            self.note_many_locals(node);
            self.note_working_constructor(node);
            self.note_mixed_return(node);
            self.note_placeholder_body(node);
            self.note_passthrough(node);
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
            self.note_ambiguous_precedence(node);
            self.note_integer_division(node);
        }
        if kind == "expression_statement" {
            self.note_discarded_expression(node);
        }
        if profile.declarator_kinds.contains(&kind)
            || profile.local_declaring_kinds.contains(&kind)
            || profile.parameter_kinds.contains(&kind)
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
        if profile.assignment_kinds.contains(&kind)
            || profile.local_declaring_kinds.contains(&kind)
            || profile.declarator_kinds.contains(&kind)
        {
            self.note_discarded_error(node);
        }
        if profile.assignment_kinds.contains(&kind) {
            self.note_copy_accumulation(node);
            self.note_self_built_collaborator(node);
        }
        if profile.assignment_kinds.contains(&kind)
            || profile.compound_assignment_kinds.contains(&kind)
        {
            self.note_loop_concatenation(node);
        }
        if profile.is_call(node) {
            self.note_perf_call(node);
            self.note_call(node);
            self.note_sleep(node);
            self.note_nested_continuation(node);
            self.note_library_exit(node);
            self.note_boolean_argument(node);
            if self
                .called_name(node)
                .is_some_and(|name| profile.test.is_test(&name))
            {
                self.test_depth += 1;
                self.holds_tests = true;
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
        if kind == "return_statement" {
            self.note_many_results(node);
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
            let excepted = |cast: Node<'a>| {
                profile
                    .imprecise_type_exceptions
                    .iter()
                    .any(|exception| self.text(cast).trim_end().ends_with(exception))
            };
            // `value as unknown as Row` and `(Row) (Object) value` are one decision
            // written as two casts, so a chain is reported once, at the outermost cast.
            let chained = node.parent().is_some_and(|outer| {
                let cast_value = outer
                    .child_by_field_name("value")
                    .or_else(|| outer.named_child(0));
                kind_of(outer) == kind && cast_value == Some(node) && !excepted(outer)
            });
            if !excepted(node) && !chained {
                self.out.imprecise_types.push(Spot {
                    span: span_of(node),
                    detail: (*label).to_owned(),
                    count: 1,
                });
            }
        }
        if node.named_child_count() == 0
            && profile.imprecise_type_names.contains(&self.text(node))
            && self.is_in_type_position(node)
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
            let target = if profile.expression_list_kinds.contains(&kind_of(target)) {
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
fn short_names(scopes: &Scopes, source: &str, scope_lines: u32) -> Vec<Spot> {
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
fn distant_declarations(
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

/// A name an import binds and the file never mentions again. Read from the module
/// facts rather than the scope table: an import binds at file level in every
/// language here, while only some of them give the bound name a node the resolver
/// would recognise as a declaration.
#[deprecated(note = "use `unused_imports_over`, which shares the file's one traversal")]
pub fn unused_imports(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
    markup: bool,
    imports: &[rqx_core::facts::ImportRequest],
) -> Vec<Spot> {
    unused_imports_over(profile, source, &Nodes::of(root), markup, imports)
}

pub fn unused_imports_over(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    markup: bool,
    imports: &[rqx_core::facts::ImportRequest],
) -> Vec<Spot> {
    let brought: Vec<(&rqx_core::facts::ImportRequest, &String)> = imports
        .iter()
        .filter(|request| !request.re_export)
        .flat_map(|request| request.locals.iter().map(move |name| (request, name)))
        .collect();
    // A tree with an error has swallowed some of its identifiers, and every one it
    // swallowed reads here as a name the file never mentions.
    if brought.is_empty() || nodes.root().has_error() {
        return Vec::new();
    }
    let mut mentioned: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for node in nodes.iter() {
        let named = profile.is_identifier(node)
            || profile.binding_identifier_kinds.contains(&kind_of(node))
            || profile.reference_only_kinds.contains(&kind_of(node))
            || profile.property_kinds.contains(&kind_of(node));
        if !named {
            continue;
        }
        let span = span_of(node);
        // The import statement spells the name it binds, so counting it there would
        // make every import look read.
        if imports
            .iter()
            .any(|request| request.span.start <= span.start && span.end <= request.span.end)
        {
            continue;
        }
        mentioned.insert(text_of(node, source));
    }
    let mut spots: Vec<Spot> = brought
        .into_iter()
        .filter(|(_, name)| {
            !name.starts_with('_')
                && !mentioned.contains(name.as_str())
                && !(markup && profile.markup_runtimes.contains(&name.as_str()))
        })
        .map(|(request, name)| Spot {
            span: request.span,
            detail: name.clone(),
            count: 1,
        })
        .collect();
    spots.sort_by_key(|spot| spot.span.start);
    spots
}

/// Only names a function declares. A package, module or class level name may be read
/// by another file, including one a build tag or platform suffix leaves out of this
/// scan, so finding no reference in this file does not show that it is unused.
fn unused_locals(scopes: &Scopes) -> Vec<Spot> {
    let mut spots: Vec<Spot> = scopes
        .of_kind(BindingKind::Local)
        .filter(|binding| scopes.is_local(binding.scope))
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
#[deprecated(note = "use `waivers_over`, which shares the file's one traversal")]
pub fn waivers(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
) -> Vec<rqx_core::suppress::Suppression> {
    waivers_over(profile, source, &Nodes::of(root))
}

pub fn waivers_over(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
) -> Vec<rqx_core::suppress::Suppression> {
    nodes
        .iter()
        .filter(|node| profile.is_comment(*node))
        .filter_map(|comment| {
            rqx_core::suppress::Suppression::parse(text_of(comment, source), span_of(comment))
        })
        .collect()
}

/// A numeric literal a reader would call a decimal. Exponent form counts, because
/// `1e-3` is stored the same way `0.001` is; an integer does not, because comparing
/// one is exact whatever it is compared against.
fn is_fractional(text: &str) -> bool {
    let body = text.trim_start_matches(['-', '+']);
    if body.is_empty() || !body.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return false;
    }
    // A hex or binary literal has no fraction, and its `e` is a digit.
    if body.starts_with("0x") || body.starts_with("0X") || body.starts_with("0b") {
        return false;
    }
    let digits = body.trim_end_matches(['f', 'F', 'd', 'D', 'l', 'L']);
    digits.contains('.') || digits.contains(['e', 'E'])
}

fn scan_comments(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    limits: Limits,
    out: &mut Hygiene,
) {
    let mut runs: Vec<(Span, u32)> = Vec::new();
    // A divider drawn inside a licence header is part of the notice.
    let notices = crate::notices::protected_comments(
        source,
        nodes.iter().filter(|node| profile.is_comment(*node)),
    );
    for comment in nodes.iter().filter(|node| profile.is_comment(*node)) {
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
        // Recorded outside the chain below: how old a note is has nothing to do with
        // which of the comment rules claims it.
        if let Some(marker) = crate::vocabulary::todo_marker(text) {
            out.todos.push(Spot {
                span,
                detail: marker.to_owned(),
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
        } else if crate::vocabulary::banner(text) && notices.binary_search(&span).is_err() {
            out.banner_comments.push(Spot {
                span,
                detail: String::new(),
                count: 1,
            });
        } else if let Some(marker) = crate::vocabulary::unattached_todo(text) {
            out.orphan_todos.push(Spot {
                span,
                detail: marker.to_owned(),
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

    note_narration(profile, source, nodes, limits, out);
    note_twin_functions(profile, source, nodes, out);
    note_unused_fixtures(profile, source, nodes, out);
    note_body_docstrings(profile, source, nodes, out);

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

#[deprecated(note = "use `collect_over`, which shares the file's one traversal")]
pub fn collect(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
    scopes: &Scopes,
    limits: Limits,
    extra: &rqx_core::config::Performance,
) -> Hygiene {
    collect_over(profile, source, &Nodes::of(root), scopes, limits, extra)
}

pub fn collect_over(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    scopes: &Scopes,
    limits: Limits,
    extra: &rqx_core::config::Performance,
) -> Hygiene {
    let root = nodes.root();
    let mut walker = Walker {
        profile,
        source,
        nodes,
        limits,
        extra,
        out: Hygiene::default(),
        numbers: HashMap::new(),
        boundaries: HashMap::new(),
        conventions: Vec::new(),
        verbs: Vec::new(),
        declared: HashMap::new(),
        text_declared: HashSet::new(),
        frames: vec![PerfFrame::default()],
        test_depth: 0,
        holds_tests: false,
        block_depth: 0,
        asserting_helpers: Helpers::default(),
    };
    walker.asserting_helpers = walker.find_asserting_helpers();
    walker.walk(root);
    // Whether the file holds tests is only knowable once it has been read. A test file
    // is a list of registrations, and registering is the whole of what loading it is
    // for. Its setup, mocks and helpers reach for the clock and the environment to
    // arrange a case, which is the test controlling them, not depending on them, and
    // the statements it seeds a database with are fixtures.
    if walker.holds_tests {
        walker.out.ambient_dependencies.clear();
        walker.out.mixed_languages.clear();
    } else {
        walker.note_import_time_work(root);
    }
    walker.out.mixed_languages = once_per_language(std::mem::take(&mut walker.out.mixed_languages));
    // Which names the file shares is only settled once every declaration has been
    // seen, so the functions that hand one back are read afterwards.
    // Which functions answer later is only settled once the file has been read, so
    // the statements that drop one of those answers are found afterwards.
    let awaited: Vec<String> = nodes
        .iter()
        .filter(|node| profile.is_function(*node) && profile.is_async(*node))
        .filter_map(|node| {
            let named = node.child_by_field_name("name")?;
            Some(text_of(named, source).to_owned())
        })
        .collect();
    if !awaited.is_empty() {
        let statements: Vec<Node<'_>> = nodes
            .iter()
            .filter(|node| kind_of(node) == "expression_statement")
            .collect();
        for statement in statements {
            walker.note_dropped_promise(statement, &awaited);
        }
    }
    let shared: Vec<String> = walker
        .out
        .module_state
        .iter()
        .map(|spot| spot.detail.clone())
        .collect();
    if !shared.is_empty() {
        for function in nodes.iter().filter(|node| profile.is_function(*node)) {
            walker.note_global_accessor(function, &shared);
        }
    }

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
    out.distant_declarations = distant_declarations(
        profile,
        root,
        scopes,
        source,
        limits.declaration_distance_lines,
    );
    scan_comments(profile, source, nodes, limits, &mut out);

    out.mutated_parameters = reassigned_parameters(
        profile,
        root,
        scopes,
        std::mem::take(&mut out.mutated_parameters),
    );
    out
}

/// One finding per foreign language in a file, at its first string, counting every
/// string written in it. Moving the language out is one change, however many strings
/// it touches.
fn once_per_language(strings: Vec<Spot>) -> Vec<Spot> {
    let mut first: Vec<Spot> = Vec::new();
    for spot in strings {
        match first.iter_mut().find(|seen| seen.detail == spot.detail) {
            Some(seen) => seen.count += spot.count,
            None => first.push(spot),
        }
    }
    first
}

/// The assignments whose target is a parameter of a function around it, one per
/// parameter. A name is matched through the scopes rather than by spelling: a local
/// that happens to share its name with a parameter elsewhere in the file is not one,
/// and neither is a declaration that introduces a new name. A receiver (Go's, or
/// Python's `self`) or a Go named result is not an input, and filling a named result
/// is how a Go function answers.
fn reassigned_parameters(
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

/// The three rules about a comment's relationship to the code under it. They share
/// one walk because each needs the same pairing of a comment with what it precedes.
fn note_narration(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    limits: Limits,
    out: &mut Hygiene,
) {
    let index = rqx_core::LineIndex::new(source);
    let comments: Vec<Node<'_>> = nodes
        .iter()
        .filter(|node| profile.is_comment(*node))
        .collect();
    let leads = crate::tree::comment_leads(
        nodes.root(),
        |node| profile.is_comment(node),
        |node| profile.is_block(node),
    );
    let leads = |comment: Node<'_>| leads.get(&comment.id()).copied();

    // A comment, a line of code, a comment, a line of code: the author is reading the
    // code aloud. A run of comment lines with no code between them is a paragraph.
    let mut run = 0_u32;
    let mut opened = None;
    let mut previous: Option<Span> = None;
    for comment in &comments {
        let span = span_of(*comment);
        // Code between two comments continues the narration; a blank line ends it,
        // and so does a second comment written directly underneath the first.
        // A comment above a field or a property signature is documentation of a
        // shape, not narration of a run of work.
        let over_a_statement = leads(*comment).is_some_and(|node| {
            let kind = kind_of(node);
            !profile.field_kinds.contains(&kind)
                && !profile.signature_kinds.contains(&kind)
                && !profile.property_definition_kinds.contains(&kind)
        });
        let narrates = over_a_statement
            && previous.is_some_and(|last| {
                let between = &source[last.end as usize..span.start as usize];
                // The first and last fragments are the tails of the two comment lines
                // themselves, so only the lines between them can be blank.
                let lines: Vec<&str> = between.lines().collect();
                let blank = lines
                    .iter()
                    .enumerate()
                    .any(|(at, line)| at > 0 && at + 1 < lines.len() && line.trim().is_empty());
                between.chars().any(|c| !c.is_whitespace()) && !blank
            });
        if narrates {
            run += 1;
        } else {
            run = if over_a_statement { 1 } else { 0 };
            opened = Some(*comment);
        }
        if run == limits.narrated_run
            && let Some(first) = opened
        {
            out.narrated_runs.push(Spot {
                span: Span::new(span_of(first).start, span.end),
                detail: run.to_string(),
                count: run,
            });
        }
        previous = Some(span);
    }

    // Each `//` line is a node of its own, so the block above a function has to be
    // put back together before it can be measured against the body.
    let mut blocks: Vec<(Span, u32)> = Vec::new();
    for comment in &comments {
        let span = span_of(*comment);
        let lines = text_of(*comment, source).lines().count() as u32;
        match blocks.last_mut() {
            Some((last, held))
                if source[last.end as usize..span.start as usize]
                    .chars()
                    .all(char::is_whitespace)
                    && index.locate(span.start).0 == index.locate(last.end).0 + 1 =>
            {
                *last = Span::new(last.start, span.end);
                *held += lines;
            }
            _ => blocks.push((span, lines)),
        }
    }

    for (span, explanation) in blocks {
        let Some(comment) = comments
            .iter()
            .copied()
            .find(|node| span_of(*node).start == span.start)
        else {
            continue;
        };
        let Some(declared) = leads(comment).map(|node| unwrap_export(profile, node)) else {
            continue;
        };
        if !profile.is_function(declared) {
            continue;
        }
        // A comment opening the file describes the file, and the first declaration
        // under it is not what it was written about.
        if nodes
            .root()
            .named_child(0)
            .is_some_and(|first| span_of(first).start == span.start)
        {
            continue;
        }
        // Lines rather than statements: a body that is one long expression — a
        // returned tree of markup, a chained pipeline — has almost no statements in
        // it and is not thereby short.
        let body = declared
            .child_by_field_name("body")
            .or_else(|| {
                declared
                    .named_children(&mut declared.walk())
                    .find(|child| profile.is_block(*child))
            })
            .map(|body| {
                let extent = span_of(body);
                index
                    .locate(extent.end)
                    .0
                    .saturating_sub(index.locate(extent.start).0)
                    + 1
            })
            .unwrap_or(0);
        if body > 0 && explanation > body * limits.explanation_ratio {
            out.long_explanations.push(Spot {
                span,
                detail: format!("{explanation} lines above {body}"),
                count: explanation,
            });
        }
        if let Some(name) = restates_signature(profile, source, comment, declared) {
            out.restated_docs.push(Spot {
                span,
                detail: name,
                count: 1,
            });
        }
    }
}

/// A language whose doc comment is a string inside the body rather than a comment
/// above the declaration.
fn note_body_docstrings(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    out: &mut Hygiene,
) {
    for declared in nodes.iter().filter(|node| profile.is_function(*node)) {
        let Some(body) = declared.child_by_field_name("body") else {
            continue;
        };
        let Some(first) = body.named_child(0) else {
            continue;
        };
        let doc = if profile.string_kinds.contains(&kind_of(first)) {
            first
        } else if kind_of(first) == "expression_statement"
            && first
                .named_child(0)
                .is_some_and(|value| profile.string_kinds.contains(&kind_of(value)))
        {
            first.named_child(0).unwrap_or(first)
        } else {
            continue;
        };
        if let Some(name) = restates_signature(profile, source, doc, declared) {
            out.restated_docs.push(Spot {
                span: span_of(doc),
                detail: name,
                count: 1,
            });
        }
    }
}

/// A declaration wearing an export wrapper is still the declaration.
fn unwrap_export<'tree>(profile: &'static LanguageProfile, node: Node<'tree>) -> Node<'tree> {
    if profile.export_kinds.contains(&kind_of(node)) {
        return node.named_child(0).unwrap_or(node);
    }
    node
}

/// Whether a doc comment says only what the signature already says. Every word it
/// adds beyond the name and the parameters is the reason to keep it.
fn restates_signature(
    profile: &'static LanguageProfile,
    source: &str,
    comment: Node<'_>,
    declared: Node<'_>,
) -> Option<String> {
    const FILLER: [&str; 18] = [
        "the", "a", "an", "of", "to", "for", "and", "or", "this", "that", "is", "it", "with",
        "from", "in", "on", "by", "as",
    ];
    const SIGNATURE_WORDS: [&str; 6] = ["return", "returns", "param", "params", "parameter", "arg"];
    let parameters =
        crate::tree::find_within(declared, profile.parameter_list_kinds, profile.block_kinds)
            .into_iter()
            .next_back();
    // C++ keeps the name under a declarator rather than in a field of its own, so
    // the last name written before the parameter list is the one.
    let name = declared
        .child_by_field_name("name")
        .or_else(|| {
            let stop = parameters.map_or(u32::MAX as usize, |list| list.start_byte());
            descendants(declared)
                .into_iter()
                .rfind(|node| node.start_byte() < stop && profile.is_identifier(*node))
        })
        .map(|node| text_of(node, source).to_owned())?;
    let mut known: Vec<String> = split_words(&name);
    if let Some(list) = parameters {
        for parameter in list.named_children(&mut list.walk()) {
            for word in split_words(text_of(parameter, source)) {
                known.push(word);
            }
        }
    }
    let body: Vec<String> = split_words(text_of(comment, source))
        .into_iter()
        .filter(|word| word.len() > 1)
        .filter(|word| !FILLER.contains(&word.as_str()))
        .filter(|word| !SIGNATURE_WORDS.contains(&word.as_str()))
        .collect();
    // Nothing said at all is `empty-block`'s business, not this rule's.
    if body.is_empty() || body.iter().any(|word| !known.contains(word)) {
        return None;
    }
    Some(name)
}

/// One identifier or one line of prose as its separate lowercase words.
fn split_words(text: &str) -> Vec<String> {
    let letters: Vec<char> = text.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    for (at, character) in letters.iter().copied().enumerate() {
        if !character.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        // A run of capitals is one word: `userID` is user and id, not user, i and d.
        let opens_word = character.is_uppercase()
            && !current.is_empty()
            && (letters[at - 1].is_lowercase()
                || letters.get(at + 1).is_some_and(|next| next.is_lowercase()));
        if opens_word {
            words.push(std::mem::take(&mut current));
        }
        current.push(character.to_ascii_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
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

/// Two functions in one file written the same way. One of them was copied, and the
/// next change has to find both.
fn note_twin_functions(
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

/// A value a setup hook prepares that no test in the file ever reads. Every case
/// pays for it and none of them wanted it.
fn note_unused_fixtures(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    out: &mut Hygiene,
) {
    let vocabulary = &profile.test;
    let setups: Vec<Node<'_>> = nodes
        .iter()
        .filter(|node| profile.is_call(*node) || profile.is_function(*node))
        .filter(|node| {
            let named = if profile.is_call(*node) {
                profile.call_tail(*node, source).map(str::to_owned)
            } else {
                // C++ keeps a function's name under a declarator, so the last name
                // written before the body is the one.
                let body_at = node
                    .child_by_field_name("body")
                    .map_or(usize::MAX, |body| body.start_byte());
                node.child_by_field_name("name")
                    .or_else(|| {
                        descendants(*node).into_iter().rfind(|inner| {
                            inner.start_byte() < body_at && profile.is_identifier(*inner)
                        })
                    })
                    .map(|name| text_of(name, source).to_owned())
            };
            named.is_some_and(|name| vocabulary.is_setup(&name))
        })
        .collect();
    if setups.is_empty() {
        return;
    }
    // Every identifier in the file by the text it spells, gathered once. Asking the
    // whole tree per prepared value made this cubic in the size of a test file.
    let mut spelled: std::collections::HashMap<&str, Vec<Node<'_>>> =
        std::collections::HashMap::new();
    for node in nodes.iter() {
        if profile.is_identifier(node) {
            spelled.entry(text_of(node, source)).or_default().push(node);
        }
    }
    for setup in setups {
        let prepared: Vec<Node<'_>> = descendants(setup)
            .into_iter()
            .filter(|node| profile.is_identifier(*node))
            .filter(|node| {
                // The target can sit inside a list of one where the language writes
                // `a, b = x, y`.
                let listed = node.parent().filter(|parent| {
                    profile.expression_list_kinds.contains(&kind_of(parent))
                        && parent.named_child_count() == 1
                });
                let statement = listed
                    .and_then(|list| list.parent())
                    .or_else(|| node.parent());
                statement.is_some_and(|parent| {
                    profile.assignment_kinds.contains(&kind_of(parent))
                        && parent.child_by_field_name("left").is_some_and(|left| {
                            left.id() == node.id() || Some(left.id()) == listed.map(|l| l.id())
                        })
                })
            })
            .collect();
        for node in prepared {
            let name = text_of(node, source);
            let read = spelled.get(name).is_some_and(|others| {
                others.iter().any(|other| {
                    other.id() != node.id()
                        && !(other.start_byte() >= setup.start_byte()
                            && other.end_byte() <= setup.end_byte())
                        // Its own declaration is not somebody reading it.
                        && !crate::tree::is_binding_position(
                            *other,
                            &[],
                            profile.borrowed_name_kinds,
                            profile.local_declaring_kinds,
                            profile.expression_list_kinds,
                        )
                })
            });
            if !read {
                out.unused_fixtures.push(Spot {
                    span: span_of(node),
                    detail: name.to_owned(),
                    count: 1,
                });
            }
        }
    }
}

/// One mention of a name: where it is, and what it did to the value. A compound
/// assignment is both, which is the case the two rules reading this disagree about.
#[derive(Clone, Copy)]
struct Touch {
    at: usize,
    /// The run of statements the mention sits in. Two writes only overwrite each
    /// other when both always happen, and a write inside a branch does not.
    block: usize,
    wrote: bool,
    read: bool,
    declared: bool,
}

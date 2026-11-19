//! Rules about test code: what a test declares, checks and leaves unused.

use std::collections::HashMap;

use rqx_core::facts::{Hygiene, Spot};
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{Nodes, descendants, kind_of, operands_of, operator_of, span_of, text_of};
use crate::vocabulary::{CONVENTIONAL, IO_BARE, TestVocabulary};

use super::{Helpers, Walker, squashed};

/// What one statement in the arm of an `if` inside a test does to the test.
enum Ending {
    /// Fails or skips it.
    Fails,
    /// Leaves the arm without doing anything of its own: a `return`, or a `t.Logf`.
    Leaves,
    /// Carries on with work of its own, so the arm is a path the test takes.
    Continues,
}

impl<'a> Walker<'a> {
    fn chain_depth(&self, node: Node<'a>) -> u32 {
        let mut depth = 0;
        let mut next = Some(node);
        while let Some(node) = next.map(|node| self.see_through(node)) {
            next = if self.profile.is_member_access(node) {
                depth += 1;
                node.child_by_field_name(self.profile.object_field)
            } else if self.profile.is_call(node) {
                node.child_by_field_name(self.profile.callee_field)
            } else {
                None
            };
        }
        depth
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
    pub(super) fn find_asserting_helpers(&self) -> Helpers {
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

    /// A framework that finds its cases by naming convention marks them with an
    /// annotation rather than a call, so both live outside `note_call`.
    pub(super) fn note_declared_test(&mut self, function: Node<'a>) {
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
        if !vocabulary.names_a_case(&label) || !self.has_a_case_signature(function) {
            return;
        }
        let measures = vocabulary.measures(&label);
        // A framework whose case is a macro puts the name in the arguments, so the
        // declaration itself is called nothing but the macro. A declaration with a
        // name of its own, like Go's `func Test(t *testing.T)`, is called that name.
        if vocabulary.case_prefixes.contains(&label.as_str())
            && function.child_by_field_name("name").is_none()
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

    /// Whether a function named like a case is declared the way its framework finds
    /// one. A name is not enough: `App.Test` sends a request, `test(StackFrame)` is a
    /// `Predicate`, and `TestMain(m *testing.M)` sets up the package's tests.
    fn has_a_case_signature(&self, function: Node<'a>) -> bool {
        match self.profile.language {
            // `go test` runs top-level functions that take `*testing.T`, `*testing.B`
            // or `*testing.F`.
            rqx_core::Language::Go => {
                let takes_a_runner = function
                    .child_by_field_name("parameters")
                    .map(|list| self.text(list))
                    .is_some_and(|list| {
                        ["*testing.T", "*testing.B", "*testing.F"]
                            .iter()
                            .any(|runner| list.contains(runner))
                    });
                function.child_by_field_name("receiver").is_none() && takes_a_runner
            }
            // JUnit 4 and 5 and TestNG mark a case with an annotation; JUnit 3 finds
            // `test` methods on a subclass of `TestCase`.
            rqx_core::Language::Java => {
                let annotated = self.annotations(function).iter().any(|annotation| {
                    let name = annotation
                        .trim_start_matches('@')
                        .split('(')
                        .next()
                        .unwrap_or_default();
                    name.rsplit('.').next().unwrap_or(name).contains("Test")
                });
                annotated || self.extends_a_test_case(function)
            }
            _ => true,
        }
    }

    /// Whether the class around a Java method extends JUnit 3's `TestCase`, directly
    /// or through a base named like one.
    fn extends_a_test_case(&self, function: Node<'a>) -> bool {
        let mut current = function.parent();
        while let Some(node) = current {
            if self.profile.is_class(node) {
                return node
                    .child_by_field_name("superclass")
                    .is_some_and(|base| self.text(base).contains("TestCase"));
            }
            current = node.parent();
        }
        false
    }

    /// Whether a call the vocabulary names as a test is one. `fit` and `xit` are also
    /// ordinary words (`fit` a shape to a box), so for those the call has to be shaped
    /// like a registration, a title or a function handed over, and the name must not
    /// be one this file declares.
    pub(super) fn registers_test(&self, call: Node<'a>, name: &str) -> bool {
        if !self.profile.test.is_prefix_marked(name) {
            return true;
        }
        let declared_here = self
            .callee_root(call)
            .is_some_and(|root| self.local_marker_names.contains(&(root.start, root.end)));
        if declared_here {
            return false;
        }
        // `fit.each(table)` hands over a table; the dotted name is the runner's alone.
        name.contains('.')
            || self
                .names()
                .arguments(call)
                .iter()
                .enumerate()
                .any(|(index, argument)| {
                    self.profile.is_function(*argument)
                        || (index == 0 && self.profile.string_kinds.contains(&kind_of(argument)))
                })
    }

    /// The name a call starts from: `fit` in `fit(...)` and in `fit.each(...)(...)`.
    fn callee_root(&self, call: Node<'a>) -> Option<Span> {
        let profile = self.profile;
        let mut current = self.see_through(call.child_by_field_name(profile.callee_field)?);
        loop {
            if profile.is_identifier(current) {
                return Some(span_of(current));
            }
            let inner = if profile.is_member_access(current) {
                current.child_by_field_name(profile.object_field)
            } else if profile.is_call(current) {
                current.child_by_field_name(profile.callee_field)
            } else {
                None
            }?;
            current = self.see_through(inner);
        }
    }

    pub(super) fn note_call(&mut self, call: Node<'a>) {
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
            match vocabulary
                .directive(&name)
                .filter(|_| self.registers_test(call, &name))
            {
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
        // A chain is one finding, at its outermost call: each call inside it is the
        // start of the next link, and the chain is longest where it ends.
        let continues = call.parent().is_some_and(|outer| {
            self.profile.is_member_access(outer)
                && outer.child_by_field_name(self.profile.object_field) == Some(call)
        });
        if let Some(callee) = measured.filter(|_| !continues) {
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
    /// to whatever the code already does. Only the two sides as wholes count: a
    /// helper such as `at(8)` that builds part of the checked value and part of the
    /// expected one is not a comparison of a value with itself.
    fn compares_itself(&self, check: Node<'a>) -> bool {
        self.compared_sides(check)
            .into_iter()
            .any(|(checked, expected)| self.same_value(checked, expected))
    }

    /// The pairs of expressions a check holds against each other, however the
    /// framework spells them: `expect(a).toBe(b)`, `assertThat(a).isEqualTo(b)`,
    /// `assertEquals(a, b)`, `require.Equal(t, a, b)`, `EXPECT_EQ(a, b)`,
    /// `assert a == b` and `assert(a === b)`.
    fn compared_sides(&self, check: Node<'a>) -> Vec<(Node<'a>, Node<'a>)> {
        let profile = self.profile;
        let mut sides = Vec::new();
        // A callback inside a check is code the check runs, not one of its sides.
        for node in crate::tree::descendants_outside(check, |node| profile.is_function(node)) {
            if profile.assertion_statement_kinds.contains(&kind_of(node)) {
                sides.extend(
                    node.named_child(0)
                        .and_then(|asserted| self.equated(asserted)),
                );
                continue;
            }
            let is_assertion = profile.is_call(node)
                && self
                    .called_name(node)
                    .is_some_and(|name| profile.test.is_assertion(&name));
            if !is_assertion {
                continue;
            }
            let arguments = self.names().arguments(node);
            // A matcher's arguments are all expected values, two of them alike in
            // `toHaveBeenCalledWith(expect.anything(), expect.anything())`.
            match self.chain_head(node) {
                Some(head) => {
                    if let (Some(checked), Some(expected)) =
                        (self.names().arguments(head).first(), arguments.first())
                    {
                        sides.push((*checked, *expected));
                    }
                }
                None => {
                    if let [checked, expected, ..] = self.compared_arguments(arguments.clone())[..]
                    {
                        sides.push((checked, expected));
                    }
                }
            }
            sides.extend(
                arguments
                    .into_iter()
                    .filter_map(|argument| self.equated(argument)),
            );
        }
        sides
    }

    /// The two sides of an equality written as an operator. A negated one is left
    /// out: `x != x` is how a value is checked for NaN, and otherwise it fails.
    fn equated(&self, node: Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
        let node = self.see_through(node);
        let operator = operator_of(node, self.source)?;
        (self.profile.binary_kinds.contains(&kind_of(node))
            && self.profile.equality_operators.contains(&operator)
            && !operator.starts_with('!'))
        .then(|| operands_of(node))
        .flatten()
    }

    /// The arguments of a positional check that are values, without the runner's
    /// handle Go passes first or the message JUnit 4 puts first.
    fn compared_arguments(&self, mut arguments: Vec<Node<'a>>) -> Vec<Node<'a>> {
        let profile = self.profile;
        let leader = match arguments[..] {
            [leader, _, _, ..] => leader,
            _ => return arguments,
        };
        let leads = match profile.language {
            rqx_core::Language::Go => profile.is_identifier(leader),
            rqx_core::Language::Java => profile.string_kinds.contains(&kind_of(leader)),
            _ => false,
        };
        if leads {
            arguments.remove(0);
        }
        arguments
    }

    /// The call a fluent check starts from: `expect(a)` in `expect(a).not.toBe(b)`,
    /// `assertThat(a)` in `assertThat(a).isEqualTo(b)`. None for a call that is not
    /// written on another call's answer.
    fn chain_head(&self, call: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        let mut head = None;
        let mut current = call;
        while let Some(mut receiver) = self.call_receiver(current) {
            while profile.is_member_access(receiver) && !profile.is_call(receiver) {
                match receiver.child_by_field_name(profile.object_field) {
                    Some(object) => receiver = self.see_through(object),
                    None => break,
                }
            }
            if !profile.is_call(receiver) {
                break;
            }
            head = Some(receiver);
            current = receiver;
        }
        head
    }

    /// What a call is written on: `a` in `a.b()`.
    fn call_receiver(&self, call: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        let reach = if profile.is_member_access(call) {
            call
        } else {
            self.see_through(call.child_by_field_name(profile.callee_field)?)
        };
        if !profile.is_member_access(reach) {
            return None;
        }
        reach
            .child_by_field_name(profile.object_field)
            .map(|object| self.see_through(object))
    }

    /// Whether two sides of a check are one value: the same expression written
    /// twice, or one side a copy of the other. A call written twice runs twice, so
    /// `expect(f(1)).toBe(f(1))` checks that `f` answers the same each time, and a
    /// side with a call in it is never the same value as itself.
    fn same_value(&self, checked: Node<'a>, expected: Node<'a>) -> bool {
        let checked = self.see_through(checked);
        let expected = self.unwrap_matcher(self.see_through(expected));
        let settled = |node: Node<'a>| {
            !descendants(node)
                .into_iter()
                .any(|inner| self.profile.is_call(inner))
        };
        (settled(checked)
            && (self.same_expression(checked, expected) || self.copies(expected, checked)))
            || (settled(expected) && self.copies(checked, expected))
    }

    fn same_expression(&self, one: Node<'a>, other: Node<'a>) -> bool {
        squashed(self.text(one)) == squashed(self.text(other))
    }

    /// `is(x)`, `equalTo(x)`, `Eq(x)`, `Equal(x)`: a matcher that only says "equal
    /// to", read as the value it wraps.
    fn unwrap_matcher(&self, expected: Node<'a>) -> Node<'a> {
        const EQUALITY_MATCHERS: [&str; 5] = ["is", "equalTo", "Eq", "Equal", "BeEquivalentTo"];
        if !self.profile.is_call(expected)
            || !self
                .called_name(expected)
                .is_some_and(|name| EQUALITY_MATCHERS.contains(&name.as_str()))
        {
            return expected;
        }
        match self.names().arguments(expected)[..] {
            [wrapped] => self.see_through(wrapped),
            _ => expected,
        }
    }

    /// A value built from `original` and nothing else: `[...x]`, `{ ...x }`,
    /// `[*x]`, `structuredClone(x)`, `copy.deepcopy(x)`, `x.clone()`, `x.slice()`.
    fn copies(&self, copy: Node<'a>, original: Node<'a>) -> bool {
        const SPREADS: [&str; 3] = ["spread_element", "list_splat", "dictionary_splat"];
        const CLONING_CALLS: [&str; 3] = ["structuredClone", "copy.copy", "copy.deepcopy"];
        const CLONING_METHODS: [&str; 3] = ["clone", "copy", "slice"];
        let profile = self.profile;
        let spread: Vec<Node<'a>> = copy
            .named_children(&mut copy.walk())
            .filter(|child| !profile.is_comment(*child))
            .collect();
        if let [only] = spread[..]
            && SPREADS.contains(&kind_of(only))
            && let Some(spread_from) = only.named_child(0)
        {
            return self.same_expression(spread_from, original);
        }
        if !profile.is_call(copy) {
            return false;
        }
        let Some(name) = self.called_name(copy) else {
            return false;
        };
        let arguments = self.names().arguments(copy);
        if let [argument] = arguments[..]
            && CLONING_CALLS.contains(&name.as_str())
        {
            return self.same_expression(argument, original);
        }
        let method = name.rsplit('.').next().unwrap_or_default();
        arguments.is_empty()
            && CLONING_METHODS.contains(&method)
            && self
                .call_receiver(copy)
                .is_some_and(|receiver| self.same_expression(receiver, original))
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
}

/// A value a setup hook prepares that no test in the file ever reads. Every case
/// pays for it and none of them wanted it.
pub(super) fn note_unused_fixtures(
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

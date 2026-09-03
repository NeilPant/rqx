use std::collections::HashMap;

use oxc::ast::ast::{
    Argument, BindingPattern, CallExpression, CatchClause, Expression, Function, IfStatement,
    NumericLiteral, Statement, TSType,
};
use oxc::ast_visit::{Visit, walk};
use oxc::semantic::{Semantic, SymbolFlags};
use oxc::span::{GetSpan, Span};
use serde::{Deserialize, Serialize};

use crate::span_io;

/// Numbers that carry their own meaning wherever they appear. Without this list the
/// real magic numbers would be buried under every loop bound in the codebase.
const CONVENTIONAL: [f64; 7] = [0.0, 1.0, 2.0, -1.0, 10.0, 100.0, 1000.0];

const TEST_ROOTS: [&str; 3] = ["it", "test", "describe"];
const ASSERTION_ROOTS: [&str; 5] = ["expect", "assert", "should", "verify", "must"];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Spot {
    #[serde(with = "span_io")]
    pub span: Span,
    pub detail: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Hygiene {
    pub unused_locals: Vec<Spot>,
    pub unreachable: Vec<Spot>,
    pub commented_code: Vec<Spot>,
    pub magic_numbers: Vec<Spot>,
    pub empty_catches: Vec<Spot>,
    pub suppressions: Vec<Spot>,
    pub flag_parameters: Vec<Spot>,
    pub mutated_parameters: Vec<Spot>,
    pub complex_conditions: Vec<Spot>,
    pub negative_conditionals: Vec<Spot>,
    pub message_chains: Vec<Spot>,
    pub skipped_tests: Vec<Spot>,
    pub focused_tests: Vec<Spot>,
    pub assertionless_tests: Vec<Spot>,
    pub short_names: Vec<Spot>,
    pub imprecise_types: Vec<Spot>,
    pub journal_comments: Vec<Spot>,
    pub redundant_comments: Vec<Spot>,
    pub slow_tests: Vec<Spot>,
    pub mixed_languages: Vec<Spot>,
    pub boundary_arithmetic: Vec<Spot>,
    pub mixed_conventions: Vec<Spot>,
    pub side_effecting_queries: Vec<Spot>,
    pub vague_names: Vec<Spot>,
    pub distant_declarations: Vec<Spot>,
    pub obscured_expressions: Vec<Spot>,
    pub buried_configuration: Vec<Spot>,
    pub inconsistent_vocabulary: Vec<Spot>,
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub condition_operands: u32,
    pub message_chain_depth: u32,
    pub commented_code_lines: u32,
    pub magic_number_repeats: u32,
    pub short_name_scope_lines: u32,
    pub slow_test_ms: u32,
    pub boundary_repeats: u32,
    pub declaration_distance_lines: u32,
    pub expression_operations: u32,
    pub configuration_depth: u32,
}

fn callee_name(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::Identifier(identifier) => Some(identifier.name.to_string()),
        Expression::StaticMemberExpression(member) => {
            Some(format!("{}.{}", callee_name(&member.object)?, member.property.name))
        }
        _ => None,
    }
}

fn root_of(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

fn is_assertion(name: &str) -> bool {
    let root = root_of(name);
    ASSERTION_ROOTS.iter().any(|candidate| root == *candidate || root.ends_with(candidate))
}

fn test_suffix(name: &str) -> Option<(&'static str, &'static str)> {
    let root = root_of(name);
    let bare = root.trim_start_matches(['x', 'f']);
    if !TEST_ROOTS.contains(&bare) {
        return None;
    }
    if root.starts_with('x') {
        return Some(("skipped", "x-prefixed"));
    }
    if root.starts_with('f') {
        return Some(("focused", "f-prefixed"));
    }
    match name.rsplit('.').next()? {
        "skip" | "todo" => Some(("skipped", "skip")),
        "only" => Some(("focused", "only")),
        _ => None,
    }
}

/// Prose can end in a semicolon, so a comment counts as code only when most of its
/// lines both end like a statement and contain statement syntax.
fn looks_like_code(text: &str) -> bool {
    let body: Vec<&str> = text
        .lines()
        .map(|line| line.trim().trim_start_matches(['/', '*', ' ']).trim())
        .filter(|line| !line.is_empty())
        .collect();
    if body.is_empty() {
        return false;
    }
    let coded = body
        .iter()
        .filter(|line| {
            let closes = line.ends_with(';') || line.ends_with('{') || line.ends_with('}');
            let syntax = line.contains(" = ")
                || line.contains("=>")
                || line.contains("()")
                || ["return ", "const ", "let ", "var ", "if (", "for (", "import ", "export "]
                    .iter()
                    .any(|start| line.starts_with(start));
            closes && syntax
        })
        .count();
    coded > 0 && coded * 2 >= body.len()
}

/// Markers that record who wrote something and when. Version control answers both
/// questions, and unlike the comment it is not left behind by the next edit.
fn journal_marker(text: &str) -> Option<&'static str> {
    ["@author", "@date", "@created", "@modified", "Created by", "Created on", "Last modified"]
        .into_iter()
        .find(|marker| text.contains(marker))
}

/// Splits an identifier the way a reader does, so `getUserName` and `get user name`
/// compare equal.
fn identifier_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for character in name.chars() {
        if character == '_' || character == '-' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else if character.is_uppercase() && !current.is_empty() {
            words.push(std::mem::take(&mut current));
            current.push(character.to_ascii_lowercase());
        } else {
            current.push(character.to_ascii_lowercase());
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words.retain(|word| word.len() > 2);
    words
}

/// A comment is redundant when every word in it is already in the name underneath.
/// Requiring at least two words keeps `// x` and decorative dividers out of it.
fn redundant_subject(text: &str, following: &str) -> Option<String> {
    const FILLER: [&str; 12] =
        ["the", "this", "that", "and", "for", "with", "from", "into", "returns", "return", "gets", "sets"];

    let prose: Vec<String> = text
        .trim_start_matches(['/', '*', ' '])
        .trim_end_matches(['*', '/', ' '])
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 2)
        .map(str::to_ascii_lowercase)
        .filter(|word| !FILLER.contains(&word.as_str()))
        .collect();
    if prose.len() < 2 {
        return None;
    }

    let subject = following
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .find(|token| {
            token.len() > 2
                && !["export", "const", "let", "var", "function", "class", "async", "public",
                     "private", "readonly", "interface", "type", "return"]
                    .contains(token)
        })?;
    let name = identifier_words(subject);
    if name.is_empty() {
        return None;
    }
    prose.iter().all(|word| name.contains(word)).then(|| subject.to_owned())
}

/// A pause inside a test is wall-clock every developer pays on every run, and the
/// thing being waited for is almost always something the test could observe directly.
fn delay_call(name: &str) -> bool {
    matches!(root_of(name), "sleep" | "delay" | "wait" | "setTimeout" | "setInterval")
        || name.ends_with(".sleep")
        || name.ends_with(".setTimeout")
}

/// Only the two languages a template literal can hold without ambiguity. Guessing at
/// CSS from braces and colons would flag every object written across lines.
fn foreign_language(text: &str) -> Option<&'static str> {
    let upper = text.to_ascii_uppercase();
    if ["SELECT ", "INSERT INTO", "UPDATE ", "DELETE FROM", "CREATE TABLE"]
        .iter()
        .any(|clause| upper.contains(clause))
    {
        return Some("SQL");
    }
    ["<div", "<span", "<table", "<html", "<section", "<button"]
        .iter()
        .any(|tag| text.contains(tag))
        .then_some("HTML")
}

fn convention_of(name: &str) -> Option<&'static str> {
    if name.chars().all(|c| !c.is_lowercase()) {
        // SCREAMING_CASE is the one convention nobody mixes by accident.
        return None;
    }
    if name.contains('_') && !name.starts_with('_') {
        return Some("snake_case");
    }
    (name.chars().any(char::is_uppercase) && name.starts_with(char::is_lowercase))
        .then_some("camelCase")
}

/// A name that promises an answer rather than a change. Asking a question should
/// never alter what the next question returns.
fn query_prefix(name: &str) -> bool {
    ["get", "is", "has", "should", "can"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|rest| rest.chars().next().is_some_and(char::is_uppercase))
    })
}

/// Bare verbs that name the act of running rather than what is done. Anything with
/// an object attached — `handleTimeout`, `processOrder` — says enough and is left be.
fn vague_verb(name: &str) -> bool {
    const VAGUE: [&str; 10] =
        ["handle", "process", "manage", "doIt", "doWork", "doStuff", "perform", "operate", "stuff", "action"];
    VAGUE.contains(&name)
}

/// A value that configures where the program points or who it talks to. Restricted
/// to what cannot be anything else: a URL or an absolute path with real segments.
fn configuration_literal(text: &str) -> Option<&'static str> {
    if text.starts_with("http://") || text.starts_with("https://") || text.contains("://") {
        return Some("a URL");
    }
    (text.starts_with('/') && text.matches('/').count() >= 2 && !text.contains(' '))
        .then_some("an absolute path")
}

/// Verbs that mean the same thing to a reader. Splitting a name into one of these
/// plus its object is how the same concept is found under two names.
const SYNONYMS: [&[&str]; 5] = [
    &["get", "fetch", "retrieve", "load", "read"],
    &["set", "put", "save", "store", "write"],
    &["remove", "delete", "destroy", "drop", "erase"],
    &["make", "create", "build", "construct"],
    &["find", "search", "lookup", "locate"],
];

fn verb_and_object(name: &str) -> Option<(&'static str, String)> {
    SYNONYMS.iter().flat_map(|family| family.iter()).find_map(|verb| {
        let rest = name.strip_prefix(*verb)?;
        let head = rest.chars().next()?;
        head.is_uppercase().then(|| (*verb, rest.to_ascii_lowercase()))
    })
}

fn same_family(left: &str, right: &str) -> bool {
    SYNONYMS.iter().any(|family| family.contains(&left) && family.contains(&right))
}

fn suppression_directive(text: &str) -> Option<&'static str> {
    ["@ts-ignore", "@ts-nocheck", "@ts-expect-error", "eslint-disable", "biome-ignore"]
        .into_iter()
        .find(|directive| text.contains(directive))
}

struct Walker<'a> {
    limits: Limits,
    out: Hygiene,
    numbers: HashMap<String, Vec<Span>>,
    boundaries: HashMap<String, Vec<Span>>,
    conventions: Vec<(String, Span)>,
    verbs: Vec<(String, String, Span)>,
    test_depth: u32,
    block_depth: u32,
    /// Furthest point each name is read at. Collected here rather than from the
    /// semantic node store, which is not populated for this build configuration.
    last_read: HashMap<String, u32>,
    /// Nearest point each name is read at. The distance a reader carries a binding is
    /// measured to its first use; the furthest read says how long it stays live, which
    /// is a different question.
    first_read: HashMap<String, u32>,
    marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> Walker<'a> {
    fn operands(expression: &Expression<'a>) -> u32 {
        match expression {
            Expression::LogicalExpression(logical) => {
                Self::operands(&logical.left) + Self::operands(&logical.right)
            }
            Expression::ParenthesizedExpression(inner) => Self::operands(&inner.expression),
            _ => 1,
        }
    }

    fn chain_depth(expression: &Expression<'a>) -> u32 {
        match expression {
            Expression::StaticMemberExpression(member) => 1 + Self::chain_depth(&member.object),
            Expression::ComputedMemberExpression(member) => 1 + Self::chain_depth(&member.object),
            Expression::CallExpression(call) => Self::chain_depth(&call.callee),
            Expression::ParenthesizedExpression(inner) => Self::chain_depth(&inner.expression),
            _ => 0,
        }
    }

    /// Looks for an assertion anywhere in the test's callback rather than tracking a
    /// stack, because a helper called from the test is still the test asserting.
    fn body_asserts(argument: &Argument<'a>) -> bool {
        struct Hunt {
            found: bool,
        }
        impl<'b> Visit<'b> for Hunt {
            fn visit_call_expression(&mut self, call: &CallExpression<'b>) {
                if callee_name(&call.callee).is_some_and(|name| is_assertion(&name)) {
                    self.found = true;
                }
                walk::walk_call_expression(self, call);
            }
        }
        let mut hunt = Hunt { found: false };
        match argument {
            Argument::ArrowFunctionExpression(arrow) => {
                hunt.visit_arrow_function_expression(arrow);
            }
            Argument::FunctionExpression(function) => {
                if let Some(body) = &function.body {
                    hunt.visit_function_body(body);
                }
            }
            _ => return true,
        }
        hunt.found
    }
}

impl Walker<'_> {
    /// Counts the decisions packed into one expression. Nothing here is named, so
    /// each one is a step the reader has to hold rather than read.
    fn operations(expression: &Expression<'_>) -> u32 {
        #[derive(Default)]
        struct Count {
            total: u32,
        }
        impl<'b> Visit<'b> for Count {
            fn visit_binary_expression(&mut self, node: &oxc::ast::ast::BinaryExpression<'b>) {
                self.total += 1;
                walk::walk_binary_expression(self, node);
            }
            fn visit_logical_expression(&mut self, node: &oxc::ast::ast::LogicalExpression<'b>) {
                self.total += 1;
                walk::walk_logical_expression(self, node);
            }
            fn visit_conditional_expression(
                &mut self,
                node: &oxc::ast::ast::ConditionalExpression<'b>,
            ) {
                self.total += 1;
                walk::walk_conditional_expression(self, node);
            }
            fn visit_call_expression(&mut self, node: &CallExpression<'b>) {
                self.total += 1;
                walk::walk_call_expression(self, node);
            }
        }
        let mut count = Count::default();
        count.visit_expression(expression);
        count.total
    }

    /// A query that writes is only interesting when the write outlives the call, so
    /// assignments to a local are not counted — `this.x` and captured names are.
    fn body_mutates(body: &oxc::ast::ast::FunctionBody<'_>) -> bool {
        use oxc::ast::ast::{AssignmentTarget, SimpleAssignmentTarget};

        struct Hunt {
            found: bool,
        }
        impl<'b> Visit<'b> for Hunt {
            fn visit_assignment_expression(
                &mut self,
                assignment: &oxc::ast::ast::AssignmentExpression<'b>,
            ) {
                if matches!(
                    &assignment.left,
                    AssignmentTarget::StaticMemberExpression(_)
                        | AssignmentTarget::ComputedMemberExpression(_)
                ) {
                    self.found = true;
                }
                walk::walk_assignment_expression(self, assignment);
            }

            fn visit_update_expression(&mut self, update: &oxc::ast::ast::UpdateExpression<'b>) {
                if matches!(&update.argument, SimpleAssignmentTarget::StaticMemberExpression(_)) {
                    self.found = true;
                }
                walk::walk_update_expression(self, update);
            }
        }
        let mut hunt = Hunt { found: false };
        hunt.visit_function_body(body);
        hunt.found
    }
}

impl Walker<'_> {
    /// Shared by plain functions and by class methods, which arrive as an unnamed
    /// function expression under a key and would otherwise be invisible to both rules.
    fn name_the_body(&mut self, name: &str, span: Span, body: Option<&oxc::ast::ast::FunctionBody<'_>>) {
        if vague_verb(name) {
            self.out.vague_names.push(Spot { span, detail: name.to_owned(), count: 1 });
        }
        if let Some((verb, object)) = verb_and_object(name) {
            self.verbs.push((verb.to_owned(), object, span));
        }
        if query_prefix(name) && body.is_some_and(Self::body_mutates) {
            self.out.side_effecting_queries.push(Spot {
                span,
                detail: name.to_owned(),
                count: 1,
            });
        }
    }
}

impl<'a> Visit<'a> for Walker<'a> {
    fn visit_method_definition(&mut self, method: &oxc::ast::ast::MethodDefinition<'a>) {
        if let Some(name) = method.key.static_name() {
            self.name_the_body(&name, method.key.span(), method.value.body.as_deref());
        }
        walk::walk_method_definition(self, method);
    }

    fn visit_function(&mut self, function: &Function<'a>, flags: oxc::syntax::scope::ScopeFlags) {
        if let Some(id) = &function.id {
            self.name_the_body(&id.name, id.span, function.body.as_deref());
        }
        if function.body.is_some() {
            for (index, parameter) in function.params.items.iter().enumerate() {
                if let BindingPattern::BindingIdentifier(binding) = &parameter.pattern
                    && parameter.type_annotation.as_ref().is_some_and(|annotation| {
                        matches!(annotation.type_annotation, TSType::TSBooleanKeyword(_))
                    })
                {
                    self.out.flag_parameters.push(Spot {
                        span: binding.span,
                        detail: binding.name.to_string(),
                        count: index as u32 + 1,
                    });
                }
            }
        }
        walk::walk_function(self, function, flags);
    }

    fn visit_catch_clause(&mut self, clause: &CatchClause<'a>) {
        if clause.body.body.is_empty() {
            self.out.empty_catches.push(Spot {
                span: clause.span,
                detail: "catch clause".to_owned(),
                count: 1,
            });
        }
        walk::walk_catch_clause(self, clause);
    }

    fn visit_statements(&mut self, statements: &oxc::allocator::Vec<'a, Statement<'a>>) {
        for (index, statement) in statements.iter().enumerate() {
            let exit = match statement {
                Statement::ReturnStatement(_) => "returns",
                Statement::ThrowStatement(_) => "throws",
                Statement::BreakStatement(_) => "breaks",
                Statement::ContinueStatement(_) => "continues",
                _ => continue,
            };
            // Declarations after a return are hoisted and still reachable.
            if let Some(next) = statements.get(index + 1)
                && !matches!(
                    next,
                    Statement::FunctionDeclaration(_)
                        | Statement::ClassDeclaration(_)
                        | Statement::TSInterfaceDeclaration(_)
                        | Statement::TSTypeAliasDeclaration(_)
                )
            {
                self.out.unreachable.push(Spot {
                    span: next.span(),
                    detail: exit.to_owned(),
                    count: 1,
                });
            }
            break;
        }
        walk::walk_statements(self, statements);
    }

    fn visit_if_statement(&mut self, statement: &IfStatement<'a>) {
        let operands = Self::operands(&statement.test);
        if operands > self.limits.condition_operands {
            self.out.complex_conditions.push(Spot {
                span: statement.test.span(),
                detail: String::new(),
                count: operands,
            });
        }
        if statement.alternate.is_some()
            && matches!(&statement.test, Expression::UnaryExpression(unary)
                if unary.operator == oxc::syntax::operator::UnaryOperator::LogicalNot)
        {
            self.out.negative_conditionals.push(Spot {
                span: statement.test.span(),
                detail: String::new(),
                count: 1,
            });
        }
        walk::walk_if_statement(self, statement);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Some(name) = callee_name(&call.callee) {
            match test_suffix(&name) {
                Some(("skipped", directive)) => self.out.skipped_tests.push(Spot {
                    span: call.span,
                    detail: directive.to_owned(),
                    count: 1,
                }),
                Some(("focused", directive)) => self.out.focused_tests.push(Spot {
                    span: call.span,
                    detail: directive.to_owned(),
                    count: 1,
                }),
                _ => {}
            }

            // Only leaf tests are checked for assertions; a `describe` asserts through
            // the tests inside it.
            if matches!(root_of(&name), "it" | "test")
                && test_suffix(&name).is_none()
                && let Some(body) = call.arguments.get(1)
                && !Self::body_asserts(body)
            {
                let label = call
                    .arguments
                    .first()
                    .and_then(|argument| match argument {
                        Argument::StringLiteral(literal) => Some(literal.value.to_string()),
                        _ => None,
                    })
                    .unwrap_or_else(|| name.clone());
                self.out.assertionless_tests.push(Spot {
                    span: call.span,
                    detail: label,
                    count: 1,
                });
            }
        }

        if let Some(name) = callee_name(&call.callee)
            && self.test_depth > 0
            && delay_call(&name)
            && let Some(pause) = call.arguments.iter().find_map(|argument| match argument {
                Argument::NumericLiteral(literal) => Some(literal.value),
                _ => None,
            })
            && pause >= f64::from(self.limits.slow_test_ms)
        {
            self.out.slow_tests.push(Spot {
                span: call.span,
                detail: name,
                count: pause as u32,
            });
        }

        let depth = Self::chain_depth(&call.callee);
        if depth > self.limits.message_chain_depth {
            self.out.message_chains.push(Spot {
                span: call.span,
                detail: String::new(),
                count: depth,
            });
        }
        let is_test = callee_name(&call.callee)
            .is_some_and(|name| TEST_ROOTS.contains(&root_of(&name).trim_start_matches(['x', 'f'])));
        self.test_depth += u32::from(is_test);
        walk::walk_call_expression(self, call);
        self.test_depth -= u32::from(is_test);
    }

    fn visit_identifier_reference(&mut self, identifier: &oxc::ast::ast::IdentifierReference<'a>) {
        let entry = self.last_read.entry(identifier.name.to_string()).or_default();
        *entry = (*entry).max(identifier.span.end);
        self.first_read
            .entry(identifier.name.to_string())
            .and_modify(|start| *start = (*start).min(identifier.span.start))
            .or_insert(identifier.span.start);
    }

    fn visit_block_statement(&mut self, block: &oxc::ast::ast::BlockStatement<'a>) {
        self.block_depth += 1;
        walk::walk_block_statement(self, block);
        self.block_depth -= 1;
    }

    fn visit_string_literal(&mut self, literal: &oxc::ast::ast::StringLiteral<'a>) {
        if self.block_depth >= self.limits.configuration_depth
            && let Some(kind) = configuration_literal(&literal.value)
        {
            self.out.buried_configuration.push(Spot {
                span: literal.span,
                detail: kind.to_owned(),
                count: self.block_depth,
            });
        }
    }

    fn visit_return_statement(&mut self, statement: &oxc::ast::ast::ReturnStatement<'a>) {
        if let Some(argument) = &statement.argument {
            let operations = Self::operations(argument);
            if operations > self.limits.expression_operations {
                self.out.obscured_expressions.push(Spot {
                    span: statement.span,
                    detail: String::new(),
                    count: operations,
                });
            }
        }
        walk::walk_return_statement(self, statement);
    }

    fn visit_template_literal(&mut self, literal: &oxc::ast::ast::TemplateLiteral<'a>) {
        let text: String = literal.quasis.iter().map(|quasi| quasi.value.raw.as_str()).collect();
        if let Some(language) = foreign_language(&text) {
            self.out.mixed_languages.push(Spot {
                span: literal.span,
                detail: language.to_owned(),
                count: 1,
            });
        }
        walk::walk_template_literal(self, literal);
    }

    fn visit_binary_expression(&mut self, expression: &oxc::ast::ast::BinaryExpression<'a>) {
        use oxc::ast::ast::BinaryOperator;
        if matches!(expression.operator, BinaryOperator::Addition | BinaryOperator::Subtraction)
            && let Expression::NumericLiteral(literal) = &expression.right
            && (literal.value - 1.0).abs() < f64::EPSILON
            && let Some(subject) = callee_name(&expression.left)
        {
            self.boundaries.entry(subject).or_default().push(expression.span);
        }
        walk::walk_binary_expression(self, expression);
    }

    fn visit_binding_identifier(&mut self, binding: &oxc::ast::ast::BindingIdentifier<'a>) {
        if let Some(convention) = convention_of(&binding.name) {
            self.conventions.push((convention.to_owned(), binding.span));
        }
    }

    fn visit_ts_any_keyword(&mut self, keyword: &oxc::ast::ast::TSAnyKeyword) {
        self.out.imprecise_types.push(Spot {
            span: keyword.span,
            detail: "any".to_owned(),
            count: 1,
        });
    }

    fn visit_ts_non_null_expression(
        &mut self,
        expression: &oxc::ast::ast::TSNonNullExpression<'a>,
    ) {
        self.out.imprecise_types.push(Spot {
            span: expression.span,
            detail: "!".to_owned(),
            count: 1,
        });
        oxc::ast_visit::walk::walk_ts_non_null_expression(self, expression);
    }

    /// `as const` narrows rather than widens, so it is the one assertion that tells
    /// the checker more than it already knew.
    fn visit_ts_as_expression(&mut self, expression: &oxc::ast::ast::TSAsExpression<'a>) {
        let is_const = matches!(
            &expression.type_annotation,
            oxc::ast::ast::TSType::TSTypeReference(reference)
                if reference.type_name.get_identifier_reference()
                    .is_some_and(|name| name.name == "const")
        );
        if !is_const {
            self.out.imprecise_types.push(Spot {
                span: expression.span,
                detail: "as".to_owned(),
                count: 1,
            });
        }
        oxc::ast_visit::walk::walk_ts_as_expression(self, expression);
    }

    fn visit_numeric_literal(&mut self, literal: &NumericLiteral<'a>) {
        if !CONVENTIONAL.iter().any(|known| (known - literal.value).abs() < f64::EPSILON) {
            let raw = literal
                .raw
                .as_ref()
                .map_or_else(|| literal.value.to_string(), std::string::ToString::to_string);
            self.numbers.entry(raw).or_default().push(literal.span);
        }
    }

    fn visit_assignment_expression(&mut self, assignment: &oxc::ast::ast::AssignmentExpression<'a>) {
        if let oxc::ast::ast::AssignmentTarget::AssignmentTargetIdentifier(target) = &assignment.left
        {
            self.out.mutated_parameters.push(Spot {
                span: target.span,
                detail: target.name.to_string(),
                count: 1,
            });
        }
        walk::walk_assignment_expression(self, assignment);
    }
}

pub fn collect(
    program: &oxc::ast::ast::Program<'_>,
    semantic: &Semantic<'_>,
    source: &str,
    limits: Limits,
) -> Hygiene {
    let mut walker = Walker {
        limits,
        last_read: HashMap::new(),
        first_read: HashMap::new(),
        out: Hygiene::default(),
        numbers: HashMap::new(),
        boundaries: HashMap::new(),
        conventions: Vec::new(),
        verbs: Vec::new(),
        test_depth: 0,
        block_depth: 0,
        marker: std::marker::PhantomData,
    };
    walker.visit_program(program);

    let mut out = std::mem::take(&mut walker.out);

    // A literal used once is usually obvious from context; the same one written three
    // times is a decision somebody will have to change in three places.
    let mut repeated: Vec<(String, Vec<Span>)> = walker
        .numbers
        .into_iter()
        .filter(|(_, spans)| spans.len() as u32 >= limits.magic_number_repeats)
        .collect();
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
    let camel = walker.conventions.iter().filter(|(style, _)| style == "camelCase").count();
    let snake = walker.conventions.iter().filter(|(style, _)| style == "snake_case").count();
    if camel > 0 && snake > 0 {
        let minority = if camel < snake { "camelCase" } else { "snake_case" };
        if let Some((_, span)) = walker.conventions.iter().find(|(style, _)| style == minority) {
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
    let mut vocabulary: Vec<Spot> = by_object
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
    vocabulary.sort_by_key(|spot| spot.span.start);
    out.inconsistent_vocabulary = vocabulary;

    out.distant_declarations =
        distant_declarations(semantic, source, &walker.first_read, limits.declaration_distance_lines);

    repeated.sort_by(|a, b| a.1[0].start.cmp(&b.1[0].start));
    for (raw, spans) in repeated {
        out.magic_numbers.push(Spot {
            span: spans[0],
            detail: raw,
            count: spans.len() as u32,
        });
    }

    out.unused_locals = unused_locals(semantic);
    out.short_names =
        short_names(semantic, source, &walker.last_read, limits.short_name_scope_lines);
    scan_comments(semantic, source, limits, &mut out);
    // A parameter that is only ever reassigned is a different finding from a local
    // that is, so the list is narrowed to actual parameters here.
    out.mutated_parameters.retain(|spot| is_parameter(semantic, &spot.detail));
    out.mutated_parameters.dedup_by(|a, b| a.detail == b.detail);
    out
}

fn is_parameter(semantic: &Semantic<'_>, name: &str) -> bool {
    let scoping = semantic.scoping();
    scoping.symbol_ids().any(|symbol_id| {
        scoping.symbol_name(symbol_id) == name
            && scoping.symbol_flags(symbol_id).contains(SymbolFlags::FunctionScopedVariable)
    })
}

/// A one-letter name is perfectly readable while its whole life fits on screen, and
/// A binding declared far above the line that first reads it makes a reader hold it
/// in mind across everything in between. Only block-scoped locals count: a module
/// constant read at the bottom of the file is where it belongs.
fn distant_declarations(
    semantic: &Semantic<'_>,
    source: &str,
    first_read: &HashMap<String, u32>,
    distance: u32,
) -> Vec<Spot> {
    let scoping = semantic.scoping();
    let mut spots = Vec::new();
    for symbol_id in scoping.symbol_ids() {
        let name = scoping.symbol_name(symbol_id);
        if name.starts_with('_') {
            continue;
        }
        if !scoping.symbol_flags(symbol_id).contains(SymbolFlags::BlockScopedVariable) {
            continue;
        }
        let declared = scoping.symbol_span(symbol_id);
        let Some(first) = first_read.get(name).copied().filter(|start| *start > declared.end) else {
            continue;
        };
        let lines = source
            .get(declared.end as usize..first as usize)
            .map_or(0, |text| text.lines().count() as u32);
        if lines > distance {
            spots.push(Spot { span: declared, detail: name.to_owned(), count: lines });
        }
    }
    spots.sort_by_key(|spot| spot.span.start);
    spots
}

/// stops being readable the moment it does not — so the scope's size decides this,
/// not the name's length alone.
fn short_names(
    semantic: &Semantic<'_>,
    source: &str,
    last_read: &HashMap<String, u32>,
    scope_lines: u32,
) -> Vec<Spot> {
    let scoping = semantic.scoping();
    let mut spots = Vec::new();
    for symbol_id in scoping.symbol_ids() {
        let name = scoping.symbol_name(symbol_id);
        if name.len() > 2 || name.starts_with('_') {
            continue;
        }
        let flags = scoping.symbol_flags(symbol_id);
        if !flags.contains(SymbolFlags::BlockScopedVariable) {
            continue;
        }
        let declared = scoping.symbol_span(symbol_id);
        let Some(last) = last_read.get(name).copied().filter(|end| *end > declared.start) else {
            continue;
        };
        let lines = source
            .get(declared.start as usize..last as usize)
            .map_or(0, |text| text.lines().count() as u32);
        if lines >= scope_lines {
            spots.push(Spot { span: declared, detail: name.to_owned(), count: lines });
        }
    }
    spots.sort_by_key(|spot| spot.span.start);
    spots
}

fn unused_locals(semantic: &Semantic<'_>) -> Vec<Spot> {
    let scoping = semantic.scoping();
    let mut spots: Vec<Spot> = scoping
        .symbol_ids()
        .filter(|symbol_id| scoping.symbol_is_unused(*symbol_id))
        .filter(|symbol_id| {
            let flags = scoping.symbol_flags(*symbol_id);
            // Block-scoped only: parameters are function-scoped and belong to the rule
            // that understands signatures, and an unused import is its own question.
            flags.contains(SymbolFlags::BlockScopedVariable)
                && !flags.intersects(SymbolFlags::Import | SymbolFlags::TypeImport)
        })
        .filter(|symbol_id| !scoping.symbol_name(*symbol_id).starts_with('_'))
        .map(|symbol_id| Spot {
            span: scoping.symbol_span(symbol_id),
            detail: scoping.symbol_name(symbol_id).to_owned(),
            count: 1,
        })
        .collect();
    spots.sort_by_key(|spot| spot.span.start);
    spots
}

fn scan_comments(semantic: &Semantic<'_>, source: &str, limits: Limits, out: &mut Hygiene) {
    let mut runs: Vec<(Span, u32)> = Vec::new();
    for comment in semantic.comments() {
        let text = comment.span.source_text(source);
        if let Some(directive) = suppression_directive(text) {
            out.suppressions.push(Spot {
                span: comment.span,
                detail: directive.to_owned(),
                count: 1,
            });
        } else if let Some(marker) = journal_marker(text) {
            out.journal_comments.push(Spot {
                span: comment.span,
                detail: marker.to_owned(),
                count: 1,
            });
        } else if let Some(subject) = source
            .get(comment.attached_to as usize..)
            .and_then(|rest| rest.lines().next())
            .and_then(|line| redundant_subject(text, line))
        {
            out.redundant_comments.push(Spot { span: comment.span, detail: subject, count: 1 });
        } else if looks_like_code(text) {
            runs.push((comment.span, text.lines().count() as u32));
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

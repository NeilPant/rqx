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
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub condition_operands: u32,
    pub message_chain_depth: u32,
    pub commented_code_lines: u32,
    pub magic_number_repeats: u32,
    pub short_name_scope_lines: u32,
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

fn suppression_directive(text: &str) -> Option<&'static str> {
    ["@ts-ignore", "@ts-nocheck", "@ts-expect-error", "eslint-disable", "biome-ignore"]
        .into_iter()
        .find(|directive| text.contains(directive))
}

struct Walker<'a> {
    limits: Limits,
    out: Hygiene,
    numbers: HashMap<String, Vec<Span>>,
    /// Furthest point each name is read at. Collected here rather than from the
    /// semantic node store, which is not populated for this build configuration.
    last_read: HashMap<String, u32>,
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

impl<'a> Visit<'a> for Walker<'a> {
    fn visit_function(&mut self, function: &Function<'a>, flags: oxc::syntax::scope::ScopeFlags) {
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

        let depth = Self::chain_depth(&call.callee);
        if depth > self.limits.message_chain_depth {
            self.out.message_chains.push(Spot {
                span: call.span,
                detail: String::new(),
                count: depth,
            });
        }
        walk::walk_call_expression(self, call);
    }

    fn visit_identifier_reference(&mut self, identifier: &oxc::ast::ast::IdentifierReference<'a>) {
        let entry = self.last_read.entry(identifier.name.to_string()).or_default();
        *entry = (*entry).max(identifier.span.end);
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
        out: Hygiene::default(),
        numbers: HashMap::new(),
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

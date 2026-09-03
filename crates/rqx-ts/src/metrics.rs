use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem;

use oxc::ast::AstKind;
use oxc::ast::ast::{
    BindingPattern, CallExpression, FormalParameters, IfStatement, LogicalExpression, Program,
    Statement, VariableDeclarator,
};
use oxc::ast_visit::Visit;
use oxc::ast_visit::walk;
use oxc::span::{GetSpan, Span};
use oxc::syntax::operator::LogicalOperator;

use crate::naming;

#[derive(Debug)]
pub struct FunctionMetrics {
    pub name: Option<String>,
    pub report: Span,
    pub parameters: u32,
    pub statements: u32,
    pub cognitive_complexity: u32,
    pub max_nesting: u32,
}

#[derive(Debug)]
pub struct ClassMetrics {
    pub name: Option<String>,
    pub report: Span,
    pub fields: u32,
    pub methods: u32,
    pub statements: u32,
}

#[derive(Debug)]
pub struct CloneCandidate {
    pub hash: u64,
    pub span: Span,
    pub statements: u32,
    pub nodes: u32,
}

#[derive(Debug)]
pub struct SuspiciousName {
    pub identifier: String,
    pub span: Span,
    pub reason: &'static str,
}

#[derive(Debug, Default)]
pub struct FileMetrics {
    pub functions: Vec<FunctionMetrics>,
    pub classes: Vec<ClassMetrics>,
    pub clones: Vec<CloneCandidate>,
    pub names: Vec<SuspiciousName>,
}

struct FunctionFrame {
    name: Option<String>,
    report: Span,
    parameters: u32,
    statements: u32,
    complexity: u32,
    nesting: u32,
    max_nesting: u32,
}

struct ClassFrame {
    name: Option<String>,
    report: Span,
    fields: u32,
    methods: u32,
    statements: u32,
}

struct BlockFrame {
    hasher: DefaultHasher,
    span: Span,
    content: Option<Span>,
    statements: u32,
    /// Blocks that only assign to fields are constructor boilerplate; they repeat
    /// across every class and "Extract Method" is the wrong advice for them.
    field_assignments: u32,
    nodes: u32,
}

pub struct MetricsWalker<'a, 'src> {
    source: &'src str,
    functions: Vec<FunctionFrame>,
    classes: Vec<ClassFrame>,
    blocks: Vec<BlockFrame>,
    pending_name: Option<(String, Span)>,
    logical_chain: Option<LogicalOperator>,
    out: FileMetrics,
    marker: std::marker::PhantomData<&'a ()>,
}

impl<'a, 'src> MetricsWalker<'a, 'src> {
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            functions: Vec::new(),
            classes: Vec::new(),
            blocks: Vec::new(),
            pending_name: None,
            logical_chain: None,
            out: FileMetrics::default(),
            marker: std::marker::PhantomData,
        }
    }

    pub fn finish(self) -> FileMetrics {
        self.out
    }

    fn increment(&mut self) {
        if let Some(frame) = self.functions.last_mut() {
            frame.complexity += 1;
        }
    }

    fn increment_with_nesting(&mut self) {
        if let Some(frame) = self.functions.last_mut() {
            frame.complexity += 1 + frame.nesting;
        }
    }

    fn push_nesting(&mut self) {
        if let Some(frame) = self.functions.last_mut() {
            frame.nesting += 1;
            frame.max_nesting = frame.max_nesting.max(frame.nesting);
        }
    }

    fn pop_nesting(&mut self) {
        if let Some(frame) = self.functions.last_mut() {
            frame.nesting = frame.nesting.saturating_sub(1);
        }
    }

    fn nested(&mut self, walk_children: impl FnOnce(&mut Self)) {
        self.push_nesting();
        walk_children(self);
        self.pop_nesting();
    }

    fn push_function(&mut self, span: Span, parameters: u32) {
        let (name, report) = match self.pending_name.take() {
            Some((name, span)) => (Some(name), span),
            None => (None, header_span(self.source, span)),
        };
        self.functions.push(FunctionFrame {
            name,
            report,
            parameters,
            statements: 0,
            complexity: 0,
            nesting: 0,
            max_nesting: 0,
        });
    }

    fn pop_function(&mut self) {
        if let Some(frame) = self.functions.pop() {
            self.out.functions.push(FunctionMetrics {
                name: frame.name,
                report: frame.report,
                parameters: frame.parameters,
                statements: frame.statements,
                cognitive_complexity: frame.complexity,
                max_nesting: frame.max_nesting,
            });
        }
    }

    fn count_statement(&mut self, span: Span, is_field_assignment: bool) {
        for frame in &mut self.functions {
            frame.statements += 1;
        }
        for frame in &mut self.classes {
            frame.statements += 1;
        }
        for frame in &mut self.blocks {
            frame.statements += 1;
            frame.field_assignments += u32::from(is_field_assignment);
            frame.content = Some(match frame.content {
                Some(existing) => Span::new(existing.start, span.end.max(existing.end)),
                None => span,
            });
        }
    }

    fn push_block(&mut self, span: Span) {
        self.blocks.push(BlockFrame {
            hasher: DefaultHasher::new(),
            span,
            content: None,
            statements: 0,
            field_assignments: 0,
            nodes: 0,
        });
    }

    fn pop_block(&mut self) {
        if let Some(frame) = self.blocks.pop() {
            if frame.statements > 0 && frame.field_assignments == frame.statements {
                return;
            }
            self.out.clones.push(CloneCandidate {
                hash: frame.hasher.finish(),
                span: frame.content.unwrap_or(frame.span),
                statements: frame.statements,
                nodes: frame.nodes,
            });
        }
    }

    fn feed_hash(&mut self, kind: &AstKind<'a>) {
        if self.blocks.is_empty() {
            return;
        }
        let token = shape_token(kind);
        for frame in &mut self.blocks {
            frame.hasher.write_u64(token);
            frame.nodes += 1;
        }
    }
}

/// Identifiers that name a *value* are normalized away so that two blocks
/// differing only in local variable names still match, but property and method
/// names are hashed, because two blocks calling different methods on the same
/// object are rarely extractable into one function — reporting them as duplicates
/// would make the suggested fix a lie.
fn shape_token(kind: &AstKind<'_>) -> u64 {
    let mut hasher = DefaultHasher::new();
    mem::discriminant(kind).hash(&mut hasher);
    match kind {
        AstKind::BinaryExpression(node) => mem::discriminant(&node.operator).hash(&mut hasher),
        AstKind::LogicalExpression(node) => mem::discriminant(&node.operator).hash(&mut hasher),
        AstKind::UnaryExpression(node) => mem::discriminant(&node.operator).hash(&mut hasher),
        AstKind::UpdateExpression(node) => mem::discriminant(&node.operator).hash(&mut hasher),
        AstKind::AssignmentExpression(node) => mem::discriminant(&node.operator).hash(&mut hasher),
        AstKind::IdentifierName(node) => node.name.as_str().hash(&mut hasher),
        _ => {}
    }
    hasher.finish()
}

/// `this.field = value;` and nothing else.
fn is_field_assignment(statement: &Statement<'_>) -> bool {
    use oxc::ast::ast::{AssignmentTarget, Expression};

    let Statement::ExpressionStatement(expression) = statement else {
        return false;
    };
    let Expression::AssignmentExpression(assignment) = &expression.expression else {
        return false;
    };
    let AssignmentTarget::StaticMemberExpression(member) = &assignment.left else {
        return false;
    };
    matches!(member.object, Expression::ThisExpression(_))
}

fn header_span(source: &str, span: Span) -> Span {
    let text = &source[span.start as usize..span.end as usize];
    let mut end = 0;
    for (offset, character) in text.char_indices() {
        if matches!(character, '(' | '{' | '\n') || offset >= 60 {
            break;
        }
        end = offset + character.len_utf8();
    }
    Span::new(span.start, span.start + end as u32)
}

fn parameter_count(params: &FormalParameters<'_>) -> u32 {
    (params.items.len() + usize::from(params.rest.is_some())) as u32
}

impl<'a> Visit<'a> for MetricsWalker<'a, '_> {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        self.feed_hash(&kind);
        match kind {
            AstKind::Function(node) => {
                if let Some(id) = &node.id {
                    self.pending_name = Some((id.name.to_string(), id.span));
                }
                self.push_function(node.span, parameter_count(&node.params));
            }
            AstKind::ArrowFunctionExpression(node) => {
                self.push_function(node.span, parameter_count(&node.params));
            }
            AstKind::Class(node) => {
                let name = node.id.as_ref().map(|id| id.name.to_string());
                let report = node
                    .id
                    .as_ref()
                    .map_or_else(|| header_span(self.source, node.span), |id| id.span);
                self.classes.push(ClassFrame {
                    name,
                    report,
                    fields: 0,
                    methods: 0,
                    statements: 0,
                });
            }
            AstKind::MethodDefinition(node) => {
                if let Some(frame) = self.classes.last_mut() {
                    frame.methods += 1;
                }
                if let Some(name) = node.key.static_name() {
                    self.pending_name = Some((name.into_owned(), node.key.span()));
                }
            }
            AstKind::PropertyDefinition(node) => {
                if let Some(frame) = self.classes.last_mut() {
                    frame.fields += 1;
                }
                if let Some(name) = node.key.static_name() {
                    self.pending_name = Some((name.into_owned(), node.key.span()));
                }
            }
            AstKind::AccessorProperty(_) => {
                if let Some(frame) = self.classes.last_mut() {
                    frame.fields += 1;
                }
            }
            AstKind::ObjectProperty(node) => {
                if let Some(name) = node.key.static_name() {
                    self.pending_name = Some((name.into_owned(), node.key.span()));
                }
            }
            AstKind::FunctionBody(node) => self.push_block(node.span),
            AstKind::BlockStatement(node) => self.push_block(node.span),
            AstKind::ConditionalExpression(_)
            | AstKind::SwitchStatement(_)
            | AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::CatchClause(_) => {
                self.increment_with_nesting();
                self.push_nesting();
            }
            AstKind::BreakStatement(node) if node.label.is_some() => self.increment(),
            AstKind::ContinueStatement(node) if node.label.is_some() => self.increment(),
            AstKind::BindingIdentifier(node) => {
                if let Some(reason) = naming::classify(&node.name) {
                    self.out.names.push(SuspiciousName {
                        identifier: node.name.to_string(),
                        span: node.span,
                        reason,
                    });
                }
            }
            _ => {}
        }
    }

    fn leave_node(&mut self, kind: AstKind<'a>) {
        match kind {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => self.pop_function(),
            AstKind::Class(_) => {
                if let Some(frame) = self.classes.pop() {
                    self.out.classes.push(ClassMetrics {
                        name: frame.name,
                        report: frame.report,
                        fields: frame.fields,
                        methods: frame.methods,
                        statements: frame.statements,
                    });
                }
            }
            AstKind::FunctionBody(_) | AstKind::BlockStatement(_) => self.pop_block(),
            AstKind::ConditionalExpression(_)
            | AstKind::SwitchStatement(_)
            | AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::CatchClause(_) => self.pop_nesting(),
            AstKind::VariableDeclarator(_) => self.pending_name = None,
            _ => {}
        }
    }

    fn visit_statement(&mut self, it: &Statement<'a>) {
        self.count_statement(it.span(), is_field_assignment(it));
        let outer_chain = self.logical_chain.take();
        walk::walk_statement(self, it);
        self.logical_chain = outer_chain;
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(id) = &it.id {
            self.pending_name = Some((id.name.to_string(), id.span));
        }
        walk::walk_variable_declarator(self, it);
    }

    /// `else` and `else if` are counted without a nesting increment, and an
    /// `else if` chain is flattened rather than treated as a nested `if`, per the
    /// SonarSource cognitive-complexity specification this metric cites.
    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        let kind = AstKind::IfStatement(self.alloc(it));
        self.enter_node(kind);
        self.increment_with_nesting();
        self.visit_expression(&it.test);
        self.nested(|walker| walker.visit_statement(&it.consequent));

        let mut alternate = it.alternate.as_ref();
        while let Some(statement) = alternate {
            match statement {
                Statement::IfStatement(inner) => {
                    self.count_statement(inner.span, false);
                    self.increment();
                    self.visit_expression(&inner.test);
                    self.nested(|walker| walker.visit_statement(&inner.consequent));
                    alternate = inner.alternate.as_ref();
                }
                other => {
                    self.increment();
                    self.nested(|walker| walker.visit_statement(other));
                    alternate = None;
                }
            }
        }
        self.leave_node(kind);
    }

    /// A run of the same logical operator is one decision for a reader, so it
    /// scores once; a change of operator starts a new run.
    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        let kind = AstKind::LogicalExpression(self.alloc(it));
        self.enter_node(kind);
        if self.logical_chain != Some(it.operator) {
            self.increment();
        }
        let outer_chain = self.logical_chain.replace(it.operator);
        self.visit_expression(&it.left);
        self.visit_expression(&it.right);
        self.logical_chain = outer_chain;
        self.leave_node(kind);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let outer_chain = self.logical_chain.take();
        walk::walk_call_expression(self, it);
        self.logical_chain = outer_chain;
    }
}

pub fn measure(source: &str, program: &Program<'_>) -> FileMetrics {
    let mut walker = MetricsWalker::new(source);
    walker.visit_program(program);
    walker.finish()
}

#[cfg(test)]
mod tests {
    use oxc::allocator::Allocator;
    use oxc::parser::Parser;
    use oxc::span::SourceType;

    use super::*;

    fn measure_source(source: &str) -> FileMetrics {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "fixture failed to parse: {source}");
        measure(source, &parsed.program)
    }

    fn only_function(source: &str) -> FunctionMetrics {
        let mut metrics = measure_source(source);
        assert_eq!(metrics.functions.len(), 1, "expected exactly one function");
        metrics.functions.pop().unwrap()
    }

    #[test]
    fn trivial_function_scores_zero() {
        let f = only_function("function add(a: number, b: number) { return a + b; }");
        assert_eq!(f.cognitive_complexity, 0);
        assert_eq!(f.max_nesting, 0);
        assert_eq!(f.parameters, 2);
        assert_eq!(f.statements, 1);
        assert_eq!(f.name.as_deref(), Some("add"));
    }

    /// The worked example from the SonarSource cognitive-complexity specification:
    /// `if` +1, nested `for` +2, its nested `if` +3, `else` +1, `catch` +1 -> 8.
    #[test]
    fn matches_sonarsource_worked_example() {
        let f = only_function(
            r#"
            function toRegexp(antPattern: string, dirSep: string) {
              if (antPattern !== null) {
                for (const token of antPattern) {
                  if (token === "**") {
                    escaping = true;
                  } else {
                    escaping = false;
                  }
                }
              }
              try {
                run();
              } catch (error) {
                report(error);
              }
            }
            "#,
        );
        assert_eq!(f.cognitive_complexity, 8, "got {f:?}");
        assert_eq!(f.max_nesting, 3);
    }

    #[test]
    fn else_if_chains_flatten_instead_of_nesting() {
        let flattened = only_function(
            "function f(a: number) { if (a === 1) { x(); } else if (a === 2) { y(); } else { z(); } }",
        );
        assert_eq!(flattened.cognitive_complexity, 3, "if +1, else-if +1, else +1");
        assert_eq!(flattened.max_nesting, 1, "an else-if chain is not deeper nesting");

        let genuinely_nested = only_function(
            "function f(a: number) { if (a === 1) { if (a === 2) { if (a === 3) { y(); } } } }",
        );
        assert_eq!(genuinely_nested.cognitive_complexity, 6, "1 + 2 + 3");
        assert_eq!(genuinely_nested.max_nesting, 3);
    }

    #[test]
    fn a_run_of_one_operator_scores_once() {
        let single = only_function("function f(a, b, c) { return a && b && c; }");
        assert_eq!(single.cognitive_complexity, 1);

        let mixed = only_function("function f(a, b, c) { return a && b || c; }");
        assert_eq!(mixed.cognitive_complexity, 2);
    }

    #[test]
    fn nested_functions_are_measured_separately() {
        let metrics = measure_source(
            "function outer() { items.forEach((item) => { if (item) { use(item); } }); }",
        );
        let outer = metrics.functions.iter().find(|f| f.name.as_deref() == Some("outer")).unwrap();
        let inner = metrics.functions.iter().find(|f| f.name.is_none()).unwrap();
        assert_eq!(outer.cognitive_complexity, 0, "the callback's `if` is not the outer's");
        assert_eq!(inner.cognitive_complexity, 1);
    }

    #[test]
    fn arrow_assigned_to_a_binding_takes_its_name() {
        let f = only_function("const reconcile = (a, b, c, d, e) => { return a; };");
        assert_eq!(f.name.as_deref(), Some("reconcile"));
        assert_eq!(f.parameters, 5);
    }

    #[test]
    fn a_pending_name_does_not_leak_to_a_later_anonymous_function() {
        let metrics = measure_source("const count = 5; run(() => { work(); });");
        let f = metrics.functions.first().unwrap();
        assert_eq!(f.name, None, "the arrow inside run() is not named `count`");
    }

    #[test]
    fn destructured_and_rest_parameters_count_as_one_each() {
        let f = only_function("function f({ a, b, c }: Options, ...rest: number[]) { return a; }");
        assert_eq!(f.parameters, 2);
    }

    #[test]
    fn this_parameter_is_not_a_parameter() {
        let f = only_function("function f(this: Window, a: number) { return a; }");
        assert_eq!(f.parameters, 1);
    }

    #[test]
    fn class_members_are_split_into_fields_and_methods() {
        let mut metrics = measure_source(
            r#"
            class Ledger {
              private balance = 0;
              private currency = "USD";
              constructor() {}
              post() {}
              get total() { return this.balance; }
            }
            "#,
        );
        let class = metrics.classes.pop().unwrap();
        assert_eq!(class.name.as_deref(), Some("Ledger"));
        assert_eq!(class.fields, 2);
        assert_eq!(class.methods, 3, "constructor, post, and the getter");
    }

    #[test]
    fn a_block_of_only_field_assignments_is_not_reported_as_a_clone() {
        let metrics = measure_source(
            r#"
            class A { constructor(a, b, c) { this.a = a; this.b = b; this.c = c; } }
            class B { constructor(a, b, c) { this.a = a; this.b = b; this.c = c; } }
            "#,
        );
        assert!(
            metrics.clones.is_empty(),
            "constructor assignment blocks are boilerplate, not extractable logic: {:?}",
            metrics.clones
        );
    }

    #[test]
    fn a_block_that_assigns_fields_and_does_work_is_still_a_clone() {
        let metrics = measure_source(
            r#"
            class A { constructor(a) { this.a = a; validate(a); register(this); } }
            class B { constructor(a) { this.a = a; validate(a); register(this); } }
            "#,
        );
        let hashes: Vec<u64> = metrics.clones.iter().map(|c| c.hash).collect();
        assert_eq!(hashes.len(), 2);
        assert_eq!(hashes[0], hashes[1]);
    }

    #[test]
    fn identical_blocks_differing_only_in_local_names_share_a_hash() {
        let metrics = measure_source(
            r#"
            function first(input: number[]) { const total = sum(input); log(total); return total; }
            function second(values: number[]) { const result = sum(values); log(result); return result; }
            "#,
        );
        let hashes: Vec<u64> = metrics.clones.iter().map(|c| c.hash).collect();
        assert_eq!(hashes.len(), 2);
        assert_eq!(hashes[0], hashes[1], "local renaming must not break the match");
    }

    #[test]
    fn blocks_calling_different_methods_do_not_share_a_hash() {
        let metrics = measure_source(
            r#"
            function first(repo: Repo) { const row = repo.save(1); log(row); return row; }
            function second(repo: Repo) { const row = repo.delete(1); log(row); return row; }
            "#,
        );
        assert_ne!(
            metrics.clones[0].hash, metrics.clones[1].hash,
            "save() and delete() are not extractable into one function"
        );
    }

    #[test]
    fn blocks_differing_only_in_an_operator_do_not_share_a_hash() {
        let metrics = measure_source(
            r#"
            function first(a: number, b: number) { const c = a + b; log(c); return c; }
            function second(a: number, b: number) { const c = a - b; log(c); return c; }
            "#,
        );
        assert_ne!(metrics.clones[0].hash, metrics.clones[1].hash);
    }

    #[test]
    fn suspicious_names_are_collected_with_their_reason() {
        let metrics = measure_source("function f() { const data = load(); const temp2 = data; }");
        let names: Vec<&str> = metrics.names.iter().map(|n| n.identifier.as_str()).collect();
        assert_eq!(names, ["data", "temp2"]);
    }
}

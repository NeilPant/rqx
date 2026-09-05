use std::hash::{DefaultHasher, Hash, Hasher};

use rqx_core::facts::{ClassMetrics, CloneCandidate, FileMetrics, FunctionMetrics, SuspiciousName};
use rqx_core::naming;
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{find_within, is_binding_position, operator_of, span_of, text_of};
use crate::vocabulary::MEMO_HINTS;

struct FunctionFrame {
    name: Option<String>,
    report: Span,
    extent: Span,
    parameters: u32,
    statements: u32,
    complexity: u32,
    nesting: u32,
    max_nesting: u32,
    loops: u32,
    max_loops: u32,
    self_calls: u32,
    memo_guard: bool,
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
    /// A second hash over a vocabulary every language shares, so the same logic
    /// ported between two of them still matches. `hasher` cannot: it reads
    /// `kind_id`, which is a number a grammar assigns and no two grammars agree on.
    shape: DefaultHasher,
    /// The literals the block holds, order-independent. Ported code keeps its
    /// constants even when its syntax changes completely.
    literals: u64,
    span: Span,
    content: Option<Span>,
    statements: u32,
    /// Blocks that only assign to fields are constructor boilerplate; they repeat
    /// across every class and "Extract Method" is the wrong advice for them.
    field_assignments: u32,
    nodes: u32,
}

pub struct MetricsWalker<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    functions: Vec<FunctionFrame>,
    classes: Vec<ClassFrame>,
    blocks: Vec<BlockFrame>,
    pending_name: Option<(String, Span)>,
    logical_chain: Option<String>,
    out: FileMetrics,
}

impl<'a> MetricsWalker<'a> {
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

    fn nested(&mut self, node: Node<'a>, field: Option<&'static str>) {
        self.push_nesting();
        self.walk(node, field);
        self.pop_nesting();
    }

    fn push_function(&mut self, node: Node<'a>) {
        let (name, report) = match self.pending_name.take() {
            Some((name, span)) => (Some(name), span),
            None => (None, header_span(self.source, span_of(node))),
        };
        // A callback handed to `forEach` runs once per element, so its body is already
        // inside every loop its caller sits in.
        let inherited = if self.profile.is_loop_callback(node, self.source) {
            self.functions.last().map_or(0, |frame| frame.loops) + 1
        } else {
            0
        };
        self.functions.push(FunctionFrame {
            name,
            report,
            extent: span_of(node),
            parameters: self.parameter_count(node),
            statements: 0,
            complexity: 0,
            nesting: 0,
            max_nesting: 0,
            loops: inherited,
            max_loops: inherited,
            self_calls: 0,
            memo_guard: false,
        });
    }

    fn pop_function(&mut self) {
        if let Some(frame) = self.functions.pop() {
            if frame.loops > 0
                && let Some(caller) = self.functions.last_mut()
            {
                caller.max_loops = caller.max_loops.max(frame.max_loops);
            }
            self.out.functions.push(FunctionMetrics {
                name: frame.name,
                report: frame.report,
                extent: frame.extent,
                parameters: frame.parameters,
                statements: frame.statements,
                cognitive_complexity: frame.complexity,
                max_nesting: frame.max_nesting,
                loop_depth: frame.max_loops,
                self_calls: frame.self_calls,
                memo_guard: frame.memo_guard,
            });
        }
    }

    fn push_loop(&mut self) {
        if let Some(frame) = self.functions.last_mut() {
            frame.loops += 1;
            frame.max_loops = frame.max_loops.max(frame.loops);
        }
    }

    fn pop_loop(&mut self) {
        if let Some(frame) = self.functions.last_mut() {
            frame.loops = frame.loops.saturating_sub(1);
        }
    }

    /// A call to the function it is written in. Recursion through a receiver other
    /// than the object itself is a different call, and is not counted.
    fn note_self_call(&mut self, node: Node<'a>) {
        let Some(frame) = self.functions.last() else {
            return;
        };
        let Some(name) = frame.name.as_deref() else {
            return;
        };
        if self.profile.call_tail(node, self.source) != Some(name) {
            return;
        }
        let own = node
            .child_by_field_name(self.profile.callee_field)
            .is_some_and(|callee| {
                self.profile.is_identifier(callee)
                    || callee
                        .child_by_field_name(self.profile.object_field)
                        .is_some_and(|object| {
                            self.profile
                                .self_names
                                .contains(&text_of(object, self.source))
                        })
            });
        if own && let Some(frame) = self.functions.last_mut() {
            frame.self_calls += 1;
        }
    }

    /// A receiver in first position — `this`, `self`, `cls` — is not an argument
    /// anybody passes, so counting it would make every method one parameter longer
    /// than the code a reader sees.
    fn parameter_count(&self, node: Node<'a>) -> u32 {
        let Some(list) = self.parameter_list(node) else {
            return 0;
        };
        let mut cursor = list.walk();
        let parameters: Vec<Node<'a>> = list
            .named_children(&mut cursor)
            .filter(|child| self.profile.parameter_kinds.contains(&child.kind()))
            .collect();
        let receivers = parameters
            .first()
            .filter(|first| {
                self.profile
                    .self_names
                    .contains(&self.parameter_name(**first))
            })
            .map_or(0, |_| 1);
        (parameters.len() - receivers) as u32
    }

    fn parameter_list(&self, node: Node<'a>) -> Option<Node<'a>> {
        find_within(
            node,
            self.profile.parameter_list_kinds,
            self.profile.block_kinds,
        )
        .into_iter()
        .next_back()
    }

    /// Some grammars hang the name off a declarator rather than off the definition,
    /// so a lookup that only reads the `name` field finds nothing there.
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

    fn parameter_name(&self, parameter: Node<'a>) -> &'a str {
        if self.profile.is_identifier(parameter) {
            return text_of(parameter, self.source);
        }
        crate::tree::descendants(parameter)
            .into_iter()
            .find(|child| self.profile.is_identifier(*child))
            .map_or("", |child| text_of(child, self.source))
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
            shape: DefaultHasher::new(),
            literals: 0,
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
                shape: frame.shape.finish(),
                literals: frame.literals,
                span: frame.content.unwrap_or(frame.span),
                statements: frame.statements,
                nodes: frame.nodes,
            });
        }
    }

    fn feed_hash(&mut self, node: Node<'a>, field: Option<&'static str>) {
        if self.blocks.is_empty() {
            return;
        }
        let token = self.shape_token(node, field);
        let neutral = self.neutral_token(node);
        // Combined by xor so the literal set does not depend on the order two
        // languages happen to write the same constants in.
        let literal = self.literal_token(node);
        for frame in &mut self.blocks {
            frame.hasher.write_u64(token);
            if let Some(neutral) = neutral {
                frame.shape.write_u64(neutral);
            }
            frame.literals ^= literal;
            frame.nodes += 1;
        }
    }

    /// One token from a vocabulary every language shares. A node that is not part of
    /// the shape — punctuation, a wrapper only one grammar has — contributes nothing,
    /// which is what lets two syntaxes for the same logic agree.
    fn neutral_token(&self, node: Node<'a>) -> Option<u64> {
        let profile = self.profile;
        let kind = node.kind();
        let shape: u64 = if kind == profile.if_kind || profile.branch_kinds.contains(&kind) {
            1
        } else if profile.loop_kinds.contains(&kind) {
            2
        } else if profile.catch_kinds.contains(&kind) {
            3
        } else if profile.is_call(node) {
            4
        } else if profile.exit_kinds.iter().any(|(name, _)| *name == kind) {
            5
        } else if profile.assignment_kinds.contains(&kind) {
            6
        } else if profile.is_block(node) {
            7
        } else if profile.number_kinds.contains(&kind) || profile.string_kinds.contains(&kind) {
            8
        } else if profile.binary_kinds.contains(&kind) {
            // The operator itself, canonicalised: `&&` and `and` are one thing.
            let operator = operator_of(node, self.source).unwrap_or_default();
            return Some(match operator {
                "&&" | "and" => 100,
                "||" | "or" => 101,
                "==" | "===" | "is" => 102,
                "!=" | "!==" => 103,
                "<" => 104,
                ">" => 105,
                "<=" => 106,
                ">=" => 107,
                "+" => 108,
                "-" => 109,
                "*" => 110,
                "/" => 111,
                _ => 112,
            });
        } else {
            return None;
        };
        Some(shape)
    }

    fn literal_token(&self, node: Node<'a>) -> u64 {
        let profile = self.profile;
        if !profile.number_kinds.contains(&node.kind())
            && !profile.string_kinds.contains(&node.kind())
        {
            return 0;
        }
        let mut hasher = DefaultHasher::new();
        text_of(node, self.source)
            .trim_matches(['"', '\'', '`'])
            .hash(&mut hasher);
        hasher.finish()
    }

    /// Identifiers that name a *value* are normalized away so that two blocks
    /// differing only in local variable names still match, but property and method
    /// names are hashed, because two blocks calling different methods on the same
    /// object are rarely extractable into one function — reporting them as duplicates
    /// would make the suggested fix a lie.
    fn shape_token(&self, node: Node<'a>, field: Option<&'static str>) -> u64 {
        let mut hasher = DefaultHasher::new();
        node.kind_id().hash(&mut hasher);
        if self.profile.binary_kinds.contains(&node.kind())
            && let Some(operator) = operator_of(node, self.source)
        {
            operator.hash(&mut hasher);
        }
        // The name of a member is hashed by the field it fills rather than by its
        // node kind: in a language where a property name is a plain identifier,
        // hashing every identifier would defeat the normalisation this depends on.
        if field == Some(self.profile.property_field) {
            text_of(node, self.source).hash(&mut hasher);
        }
        hasher.finish()
    }

    /// `this.field = value;` and nothing else.
    fn is_field_assignment(&self, statement: Node<'a>) -> bool {
        let mut cursor = statement.walk();
        let Some(assignment) = statement
            .named_children(&mut cursor)
            .find(|child| child.kind() == "assignment_expression")
        else {
            return false;
        };
        let Some(target) = assignment.child_by_field_name("left") else {
            return false;
        };
        if !self.profile.is_member_access(target) {
            return false;
        }
        target.child_by_field_name("object").is_some_and(|object| {
            self.profile
                .self_names
                .contains(&text_of(object, self.source))
        })
    }

    fn note_name(&mut self, node: Node<'a>) {
        let identifier = text_of(node, self.source);
        if let Some(reason) = naming::classify(identifier) {
            self.out.names.push(SuspiciousName {
                identifier: identifier.to_owned(),
                span: span_of(node),
                reason,
            });
        }
    }

    fn is_binding(&self, node: Node<'a>) -> bool {
        self.profile.is_identifier(node)
            && is_binding_position(
                node,
                self.profile.parameter_kinds,
                self.profile.borrowed_name_kinds,
                self.profile.local_declaring_kinds,
                self.profile.expression_list_kinds,
            )
    }

    fn walk_children(&mut self, node: Node<'a>) {
        let mut cursor = node.walk();
        if !cursor.goto_first_child() {
            return;
        }
        loop {
            let child = cursor.node();
            if child.is_named() {
                let field = cursor.field_name();
                self.walk(child, field);
            }
            if !cursor.goto_next_sibling() {
                return;
            }
        }
    }

    /// `else` and `else if` are counted without a nesting increment, and an
    /// `else if` chain is flattened rather than treated as a nested `if`, per the
    /// SonarSource cognitive-complexity specification this metric cites.
    fn walk_if(&mut self, node: Node<'a>) {
        self.increment_with_nesting();
        if let Some(condition) = node.child_by_field_name(self.profile.condition_field) {
            self.walk(condition, Some(self.profile.condition_field));
        }
        if let Some(consequence) = node.child_by_field_name("consequence") {
            self.nested(consequence, Some("consequence"));
        }

        if self.profile.else_is_sibling_list {
            let mut cursor = node.walk();
            let clauses: Vec<Node<'a>> = node
                .named_children(&mut cursor)
                .filter(|child| self.profile.else_clause_kinds.contains(&child.kind()))
                .collect();
            for clause in clauses {
                self.increment();
                if let Some(condition) = clause.child_by_field_name(self.profile.condition_field) {
                    self.walk(condition, Some(self.profile.condition_field));
                }
                let mut inner = clause.walk();
                let bodies: Vec<Node<'a>> = clause
                    .named_children(&mut inner)
                    .filter(|child| self.profile.is_block(*child))
                    .collect();
                for body in bodies {
                    self.nested(body, None);
                }
            }
            return;
        }

        let mut alternative = node.child_by_field_name(self.profile.else_field);
        while let Some(branch) = alternative {
            // Some grammars wrap the tail in an `else_clause`; the branch itself is
            // the single child that matters.
            let branch = if branch.kind() == self.profile.if_kind || branch.named_child_count() == 0
            {
                branch
            } else {
                branch.named_child(0).unwrap_or(branch)
            };
            if branch.kind() == self.profile.if_kind {
                self.count_statement(span_of(branch), false);
                self.increment();
                if let Some(condition) = branch.child_by_field_name(self.profile.condition_field) {
                    self.walk(condition, Some(self.profile.condition_field));
                }
                if let Some(consequence) = branch.child_by_field_name("consequence") {
                    self.nested(consequence, Some("consequence"));
                }
                alternative = branch.child_by_field_name(self.profile.else_field);
            } else {
                self.increment();
                self.nested(branch, None);
                alternative = None;
            }
        }
    }

    /// A run of the same logical operator is one decision for a reader, so it
    /// scores once; a change of operator starts a new run.
    fn walk_binary(&mut self, node: Node<'a>) -> bool {
        let Some(operator) = operator_of(node, self.source) else {
            return false;
        };
        if !self.profile.logical_operators.contains(&operator) {
            return false;
        }
        if self.logical_chain.as_deref() != Some(operator) {
            self.increment();
        }
        let outer = self.logical_chain.replace(operator.to_owned());
        self.walk_children(node);
        self.logical_chain = outer;
        true
    }

    /// A function's own body is not a statement inside the function; a bare block
    /// written inside one is. Grammars give both the same node kind, so the parent
    /// is what separates them.
    fn counts_as_statement(&self, node: Node<'a>) -> bool {
        if self.profile.is_statement(node) {
            return true;
        }
        self.profile.is_block(node)
            && node.parent().is_some_and(|parent| {
                !self.profile.is_function(parent)
                    && !self.profile.is_class(parent)
                    && !self
                        .profile
                        .structural_block_parents
                        .contains(&parent.kind())
            })
    }

    fn walk(&mut self, node: Node<'a>, field: Option<&'static str>) {
        self.feed_hash(node, field);
        let kind = node.kind();
        let profile = self.profile;

        if self.counts_as_statement(node) {
            self.count_statement(span_of(node), self.is_field_assignment(node));
            let outer = self.logical_chain.take();
            self.walk_inner(node);
            self.logical_chain = outer;
            return;
        }
        if profile.is_call(node) {
            self.note_self_call(node);
            let outer = self.logical_chain.take();
            self.walk_inner(node);
            self.logical_chain = outer;
            return;
        }
        if kind == profile.if_kind {
            self.walk_if(node);
            return;
        }
        self.walk_inner(node);
    }

    /// The half of `walk` that runs after statement and call bookkeeping, so those
    /// two can wrap it without repeating everything below.
    fn walk_inner(&mut self, node: Node<'a>) {
        let kind = node.kind();
        let profile = self.profile;

        if kind == profile.if_kind {
            self.walk_if(node);
            return;
        }
        if profile.binary_kinds.contains(&kind) && self.walk_binary(node) {
            return;
        }
        if profile.declarator_kinds.contains(&kind) {
            if let Some(name) = node.child_by_field_name("name")
                && profile.is_identifier(name)
            {
                self.pending_name = Some((text_of(name, self.source).to_owned(), span_of(name)));
            }
            self.walk_children(node);
            self.pending_name = None;
            return;
        }
        if profile.method_kinds.contains(&kind) {
            if let Some(frame) = self.classes.last_mut() {
                frame.methods += 1;
            }
            if let Some(name) = node.child_by_field_name("name") {
                self.pending_name = Some((text_of(name, self.source).to_owned(), span_of(name)));
            }
        }
        let assigned_field = profile.assigned_field_kinds.contains(&kind)
            && node.parent().is_some_and(|parent| {
                parent
                    .parent()
                    .is_some_and(|grandparent| profile.is_class(grandparent))
            });
        if profile.field_kinds.contains(&kind) || assigned_field {
            if let Some(frame) = self.classes.last_mut() {
                frame.fields += 1;
            }
            if let Some(name) = node.child_by_field_name("name") {
                self.pending_name = Some((text_of(name, self.source).to_owned(), span_of(name)));
            }
        }
        if profile.is_function(node) {
            if let Some(name) = self.function_name(node) {
                self.pending_name = Some((text_of(name, self.source).to_owned(), span_of(name)));
            }
            self.push_function(node);
            self.walk_children(node);
            self.pop_function();
            return;
        }
        if profile.is_class(node) {
            let named = node.child_by_field_name("name");
            self.classes.push(ClassFrame {
                name: named.map(|id| text_of(id, self.source).to_owned()),
                report: named.map_or_else(|| header_span(self.source, span_of(node)), span_of),
                fields: 0,
                methods: 0,
                statements: 0,
            });
            self.walk_children(node);
            if let Some(frame) = self.classes.pop() {
                self.out.classes.push(ClassMetrics {
                    name: frame.name,
                    report: frame.report,
                    fields: frame.fields,
                    methods: frame.methods,
                    statements: frame.statements,
                });
            }
            return;
        }
        if profile.is_block(node) {
            self.push_block(span_of(node));
            self.walk_children(node);
            self.pop_block();
            return;
        }
        if profile.comprehension_kinds.contains(&kind) {
            let clauses = clause_count(node).max(1);
            for _ in 0..clauses {
                self.push_loop();
            }
            self.walk_children(node);
            for _ in 0..clauses {
                self.pop_loop();
            }
            return;
        }
        if profile.branch_kinds.contains(&kind) {
            let repeats = profile.loop_kinds.contains(&kind);
            self.increment_with_nesting();
            self.push_nesting();
            if repeats {
                self.push_loop();
            }
            self.walk_children(node);
            if repeats {
                self.pop_loop();
            }
            self.pop_nesting();
            return;
        }
        if profile.flat_branch_kinds.contains(&kind) {
            self.increment();
        }
        if self.is_binding(node) {
            self.note_name(node);
        }
        if profile.is_identifier(node)
            && let Some(frame) = self.functions.last()
            && !frame.memo_guard
            && crate::vocabulary::mentions_any(text_of(node, self.source), &MEMO_HINTS)
            && let Some(frame) = self.functions.last_mut()
        {
            frame.memo_guard = true;
        }
        self.walk_children(node);
    }
}

/// A comprehension loops once per `for` clause it holds, so `[a for x in xs for y in ys]`
/// is two loops however it is spelled.
fn clause_count(node: Node<'_>) -> u32 {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() == "for_in_clause")
        .count() as u32
}

fn header_span(source: &str, span: Span) -> Span {
    let text = span.text(source);
    let mut end = 0;
    for (offset, character) in text.char_indices() {
        if matches!(character, '(' | '{' | '\n') || offset >= 60 {
            break;
        }
        end = offset + character.len_utf8();
    }
    Span::new(span.start, span.start + end as u32)
}

pub fn measure(profile: &'static LanguageProfile, source: &str, root: Node<'_>) -> FileMetrics {
    let mut walker = MetricsWalker {
        profile,
        source,
        functions: Vec::new(),
        classes: Vec::new(),
        blocks: Vec::new(),
        pending_name: None,
        logical_chain: None,
        out: FileMetrics::default(),
    };
    walker.walk(root, None);
    walker.out
}

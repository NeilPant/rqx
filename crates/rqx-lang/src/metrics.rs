use std::hash::{DefaultHasher, Hash, Hasher};

use rqx_core::Language;
use rqx_core::facts::{ClassMetrics, CloneCandidate, FileMetrics, FunctionMetrics, SuspiciousName};
use rqx_core::naming;
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{
    field_name_under, find_within, is_binding_position, kind_of, operator_of, span_of, text_of,
};
use crate::vocabulary::MEMO_HINTS;

struct FunctionFrame {
    name: Option<String>,
    report: Span,
    extent: Span,
    parameters: u32,
    /// Whether the last parameter takes any number of arguments.
    variadic: bool,
    /// The names the parameters bind. One that repeats the function's own name hides
    /// the function inside its body.
    parameter_names: Vec<String>,
    statements: u32,
    complexity: u32,
    /// The same decisions, counted flat. Cognitive complexity weighs a nested
    /// branch more heavily; this one asks only how many paths there are.
    decisions: u32,
    nesting: u32,
    max_nesting: u32,
    loops: u32,
    max_loops: u32,
    self_calls: u32,
    certain_self_calls: u32,
    memo_guard: bool,
    recursion: Recursion,
}

/// How a function names itself when it recurses. A method that calls a function of
/// the same name without its receiver is calling that other function: cobra's
/// `(c *Command) MarkFlagRequired` calls the package's `MarkFlagRequired`.
enum Recursion {
    /// A plain function, or a Java or C++ method, which reach their own members
    /// without a receiver: an unqualified call is the function itself.
    Unqualified,
    /// A method in a language that has to name its receiver: `self.f()` in Python,
    /// `this.f()` in TypeScript, and `c.f()` in Go, whose receiver is named here.
    ThroughReceiver(Option<String>),
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
    pending_name: Option<PendingName>,
    logical_chain: Option<String>,
    out: FileMetrics,
}

/// The name a declaration, field or method is about to give the function written as
/// its value, and the node that gives it.
struct PendingName {
    name: String,
    span: Span,
    owner: usize,
}

impl<'a> MetricsWalker<'a> {
    fn increment(&mut self) {
        if let Some(frame) = self.functions.last_mut() {
            frame.complexity += 1;
            frame.decisions += 1;
        }
    }

    fn increment_with_nesting(&mut self) {
        if let Some(frame) = self.functions.last_mut() {
            frame.complexity += 1 + frame.nesting;
            frame.decisions += 1;
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

    fn name_next_function(&mut self, owner: Node<'a>, name: Node<'a>) {
        self.pending_name = Some(PendingName {
            name: text_of(name, self.source).to_owned(),
            span: span_of(name),
            owner: owner.id(),
        });
    }

    /// Whether `owner` names `function`: it is the function itself, or the function
    /// is its value. `const unsubscribe = bus.subscribe(() => …)` names the result of
    /// the call, not the callback handed to it.
    fn is_named_by(&self, function: Node<'a>, owner: usize) -> bool {
        if function.id() == owner {
            return true;
        }
        let profile = self.profile;
        let mut parent = function.parent();
        while let Some(wrapper) = parent.filter(|node| {
            profile.transparent_kinds.contains(&kind_of(*node))
                || profile.expression_list_kinds.contains(&kind_of(*node))
        }) {
            parent = wrapper.parent();
        }
        parent.is_some_and(|parent| parent.id() == owner)
    }

    fn push_function(&mut self, node: Node<'a>) {
        let pending = self
            .pending_name
            .take()
            .filter(|pending| self.is_named_by(node, pending.owner));
        let (name, report) = match pending {
            Some(pending) => (Some(pending.name), pending.span),
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
            variadic: self.parameter_list(node).is_some_and(|list| {
                list.named_children(&mut list.walk())
                    .any(|parameter| kind_of(parameter).starts_with("spread"))
            }),
            parameter_names: self.parameter_names(node),
            statements: 0,
            complexity: 0,
            decisions: 0,
            nesting: 0,
            max_nesting: 0,
            loops: inherited,
            max_loops: inherited,
            self_calls: 0,
            certain_self_calls: 0,
            memo_guard: false,
            recursion: self.recursion_of(node),
        });
    }

    fn recursion_of(&self, function: Node<'a>) -> Recursion {
        let profile = self.profile;
        if matches!(profile.language, Language::Java | Language::Cpp) {
            return Recursion::Unqualified;
        }
        let receiver = function.child_by_field_name("receiver");
        let in_a_class = function
            .parent()
            .filter(|parent| kind_of(*parent) == "decorated_definition")
            .or(Some(function))
            .and_then(|member| member.parent())
            .is_some_and(|body| {
                profile.is_class(body) || body.parent().is_some_and(|class| profile.is_class(class))
            });
        let is_method = receiver.is_some()
            || (profile.method_kinds.contains(&kind_of(function))
                && (in_a_class || kind_of(function) != "function_definition"));
        if !is_method {
            return Recursion::Unqualified;
        }
        // Python's receiver is whatever the first parameter is called, and a method
        // nested in another sees the outer one's `self` under that name.
        // A static method has no receiver at all.
        if profile.language == Language::Python {
            let decorated = function
                .parent()
                .filter(|parent| kind_of(*parent) == "decorated_definition");
            let is_static = decorated.is_some_and(|definition| {
                definition
                    .named_children(&mut definition.walk())
                    .any(|child| text_of(child, self.source).trim() == "@staticmethod")
            });
            let receiver = (!is_static)
                .then(|| self.parameter_names(function).into_iter().next())
                .flatten();
            return Recursion::ThroughReceiver(receiver);
        }
        let named = receiver.and_then(|list| {
            crate::tree::descendants(list)
                .into_iter()
                .find(|node| {
                    profile.is_identifier(*node)
                        && node
                            .parent()
                            .is_some_and(|parent| field_name_under(parent, *node) == Some("name"))
                })
                .map(|name| text_of(name, self.source).to_owned())
        });
        Recursion::ThroughReceiver(named)
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
                cyclomatic_complexity: frame.decisions + 1,
                max_nesting: frame.max_nesting,
                loop_depth: frame.max_loops,
                self_calls: frame.self_calls,
                certain_self_calls: frame.certain_self_calls,
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
        // The receiver decides it, not the shape of the callee: a grammar that keeps
        // the method name in a field of its own leaves an identifier there whether or
        // not the call has an object in front of it.
        let receiver = node
            .child_by_field_name(self.profile.object_field)
            .or_else(|| {
                node.child_by_field_name(self.profile.callee_field)
                    .filter(|callee| self.profile.is_member_access(*callee))
                    .and_then(|callee| callee.child_by_field_name(self.profile.object_field))
            });
        let Some(frame) = self.functions.last() else {
            return;
        };
        let own = match (receiver, &frame.recursion) {
            // A qualified C++ call, `::name()`, `Base::name()` or `detail::name()`,
            // names another function, and a parameter of the same name hides this one.
            (None, Recursion::Unqualified) => {
                !self.callee_text(node).contains("::")
                    && !frame.parameter_names.contains(&name.to_owned())
                    && !self.rebound_inside(node, name)
            }
            (None, Recursion::ThroughReceiver(_)) => false,
            // A Python function that is not a method has no receiver: `self.f()` in a
            // helper nested inside a method reaches the method's object.
            (Some(_), Recursion::Unqualified) if self.profile.language == Language::Python => false,
            (Some(object), Recursion::ThroughReceiver(Some(first)))
                if self.profile.language == Language::Python =>
            {
                text_of(object, self.source) == first
            }
            (Some(object), recursion) => {
                let object = text_of(object, self.source);
                self.profile.self_names.contains(&object)
                    || matches!(recursion, Recursion::ThroughReceiver(Some(name)) if name == object)
            }
        };
        // Java picks an overload by its arguments, so `size(items)` inside `size()` is
        // another method.
        let overload = self.profile.language == Language::Java
            && self
                .argument_count(node)
                .is_some_and(|arguments| arguments != frame.parameters && !frame.variadic);
        let certain = !matches!(self.profile.language, Language::Java | Language::Cpp)
            || self.passes_parameters_on(node, &frame.parameter_names);
        if own
            && !overload
            && let Some(frame) = self.functions.last_mut()
        {
            frame.self_calls += 1;
            if certain {
                frame.certain_self_calls += 1;
            }
        }
    }

    /// Whether the function a call is written in gives the called name a new meaning:
    /// a nested function of that name, or an assignment to it, as Python's
    /// `global pager; pager = get_pager()` does before calling `pager`.
    fn rebound_inside(&self, call: Node<'a>, name: &str) -> bool {
        let profile = self.profile;
        let mut enclosing = call.parent();
        while let Some(node) = enclosing.filter(|node| !profile.is_function(*node)) {
            enclosing = node.parent();
        }
        let Some(function) = enclosing else {
            return false;
        };
        crate::tree::descendants(function)
            .into_iter()
            .filter(|inner| inner.id() != function.id())
            .any(|inner| {
                let names = |named: Node<'a>| text_of(named, self.source) == name;
                if profile.is_function(inner) {
                    return self.function_name(inner).is_some_and(names);
                }
                (profile.assignment_kinds.contains(&kind_of(inner))
                    || profile.local_declaring_kinds.contains(&kind_of(inner)))
                    && inner
                        .child_by_field_name("left")
                        .is_some_and(|target| profile.is_identifier(target) && names(target))
            })
    }

    /// Whether a call's arguments are the function's own parameters, in order and
    /// unchanged: `f(a, b)` inside `f(a, b)`. Only then does a language that picks an
    /// overload by argument types certainly pick this one again; `f(String(s))` or
    /// `f((Long) v)` asks for another.
    fn passes_parameters_on(&self, call: Node<'a>, parameters: &[String]) -> bool {
        let mut cursor = call.walk();
        let Some(list) = call
            .named_children(&mut cursor)
            .find(|child| self.profile.argument_list_kinds.contains(&kind_of(child)))
        else {
            return false;
        };
        let arguments: Vec<&str> = list
            .named_children(&mut list.walk())
            .map(|argument| text_of(argument, self.source).trim())
            .collect();
        arguments.len() == parameters.len()
            && arguments
                .iter()
                .zip(parameters)
                .all(|(argument, parameter)| argument == parameter)
    }

    fn callee_text(&self, call: Node<'a>) -> &'a str {
        call.child_by_field_name(self.profile.callee_field)
            .map_or("", |callee| text_of(callee, self.source))
    }

    fn argument_count(&self, call: Node<'a>) -> Option<u32> {
        let mut cursor = call.walk();
        call.named_children(&mut cursor)
            .find(|child| self.profile.argument_list_kinds.contains(&kind_of(child)))
            .map(|list| list.named_child_count() as u32)
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
            .filter(|child| self.profile.parameter_kinds.contains(&kind_of(child)))
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
        crate::parameters::input_list(self.profile, node)
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
                || self.profile.property_kinds.contains(&kind_of(child))
        })
    }

    fn parameter_names(&self, node: Node<'a>) -> Vec<String> {
        let Some(list) = self.parameter_list(node) else {
            return Vec::new();
        };
        let mut cursor = list.walk();
        list.named_children(&mut cursor)
            .filter(|child| self.profile.parameter_kinds.contains(&kind_of(child)))
            .map(|parameter| self.parameter_name(parameter).to_owned())
            .collect()
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
        let kind = kind_of(node);
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
        if !profile.number_kinds.contains(&kind_of(node))
            && !profile.string_kinds.contains(&kind_of(node))
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
        if self.profile.binary_kinds.contains(&kind_of(node))
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
            .find(|child| kind_of(child) == "assignment_expression")
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
                .filter(|child| self.profile.else_clause_kinds.contains(&kind_of(child)))
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
            let branch =
                if kind_of(branch) == self.profile.if_kind || branch.named_child_count() == 0 {
                    branch
                } else {
                    branch.named_child(0).unwrap_or(branch)
                };
            if kind_of(branch) == self.profile.if_kind {
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
                        .contains(&kind_of(parent))
            })
    }

    fn walk(&mut self, node: Node<'a>, field: Option<&'static str>) {
        self.feed_hash(node, field);
        let kind = kind_of(node);
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
        let kind = kind_of(node);
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
                self.name_next_function(node, name);
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
                self.name_next_function(node, name);
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
                self.name_next_function(node, name);
            }
        }
        if profile.is_function(node) {
            if let Some(name) = self.function_name(node) {
                self.name_next_function(node, name);
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
        .filter(|child| kind_of(child) == "for_in_clause")
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

//! How many times a loop runs, as one of: a fixed number, as many as a collection or a
//! count holds, logarithmically many, or not known. Early exits are ignored: a loop
//! that may stop early is still bounded by what it could have run through.

use rqx_core::cost::{Bound, SizeSrc};
use tree_sitter::Node;

use super::env::{Env, SizeRef};
use super::types::{declarations, declares, is_fixed_literal, is_integer};
use crate::names::Names;
use crate::tree::{descendants, kind_of, operands_of, operator_of};

/// A loop that counts: the node holding its header, and the fields of the header.
pub struct CountedLoop {
    pub kind: &'static str,
    pub init: &'static str,
    pub condition: &'static str,
    pub update: &'static str,
}

/// How a language writes the loops the cost analysis has to size. Node kinds and field
/// names only: what a loop's header means is read the same way in every language.
pub struct LoopVocabulary {
    /// Loops that run once per element, as the node holding the collection and the
    /// field it is in. The node is the loop itself, or a header the loop holds.
    pub each: &'static [(&'static str, &'static str)],
    pub counted: &'static [CountedLoop],
    /// Loops that run while a condition holds, as the node and its condition's field.
    /// An empty field means the first part of the node that is not its body.
    pub conditional: &'static [(&'static str, &'static str)],
    /// The `for` clauses of a comprehension, each a loop over the field named.
    pub clauses: &'static [(&'static str, &'static str)],
    /// Nodes that step a counter on their own: `i++`.
    pub steps: &'static [&'static str],
    /// Members that hold a collection's length: `xs.length`.
    pub size_members: &'static [&'static str],
    /// Calls that answer a collection's length. On a receiver it is the receiver's
    /// length, bare it is the first argument's.
    pub size_calls: &'static [&'static str],
    /// Calls that count out the integers up to their last argument.
    pub range_calls: &'static [&'static str],
    /// Methods whose answer is no larger than their receiver, so a loop over the answer
    /// is a loop over the receiver: `filter`, `keys`, `begin`.
    pub derived_methods: &'static [&'static str],
    /// The same for functions that take the collection as their first argument.
    pub derived_functions: &'static [&'static str],
    /// Calls that add one element to the collection they are made on, or bare to their
    /// first argument.
    pub appenders: &'static [&'static str],
}

/// How a loop's counter moves between passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// By a fixed amount: `i++`, `i += 2`.
    Linear,
    /// By a factor: `i *= 2`, `n /= 2`, `n >>= 1`.
    Geometric,
    /// Reassigned from another variable, as the two ends of a binary search are.
    Converging,
    Unknown,
}

/// Calls that hand back the number they were given, rounded or converted.
const PASS_THROUGH: [&str; 12] = [
    "int",
    "float",
    "abs",
    "Math.floor",
    "Math.ceil",
    "Math.trunc",
    "Math.abs",
    "parseInt",
    "Number",
    "Integer.valueOf",
    "Math.floorDiv",
    "len",
];

/// A name written in capitals is a constant somebody named.
fn is_constant_name(name: &str) -> bool {
    name.chars().any(|c| c.is_ascii_uppercase()) && !name.chars().any(|c| c.is_ascii_lowercase())
}

pub struct Bounds<'e, 'a> {
    names: Names<'a>,
    env: &'e mut Env,
}

impl<'e, 'a> Bounds<'e, 'a> {
    pub fn new(names: Names<'a>, env: &'e mut Env) -> Self {
        Self { names, env }
    }

    pub const fn names(&self) -> Names<'a> {
        self.names
    }

    pub fn env(&mut self) -> &mut Env {
        self.env
    }

    fn vocabulary(&self) -> &'static LoopVocabulary {
        &self.names.profile.loops
    }

    /// The bound of a loop statement, or `None` when the node is not one.
    pub fn of_loop(&mut self, node: Node<'a>) -> Option<Bound> {
        let vocabulary = self.vocabulary();
        for (kind, field) in vocabulary.each {
            if let Some(header) = self.header(node, kind)
                && let Some(iterable) = header.child_by_field_name(field)
            {
                return Some(self.iterable_bound(iterable));
            }
        }
        for spec in vocabulary.counted {
            if let Some(header) = self.header(node, spec.kind) {
                return Some(self.counted_bound(header, spec));
            }
        }
        for (kind, field) in vocabulary.conditional {
            if kind_of(node) != *kind {
                continue;
            }
            let body = node.child_by_field_name("body");
            let condition = if field.is_empty() {
                self.first_part_but(node, body)
            } else {
                node.child_by_field_name(field)
            };
            return Some(condition.map_or(Bound::Unknown, |condition| {
                self.conditional_bound(condition, body)
            }));
        }
        None
    }

    /// The loop node itself, or the header child of that kind it holds.
    fn header(&self, node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        if kind_of(node) == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .find(|child| kind_of(child) == kind)
    }

    fn first_part_but(&self, node: Node<'a>, body: Option<Node<'a>>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .find(|child| Some(*child) != body && !self.is_header(*child))
    }

    fn is_header(&self, node: Node<'a>) -> bool {
        let vocabulary = self.vocabulary();
        let kind = kind_of(node);
        vocabulary.each.iter().any(|(each, _)| *each == kind)
            || vocabulary.counted.iter().any(|spec| spec.kind == kind)
    }

    /// The bound of a loop over `iterable`, as a comprehension clause, a callback
    /// handed to `map` or a `for` over it all are.
    pub fn iterable_bound(&mut self, iterable: Node<'a>) -> Bound {
        let range = self.range_bound(iterable);
        if let Some(bound) = range {
            return bound;
        }
        match self.collection(iterable) {
            Some(SizeRef::Const) => Bound::Const,
            Some(SizeRef::Var(var)) => Bound::Size(var),
            None => Bound::Unknown,
        }
    }

    /// `range(n)`, which counts rather than holds.
    fn range_bound(&mut self, iterable: Node<'a>) -> Option<Bound> {
        let node = self.names.see_through(iterable);
        if !self.names.profile.is_call(node) {
            return None;
        }
        let called = self.names.called_name(node)?;
        let tail = called.rsplit('.').next().unwrap_or(&called);
        if !self.vocabulary().range_calls.contains(&tail) {
            return None;
        }
        let arguments = self.names.arguments(node);
        let last = if arguments.len() > 1 {
            arguments.get(1)
        } else {
            arguments.first()
        };
        Some(match last.and_then(|argument| self.count(*argument)) {
            Some(SizeRef::Const) => Bound::Const,
            Some(SizeRef::Var(var)) => Bound::Size(var),
            None => Bound::Unknown,
        })
    }

    /// How large a collection is. A name is as large as the collection it holds, and
    /// an answer derived from another is as large as that one.
    pub fn collection(&mut self, expr: Node<'a>) -> Option<SizeRef> {
        let node = self.names.see_through(expr);
        if is_fixed_literal(self.names.profile, node) {
            return Some(SizeRef::Const);
        }
        if let Some(size) = self.derived(node) {
            return size;
        }
        self.named_collection(node)
    }

    /// The size of an expression that is derived from another collection, with the
    /// outer `Option` saying whether it was one at all.
    fn derived(&mut self, node: Node<'a>) -> Option<Option<SizeRef>> {
        if !self.names.profile.is_call(node) {
            return None;
        }
        let called = self.names.called_name(node)?;
        let tail = called.rsplit('.').next().unwrap_or(&called);
        let vocabulary = self.vocabulary();
        let source = if vocabulary
            .derived_functions
            .iter()
            .any(|function| self.names.answers_to(&called, function))
        {
            self.names.arguments(node).into_iter().next()
        } else if vocabulary.derived_methods.contains(&tail) {
            self.receiver_expression(node)
        } else {
            None
        }?;
        Some(self.collection(source))
    }

    /// The expression a method call is made on, which a grammar holds in the callee or
    /// in a field of the call.
    pub fn receiver_expression(&self, call: Node<'a>) -> Option<Node<'a>> {
        let profile = self.names.profile;
        call.child_by_field_name(profile.object_field).or_else(|| {
            call.child_by_field_name(profile.callee_field)
                .filter(|callee| profile.is_member_access(*callee))
                .and_then(|callee| callee.child_by_field_name(profile.object_field))
        })
    }

    fn named_collection(&mut self, node: Node<'a>) -> Option<SizeRef> {
        let profile = self.names.profile;
        let key = if profile.is_identifier(node) {
            self.names.text(node).to_owned()
        } else if profile.is_member_access(node) {
            self.names.callee_name(node)?
        } else {
            return None;
        };
        if let Some(size) = self.env.binding(&key).and_then(|binding| binding.size) {
            return Some(size);
        }
        if is_constant_name(&key) {
            return Some(SizeRef::Const);
        }
        Some(SizeRef::Var(self.variable(&key)))
    }

    /// The size variable for a name, from wherever the name came from.
    pub fn variable(&mut self, key: &str) -> u16 {
        let parameter = self.env.binding(key).and_then(|binding| binding.parameter);
        let source = if let Some(position) = parameter {
            SizeSrc::Param(position)
        } else if self.reaches_through_a_name(key) {
            SizeSrc::Field
        } else if self.env.binding(key).is_some() {
            SizeSrc::Local
        } else {
            SizeSrc::Global
        };
        self.env.var(key, source)
    }

    /// `this.items` and `order.items` are read through an object the function holds,
    /// where a bare `items` is a name of its own.
    fn reaches_through_a_name(&self, key: &str) -> bool {
        let Some((head, _)) = key.split_once('.') else {
            return false;
        };
        self.names.profile.self_names.contains(&head)
            || self.env.receiver() == Some(head)
            || self.env.binding(head).is_some()
    }

    /// The length of what a `.length`, `.size()` or `len(x)` is asked of.
    fn measured(&mut self, node: Node<'a>) -> Option<SizeRef> {
        let profile = self.names.profile;
        let vocabulary = self.vocabulary();
        let subject = if profile.is_call(node) {
            let called = self.names.called_name(node)?;
            let tail = called.rsplit('.').next().unwrap_or(&called);
            if !vocabulary.size_calls.contains(&tail) {
                return None;
            }
            self.receiver_expression(node)
                .or_else(|| self.names.arguments(node).into_iter().next())
        } else if profile.is_member_access(node) {
            let property = self.names.member_property(node)?;
            vocabulary
                .size_members
                .contains(&self.names.text(property))
                .then(|| node.child_by_field_name(profile.object_field))
                .flatten()
        } else {
            None
        }?;
        self.collection(subject)
    }

    /// The value of an expression that is a count: a literal, a constant, a measured
    /// length, a count passed in, or one of those moved by a fixed amount.
    pub fn count(&mut self, expr: Node<'a>) -> Option<SizeRef> {
        let profile = self.names.profile;
        let node = self.names.see_through(expr);
        let kind = kind_of(node);
        if profile.number_kinds.contains(&kind) {
            return Some(SizeRef::Const);
        }
        if profile.is_identifier(node) {
            return self.named_count(self.names.text(node));
        }
        if let Some(size) = self.measured(node) {
            return Some(size);
        }
        if profile.binary_kinds.contains(&kind)
            && let Some((left, right)) = operands_of(node)
        {
            let (left, right) = (self.count(left), self.count(right));
            return match (left, right) {
                (Some(SizeRef::Const), other) | (other, Some(SizeRef::Const)) => other,
                _ => None,
            };
        }
        if profile.is_call(node)
            && let Some(called) = self.names.called_name(node)
            && PASS_THROUGH.contains(&called.as_str())
            && let [only] = self.names.arguments(node).as_slice()
        {
            return self.count(*only);
        }
        if profile.is_member_access(node) {
            return self
                .names
                .callee_name(node)
                .and_then(|key| self.named_count(&key));
        }
        None
    }

    fn named_count(&mut self, name: &str) -> Option<SizeRef> {
        let binding = self.env.binding(name);
        if let Some(size) = binding.and_then(|binding| binding.size) {
            return Some(size);
        }
        if let Some(position) = binding.and_then(|binding| binding.parameter) {
            let counts = binding
                .and_then(|binding| binding.fact.as_ref())
                .is_none_or(|fact| is_integer(self.names.profile.language, &fact.head));
            return counts.then(|| SizeRef::Var(self.env.var(name, SizeSrc::Param(position))));
        }
        is_constant_name(name).then_some(SizeRef::Const)
    }

    fn counted_bound(&mut self, header: Node<'a>, spec: &CountedLoop) -> Bound {
        let init = header.child_by_field_name(spec.init);
        let condition = header.child_by_field_name(spec.condition);
        let update = header.child_by_field_name(spec.update);
        let step = update.map_or(Step::Unknown, |update| self.step_of(update, None));
        let start = init.and_then(|init| self.declared_start(init));
        let counter = init
            .and_then(|init| self.declared_name(init))
            .map(str::to_owned);
        let Some((left, right)) = condition.and_then(|node| self.comparison(node)) else {
            return Bound::Unknown;
        };
        let operator = condition
            .map(|node| self.names.see_through(node))
            .and_then(|node| operator_of(node, self.names.source))
            .unwrap_or("<");
        let counter_on_left = counter
            .as_deref()
            .is_none_or(|name| self.names.text(left) == name);
        let (limit, upward) = if counter_on_left {
            (right, matches!(operator, "<" | "<=" | "!="))
        } else {
            (left, matches!(operator, ">" | ">=" | "!="))
        };
        let limit = self.count(limit);
        let primary = if upward {
            limit
        } else {
            prefer_variable(start, limit)
        };
        self.shaped(primary, step == Step::Geometric)
    }

    fn shaped(&self, size: Option<SizeRef>, geometric: bool) -> Bound {
        match size {
            Some(SizeRef::Const) => Bound::Const,
            Some(SizeRef::Var(var)) if geometric => Bound::Log(var),
            Some(SizeRef::Var(var)) => Bound::Size(var),
            None => Bound::Unknown,
        }
    }

    /// The first comparison in a condition, whose two sides are what a loop is
    /// measured by.
    fn comparison(&self, condition: Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
        let node = self.names.see_through(condition);
        if !self.names.profile.binary_kinds.contains(&kind_of(node)) {
            return None;
        }
        let (left, right) = operands_of(node)?;
        Some((self.names.see_through(left), self.names.see_through(right)))
    }

    /// The node in a loop's header that declares its counter.
    fn declaring_node(&self, init: Node<'a>) -> Option<Node<'a>> {
        let profile = self.names.profile;
        std::iter::once(init)
            .chain(descendants(init))
            .find(|node| declares(profile, *node) && !declarations(&self.names, *node).is_empty())
    }

    fn declared_name(&self, init: Node<'a>) -> Option<&'a str> {
        let node = self.declaring_node(init)?;
        declarations(&self.names, node)
            .into_iter()
            .next()
            .map(|declaration| declaration.name)
    }

    fn declared_start(&mut self, init: Node<'a>) -> Option<SizeRef> {
        let node = self.declaring_node(init)?;
        let value = declarations(&self.names, node).into_iter().next()?.value?;
        self.count(value)
    }

    /// How a statement that moves a counter moves it. `name` narrows it to the writes
    /// of one variable, which a loop with no header has to look for in its body.
    fn step_of(&self, update: Node<'a>, only: Option<&str>) -> Step {
        let text = self.names.text(update);
        let target = self
            .update_target(update)
            .map(|target| self.names.text(target));
        if let Some(wanted) = only
            && target != Some(wanted)
        {
            return Step::Unknown;
        }
        if ["*=", "/=", ">>=", "<<=", "//="]
            .iter()
            .any(|op| text.contains(op))
        {
            return Step::Geometric;
        }
        if ["+=", "-=", "++", "--"].iter().any(|op| text.contains(op)) {
            return Step::Linear;
        }
        let Some((_, value)) = text.split_once('=') else {
            return Step::Unknown;
        };
        let own = target.unwrap_or_default();
        let from_another = value
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| !word.is_empty() && !word.chars().all(|c| c.is_ascii_digit()))
            .any(|word| word != own);
        if from_another {
            return Step::Converging;
        }
        if value.contains(['+', '-']) {
            Step::Linear
        } else if value.contains(['*', '/', '>', '<']) {
            Step::Geometric
        } else {
            Step::Unknown
        }
    }

    /// What a step statement writes to: the left of an assignment, or the operand of
    /// an increment.
    fn update_target(&self, update: Node<'a>) -> Option<Node<'a>> {
        let target = update
            .child_by_field_name("left")
            .or_else(|| update.child_by_field_name("argument"))
            .or_else(|| update.named_child(0))?;
        let target = self.names.see_through(target);
        if self
            .names
            .profile
            .expression_list_kinds
            .contains(&kind_of(target))
        {
            return target.named_child(0);
        }
        Some(target)
    }

    /// The first step the body takes on `name`.
    fn body_step(&self, body: Node<'a>, name: &str) -> Step {
        let profile = self.names.profile;
        let steps = self.vocabulary().steps;
        descendants(body)
            .into_iter()
            .filter(|node| {
                let kind = kind_of(node);
                profile.assignment_kinds.contains(&kind) || steps.contains(&kind)
            })
            .map(|node| self.step_of(node, Some(name)))
            .find(|step| *step != Step::Unknown)
            .unwrap_or(Step::Unknown)
    }

    fn conditional_bound(&mut self, condition: Node<'a>, body: Option<Node<'a>>) -> Bound {
        let Some((left, right)) = self.comparison(condition) else {
            return Bound::Unknown;
        };
        let Some(body) = body else {
            return Bound::Unknown;
        };
        let operator =
            operator_of(self.names.see_through(condition), self.names.source).unwrap_or("<");
        let profile = self.names.profile;
        let names = self.names;
        let name_of = |node: Node<'a>| profile.is_identifier(node).then(|| names.text(node));
        let (left_name, right_name) = (name_of(left), name_of(right));

        if let (Some(low), Some(high)) = (left_name, right_name)
            && self.body_step(body, low) == Step::Converging
            && self.body_step(body, high) == Step::Converging
        {
            let size = self.count(right).or_else(|| self.count(left));
            return match size {
                Some(SizeRef::Var(var)) => Bound::Log(var),
                _ => Bound::Log(self.env.var(high, SizeSrc::Local)),
            };
        }

        for (variable, other, name, variable_is_left) in [
            (left, right, left_name, true),
            (right, left, right_name, false),
        ] {
            let Some(name) = name else { continue };
            let step = self.body_step(body, name);
            if step == Step::Unknown {
                continue;
            }
            let upward = if variable_is_left {
                matches!(operator, "<" | "<=" | "!=")
            } else {
                matches!(operator, ">" | ">=" | "!=")
            };
            let limit = self.count(other);
            let size = if upward {
                limit
            } else {
                prefer_variable(self.count(variable), limit)
            };
            return self.shaped(size, step == Step::Geometric);
        }
        Bound::Unknown
    }
}

/// The first of two sizes that grows, or else whichever one is known.
fn prefer_variable(first: Option<SizeRef>, second: Option<SizeRef>) -> Option<SizeRef> {
    [first, second]
        .into_iter()
        .flatten()
        .find(|size| matches!(size, SizeRef::Var(_)))
        .or(first)
        .or(second)
}

#[cfg(test)]
mod tests;

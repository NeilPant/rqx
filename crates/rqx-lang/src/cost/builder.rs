//! Reads one function body into its cost: one visit, producing the loop tree the time
//! and space come from, the calls the function makes, and the signals beside them.
//! Subtrees that cost nothing are not written down.

use std::collections::{BTreeSet, HashSet};

use rqx_core::config::Performance;
use rqx_core::cost::{
    ArgSize, Bound, CallSite, CalleeRef, Conf, Cost, FunctionCost, Indicators, Param, Recurrence,
    SizeSrc,
};
use tree_sitter::Node;

use super::bindings::{bind_assignment, bind_declaration, bind_parameters};
use super::bounds::Bounds;
use super::env::{Env, SizeRef};
use super::file::{FileFacts, Site, identity};
use super::indicators::independent_awaits;
use super::recurrence::{Reduction, combined, reduction};
use super::remap::{Remap, referenced as referenced_by, referenced_by_calls};
use super::shape::{Parts, seq};
use super::tables::{CallFacts, CostTables, Entry, SizeOf};
use super::types::{KEYED_GUESS, TypeFact, constructed_fact, declares, names_a_keyed_value};
use crate::names::Names;
use crate::tree::{descendants, kind_of, operator_of};
use crate::vocabulary::{IO_BARE, MEMO_HINTS, asks_for_data, mentions_any, reaches_outside};

/// What a function is read against: the library tables, the project's own names for
/// clients, and what the file around it says.
pub struct Shared<'f> {
    pub tables: &'f CostTables,
    pub performance: &'f Performance,
    pub file: &'f FileFacts,
}

pub struct Built<'a> {
    /// `None` for a function that is plainly constant time and calls nothing.
    pub cost: Option<FunctionCost>,
    /// Functions written inside this one that are not callbacks of a loop.
    pub nested: Vec<Site<'a>>,
}

/// What one loop's passes allocate, so the innermost loop can say how much each pass
/// does.
#[derive(Default)]
struct LoopFrame {
    allocations: u32,
    holds_a_loop: bool,
}

/// Collections that are filled by writing a key into them.
const KEYED_HEADS: [&str; 8] = [
    "dict",
    "object",
    "map",
    "unordered_map",
    "Map",
    "HashMap",
    "TreeMap",
    "LinkedHashMap",
];

struct Visitor<'a, 'f> {
    names: Names<'a>,
    shared: &'f Shared<'f>,
    class: Option<String>,
    name: Option<&'a str>,
    env: Env,
    params: Vec<Param>,
    calls: Vec<CallSite>,
    depth: u8,
    frames: Vec<LoopFrame>,
    /// The bound of each loop the visit is inside, outermost first.
    loop_bounds: Vec<Bound>,
    nested: Vec<Site<'a>>,
    nested_names: HashSet<String>,
    reductions: Vec<Reduction>,
    is_async: bool,
    conf: Conf,
    allocations_per_pass: u32,
    blocking_async: bool,
    locks_in_loop: bool,
    unbounded_fanout: bool,
    static_growth: BTreeSet<String>,
}

pub fn build<'a>(names: Names<'a>, shared: &Shared<'_>, site: &Site<'a>) -> Built<'a> {
    let function = site.node;
    let mut visitor = Visitor {
        names,
        shared,
        class: site.class.clone(),
        name: identity(&names, function),
        env: Env::with_fields(shared.file.fields_of(site.class.as_deref())),
        params: Vec::new(),
        calls: Vec::new(),
        depth: 0,
        frames: Vec::new(),
        loop_bounds: Vec::new(),
        nested: Vec::new(),
        nested_names: HashSet::new(),
        reductions: Vec::new(),
        is_async: names.profile.is_async(function),
        conf: Conf::Verified,
        allocations_per_pass: 0,
        blocking_async: false,
        locks_in_loop: false,
        unbounded_fanout: false,
        static_growth: BTreeSet::new(),
    };
    visitor.params = bind_parameters(&mut visitor.bounds(), function);
    let inherited = u8::from(names.profile.is_loop_callback(function, names.source));
    visitor.depth = inherited;

    let body = names.function_body(function);
    let parts = body.map_or_else(Parts::default, |body| visitor.visit(body));
    let cost = visitor.finish(function, body, parts);
    Built {
        cost,
        nested: visitor.nested,
    }
}

fn tail_of(dotted: &str) -> &str {
    dotted.rsplit('.').next().unwrap_or(dotted)
}

fn line_of(node: Node<'_>) -> u32 {
    node.start_position().row as u32 + 1
}

impl<'a> Visitor<'a, '_> {
    fn bounds(&mut self) -> Bounds<'_, 'a> {
        Bounds::new(self.names, &mut self.env)
    }

    fn weaken(&mut self, to: Conf) {
        self.conf = self.conf.weaker(to);
    }

    fn note_bound(&mut self, bound: Bound) {
        if bound == Bound::Unknown {
            self.weaken(Conf::Heuristic);
        }
    }

    fn enter_loop(&mut self, bound: Bound) {
        self.loop_bounds.push(bound);
        self.depth = self.depth.saturating_add(1);
        if let Some(outer) = self.frames.last_mut() {
            outer.holds_a_loop = true;
        }
        self.frames.push(LoopFrame::default());
    }

    fn exit_loop(&mut self) {
        self.loop_bounds.pop();
        self.depth = self.depth.saturating_sub(1);
        if let Some(frame) = self.frames.pop()
            && !frame.holds_a_loop
        {
            self.allocations_per_pass = self.allocations_per_pass.max(frame.allocations);
        }
    }

    /// The size of a call's argument, for the callee to be costed in the caller's terms.
    fn arg_size(&mut self, argument: Node<'a>) -> ArgSize {
        let size = {
            let mut bounds = self.bounds();
            bounds
                .count(argument)
                .or_else(|| bounds.collection(argument))
        };
        self.arg_of(size)
    }

    fn arg_of(&mut self, size: Option<SizeRef>) -> ArgSize {
        match size {
            Some(SizeRef::Const) => ArgSize::Const,
            Some(SizeRef::Var(var)) => match self.env.vars()[usize::from(var)].src {
                SizeSrc::Param(position) => ArgSize::Param(position),
                _ => ArgSize::Local(var),
            },
            None => ArgSize::Unknown,
        }
    }

    fn visit_children(&mut self, node: Node<'a>) -> Parts {
        let mut parts = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            parts.push(self.visit(child));
        }
        Parts::sequence(parts)
    }

    fn visit(&mut self, node: Node<'a>) -> Parts {
        let profile = self.names.profile;
        let kind = kind_of(node);

        if profile.is_function(node) {
            self.defer_function(node);
            return Parts::default();
        }
        if profile.is_class(node) {
            return Parts::default();
        }
        if profile.signals.allocating.contains(&kind)
            && let Some(frame) = self.frames.last_mut()
        {
            frame.allocations += 1;
        }
        if self.depth > 0 {
            self.note_statement_in_loop(node, kind);
        }
        if profile.comprehension_kinds.contains(&kind) {
            return self.comprehension(node);
        }
        if profile.loop_kinds.contains(&kind) {
            return self.looping(node);
        }
        if profile.is_call(node) {
            return self.call(node);
        }
        if kind == profile.if_kind
            || profile.switch_kinds.contains(&kind)
            || profile.ternary_kinds.contains(&kind)
        {
            let mut cursor = node.walk();
            let children: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
            let parts = children
                .into_iter()
                .map(|child| self.visit(child))
                .collect();
            return Parts::alternatives(parts);
        }

        let parts = self.visit_children(node);
        // What a name stands for is known only once the value it is set from has been read.
        if declares(profile, node) {
            bind_declaration(&mut self.bounds(), node);
        } else if profile.assignment_kinds.contains(&kind) {
            bind_assignment(&mut self.bounds(), node);
        }
        if profile.assignment_kinds.contains(&kind) {
            return self.with_growth_by_key(node, parts);
        }
        if profile.language == rqx_core::Language::Python && kind == "comparison_operator" {
            return self.with_membership(node, parts);
        }
        parts
    }

    /// A function written inside this one is read on its own, later. Its name is known
    /// from here on, so a call to it below is a call to a function of this file.
    fn defer_function(&mut self, function: Node<'a>) {
        if let Some(name) = identity(&self.names, function) {
            self.nested_names.insert(name.to_owned());
        }
        self.nested.push(Site {
            node: function,
            class: self.class.clone(),
        });
    }

    fn note_statement_in_loop(&mut self, node: Node<'a>, kind: &str) {
        let signals = &self.names.profile.signals;
        if signals.lock_kinds.contains(&kind) {
            self.locks_in_loop = true;
        }
        if signals.spawn_kinds.contains(&kind) {
            self.unbounded_fanout = true;
        }
        if !signals.lock_types.is_empty()
            && let Some(declared) = node.child_by_field_name("type")
            && signals
                .lock_types
                .iter()
                .any(|lock| self.names.text(declared).contains(lock))
        {
            self.locks_in_loop = true;
        }
    }

    // -- loops ---------------------------------------------------------------------

    fn looping(&mut self, node: Node<'a>) -> Parts {
        let bound = self.bounds().of_loop(node).unwrap_or(Bound::Unknown);
        self.note_bound(bound);
        self.bind_loop_variables(node);
        self.enter_loop(bound);
        let body = self.visit_children(node);
        self.exit_loop();
        body.repeated(bound, line_of(node))
    }

    /// The names a `for` over a collection introduces, which hold elements of unknown size.
    fn bind_loop_variables(&mut self, node: Node<'a>) {
        let Some(header) = self.bounds().each_header(node) else {
            return;
        };
        let profile = self.names.profile;
        for field in ["left", "name", "declarator"] {
            let Some(holder) = header.child_by_field_name(field) else {
                continue;
            };
            for identifier in std::iter::once(holder).chain(descendants(holder)) {
                if profile.is_identifier(identifier) {
                    let name = self.names.text(identifier);
                    self.env.bind(name, super::env::Binding::default());
                }
            }
        }
    }

    fn comprehension(&mut self, node: Node<'a>) -> Parts {
        let vocabulary = &self.names.profile.loops;
        let mut cursor = node.walk();
        let clauses: Vec<Node<'a>> = node
            .named_children(&mut cursor)
            .filter(|child| {
                vocabulary
                    .clauses
                    .iter()
                    .any(|(kind, _)| *kind == kind_of(child))
            })
            .collect();
        let mut bounds = Vec::new();
        for clause in &clauses {
            let bound = self.clause_bound(*clause);
            self.note_bound(bound);
            self.bind_loop_variables_of(*clause);
            bounds.push(bound);
        }
        for bound in &bounds {
            self.enter_loop(*bound);
        }
        let mut parts = vec![self.visit_children(node)];
        for _ in &bounds {
            self.exit_loop();
        }
        // A list, set or dictionary holds one result per pass; a generator holds none.
        if !kind_of(node).contains("generator") {
            parts.push(Parts {
                grow: Some(Cost::One),
                ..Parts::default()
            });
        }
        let mut parts = Parts::sequence(parts);
        for bound in bounds.into_iter().rev() {
            parts = parts.repeated(bound, line_of(node));
        }
        parts
    }

    fn clause_bound(&mut self, clause: Node<'a>) -> Bound {
        let vocabulary = &self.names.profile.loops;
        vocabulary
            .clauses
            .iter()
            .find(|(kind, _)| *kind == kind_of(clause))
            .and_then(|(_, field)| clause.child_by_field_name(field))
            .map_or(Bound::Unknown, |iterable| {
                self.bounds().iterable_bound(iterable)
            })
    }

    fn bind_loop_variables_of(&mut self, header: Node<'a>) {
        let profile = self.names.profile;
        let Some(holder) = header.child_by_field_name("left") else {
            return;
        };
        for identifier in std::iter::once(holder).chain(descendants(holder)) {
            if profile.is_identifier(identifier) {
                let name = self.names.text(identifier);
                self.env.bind(name, super::env::Binding::default());
            }
        }
    }

    // -- calls ---------------------------------------------------------------------

    fn call(&mut self, node: Node<'a>) -> Parts {
        let profile = self.names.profile;
        let dotted = self.names.called_name(node);
        let tail = dotted.as_deref().map(tail_of);
        let is_hof = tail.is_some_and(|tail| profile.hof_loops.contains(&tail));
        let arguments = self.names.arguments(node);
        let callbacks: Vec<Node<'a>> = if is_hof {
            arguments
                .iter()
                .copied()
                .filter(|argument| profile.is_function(*argument))
                .collect()
        } else {
            Vec::new()
        };
        let by_name: Vec<&'a str> = if is_hof {
            arguments
                .iter()
                .filter(|argument| profile.is_identifier(**argument))
                .map(|argument| self.names.text(*argument))
                .filter(|name| self.is_local_function(name))
                .collect()
        } else {
            Vec::new()
        };

        let mut parts = self.evaluated_before(node, &callbacks);
        if is_hof && (!callbacks.is_empty() || !by_name.is_empty()) {
            parts.push(self.callback_loop(node, &arguments, &callbacks, &by_name));
            return Parts::sequence(parts);
        }
        if let Some(dotted) = dotted {
            parts.push(self.call_itself(node, &dotted, &arguments));
        }
        Parts::sequence(parts)
    }

    /// The callee and the arguments, which run once before the call does.
    fn evaluated_before(&mut self, call: Node<'a>, callbacks: &[Node<'a>]) -> Vec<Parts> {
        let profile = self.names.profile;
        let mut parts = Vec::new();
        let mut cursor = call.walk();
        for child in call.named_children(&mut cursor) {
            if profile.argument_list_kinds.contains(&kind_of(child)) {
                let mut inner = child.walk();
                for argument in child.named_children(&mut inner) {
                    if !callbacks.contains(&argument) {
                        parts.push(self.visit(argument));
                    }
                }
            } else {
                parts.push(self.visit(child));
            }
        }
        parts
    }

    fn is_local_function(&self, name: &str) -> bool {
        self.shared.file.functions.contains(name) || self.nested_names.contains(name)
    }

    /// A callback handed to `map` or `forEach` runs once per element of what it is
    /// called on, so its body is the body of a loop over that.
    fn callback_loop(
        &mut self,
        call: Node<'a>,
        arguments: &[Node<'a>],
        callbacks: &[Node<'a>],
        by_name: &[&'a str],
    ) -> Parts {
        let source = self
            .bounds()
            .receiver_expression(call)
            .or_else(|| arguments.first().copied());
        let bound = source.map_or(Bound::Unknown, |source| {
            self.bounds().iterable_bound(source)
        });
        self.note_bound(bound);
        self.enter_loop(bound);
        let mut body = Vec::new();
        for callback in callbacks {
            body.push(self.callback_body(*callback));
        }
        for name in by_name {
            let line = line_of(call);
            let index = self.push_call(CalleeRef::Local((*name).to_owned()), Vec::new(), line);
            body.push(Parts::timed(Cost::Call(index)));
        }
        self.exit_loop();
        Parts::sequence(body).repeated(bound, line_of(call))
    }

    fn callback_body(&mut self, callback: Node<'a>) -> Parts {
        let profile = self.names.profile;
        let mut cursor = callback.walk();
        for parameter in callback.named_children(&mut cursor) {
            if profile.is_identifier(parameter)
                || profile.parameter_list_kinds.contains(&kind_of(parameter))
            {
                for identifier in std::iter::once(parameter).chain(descendants(parameter)) {
                    if profile.is_identifier(identifier) {
                        self.env
                            .bind(self.names.text(identifier), super::env::Binding::default());
                    }
                }
            }
        }
        self.names
            .function_body(callback)
            .map_or_else(Parts::default, |body| self.visit(body))
    }

    fn push_call(&mut self, callee: CalleeRef, args: Vec<ArgSize>, line: u32) -> u16 {
        self.calls.push(CallSite {
            callee,
            args,
            line,
            in_loop: self.depth,
        });
        (self.calls.len() - 1) as u16
    }

    /// What the call itself costs, beyond what was evaluated to make it.
    fn call_itself(&mut self, node: Node<'a>, dotted: &str, arguments: &[Node<'a>]) -> Parts {
        let (receiver, tail) = match dotted.rsplit_once('.') {
            Some((receiver, tail)) => (Some(receiver), tail),
            None => (None, dotted),
        };
        let line = line_of(node);
        let mut parts = Parts::default();

        self.note_signals(dotted, receiver, tail, arguments);
        if self.is_round_trip(receiver, tail) {
            parts.io = Some(Cost::One);
        }
        if self.is_self_call(dotted) {
            self.note_recursion(arguments);
        }
        if let Some(name) = self.grown_local(receiver, tail, arguments) {
            parts.grow = Some(Cost::One);
            self.bound_by_enclosing_loop(&name);
        }

        let lookup = self.library(node, dotted, receiver, tail, arguments, line);
        match lookup {
            Library::Matched(library) => {
                parts.time = library.time;
                parts.temp = library.temp;
            }
            Library::Unmatched => {
                if let Some(callee) = self.callee_of(node, receiver, tail) {
                    let args = arguments
                        .iter()
                        .map(|argument| self.arg_size(*argument))
                        .collect();
                    let index = self.push_call(callee, args, line);
                    parts.time = Cost::Call(index);
                }
            }
        }
        parts
    }

    fn library(
        &mut self,
        node: Node<'a>,
        dotted: &str,
        receiver: Option<&str>,
        tail: &str,
        arguments: &[Node<'a>],
        line: u32,
    ) -> Library {
        if receiver.is_none() && self.is_local_function(tail) {
            return Library::Unmatched;
        }
        let literals: Vec<Option<&str>> = arguments
            .iter()
            .map(|argument| {
                self.is_literal(*argument)
                    .then(|| self.names.text(*argument))
            })
            .collect();
        let call = CallFacts {
            literals: &literals,
        };
        let language = self.names.profile.language;
        let tables = self.shared.tables;
        let receiver_node = self.bounds().receiver_expression(node);

        let mut found = tables.function(language, dotted, &call);
        let mut keyed = false;
        if found.is_none() && receiver.is_some() {
            let fact = receiver_node.and_then(|expression| self.fact_of(expression));
            match fact {
                Some(fact) if fact.is_keyed_guess() => keyed = true,
                Some(fact) => {
                    found = tables.method(language, &fact.head, tail, &call);
                    if !fact.declared && found.is_some() {
                        self.weaken(Conf::Heuristic);
                    }
                }
                None => {
                    found = tables.any_method(language, tail, &call);
                    if found.is_some() {
                        self.weaken(Conf::Heuristic);
                    }
                }
            }
        }
        if keyed {
            return Library::Matched(Charge::default());
        }
        let Some((index, entry)) = found else {
            return Library::Unmatched;
        };
        Library::Matched(self.charge(index, entry, receiver_node, arguments, line))
    }

    fn charge(
        &mut self,
        index: u16,
        entry: &Entry,
        receiver: Option<Node<'a>>,
        arguments: &[Node<'a>],
        line: u32,
    ) -> Charge {
        let size = match entry.size_of {
            SizeOf::Receiver => receiver.and_then(|source| self.bounds().collection(source)),
            // A call that is not given the thing it works on has nothing to work through.
            SizeOf::Arg(position) => match arguments.get(usize::from(position)) {
                Some(source) => self.bounds().collection(*source),
                None => Some(SizeRef::Const),
            },
        };
        if size.is_none() {
            self.weaken(Conf::Heuristic);
        }
        let size = self.arg_of(size);
        let arg = if entry
            .cost
            .terms()
            .iter()
            .any(|term| term.factors().iter().any(|(variable, _, _)| *variable == 1))
        {
            arguments
                .first()
                .map_or(ArgSize::Unknown, |first| self.arg_size(*first))
        } else {
            ArgSize::Const
        };
        let lib = Cost::Lib {
            entry: index,
            size,
            line,
            arg,
        };
        Charge {
            time: if entry.cost.is_one() {
                Cost::One
            } else {
                lib.clone()
            },
            temp: if entry.space.is_one() { Cost::One } else { lib },
        }
    }

    fn is_literal(&self, node: Node<'a>) -> bool {
        let profile = self.names.profile;
        let kind = kind_of(node);
        profile.string_kinds.contains(&kind) || profile.number_kinds.contains(&kind)
    }

    /// What a receiver is, as far as it can be told.
    fn fact_of(&mut self, expression: Node<'a>) -> Option<TypeFact> {
        let profile = self.names.profile;
        let node = self.names.see_through(expression);
        if profile.is_identifier(node) {
            let name = self.names.text(node);
            return self
                .env
                .binding(name)
                .and_then(|binding| binding.fact.clone())
                .or_else(|| names_a_keyed_value(name).then(|| TypeFact::guessed(KEYED_GUESS)));
        }
        if profile.is_member_access(node)
            && let Some(dotted) = self.names.callee_name(node)
        {
            return self
                .field_fact(&dotted)
                .or_else(|| names_a_keyed_value(&dotted).then(|| TypeFact::guessed(KEYED_GUESS)));
        }
        constructed_fact(&self.names, node)
    }

    /// `this.items` is the member `items` of the class the function is in.
    fn field_fact(&self, dotted: &str) -> Option<TypeFact> {
        let (head, field) = dotted.split_once('.')?;
        let own =
            self.names.profile.self_names.contains(&head) || self.env.receiver() == Some(head);
        if !own || field.contains('.') {
            return None;
        }
        self.env.field(field).cloned()
    }

    // -- calls to the function's own code --------------------------------------------

    fn callee_of(
        &mut self,
        node: Node<'a>,
        receiver: Option<&str>,
        tail: &str,
    ) -> Option<CalleeRef> {
        let file = self.shared.file;
        let Some(receiver) = receiver else {
            if self.is_local_function(tail) {
                return Some(CalleeRef::Local(tail.to_owned()));
            }
            let imported = file.imports.get(tail)?;
            return Some(CalleeRef::Imported {
                local: tail.to_owned(),
                member: imported.member.clone(),
                req: imported.request,
            });
        };
        let head = receiver.split('.').next().unwrap_or(receiver);
        let profile = self.names.profile;
        if receiver == head
            && (profile.self_names.contains(&head) || self.env.receiver() == Some(head))
        {
            return Some(CalleeRef::Method {
                recv_type: self.class.clone(),
                name: tail.to_owned(),
            });
        }
        if let Some(imported) = file.imports.get(head)
            && self.env.binding(head).is_none()
        {
            return Some(if imported.namespace {
                CalleeRef::Imported {
                    local: head.to_owned(),
                    member: Some(tail.to_owned()),
                    req: imported.request,
                }
            } else {
                CalleeRef::Method {
                    recv_type: Some(head.to_owned()),
                    name: tail.to_owned(),
                }
            });
        }
        if file.fields.contains_key(head) && self.env.binding(head).is_none() {
            return Some(CalleeRef::Method {
                recv_type: Some(head.to_owned()),
                name: tail.to_owned(),
            });
        }
        let receiver_node = self.bounds().receiver_expression(node);
        match receiver_node.and_then(|expression| self.fact_of(expression)) {
            Some(fact) if fact.is_keyed_guess() => None,
            Some(fact) if self.shared.tables.knows_type(profile.language, &fact.head) => None,
            Some(fact) => Some(CalleeRef::Method {
                recv_type: Some(fact.head),
                name: tail.to_owned(),
            }),
            None if self.depth > 0 || file.functions.contains(tail) => Some(CalleeRef::Method {
                recv_type: None,
                name: tail.to_owned(),
            }),
            None => None,
        }
    }

    fn is_self_call(&self, dotted: &str) -> bool {
        let Some(name) = self.name else {
            return false;
        };
        let profile = self.names.profile;
        match dotted.split_once('.') {
            None => dotted == name,
            Some((head, rest)) => {
                rest == name
                    && (profile.self_names.contains(&head) || self.env.receiver() == Some(head))
            }
        }
    }

    fn note_recursion(&mut self, arguments: &[Node<'a>]) {
        let language = self.names.profile.language;
        let found = arguments
            .iter()
            .zip(&self.params)
            .map(|(argument, parameter)| {
                reduction(language, self.names.text(*argument), &parameter.name)
            })
            .find(|found| found.shrink != rqx_core::cost::Shrink::Other)
            .unwrap_or(Reduction {
                shrink: rqx_core::cost::Shrink::Other,
                copies: false,
            });
        self.reductions.push(found);
    }

    // -- signals ---------------------------------------------------------------------

    fn is_round_trip(&self, receiver: Option<&str>, tail: &str) -> bool {
        let extra = self.shared.performance;
        match receiver {
            Some(receiver) => {
                reaches_outside(receiver, &extra.receivers)
                    && asks_for_data(tail, &extra.methods)
                    && !names_a_keyed_value(receiver)
            }
            None => IO_BARE.contains(&tail.to_ascii_lowercase().as_str()),
        }
    }

    fn note_signals(
        &mut self,
        dotted: &str,
        receiver: Option<&str>,
        tail: &str,
        arguments: &[Node<'a>],
    ) {
        let profile = self.names.profile;
        let signals = &profile.signals;
        if self.is_async && self.names.is_blocking(dotted) {
            self.blocking_async = true;
        }
        if self.depth > 0 && signals.lock_calls.contains(&tail) {
            self.locks_in_loop = true;
        }
        let starts_many = signals
            .fanout_calls
            .iter()
            .any(|call| self.names.answers_to(dotted, call))
            && arguments
                .iter()
                .any(|argument| self.holds_a_loop(*argument));
        let spawns_in_a_loop = self.depth > 0
            && signals
                .spawn_calls
                .iter()
                .any(|call| self.names.answers_to(dotted, call));
        if starts_many || spawns_in_a_loop {
            self.unbounded_fanout = true;
        }
        if profile.loops.appenders.contains(&tail) {
            let subject =
                receiver.or_else(|| arguments.first().map(|first| self.names.text(*first)));
            let shared = subject.map(tail_of);
            if let Some(name) = shared
                && self.shared.file.globals.contains(name)
            {
                self.static_growth.insert(name.to_owned());
            }
        }
    }

    /// Whether an argument spreads over a collection: a comprehension, or a `map`.
    fn holds_a_loop(&self, argument: Node<'a>) -> bool {
        let profile = self.names.profile;
        std::iter::once(argument)
            .chain(descendants(argument))
            .any(|node| {
                profile.comprehension_kinds.contains(&kind_of(node))
                    || (profile.is_call(node)
                        && self
                            .names
                            .called_name(node)
                            .is_some_and(|called| profile.hof_loops.contains(&tail_of(&called))))
            })
    }

    /// The collection made here that a call adds one element to.
    fn grown_local(
        &self,
        receiver: Option<&str>,
        tail: &str,
        arguments: &[Node<'a>],
    ) -> Option<String> {
        if !self.names.profile.loops.appenders.contains(&tail) {
            return None;
        }
        let subject = match receiver {
            Some(receiver) if !receiver.contains('.') => receiver.to_owned(),
            Some(_) => return None,
            None => self.names.text(*arguments.first()?).to_owned(),
        };
        self.env
            .binding(&subject)
            .filter(|binding| binding.parameter.is_none())?;
        Some(subject)
    }

    /// A collection added to once per pass of the only loop around it is no longer than
    /// that loop is long.
    fn bound_by_enclosing_loop(&mut self, name: &str) {
        let [Bound::Size(loop_size)] = self.loop_bounds.as_slice() else {
            return;
        };
        let loop_size = *loop_size;
        if self
            .env
            .binding(name)
            .is_some_and(|binding| binding.size.is_some())
        {
            return;
        }
        let own = self.bounds().variable(name);
        self.env.merge(own, loop_size);
    }

    /// `seen[key] = value` adds an entry to a map made here.
    fn with_growth_by_key(&mut self, assignment: Node<'a>, parts: Parts) -> Parts {
        let Some(target) = assignment.child_by_field_name("left") else {
            return parts;
        };
        let target = self.names.see_through(target);
        let kind = kind_of(target);
        if !(kind.contains("subscript") || kind.contains("index")) {
            return parts;
        }
        let subject = target
            .child_by_field_name("value")
            .or_else(|| target.child_by_field_name("object"))
            .or_else(|| target.child_by_field_name("operand"))
            .or_else(|| target.named_child(0))
            .filter(|subject| self.names.profile.is_identifier(*subject))
            .map(|subject| self.names.text(subject));
        let grows = subject
            .and_then(|name| self.env.binding(name))
            .is_some_and(|binding| {
                binding.parameter.is_none()
                    && binding
                        .fact
                        .as_ref()
                        .is_some_and(|fact| KEYED_HEADS.contains(&fact.head.as_str()))
            });
        if !grows {
            return parts;
        }
        Parts::sequence(vec![
            parts,
            Parts {
                grow: Some(Cost::One),
                ..Parts::default()
            },
        ])
    }

    /// `x in values` looks through `values` when it is a list.
    fn with_membership(&mut self, comparison: Node<'a>, parts: Parts) -> Parts {
        let mut cursor = comparison.walk();
        let is_membership = comparison
            .children(&mut cursor)
            .any(|child| !child.is_named() && self.names.text(child) == "in");
        if !is_membership
            || operator_of(comparison, self.names.source).is_some_and(|operator| operator == "is")
        {
            return parts;
        }
        let Some(subject) =
            comparison.named_child(comparison.named_child_count().saturating_sub(1))
        else {
            return parts;
        };
        let Some(fact) = self.fact_of(subject) else {
            return parts;
        };
        let language = self.names.profile.language;
        let call = CallFacts::NONE;
        let tables = self.shared.tables;
        let Some((index, entry)) = tables.method(language, &fact.head, "__contains__", &call)
        else {
            return parts;
        };
        if !fact.declared {
            self.weaken(Conf::Heuristic);
        }
        let charge = self.charge(index, entry, Some(subject), &[], line_of(comparison));
        Parts::sequence(vec![
            parts,
            Parts {
                time: charge.time,
                temp: charge.temp,
                ..Parts::default()
            },
        ])
    }

    // -- the finished record ----------------------------------------------------------

    fn finish(
        &mut self,
        function: Node<'a>,
        body: Option<Node<'a>>,
        parts: Parts,
    ) -> Option<FunctionCost> {
        let profile = self.names.profile;
        let remap = self.remap_for(&parts);
        let parts = remap.parts(parts);
        let calls: Vec<CallSite> = std::mem::take(&mut self.calls)
            .into_iter()
            .map(|call| remap.call(call))
            .collect();
        let rec = self.recurrence(function, body);
        let seq_awaits = body
            .filter(|_| self.is_async)
            .map(|body| {
                let holder = body
                    .named_children(&mut body.walk())
                    .find(|child| profile.is_block(*child))
                    .unwrap_or(body);
                independent_awaits(&self.names, holder).len()
            })
            .unwrap_or(0);
        let ind = Indicators {
            round_trips: parts.io.clone().unwrap_or_default(),
            allocs_per_iter: u8::try_from(self.allocations_per_pass).unwrap_or(u8::MAX),
            blocking_async: self.blocking_async,
            locks_in_loop: self.locks_in_loop,
            unbounded_fanout: self.unbounded_fanout,
            seq_awaits: u8::try_from(seq_awaits).unwrap_or(u8::MAX),
            static_growth: std::mem::take(&mut self.static_growth)
                .into_iter()
                .collect(),
        };
        let space = seq([parts.grow.unwrap_or_default(), parts.temp]);
        if parts.time.is_one()
            && space.is_one()
            && calls.is_empty()
            && ind.is_empty()
            && rec.is_none()
        {
            return None;
        }
        let name = self.name.map(str::to_owned);
        let qual = match (&self.class, &name) {
            (Some(class), Some(name)) => format!("{class}.{name}"),
            (None, Some(name)) => name.clone(),
            _ => "<anonymous>".to_owned(),
        };
        let exported_as = name
            .clone()
            .filter(|name| self.shared.file.exports.contains(name));
        Some(FunctionCost {
            name,
            qual,
            start: function.start_byte() as u32,
            line: line_of(function),
            params: std::mem::take(&mut self.params),
            vars: remap.into_vars(),
            time: parts.time,
            space,
            calls,
            rec,
            ind,
            conf: self.conf,
            exported_as,
        })
    }

    /// Which size variables the finished record refers to, and which of them turned out
    /// to be the same size as another.
    fn remap_for(&self, parts: &Parts) -> Remap {
        let mut referenced = BTreeSet::new();
        for cost in [
            Some(&parts.time),
            parts.grow.as_ref(),
            Some(&parts.temp),
            parts.io.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            referenced_by(cost, &mut referenced);
        }
        referenced_by_calls(&self.calls, &mut referenced);
        Remap::new(self.env.vars(), self.env.same_as(), &referenced)
    }

    fn recurrence(&self, function: Node<'a>, body: Option<Node<'a>>) -> Option<Recurrence> {
        if self.reductions.is_empty() {
            return None;
        }
        let shape = combined(&self.reductions);
        Some(Recurrence {
            calls: u8::try_from(self.reductions.len()).unwrap_or(u8::MAX),
            shrink: shape.shrink,
            copies_arg: shape.copies,
            memo: self.is_memoised(function, body),
        })
    }

    /// A cache the author put in front of the recursion: a decorator that remembers
    /// answers, or a table the body reads before it recurses.
    fn is_memoised(&self, function: Node<'a>, body: Option<Node<'a>>) -> bool {
        let profile = self.names.profile;
        let decorated = function.parent().is_some_and(|parent| {
            profile.decorated_kinds.contains(&kind_of(parent))
                && ["lru_cache", "cache", "memoize", "Cacheable"]
                    .iter()
                    .any(|word| self.names.text(parent).contains(word))
        });
        decorated
            || body.is_some_and(|body| {
                descendants(body).into_iter().any(|node| {
                    profile.is_identifier(node) && mentions_any(self.names.text(node), &MEMO_HINTS)
                })
            })
    }
}

/// The time and temporary space of a library call.
#[derive(Default)]
struct Charge {
    time: Cost,
    temp: Cost,
}

enum Library {
    /// The call is a library call the tables cost, possibly at nothing.
    Matched(Charge),
    Unmatched,
}

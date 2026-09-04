use std::collections::{BTreeMap, BTreeSet, HashMap};

use rqx_core::facts::{Chain, Clump, Discriminant, Mention, PatternLimits, Patterns};
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{
    descendants, find_within, is_binding_position, operands_of, operator_of, span_of, text_of,
};

/// `strName`, `bEnabled`, `IOrder`: a type spelled into a name the compiler already
/// knows the type of.
fn encoding_of(name: &str, prefixes: &[(&'static str, &'static str)]) -> Option<&'static str> {
    let mut characters = name.chars();
    let first = characters.next()?;
    let second = characters.next()?;
    // `IOrder` but not `Iterator`: a capital directly after the I is the convention.
    if first == 'I' && second.is_ascii_uppercase() {
        return Some("interface I-prefix");
    }
    if first == 'b' && (second.is_ascii_uppercase() || second == '_') {
        return Some("boolean b-prefix");
    }
    prefixes.iter().find_map(|(prefix, label)| {
        let rest = name.strip_prefix(prefix)?;
        // A capital starts the next word in camelCase; an underscore does the same
        // job in snake_case, and the prefix means the same thing in both.
        rest.chars()
            .next()
            .filter(|next| next.is_ascii_uppercase() || *next == '_')
            .map(|_| *label)
    })
}

/// One value and every place it is switched on. `assigns` is what separates a
/// Strategy from a State: a branch that writes back to the value it dispatched on is
/// a transition, not a choice of algorithm.
struct Switch {
    sites: u32,
    branches: u32,
    assigns: bool,
    /// Every branch doing nothing but call one function of the same shape. A switch
    /// like that is already a dispatch table; it just has no table.
    dispatch: Option<u32>,
    dispatches: bool,
    span: Span,
}

impl Switch {
    /// One branch that does anything but call settles it for the whole switch: a
    /// dispatch table with a hole in it is not one.
    fn note_dispatch(&mut self, arities: &[Option<u32>]) {
        if arities.is_empty() {
            self.dispatches = false;
            return;
        }
        for arity in arities {
            match (*arity, self.dispatch) {
                (None, _) => self.dispatches = false,
                (Some(found), None) => self.dispatch = Some(found),
                (Some(found), Some(seen)) if found != seen => self.dispatches = false,
                (Some(_), Some(_)) => {}
            }
        }
    }

    const fn is_dispatch(&self) -> bool {
        self.dispatches && self.dispatch.is_some()
    }

    fn at(span: Span) -> Self {
        Self {
            sites: 0,
            branches: 0,
            assigns: false,
            dispatch: None,
            dispatches: true,
            span,
        }
    }
}

struct Collector<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    signatures: Vec<(Vec<String>, Span)>,
    switches: HashMap<String, Switch>,
    class_stack: Vec<String>,
    subclasses: HashMap<String, String>,
    references: Vec<(String, String, Span)>,
    out: Patterns,
}

impl<'a> Collector<'a> {
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

    /// The dotted name a value is written with, so `this.kind` and `order.kind` are
    /// each one subject a switch can be about.
    fn subject_text(&self, node: Node<'a>) -> Option<String> {
        let node = self.see_through(node);
        if self.profile.is_identifier(node) || self.profile.self_names.contains(&self.text(node)) {
            return Some(self.text(node).to_owned());
        }
        if self.profile.is_member_access(node) {
            let object = self.subject_text(node.child_by_field_name(self.profile.object_field)?)?;
            let property = node.child_by_field_name(self.profile.property_field)?;
            return Some(format!("{object}.{}", self.text(property)));
        }
        None
    }

    /// Deliberately looser than `comparison_subject`, which reads only equality
    /// against a literal and so cannot see a chain of predicates at all.
    fn condition_topic(&self, node: Node<'a>) -> Option<String> {
        let node = self.see_through(node);
        if self.profile.is_call(node) {
            return node
                .child_by_field_name(self.profile.callee_field)
                .and_then(|callee| self.subject_text(callee));
        }
        if let Some(topic) = self.subject_text(node) {
            return Some(topic);
        }
        if self.profile.binary_kinds.contains(&node.kind()) {
            return operands_of(node).and_then(|(left, _)| self.condition_topic(left));
        }
        node.named_child(0)
            .and_then(|inner| self.condition_topic(inner))
    }

    /// Every branch's topic, in order, or nothing when a branch cannot be read: a
    /// chain with a hole in it is not one this can make a claim about.
    fn chain_topics(&self, statement: Node<'a>) -> Vec<String> {
        let Some(condition) = statement.child_by_field_name(self.profile.condition_field) else {
            return Vec::new();
        };
        let Some(head) = self.condition_topic(condition) else {
            return Vec::new();
        };
        let mut topics = vec![head];

        if self.profile.else_is_sibling_list {
            let mut cursor = statement.walk();
            for clause in statement.named_children(&mut cursor) {
                if !self.profile.else_clause_kinds.contains(&clause.kind()) {
                    continue;
                }
                let Some(condition) = clause.child_by_field_name(self.profile.condition_field)
                else {
                    continue;
                };
                match self.condition_topic(condition) {
                    Some(topic) => topics.push(topic),
                    None => return Vec::new(),
                }
            }
            return topics;
        }

        let mut alternative = statement.child_by_field_name(self.profile.else_field);
        while let Some(branch) = alternative {
            let branch = if branch.kind() == self.profile.if_kind {
                branch
            } else {
                match branch.named_child(0) {
                    Some(inner) if inner.kind() == self.profile.if_kind => inner,
                    _ => break,
                }
            };
            let Some(condition) = branch.child_by_field_name(self.profile.condition_field) else {
                break;
            };
            match self.condition_topic(condition) {
                Some(topic) => topics.push(topic),
                None => return Vec::new(),
            }
            alternative = branch.child_by_field_name(self.profile.else_field);
        }
        topics
    }

    /// The argument count of the one call a branch makes, when the branch does
    /// nothing else. Anything more than a call is behaviour that has to move with the
    /// case, which is a different refactoring from turning cases into commands.
    fn call_arity(&self, node: Node<'a>) -> Option<u32> {
        let node = self.see_through(node);
        if self.profile.is_call(node) {
            let mut cursor = node.walk();
            let arguments = node
                .named_children(&mut cursor)
                .find(|child| self.profile.argument_list_kinds.contains(&child.kind()))?;
            return Some(arguments.named_child_count() as u32);
        }
        node.named_child(0).and_then(|inner| self.call_arity(inner))
    }

    /// Filtering to statements is what drops a case label, which every grammar spells
    /// differently and some do not put in a field at all.
    fn statements_arity(&self, holder: Node<'a>) -> Option<u32> {
        let mut cursor = holder.walk();
        let statements: Vec<Node<'a>> = holder
            .named_children(&mut cursor)
            .filter(|child| self.profile.is_statement(*child) || self.profile.is_block(*child))
            .collect();
        let [only] = statements.as_slice() else {
            return None;
        };
        self.call_arity(*only)
    }

    fn body_arity(&self, branch: Node<'a>) -> Option<u32> {
        // Every one of the five grammars calls an `if`'s body its consequence.
        let body = branch.child_by_field_name("consequence")?;
        if self.profile.is_block(body) {
            self.statements_arity(body)
        } else {
            self.call_arity(body)
        }
    }

    fn note_handler_chain(&mut self, statement: Node<'a>) {
        let topics = self.chain_topics(statement);
        if topics.len() < 3 {
            return;
        }
        // One value deciding every branch is a discriminant, and `type-code-switch`
        // already reports that; a chain of handlers is the case where each branch asks
        // a different question.
        let distinct: BTreeSet<&str> = topics.iter().map(String::as_str).collect();
        if distinct.len() != topics.len() {
            return;
        }
        self.out.handler_chains.push(Chain {
            topics,
            span: span_of(statement),
        });
    }

    /// The subject of an `if (x === "a") … else if (x === "b")` chain, which is a
    /// switch wearing a different syntax.
    fn comparison_subject(&self, condition: Node<'a>) -> Option<String> {
        let node = self.see_through(condition);
        if !self.profile.binary_kinds.contains(&node.kind()) {
            return None;
        }
        let operator = operator_of(node, self.source)?;
        if !self.profile.equality_operators.contains(&operator) {
            return None;
        }
        let (left, right) = operands_of(node)?;
        let right = self.see_through(right);
        let literal = self.profile.string_kinds.contains(&right.kind())
            || self.profile.number_kinds.contains(&right.kind());
        literal.then(|| self.subject_text(left)).flatten()
    }

    /// Whether any branch writes back to the value being switched on. A `Strategy`
    /// chooses behaviour from a value it leaves alone; a `State` is the same shape
    /// where the branches are also the transitions.
    fn assigns_subject(&self, statement: Node<'a>, subject: &str) -> bool {
        descendants(statement)
            .into_iter()
            .filter(|node| self.profile.assignment_kinds.contains(&node.kind()))
            .filter_map(|assignment| assignment.child_by_field_name("left"))
            .any(|target| {
                let target = if self.profile.expression_list_kinds.contains(&target.kind()) {
                    target.named_child(0).unwrap_or(target)
                } else {
                    target
                };
                self.subject_text(target).is_some_and(|written| written == subject)
            })
    }

    fn note_chain(&mut self, statement: Node<'a>) {
        let Some(condition) = statement.child_by_field_name(self.profile.condition_field) else {
            return;
        };
        let Some(subject) = self.comparison_subject(condition) else {
            return;
        };
        let mut branches = 1;
        let mut arities = vec![self.body_arity(statement)];
        if self.profile.else_is_sibling_list {
            let mut cursor = statement.walk();
            for clause in statement.named_children(&mut cursor) {
                if !self.profile.else_clause_kinds.contains(&clause.kind()) {
                    continue;
                }
                let matches_subject = clause
                    .child_by_field_name(self.profile.condition_field)
                    .and_then(|condition| self.comparison_subject(condition))
                    .is_some_and(|other| other == subject);
                if !matches_subject {
                    break;
                }
                branches += 1;
                arities.push(self.body_arity(clause));
            }
            if branches >= 2 {
                let assigns = self.assigns_subject(statement, &subject);
                let entry = self
                    .switches
                    .entry(subject)
                    .or_insert_with(|| Switch::at(span_of(statement)));
                entry.sites += 1;
                entry.branches = entry.branches.max(branches);
                entry.assigns |= assigns;
                entry.note_dispatch(&arities);
            }
            return;
        }
        let mut alternative = statement.child_by_field_name(self.profile.else_field);
        while let Some(branch) = alternative {
            let branch = if branch.kind() == self.profile.if_kind {
                branch
            } else {
                match branch.named_child(0) {
                    Some(inner) if inner.kind() == self.profile.if_kind => inner,
                    _ => break,
                }
            };
            let matches_subject = branch
                .child_by_field_name(self.profile.condition_field)
                .and_then(|condition| self.comparison_subject(condition))
                .is_some_and(|other| other == subject);
            if !matches_subject {
                break;
            }
            branches += 1;
            arities.push(self.body_arity(branch));
            alternative = branch.child_by_field_name(self.profile.else_field);
        }
        if branches >= 2 {
            let assigns = self.assigns_subject(statement, &subject);
            let entry = self
                .switches
                .entry(subject)
                .or_insert_with(|| Switch::at(span_of(statement)));
            entry.sites += 1;
            entry.branches = entry.branches.max(branches);
            entry.assigns |= assigns;
            entry.note_dispatch(&arities);
        }
    }

    fn note_switch(&mut self, statement: Node<'a>) {
        let Some(value) = statement.child_by_field_name(self.profile.switch_subject_field) else {
            return;
        };
        let Some(subject) = self.subject_text(value) else {
            return;
        };
        let mut cursor = statement.walk();
        let cases: Vec<Node<'a>> = statement
            .child_by_field_name("body")
            .unwrap_or(statement)
            .named_children(&mut cursor)
            .filter(|child| self.profile.case_kinds.contains(&child.kind()))
            .collect();
        let arities: Vec<Option<u32>> = cases
            .iter()
            .map(|case| self.statements_arity(*case))
            .collect();
        let cases = cases.len() as u32;
        let assigns = self.assigns_subject(statement, &subject);
        let entry = self
            .switches
            .entry(subject)
            .or_insert_with(|| Switch::at(span_of(statement)));
        entry.sites += 1;
        entry.branches = entry.branches.max(cases);
        entry.assigns |= assigns;
        entry.note_dispatch(&arities);
    }

    /// A method declared apart from its type names the type in its receiver, which is
    /// the only thing joining the two.
    fn receiver_type(&self, method: Node<'a>) -> Option<String> {
        let receiver = method.child_by_field_name("receiver")?;
        descendants(receiver)
            .into_iter()
            .find(|node| node.kind() == "type_identifier")
            .map(|node| self.text(node).to_owned())
    }

    fn note_signature(&mut self, function: Node<'a>) {
        let Some(list) = find_within(
            function,
            self.profile.parameter_list_kinds,
            self.profile.block_kinds,
        )
        .into_iter()
        .next_back() else {
            return;
        };
        let mut inner = list.walk();
        let names: Vec<String> = list
            .named_children(&mut inner)
            .filter(|child| self.profile.parameter_kinds.contains(&child.kind()))
            .filter_map(|parameter| {
                if self.profile.is_identifier(parameter) {
                    return Some(self.text(parameter).to_owned());
                }
                descendants(parameter)
                    .into_iter()
                    .find(|child| self.profile.is_identifier(*child))
                    .map(|child| self.text(child).to_owned())
            })
            .collect();
        if names.len() >= 2 {
            self.signatures.push((names, span_of(function)));
        }
    }

    /// A union of three operands is written as two nested unions in some grammars,
    /// so the operands are the leaves rather than the immediate children.
    fn union_members(&self, node: Node<'a>) -> Vec<Node<'a>> {
        if !self.profile.union_type_kinds.contains(&node.kind()) {
            return vec![node];
        }
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .flat_map(|child| self.union_members(child))
            .collect()
    }

    /// A union of three or more string literals is a type code: a set of named cases
    /// with no type of its own to hang behaviour on.
    fn note_type_code(&mut self, alias: Node<'a>) {
        let Some(name) = alias.child_by_field_name(self.profile.alias_name_field) else {
            return;
        };
        let Some(value) = alias.child_by_field_name(self.profile.alias_value_field) else {
            return;
        };
        // The union may be wrapped: `type X = Literal[...]` puts one type node around
        // the list, where `type X = "a" | "b"` is the list itself.
        let Some(union) = descendants(value)
            .into_iter()
            .find(|node| self.profile.union_type_kinds.contains(&node.kind()))
        else {
            return;
        };
        let members = self.union_members(union);
        let strings = members
            .iter()
            .filter(|node| {
                self.profile.literal_type_kinds.contains(&node.kind())
                    && self.profile.string_kinds.contains(
                        &node
                            .named_child(0)
                            .map_or(node.kind(), |inner| inner.kind()),
                    )
            })
            .count();
        if strings >= 3 && strings == members.len() {
            let label = self.text(name).to_owned();
            self.out.type_codes.push(Mention {
                name: label,
                span: span_of(name),
            });
        }
    }

    fn walk(&mut self, node: Node<'a>) {
        let kind = node.kind();
        let profile = self.profile;
        let mut pushed_class = false;

        if profile.is_function(node) {
            self.note_signature(node);
        }
        if profile.method_kinds.contains(&kind)
            && !profile.is_class(node)
            && let Some(owner) = self.receiver_type(node)
        {
            self.class_stack.push(owner);
            pushed_class = true;
        }
        if profile.is_class(node) {
            let name = node
                .child_by_field_name("name")
                .map(|id| self.text(id).to_owned());
            if let Some(name) = name.clone() {
                let mut cursor = node.walk();
                let parent = node
                    .named_children(&mut cursor)
                    .flat_map(|child| {
                        let mut inner = child.walk();
                        let nested: Vec<Node<'a>> = child.named_children(&mut inner).collect();
                        std::iter::once(child).chain(nested)
                    })
                    .find(|child| profile.extends_kinds.contains(&child.kind()))
                    .and_then(|clause| {
                        let mut inner = clause.walk();
                        clause
                            .named_children(&mut inner)
                            .find(|child| profile.binding_identifier_kinds.contains(&child.kind()))
                    })
                    .map(|id| self.text(id).to_owned());
                if let Some(parent) = parent {
                    self.subclasses.insert(name, parent);
                }
            }
            self.class_stack.push(name.unwrap_or_default());
            pushed_class = true;
        }
        // Only a name can be in binding position, and answering the question costs a
        // walk of every sibling: a string literal's thousands of escape sequences each
        // rescanning the whole literal is quadratic in one node's child count.
        let names = profile.is_identifier(node)
            || profile.reference_only_kinds.contains(&kind)
            || profile.binding_identifier_kinds.contains(&kind);
        let declares = names
            && is_binding_position(
                node,
                profile.parameter_kinds,
                profile.borrowed_name_kinds,
                profile.local_declaring_kinds,
                profile.expression_list_kinds,
            );
        if (profile.is_identifier(node) || profile.reference_only_kinds.contains(&kind))
            && !declares
            && let Some(owner) = self.class_stack.last()
            && !owner.is_empty()
        {
            let owner = owner.clone();
            self.references
                .push((owner, self.text(node).to_owned(), span_of(node)));
        }
        if profile.binding_identifier_kinds.contains(&kind)
            && declares
            && let Some(encoding) = encoding_of(self.text(node), profile.hungarian_prefixes)
        {
            self.out.encoded_names.push(Mention {
                name: format!("{} ({encoding})", self.text(node)),
                span: span_of(node),
            });
        }
        if profile.property_kinds.contains(&kind)
            && node
                .parent()
                .is_some_and(|parent| profile.signature_kinds.contains(&parent.kind()))
            && let Some(encoding) = encoding_of(self.text(node), profile.hungarian_prefixes)
        {
            let name = self.text(node).to_owned();
            let span = node.parent().map_or(span_of(node), span_of);
            self.out.encoded_names.push(Mention {
                name: format!("{name} ({encoding})"),
                span,
            });
        }
        if profile.switch_kinds.contains(&kind) {
            self.note_switch(node);
        }
        if kind == profile.if_kind {
            self.note_chain(node);
            self.note_handler_chain(node);
        }
        if profile.type_alias_kinds.contains(&kind) {
            self.note_type_code(node);
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.walk(child);
        }
        if pushed_class {
            self.class_stack.pop();
        }
    }
}

pub fn collect(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
    limits: &PatternLimits,
) -> Patterns {
    let mut collector = Collector {
        profile,
        source,
        signatures: Vec::new(),
        switches: HashMap::new(),
        class_stack: Vec::new(),
        subclasses: HashMap::new(),
        references: Vec::new(),
        out: Patterns::default(),
    };
    collector.walk(root);

    let mut out = std::mem::take(&mut collector.out);

    // Data clumps: the same set of names travelling together through several
    // signatures is a concept the code has not named.
    let mut groups: BTreeMap<Vec<String>, (u32, Span)> = BTreeMap::new();
    for (names, span) in &collector.signatures {
        if names.len() < limits.clump_size as usize {
            continue;
        }
        let mut sorted: Vec<String> = names.clone();
        sorted.sort();
        sorted.dedup();
        let entry = groups.entry(sorted).or_insert((0, *span));
        entry.0 += 1;
    }
    for (names, (sites, span)) in groups {
        if sites >= limits.clump_sites {
            out.clumps.push(Clump { names, sites, span });
        }
    }

    // Every `else if` is itself an `if`, so the walk reaches the tail of a chain again
    // and would report it as a second, shorter chain.
    let mut chains = std::mem::take(&mut out.handler_chains);
    chains.sort_by_key(|chain| (chain.span.start, std::cmp::Reverse(chain.span.end)));
    for chain in chains {
        let nested = out.handler_chains.iter().any(|outer| {
            outer.span.start <= chain.span.start && chain.span.end <= outer.span.end
        });
        if !nested {
            out.handler_chains.push(chain);
        }
    }

    for (name, switch) in collector.switches {
        if switch.sites >= 2 && switch.branches >= 3 {
            out.discriminants.push(Discriminant {
                name,
                sites: switch.sites,
                branches: switch.branches,
                assigns_subject: switch.assigns,
                uniform_dispatch: switch.is_dispatch(),
                span: switch.span,
            });
        }
    }

    // A base class that names one of its own subclasses cannot be understood alone.
    let subclasses: BTreeSet<(&str, &str)> = collector
        .subclasses
        .iter()
        .map(|(child, parent)| (parent.as_str(), child.as_str()))
        .collect();
    for (owner, referenced, span) in &collector.references {
        if subclasses.contains(&(owner.as_str(), referenced.as_str())) {
            out.derived_references.push(Mention {
                name: referenced.clone(),
                span: *span,
            });
        }
    }

    // Two classes that each name the other are coupled in both directions, which is
    // the shape Inappropriate Intimacy describes: neither can be read, tested or
    // moved without the other coming with it.
    let mut reaches: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut first_site: BTreeMap<(&str, &str), Span> = BTreeMap::new();
    for (owner, referenced, span) in &collector.references {
        if owner == referenced {
            continue;
        }
        reaches.entry(owner).or_default().insert(referenced);
        first_site
            .entry((owner.as_str(), referenced.as_str()))
            .or_insert(*span);
    }
    // A base and its own subclass naming each other is inheritance, not intimacy, and
    // `base-depends-on-derived` already has an opinion about it.
    let related = |left: &str, right: &str| {
        collector
            .subclasses
            .get(left)
            .is_some_and(|parent| parent == right)
            || collector
                .subclasses
                .get(right)
                .is_some_and(|parent| parent == left)
    };
    let mut pairs: BTreeSet<(&str, &str)> = BTreeSet::new();
    for (owner, targets) in &reaches {
        for target in targets {
            if reaches
                .get(*target)
                .is_some_and(|back| back.contains(owner))
                && !related(owner, target)
            {
                pairs.insert(if owner < target {
                    (owner, target)
                } else {
                    (target, owner)
                });
            }
        }
    }
    for (left, right) in pairs {
        if let Some(span) = first_site
            .get(&(left, right))
            .or_else(|| first_site.get(&(right, left)))
        {
            out.intimate_pairs.push(Mention {
                name: format!("{left} and {right}"),
                span: *span,
            });
        }
    }

    out.intimate_pairs.sort_by_key(|item| item.span.start);
    out.clumps.sort_by_key(|clump| clump.span.start);
    out.discriminants.sort_by_key(|item| item.span.start);
    out.derived_references.sort_by_key(|item| item.span.start);
    out.encoded_names.sort_by_key(|item| item.span.start);
    out.type_codes.sort_by_key(|item| item.span.start);
    out
}

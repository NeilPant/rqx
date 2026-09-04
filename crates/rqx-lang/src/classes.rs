use std::collections::{HashMap, HashSet};

use rqx_core::facts::{ClassReport, Envy, Field, Method, Named, Pair};
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{descendants, find_within, operator_of, span_of, text_of};

/// A class as the rules understand it: a name, and the members that belong to it.
/// Most languages write that as one node; Go writes the type in one place and its
/// methods in another, so the shape is assembled rather than found.
pub struct ClassUnit<'tree> {
    pub name: String,
    pub span: Span,
    pub members: Vec<Node<'tree>>,
    pub has_heritage: bool,
    /// The names after `extends` and `implements`. `has_heritage` answers whether a
    /// class promises anything; this answers what.
    pub supertypes: Vec<String>,
}

/// The type inside a collection, read from how the type is written rather than from
/// any resolution: `Handler[]`, `[]Handler`, `List<Handler>` and `map[string]Handler`
/// all say the same thing in four grammars, and none of them needs a type checker.
fn element_of(type_name: &str) -> Option<String> {
    let trimmed = type_name.trim();
    if let Some(inner) = trimmed.strip_suffix("[]") {
        return non_empty(inner);
    }
    if let Some(rest) = trimmed.strip_prefix("map[") {
        return rest.split_once(']').and_then(|(_, value)| non_empty(value));
    }
    if let Some(inner) = trimmed.strip_prefix("[]") {
        return non_empty(inner);
    }
    // `List<Handler>` and `list[str]` are the same statement in two grammars, and a
    // map's element is what it maps to rather than what it maps from.
    let open = trimmed.find(['<', '['])?;
    let close = trimmed.rfind(['>', ']'])?;
    let inner = trimmed.get(open + 1..close)?;
    non_empty(inner.rsplit(',').next().unwrap_or(inner))
}

fn non_empty(text: &str) -> Option<String> {
    let text = text
        .trim()
        .trim_end_matches(['*', '&'])
        .trim()
        .trim_matches(['"', '\''])
        .trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// What a method reaches for. Counting the receiver against everything else is what
/// Feature Envy actually is: a method more interested in another object than its own.
#[derive(Default)]
struct Reach {
    own: u32,
    foreign: HashMap<String, u32>,
    reads: HashSet<String>,
    /// Fields the body tests before deciding what to do, which is how a method says
    /// it must not be called until something else has run.
    guards: HashSet<String>,
    /// `self.x = …`, which is how a language with no field syntax declares state.
    assigned: Vec<(String, Span)>,
    /// Accesses that are not assignment targets. A member only ever written to is
    /// unreachable in the sense the rule means, even though it is mentioned.
    read_only: HashSet<String>,
}

struct Collector<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    envy_ratio: u32,
    /// The name the method currently being read calls its own object.
    receiver: Option<String>,
    reports: Vec<ClassReport>,
}

impl<'a> Collector<'a> {
    fn text(&self, node: Node<'a>) -> &'a str {
        text_of(node, self.source)
    }

    fn is_receiver(&self, node: Node<'a>) -> bool {
        let name = self.text(node);
        self.profile.self_names.contains(&name) || self.receiver.as_deref() == Some(name)
    }

    /// A method written with an explicit receiver declares its own name for `this`.
    /// The receiver list comes before the method's own, so a second parameter list is
    /// what distinguishes the two.
    fn receiver_name(&self, member: Node<'a>) -> Option<String> {
        let lists = self.parameter_lists(member);
        let receiver = lists.first().filter(|_| lists.len() > 1)?;
        let mut cursor = receiver.walk();
        let declaration = receiver.named_children(&mut cursor).next()?;
        let mut inner = declaration.walk();
        declaration
            .named_children(&mut inner)
            .find(|child| self.profile.is_identifier(*child))
            .map(|child| self.text(child).to_owned())
    }

    fn parameter_lists(&self, member: Node<'a>) -> Vec<Node<'a>> {
        find_within(
            member,
            self.profile.parameter_list_kinds,
            self.profile.block_kinds,
        )
    }

    /// A call and a field access are both member accesses in some grammars but name
    /// their member through different fields —
    /// Java's `this.total` uses `field` and its `this.count()` uses `name` — and a
    /// call on the receiver is a use of that member either way.
    fn property_of(&self, node: Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name(self.profile.property_field)
            .or_else(|| node.child_by_field_name("name"))
    }

    /// `this.ready` and `!this.ready` both say the same thing: this method refuses to
    /// run until something else has happened.
    fn guarded_field(&self, test: Node<'a>) -> Option<String> {
        let mut current = test;
        loop {
            if self.profile.is_member_access(current) {
                let object = current.child_by_field_name(self.profile.object_field)?;
                if self.is_receiver(object) {
                    return Some(self.text(self.property_of(current)?).to_owned());
                }
                return None;
            }
            let operator_is_negation = operator_of(current, self.source)
                .is_some_and(|operator| self.profile.negation_operators.contains(&operator));
            if operator_is_negation || self.profile.transparent_kinds.contains(&current.kind()) {
                current = current.named_child(current.named_child_count().saturating_sub(1))?;
                continue;
            }
            return None;
        }
    }

    fn reach(&self, body: Node<'a>) -> Reach {
        let mut reach = Reach::default();
        for node in descendants(body) {
            if node.kind() == self.profile.if_kind
                && let Some(condition) = node.child_by_field_name(self.profile.condition_field)
                && let Some(field) = self.guarded_field(condition)
            {
                reach.guards.insert(field);
            }
            if self.profile.literal_field_kinds.contains(&node.kind())
                && let Some(key) = node.named_child(0)
            {
                reach.reads.insert(self.text(key).to_owned());
            }
            if !self.profile.is_member_access(node) {
                continue;
            }
            let (Some(object), Some(property)) = (
                node.child_by_field_name(self.profile.object_field),
                self.property_of(node),
            ) else {
                continue;
            };
            if self.is_receiver(object) {
                reach.own += 1;
                reach.reads.insert(self.text(property).to_owned());
                let assigns = node.parent().is_some_and(|parent| {
                    self.profile.assignment_kinds.contains(&parent.kind())
                        && parent.child_by_field_name("left") == Some(node)
                });
                if assigns {
                    reach
                        .assigned
                        .push((self.text(property).to_owned(), span_of(property)));
                } else {
                    reach.read_only.insert(self.text(property).to_owned());
                }
            } else if self.profile.is_identifier(object) {
                *reach
                    .foreign
                    .entry(self.text(object).to_owned())
                    .or_insert(0) += 1;
            }
        }
        reach
    }

    /// A body that only throws, or does nothing at all, is the shape of an inherited
    /// member the subclass cannot honour.
    fn refuses(&self, body: Option<Node<'a>>) -> bool {
        let Some(body) = body else { return false };
        let mut cursor = body.walk();
        let statements: Vec<Node<'a>> = body
            .named_children(&mut cursor)
            .filter(|child| !self.profile.is_comment(*child))
            .collect();
        match statements.as_slice() {
            [] => true,
            [only] => self.profile.throw_kinds.contains(&only.kind()),
            _ => false,
        }
    }

    /// `return this.other.thing(...)` or `return this.other.thing` and nothing else —
    /// a property pass-through forwards just as much as a call does.
    fn delegates(&self, body: Option<Node<'a>>) -> bool {
        let Some(body) = body else { return false };
        let mut cursor = body.walk();
        let statements: Vec<Node<'a>> = body
            .named_children(&mut cursor)
            .filter(|child| !self.profile.is_comment(*child))
            .collect();
        let [statement] = statements.as_slice() else {
            return false;
        };
        let Some(value) = statement.named_child(0) else {
            return false;
        };
        // A `return` may hold a list of values in some grammars; delegation is the
        // single value inside it.
        let value = if self.profile.expression_list_kinds.contains(&value.kind()) {
            value.named_child(0).unwrap_or(value)
        } else {
            value
        };
        let target = if self.profile.is_member_access(value) {
            value
        } else if self.profile.is_call(value) {
            match value.child_by_field_name(self.profile.callee_field) {
                Some(function) => function,
                None => return false,
            }
        } else {
            value
        };
        // Two hops from the receiver: `this.inner.thing`. One hop is the object's own
        // state, which is not delegation.
        if !self.profile.is_member_access(target) {
            return false;
        }
        target
            .child_by_field_name(self.profile.object_field)
            .filter(|inner| self.profile.is_member_access(*inner))
            .and_then(|inner| inner.child_by_field_name(self.profile.object_field))
            .is_some_and(|receiver| self.is_receiver(receiver))
    }

    fn member_name(&self, member: Node<'a>) -> Option<String> {
        if let Some(name) = member.child_by_field_name("name") {
            return Some(self.text(name).to_owned());
        }
        if let Some(name) = self.declared_name(member) {
            return Some(self.text(name).to_owned());
        }
        if let Some(name) = self.bare_name(member) {
            return Some(self.text(name).to_owned());
        }
        // A field written `int x = 1;` names itself inside a declarator rather than
        // beside the type.
        let mut cursor = member.walk();
        member
            .named_children(&mut cursor)
            .find(|child| self.profile.name_holder_kinds.contains(&child.kind()))
            .and_then(|holder| holder.child_by_field_name("name"))
            .map(|name| self.text(name).to_owned())
    }

    fn member_name_span(&self, member: Node<'a>) -> Span {
        if let Some(name) = member.child_by_field_name("name") {
            return span_of(name);
        }
        if let Some(name) = self.declared_name(member) {
            return span_of(name);
        }
        if let Some(name) = self.bare_name(member) {
            return span_of(name);
        }
        let mut cursor = member.walk();
        member
            .named_children(&mut cursor)
            .find(|child| self.profile.name_holder_kinds.contains(&child.kind()))
            .and_then(|holder| holder.child_by_field_name("name"))
            .map_or(span_of(member), span_of)
    }

    fn is_private(&self, member: Node<'a>) -> bool {
        let mut cursor = member.walk();
        member.children(&mut cursor).any(|child| {
            self.profile.private_markers.contains(&child.kind())
                || self.profile.private_markers.contains(&self.text(child))
        }) || self.member_name(member).is_some_and(|name| {
            self.profile
                .private_name_prefixes
                .iter()
                .any(|p| name.starts_with(p))
        })
    }

    fn has_modifier(&self, member: Node<'a>, markers: &[&str]) -> bool {
        let mut cursor = member.walk();
        let children: Vec<Node<'a>> = member.children(&mut cursor).collect();
        let matches =
            |node: &Node<'a>| markers.contains(&node.kind()) || markers.contains(&self.text(*node));
        if children.iter().any(matches) {
            return true;
        }
        // A language that gathers modifiers into one node keeps them a level below.
        for container in children.iter().filter(|child| {
            self.profile
                .modifier_container_kinds
                .contains(&child.kind())
        }) {
            let mut inner = container.walk();
            if container.children(&mut inner).any(|child| matches(&child)) {
                return true;
            }
        }
        self.decorators(member)
            .iter()
            .any(|name| markers.contains(&name.as_str()))
    }

    /// `@staticmethod` and `@property` are written above the definition, on a wrapper
    /// node, so a modifier check that only looks at the member itself misses them.
    fn decorators(&self, member: Node<'a>) -> Vec<String> {
        let Some(parent) = member.parent() else {
            return Vec::new();
        };
        if !self.profile.decorated_kinds.contains(&parent.kind()) {
            return Vec::new();
        }
        let mut cursor = parent.walk();
        parent
            .named_children(&mut cursor)
            .filter(|child| self.profile.decorator_kinds.contains(&child.kind()))
            .map(|child| {
                self.text(child)
                    .trim_start_matches('@')
                    .split('(')
                    .next()
                    .unwrap_or("")
                    .to_owned()
            })
            .collect()
    }

    /// The run of statements a member holds. Some grammars put a statement list
    /// inside the braces, so the braces are not where the statements are.
    fn body_of(&self, member: Node<'a>) -> Option<Node<'a>> {
        let body = member.child_by_field_name("body")?;
        if self.profile.is_block(body) {
            return Some(body);
        }
        let mut cursor = body.walk();
        let inner = body
            .named_children(&mut cursor)
            .find(|child| self.profile.is_block(*child));
        Some(inner.unwrap_or(body))
    }

    fn parameters(&self, member: Node<'a>) -> Vec<Node<'a>> {
        // The method's own parameters are the last list: anything before it is a
        // receiver, which is not an argument a caller passes.
        let Some(list) = self.parameter_lists(member).into_iter().next_back() else {
            return Vec::new();
        };
        let mut inner = list.walk();
        list.named_children(&mut inner)
            .filter(|child| self.profile.parameter_kinds.contains(&child.kind()))
            .collect()
    }

    /// The type written on a declaration. TypeScript and Python wrap it in a
    /// `type_annotation`; the other three put the type node straight in the field.
    fn type_text(&self, node: Node<'a>) -> String {
        let inner = if node.kind() == "type_annotation" {
            node.named_child(0).unwrap_or(node)
        } else {
            node
        };
        // A forward reference is written as a string, and `"Palette"` names the same
        // type as `Palette`.
        self.text(inner).trim().trim_matches(['"', '\'']).to_owned()
    }

    fn declared_type(&self, member: Node<'a>) -> Option<String> {
        // A class body with no field syntax spells a field as a statement wrapping an
        // assignment, and it is the assignment that carries the annotation.
        member
            .child_by_field_name("type")
            .or_else(|| member.named_child(0)?.child_by_field_name("type"))
            .map(|node| self.type_text(node))
            .filter(|text| !text.is_empty())
    }

    /// Each grammar names the field one of these three, and none of them names it
    /// twice.
    fn declared_return(&self, member: Node<'a>) -> Option<String> {
        let node = ["return_type", "result", "type"]
            .into_iter()
            .find_map(|field| member.child_by_field_name(field))?;
        let text = self.type_text(node);
        (!text.is_empty()).then_some(text)
    }

    fn first_parameter_type(&self, member: Node<'a>) -> Option<&'a str> {
        let parameter = self.parameters(member).into_iter().next()?;
        let annotation = descendants(parameter)
            .into_iter()
            .find(|child| self.profile.type_annotation_kinds.contains(&child.kind()))?;
        Some(
            self.text(annotation.named_child(0).unwrap_or(annotation))
                .trim_matches(['"', '\'']),
        )
    }

    /// A class-level `name = value`, where the language has no field syntax of its
    /// own. The receiver form — `self.name = value` inside a method — is picked up
    /// from the method's reach instead.
    fn assigned_name(&self, member: Node<'a>) -> Option<(String, Span)> {
        let mut cursor = member.walk();
        let assignment = member
            .named_children(&mut cursor)
            .find(|child| self.profile.assignment_kinds.contains(&child.kind()))?;
        let target = assignment.child_by_field_name("left")?;
        self.profile
            .is_identifier(target)
            .then(|| (self.text(target).to_owned(), span_of(target)))
    }

    /// The name a declarator holds, for languages that wrap it rather than naming the
    /// declaration directly.
    fn declared_name(&self, member: Node<'a>) -> Option<Node<'a>> {
        if self.profile.declarator_wrapper_kinds.is_empty() {
            return None;
        }
        let named_by = |node: Node<'a>| {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).find(|child| {
                self.profile.is_identifier(*child)
                    || self.profile.property_kinds.contains(&child.kind())
            })
        };
        // Declarators nest through their own `declarator` field: a method returning a
        // pointer is a `pointer_declarator` around the `function_declarator` that
        // holds the name, and only the innermost one names anything.
        let mut current = member.child_by_field_name("declarator");
        while let Some(node) = current {
            if let Some(name) = named_by(node) {
                return Some(name);
            }
            current = node.child_by_field_name("declarator");
        }
        find_within(
            member,
            self.profile.declarator_wrapper_kinds,
            self.profile.block_kinds,
        )
        .into_iter()
        .find_map(named_by)
    }

    /// A member written with a parameter list is a method, even where the grammar
    /// gives it the same node kind as a field.
    fn declares_a_signature(&self, member: Node<'a>) -> bool {
        !self.profile.declarator_wrapper_kinds.is_empty()
            && !find_within(
                member,
                self.profile.parameter_list_kinds,
                self.profile.block_kinds,
            )
            .is_empty()
    }

    /// A member that spells its name beside its type rather than in a `name` field.
    fn bare_name(&self, member: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = member.walk();
        member
            .named_children(&mut cursor)
            .find(|child| self.profile.property_kinds.contains(&child.kind()))
    }

    /// A field given a value where it is declared is set for the object's whole life.
    fn has_initialiser(&self, member: Node<'a>) -> bool {
        if member.child_by_field_name("value").is_some() {
            return true;
        }
        let mut cursor = member.walk();
        member
            .named_children(&mut cursor)
            .filter(|child| self.profile.name_holder_kinds.contains(&child.kind()))
            .any(|holder| holder.child_by_field_name("value").is_some())
    }

    fn is_private_name(&self, name: &str) -> bool {
        self.profile
            .private_name_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
    }

    fn examine(&mut self, unit: &ClassUnit<'a>) {
        let name = unit.name.clone();
        let has_heritage = unit.has_heritage;

        let mut report = ClassReport {
            name: name.clone(),
            span: unit.span,
            fields: 0,
            methods: 0,
            real_methods: 0,
            trivial: 0,
            public_members: 0,
            delegating: 0,
            has_heritage,
            refused: Vec::new(),
            dead_private: Vec::new(),
            temporary_fields: Vec::new(),
            derived_references: Vec::new(),
            static_on_own_type: Vec::new(),
            envious: Vec::new(),
            guarded_fields: Vec::new(),
            misleading_name: None,
            method_names: Vec::new(),
            arities: Vec::new(),
            held: Vec::new(),
            declared: Vec::new(),
            supertypes: unit.supertypes.clone(),
        };

        let mut private_members: Vec<(String, Span)> = Vec::new();
        let mut read_anywhere: HashSet<String> = HashSet::new();
        let mut field_users: HashMap<String, Vec<String>> = HashMap::new();
        let mut constructor_writes: HashSet<String> = HashSet::new();
        let mut guard_users: HashMap<String, Vec<String>> = HashMap::new();
        let mut method_names: Vec<String> = Vec::new();
        let mut declared_fields: HashMap<String, Span> = HashMap::new();

        let members: Vec<Node<'a>> = unit
            .members
            .iter()
            .map(|member| {
                if self.profile.decorated_kinds.contains(&member.kind()) {
                    member
                        .named_child(member.named_child_count().saturating_sub(1))
                        .unwrap_or(*member)
                } else {
                    *member
                }
            })
            .collect();

        // A positional access label applies to everything after it, so visibility is
        // carried forward rather than read off each member.
        let mut section_private = self.profile.default_members_are_private;
        for member in members {
            let kind = member.kind();
            if self.profile.access_section_kinds.contains(&kind) {
                section_private = self.text(member).trim_end_matches(':').trim() == "private";
                continue;
            }
            if self.profile.assigned_field_kinds.contains(&kind) {
                if let Some((field, span)) = self.assigned_name(member) {
                    report.fields += 1;
                    declared_fields.insert(field.clone(), span);
                    let is_public = !self.is_private_name(&field);
                    let type_name = self.declared_type(member);
                    report.held.push(Field {
                        name: field.clone(),
                        element_type: type_name.as_deref().and_then(element_of),
                        type_name,
                        is_static: self.has_modifier(member, self.profile.static_markers),
                        is_public,
                        span,
                    });
                    if is_public {
                        report.public_members += 1;
                    } else {
                        private_members.push((field, span));
                    }
                }
                continue;
            }
            if self.profile.field_kinds.contains(&kind) && !self.declares_a_signature(member) {
                report.fields += 1;
                let Some(field) = self.member_name(member) else {
                    continue;
                };
                let type_name = self.declared_type(member);
                report.held.push(Field {
                    name: field.clone(),
                    element_type: type_name.as_deref().and_then(element_of),
                    type_name,
                    is_static: self.has_modifier(member, self.profile.static_markers),
                    is_public: !(section_private || self.is_private(member)),
                    span: span_of(member),
                });
                // A field with an initialiser is set for the object's whole life,
                // which is the opposite of a temporary field.
                if !self.has_initialiser(member) {
                    declared_fields.insert(field.clone(), span_of(member));
                }
                if section_private || self.is_private(member) {
                    private_members.push((field, span_of(member)));
                } else {
                    report.public_members += 1;
                }
                continue;
            }
            // What is a member at all is decided when the unit is assembled, so a
            // function that got here belongs to this class however it is written.
            if !self.profile.method_kinds.contains(&kind) && !self.profile.is_function(member) {
                continue;
            }
            if self.profile.field_kinds.contains(&kind) && !self.declares_a_signature(member) {
                continue;
            }
            report.methods += 1;
            let Some(method) = self.member_name(member) else {
                continue;
            };
            let key = self.member_name_span(member);
            // A C++ constructor is a member named after its class, which is the only
            // thing that marks it; the profile has no keyword to declare.
            let is_constructor = method == name
                || self.profile.constructor_names.contains(&method.as_str())
                || self.profile.constructor_kinds.contains(&kind)
                || self
                    .profile
                    .constructor_name_prefixes
                    .iter()
                    .any(|prefix| method == format!("{prefix}{name}"));
            let accessor = self.has_modifier(member, self.profile.accessor_markers);

            if section_private || self.is_private(member) {
                if !is_constructor {
                    private_members.push((method.clone(), key));
                }
            } else {
                report.public_members += 1;
            }

            self.receiver = self.receiver_name(member);
            let body = self.body_of(member);
            if accessor || self.delegates(body) {
                report.trivial += 1;
            }
            if self.delegates(body) && !is_constructor {
                report.delegating += 1;
            }
            if !is_constructor {
                report.real_methods += 1;
            }
            if has_heritage && !is_constructor && self.refuses(body) {
                report.refused.push(Named {
                    name: method.clone(),
                    span: key,
                });
            }
            if self.has_modifier(member, self.profile.static_markers)
                && self.first_parameter_type(member) == Some(name.as_str())
            {
                report.static_on_own_type.push(Named {
                    name: method.clone(),
                    span: key,
                });
            }

            // A constructor parameter carrying a visibility modifier declares a field
            // as well as a parameter, and it is how most TypeScript states what it
            // holds. Nothing else in the class body mentions it.
            if is_constructor {
                for parameter in self.parameters(member) {
                    if !self.has_modifier(parameter, self.profile.private_markers)
                        && !self.has_modifier(parameter, self.profile.field_declaring_modifiers)
                    {
                        continue;
                    }
                    // A parameter binds its name through `pattern`, where a member
                    // binds it through `name`.
                    let Some(field) = parameter
                        .child_by_field_name("pattern")
                        .map(|node| self.text(node).to_owned())
                        .or_else(|| self.member_name(parameter))
                    else {
                        continue;
                    };
                    let type_name = self.declared_type(parameter);
                    report.fields += 1;
                    report.held.push(Field {
                        name: field,
                        element_type: type_name.as_deref().and_then(element_of),
                        type_name,
                        is_static: false,
                        is_public: !self.has_modifier(parameter, self.profile.private_markers),
                        span: span_of(parameter),
                    });
                }
            }

            let reach = body.map(|body| self.reach(body)).unwrap_or_default();
            report.declared.push(Method {
                name: method.clone(),
                returns: self.declared_return(member),
                arity: self.parameters(member).len() as u32,
                is_static: self.has_modifier(member, self.profile.static_markers),
                is_constructor,
                is_abstract: body.is_none()
                    || self.has_modifier(member, self.profile.abstract_markers),
                is_public: !(section_private || self.is_private(member)),
                calls_own: reach.reads.iter().cloned().collect(),
                span: key,
            });
            if !is_constructor {
                method_names.push(method.clone());
                report.arities.push(self.parameters(member).len() as u32);
                for field in &reach.guards {
                    guard_users
                        .entry(field.clone())
                        .or_default()
                        .push(method.clone());
                }
            }
            if self.profile.assigned_field_kinds.is_empty() {
                read_anywhere.extend(reach.reads.iter().cloned());
            } else {
                read_anywhere.extend(reach.read_only.iter().cloned());
            }
            for field in &reach.reads {
                field_users
                    .entry(field.clone())
                    .or_default()
                    .push(method.clone());
            }
            if is_constructor {
                constructor_writes.extend(reach.reads.iter().cloned());
            }
            if !self.profile.assigned_field_kinds.is_empty() {
                for (field, span) in &reach.assigned {
                    if declared_fields.insert(field.clone(), *span).is_none() {
                        report.fields += 1;
                        if self.is_private_name(field) {
                            private_members.push((field.clone(), *span));
                        } else {
                            report.public_members += 1;
                        }
                    }
                }
            }

            if !is_constructor && !accessor {
                // `max_by_key` over a map resolves ties by hash order, which is not
                // stable across runs; the name breaks them instead.
                if let Some((owner, foreign)) = reach
                    .foreign
                    .iter()
                    .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
                    && *foreign > reach.own.max(1) * self.envy_ratio
                {
                    report.envious.push(Envy {
                        method: method.clone(),
                        owner: owner.clone(),
                        foreign: *foreign,
                        own: reach.own,
                        span: key,
                    });
                }
            }
        }

        for (member, span) in private_members {
            if !read_anywhere.contains(&member) {
                report.dead_private.push(Named { name: member, span });
            }
        }

        // A pattern name is a promise about the interface. Only two are unambiguous
        // enough to check: a factory makes things, a builder builds one.
        let promise = |suffix: &str, verbs: &[&str]| {
            name.ends_with(suffix)
                && name.len() > suffix.len()
                && !method_names.iter().any(|method| {
                    verbs
                        .iter()
                        .any(|verb| method.to_ascii_lowercase().starts_with(verb))
                })
        };
        if promise("Factory", &["create", "make", "build", "of", "from", "new"])
            || promise("Builder", &["build", "with", "add", "set"])
        {
            report.misleading_name = Some(Named {
                name: name.clone(),
                span: report.span,
            });
        }

        report.arities.sort_unstable();
        report.method_names = method_names;

        for (field, guards) in guard_users {
            if guards.len() >= 2 && !constructor_writes.contains(&field) {
                let span = declared_fields.get(&field).copied().unwrap_or(report.span);
                report.guarded_fields.push(Pair {
                    name: field,
                    other: guards.join(", "),
                    span,
                });
            }
        }

        for (field, users) in field_users {
            // The field's own declaration, not the class's: a finding that names a
            // field should point at it, and sharing one span across several findings
            // makes them indistinguishable to any stable ordering.
            if let Some(span) = declared_fields.get(&field)
                && users.len() == 1
                && !constructor_writes.contains(&field)
            {
                report.temporary_fields.push(Pair {
                    name: field,
                    other: users[0].clone(),
                    span: *span,
                });
            }
        }
        self.receiver = None;
        report.dead_private.sort_by_key(|member| member.span.start);
        report.temporary_fields.sort_by(|a, b| a.name.cmp(&b.name));
        report.guarded_fields.sort_by(|a, b| a.name.cmp(&b.name));
        self.reports.push(report);
    }
}

/// The default assembly: a class is one node with a body, and its members are what
/// that body holds.
pub fn units<'tree>(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'tree>,
) -> Vec<ClassUnit<'tree>> {
    let mut found = Vec::new();
    for class in descendants(root)
        .into_iter()
        .filter(|node| profile.is_class(*node))
    {
        let (Some(named), Some(body)) = (
            class.child_by_field_name("name"),
            class.child_by_field_name("body"),
        ) else {
            continue;
        };
        let mut cursor = class.walk();
        let has_heritage = class
            .named_children(&mut cursor)
            .any(|child| profile.heritage_kinds.contains(&child.kind()));
        let supertypes: Vec<String> = class
            .named_children(&mut class.walk())
            .filter(|child| profile.heritage_kinds.contains(&child.kind()))
            .flat_map(|clause| {
                descendants(clause)
                    .into_iter()
                    .filter(|node| profile.binding_identifier_kinds.contains(&node.kind()))
                    .map(|node| text_of(node, source).to_owned())
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut inner = body.walk();
        found.push(ClassUnit {
            name: text_of(named, source).to_owned(),
            span: span_of(named),
            members: body.named_children(&mut inner).collect(),
            has_heritage,
            supertypes,
        });
    }
    found
}

pub fn collect(
    profile: &'static LanguageProfile,
    source: &str,
    units: &[ClassUnit<'_>],
    envy_ratio: u32,
) -> Vec<ClassReport> {
    let mut collector = Collector {
        profile,
        source,
        envy_ratio: envy_ratio.max(1),
        receiver: None,
        reports: Vec::new(),
    };
    for unit in units {
        collector.examine(unit);
    }
    collector.reports
}

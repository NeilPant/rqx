//! Rules about the shape of classes and the members they declare.

use rqx_core::facts::Spot;
use rqx_core::rule::RuleId;
use tree_sitter::Node;

use crate::tree::{descendants, kind_of, span_of};

use super::Walker;

impl<'a> Walker<'a> {
    /// Ways out of one body, past the guard clauses. A `return` in the first few
    /// statements is a precondition a reader expects; the ones written deep in the
    /// body are the ones they have to hunt for.
    /// A body with nothing in it. An abstract declaration has no body at all, so it
    /// never reaches here; a constructor is exempt because an empty one is how several
    /// languages spell "the default, but private".
    pub(super) fn note_empty_method(&mut self, function: Node<'a>) {
        let profile = self.profile;
        if profile.constructor_kinds.contains(&kind_of(function))
            || self.initialises_outside_its_body(function)
        {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        // `empty_body_kinds` is how each language spells nothing: a comment
        // everywhere, and `pass` or `...` in Python.
        let empty = body
            .named_children(&mut body.walk())
            .all(|child| profile.empty_body_kinds.contains(&kind_of(child)));
        if !empty {
            return;
        }
        // An empty function literal passed as a callback is a deliberate no-op, and
        // being empty is the whole of what it says.
        let Some(name) = self.function_name(function) else {
            return;
        };
        let name = self.text(name).to_owned();
        self.out.empty_methods.push(Spot {
            span: span_of(function),
            detail: name,
            count: 1,
        });
    }

    /// A constructor whose work is written outside the braces: a C++ member
    /// initialiser list, `Gauge(int scale) : scale(scale) {}`, or TypeScript parameter
    /// properties, `constructor(private readonly clock: Clock) {}`.
    fn initialises_outside_its_body(&self, function: Node<'a>) -> bool {
        let profile = self.profile;
        let mut cursor = function.walk();
        if function
            .named_children(&mut cursor)
            .any(|child| kind_of(child) == "field_initializer_list")
        {
            return true;
        }
        crate::parameters::input_list(profile, function).is_some_and(|list| {
            descendants(list)
                .into_iter()
                .any(|node| profile.field_declaring_modifiers.contains(&kind_of(node)))
        })
    }

    /// A type that inherits and adds nothing, so every use of it could have used
    /// what it came from.
    pub(super) fn note_class_shape(&mut self, class: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.class_body(class) else {
            return;
        };
        let members: Vec<Node<'a>> = body
            .named_children(&mut body.walk())
            .filter(|child| !profile.is_comment(*child))
            .map(|child| {
                // A decorated member is wrapped, and the wrapper is not the member.
                if profile.decorated_kinds.contains(&kind_of(child)) {
                    child
                        .named_child(child.named_child_count().saturating_sub(1))
                        .unwrap_or(child)
                } else {
                    child
                }
            })
            .collect();
        let inherits = descendants(class).into_iter().any(|node| {
            node.start_byte() < body.start_byte() && profile.heritage_kinds.contains(&kind_of(node))
        });
        let Some(name) = self.class_name(class) else {
            return;
        };
        if inherits
            && members
                .iter()
                .all(|member| profile.empty_body_kinds.contains(&kind_of(member)))
        {
            self.out.empty_subclasses.push(Spot {
                span: span_of(class),
                detail: name.clone(),
                count: 1,
            });
        }
        let behaviour: Vec<Node<'a>> = members
            .iter()
            .copied()
            .filter(|member| profile.method_kinds.contains(&kind_of(member)))
            .collect();
        if !inherits
            && behaviour.len() > 1
            && behaviour.iter().all(|member| self.is_static(*member))
        {
            self.out.static_only_classes.push(Spot {
                span: span_of(class),
                detail: name,
                count: behaviour.len() as u32,
            });
            self.focus_on_name(RuleId::StaticOnlyClass, class);
        }
    }

    /// Construction that does work rather than only recording what it was given: the
    /// object cannot be made without whatever that work needs.
    pub(super) fn note_working_constructor(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let named = self.function_name(function).map(|node| self.text(node));
        let is_constructor = profile.constructor_kinds.contains(&kind_of(function))
            || named.is_some_and(|name| profile.constructor_names.contains(&name))
            || named
                .is_some_and(|name| self.enclosing_class_name(function).as_deref() == Some(name));
        if !is_constructor {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        // Handing the arguments to the parent's constructor is recording them too, so
        // `super(...)` and Python's `super().__init__(...)` are not work.
        // So is making an empty collection or a lock to keep them in.
        let is_own_or_parent = |name: &str| {
            let receiver = name.split('.').next().unwrap_or(name);
            profile.self_names.contains(&name) || profile.super_names.contains(&receiver)
        };
        let records = |call: Node<'a>| {
            self.callee_name(call)
                .is_some_and(|name| is_own_or_parent(&name))
                || self.builds_data_type(call)
        };
        let working = holder
            .named_children(&mut holder.walk())
            .filter(|statement| !profile.is_comment(*statement))
            .filter(|statement| {
                descendants(*statement)
                    .into_iter()
                    .any(|node| profile.is_call(node) && !records(node))
            })
            .count() as u32;
        if working == 0 {
            return;
        }
        self.out.working_constructors.push(Spot {
            span: span_of(function),
            detail: working.to_string(),
            count: working,
        });
    }

    /// A field filled with something the object built itself. Nothing can be put in
    /// its place, so nothing can be tested without the real thing.
    pub(super) fn note_self_built_collaborator(&mut self, assignment: Node<'a>) {
        let profile = self.profile;
        let Some(target) = assignment.child_by_field_name("left") else {
            return;
        };
        if !profile.is_member_access(target) {
            return;
        }
        let reaches_self = target
            .child_by_field_name(profile.object_field)
            .map(|object| self.text(object))
            .is_some_and(|object| profile.self_names.contains(&object));
        if !reaches_self {
            return;
        }
        let Some(value) = assignment.child_by_field_name("right") else {
            return;
        };
        let value = self.see_through(value);
        let Some(built) = self.constructed_type(value) else {
            return;
        };
        if profile.data_types.contains(&unqualified_type(&built)) {
            return;
        }
        self.out.self_built_collaborators.push(Spot {
            span: span_of(assignment),
            detail: built,
            count: 1,
        });
    }

    /// Whether `node` makes one of the standard types that only hold data.
    fn builds_data_type(&self, node: Node<'a>) -> bool {
        self.constructed_type(node)
            .is_some_and(|built| self.profile.data_types.contains(&unqualified_type(&built)))
    }

    /// What a `new` builds, whichever field the grammar keeps it in — or, in a
    /// language with no `new`, a call on a name spelled like a type.
    fn constructed_type(&self, node: Node<'a>) -> Option<String> {
        if let Some(built) = self
            .profile
            .constructor_field
            .and_then(|field| node.child_by_field_name(field))
        {
            return self
                .callee_name(built)
                .or_else(|| Some(self.text(built).to_owned()));
        }
        let name = self.called_name(node)?;
        let tail = name.rsplit('.').next().unwrap_or(&name);
        tail.chars()
            .next()
            .is_some_and(|first| first.is_uppercase())
            .then(|| tail.to_owned())
    }

    /// Reading a member the other type marked as its own business.
    pub(super) fn note_private_reach(&mut self, access: Node<'a>) {
        let profile = self.profile;
        if profile.private_name_prefixes.is_empty() {
            return;
        }
        let Some(property) = self.member_property(access) else {
            return;
        };
        let name = self.text(property);
        if !profile
            .private_name_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            return;
        }
        let receiver = access
            .child_by_field_name(profile.object_field)
            .map(|object| self.text(object));
        // The receiver is the only one entitled to it, and so is a name that is
        // itself private.
        if receiver.is_none_or(|object| {
            profile.self_names.contains(&object)
                || profile
                    .private_name_prefixes
                    .iter()
                    .any(|prefix| object.starts_with(prefix))
        }) {
            return;
        }
        self.out.private_reaches.push(Spot {
            span: span_of(access),
            detail: name.to_owned(),
            count: 1,
        });
    }

    /// A type declared abstract that leaves nothing for anyone to fill in.
    pub(super) fn note_empty_abstraction(&mut self, class: Node<'a>) {
        let profile = self.profile;
        if profile.abstract_markers.is_empty() {
            return;
        }
        let mut cursor = class.walk();
        let marked = class
            .children(&mut cursor)
            .any(|node| profile.abstract_markers.contains(&self.text(node)));
        if !marked {
            return;
        }
        let Some(body) = self.class_body(class) else {
            return;
        };
        let unfinished = descendants(body).into_iter().any(|node| {
            let mut inner = node.walk();
            profile.method_kinds.contains(&kind_of(node))
                && (node
                    .children(&mut inner)
                    .any(|child| profile.abstract_markers.contains(&self.text(child)))
                    || profile.signature_kinds.contains(&kind_of(node)))
        }) || descendants(body)
            .into_iter()
            .any(|node| profile.signature_kinds.contains(&kind_of(node)));
        if unfinished {
            return;
        }
        let Some(name) = self.class_name(class) else {
            return;
        };
        self.out.empty_abstractions.push(Spot {
            span: span_of(class),
            detail: name,
            count: 1,
        });
    }

    /// A field only one method ever reads. It is that method's local, kept where
    /// every other method can see it.
    pub(super) fn note_lonely_fields(&mut self, class: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.class_body(class) else {
            return;
        };
        let members: Vec<Node<'a>> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.method_kinds.contains(&kind_of(child)))
            .collect();
        // A type whose methods are declared here and written elsewhere cannot say who
        // reads a field: most of its behaviour is in another file.
        if members.len() < 2
            || members
                .iter()
                .any(|member| self.function_body(*member).is_none())
        {
            return;
        }
        let declared: Vec<(String, Node<'a>)> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.field_kinds.contains(&kind_of(child)))
            .filter_map(|field| {
                // The declared name, not the type beside it: both are identifiers
                // under the same node in most of these grammars.
                let holder = field
                    .named_children(&mut field.walk())
                    .find(|child| profile.declarator_kinds.contains(&kind_of(child)))
                    .unwrap_or(field);
                let spells_a_name = |node: &Node<'a>| {
                    profile.is_identifier(*node) || profile.property_kinds.contains(&kind_of(node))
                };
                let name = profile
                    .binding_fields
                    .iter()
                    .find_map(|binding| holder.child_by_field_name(binding))
                    .filter(spells_a_name)
                    .or_else(|| descendants(holder).into_iter().find(spells_a_name))?;
                Some((self.text(name).to_owned(), name))
            })
            .collect();
        for (name, at) in declared {
            let readers = members
                .iter()
                .filter(|member| {
                    descendants(**member)
                        .into_iter()
                        .any(|node| profile.is_identifier(node) && self.text(node) == name)
                })
                .count();
            if readers == 1 {
                self.out.lonely_fields.push(Spot {
                    span: span_of(at),
                    detail: name,
                    count: 1,
                });
            }
        }
    }

    /// How many other types one type has to know about to do its job.
    pub(super) fn note_many_collaborators(&mut self, class: Node<'a>) {
        let profile = self.profile;
        let Some(name) = self.class_name(class) else {
            return;
        };
        let mut known: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for node in descendants(class) {
            if let Some(built) = self.constructed_type(node) {
                known.insert(built);
                continue;
            }
            if profile.type_annotation_kinds.contains(&kind_of(node))
                && let Some(inner) = node.named_child(0)
                && profile.binding_identifier_kinds.contains(&kind_of(inner))
            {
                known.insert(self.text(inner).to_owned());
            }
        }
        known.remove(&name);
        let count = known.len() as u32;
        if count > self.limits.class_collaborators {
            self.out.many_collaborators.push(Spot {
                span: span_of(class),
                detail: name,
                count,
            });
            self.focus_on_name(RuleId::ManyCollaborators, class);
        }
    }

    /// A function whose whole body hands back something the file keeps at module
    /// level.
    pub(super) fn note_global_accessor(&mut self, function: Node<'a>, shared: &[String]) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        let mut cursor = holder.walk();
        let mut statements = holder
            .named_children(&mut cursor)
            .filter(|child| !profile.is_comment(*child));
        let Some(only) = statements.next() else {
            return;
        };
        if statements.next().is_some() || kind_of(only) != "return_statement" {
            return;
        }
        let Some(mut answer) = only.named_child(0).map(|node| self.see_through(node)) else {
            return;
        };
        while profile.expression_list_kinds.contains(&kind_of(answer))
            && answer.named_child_count() == 1
        {
            let Some(inner) = answer.named_child(0) else {
                break;
            };
            answer = self.see_through(inner);
        }
        if !profile.is_identifier(answer) {
            return;
        }
        let name = self.text(answer).to_owned();
        if !shared.contains(&name) {
            return;
        }
        self.out.global_accessors.push(Spot {
            span: self.name_span(function),
            detail: name,
            count: 1,
        });
    }

    /// A member that overrides something and then calls the thing it overrode,
    /// changing nothing on the way.
    pub(super) fn note_passthrough_override(&mut self, function: Node<'a>) {
        let profile = self.profile;
        if profile.super_names.is_empty() {
            return;
        }
        let Some(name) = self.function_name(function).map(|node| self.text(node)) else {
            return;
        };
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        let mut cursor = holder.walk();
        let mut statements = holder
            .named_children(&mut cursor)
            .filter(|child| !profile.is_comment(*child));
        let Some(only) = statements.next() else {
            return;
        };
        if statements.next().is_some() {
            return;
        }
        let Some(call) = self.only_call(only) else {
            return;
        };
        let Some(dotted) = self.called_name(call) else {
            return;
        };
        let mut parts = dotted.rsplitn(2, '.');
        let tail = parts.next().unwrap_or_default();
        let receiver = parts.next().unwrap_or_default();
        if tail != name || !profile.super_names.contains(&receiver) {
            return;
        }
        self.out.passthrough_overrides.push(Spot {
            span: self.name_span(function),
            detail: name.to_owned(),
            count: 1,
        });
    }

    /// A type whose methods fall into groups that share no state. Each group is a
    /// type somebody has not separated yet.
    pub(super) fn note_split_cohesion(&mut self, class: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.class_body(class) else {
            return;
        };
        let fields: Vec<String> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.field_kinds.contains(&kind_of(child)))
            .filter_map(|field| self.declared_field_name(field))
            .collect();
        if fields.len() < 2 {
            return;
        }
        let touched: Vec<std::collections::BTreeSet<&str>> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.method_kinds.contains(&kind_of(child)))
            .map(|member| {
                let names: Vec<&str> = descendants(member)
                    .into_iter()
                    .filter(|node| {
                        profile.is_identifier(*node)
                            || profile.property_kinds.contains(&kind_of(node))
                    })
                    .map(|node| self.text(node))
                    .collect();
                fields
                    .iter()
                    .filter(|field| names.contains(&field.as_str()))
                    .map(String::as_str)
                    .collect()
            })
            .filter(|used: &std::collections::BTreeSet<&str>| !used.is_empty())
            .collect();
        if touched.len() < 2 {
            return;
        }
        // Two methods belong together when they share a field, directly or through a
        // third; the groups left over are the types inside this one.
        let mut groups: Vec<std::collections::BTreeSet<&str>> = Vec::new();
        for used in touched {
            let (mut joined, apart): (Vec<_>, Vec<_>) = groups
                .into_iter()
                .partition(|group| group.intersection(&used).next().is_some());
            let mut merged = used;
            for group in joined.drain(..) {
                merged.extend(group);
            }
            groups = apart;
            groups.push(merged);
        }
        if groups.len() < 2 {
            return;
        }
        let Some(name) = self.class_name(class) else {
            return;
        };
        self.out.split_cohesion.push(Spot {
            span: span_of(class),
            detail: name,
            count: groups.len() as u32,
        });
    }

    fn declared_field_name(&self, field: Node<'a>) -> Option<String> {
        let profile = self.profile;
        let spells_a_name = |node: &Node<'a>| {
            profile.is_identifier(*node) || profile.property_kinds.contains(&kind_of(node))
        };
        let holder = field
            .named_children(&mut field.walk())
            .find(|child| profile.declarator_kinds.contains(&kind_of(child)))
            .unwrap_or(field);
        profile
            .binding_fields
            .iter()
            .find_map(|binding| holder.child_by_field_name(binding))
            .filter(spells_a_name)
            .or_else(|| descendants(holder).into_iter().find(spells_a_name))
            .map(|node| self.text(node).to_owned())
    }

    /// A method that never mentions the instance it was written on.
    pub(super) fn note_instance_free_method(&mut self, function: Node<'a>) {
        let profile = self.profile;
        if profile.self_names.is_empty() || self.is_static(function) {
            return;
        }
        let inside_class = function
            .parent()
            .and_then(|parent| parent.parent())
            .is_some_and(|grand| profile.is_class(grand));
        if !inside_class || profile.constructor_kinds.contains(&kind_of(function)) {
            return;
        }
        let Some(name) = self
            .function_name(function)
            .map(|node| self.text(node).to_owned())
        else {
            return;
        };
        if profile.constructor_names.contains(&name.as_str()) {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        let mentions_self = descendants(body)
            .into_iter()
            .any(|node| profile.self_names.contains(&self.text(node)));
        if mentions_self {
            return;
        }
        self.out.instance_free_methods.push(Spot {
            span: self.name_span(function),
            detail: name,
            count: 1,
        });
    }

    /// A field the constructor sets and nothing else ever writes.
    pub(super) fn note_assign_once_fields(&mut self, class: Node<'a>) {
        let profile = self.profile;
        if profile.immutable_markers.is_empty() {
            return;
        }
        let Some(body) = self.class_body(class) else {
            return;
        };
        let fields: Vec<(String, Node<'a>)> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.field_kinds.contains(&kind_of(child)))
            .filter(|child| {
                let head = self.text(*child);
                !profile
                    .immutable_markers
                    .iter()
                    .any(|marker| head.split_whitespace().any(|word| word == *marker))
            })
            .filter_map(|field| Some((self.declared_field_name(field)?, field)))
            .collect();
        if fields.is_empty() {
            return;
        }
        let members: Vec<Node<'a>> = body
            .named_children(&mut body.walk())
            .filter(|child| profile.method_kinds.contains(&kind_of(child)))
            .collect();
        for (name, at) in fields {
            let written_outside = members.iter().any(|member| {
                let is_constructor = self
                    .function_name(*member)
                    .map(|node| self.text(node))
                    .is_some_and(|named| {
                        profile.constructor_names.contains(&named)
                            || self.enclosing_class_name(*member).as_deref() == Some(named)
                    })
                    || profile.constructor_kinds.contains(&kind_of(member));
                if is_constructor {
                    return false;
                }
                self.function_body(*member).is_some_and(|inner| {
                    self.touches(inner)
                        .get(name.as_str())
                        .is_some_and(|mentions| mentions.iter().any(|touch| touch.wrote))
                })
            });
            if !written_outside {
                self.out.assign_once_fields.push(Spot {
                    span: span_of(at),
                    detail: name,
                    count: 1,
                });
            }
        }
    }
}

/// A constructed type without its type arguments or its namespace: `std::vector<int>`,
/// `java.util.HashMap<>` and `collections.OrderedDict` name `vector`, `HashMap` and
/// `OrderedDict`.
fn unqualified_type(written: &str) -> &str {
    let bare = written.split('<').next().unwrap_or(written).trim();
    bare.rsplit(['.', ':']).next().unwrap_or(bare)
}

#[cfg(test)]
mod tests {
    use super::unqualified_type;

    #[test]
    fn a_type_is_named_without_its_arguments_or_namespace() {
        assert_eq!(unqualified_type("std::vector<int>"), "vector");
        assert_eq!(unqualified_type("java.util.HashMap<>"), "HashMap");
        assert_eq!(unqualified_type("collections.OrderedDict"), "OrderedDict");
        assert_eq!(unqualified_type("Set"), "Set");
    }
}

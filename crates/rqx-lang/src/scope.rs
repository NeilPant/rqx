use std::collections::HashMap;

use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{declares_through_list, field_name, span_of, text_of};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BindingKind {
    Parameter,
    Local,
    Member,
    Declaration,
    Import,
}

#[derive(Debug)]
pub struct Binding {
    pub name: String,
    pub span: Span,
    pub kind: BindingKind,
    pub scope: usize,
    /// Reads and writes that resolve here. A binding with none is unused; the count
    /// is kept because `short-name-wide-scope` needs the last one, not just whether
    /// there was one.
    pub references: Vec<Span>,
}

struct Scope {
    parent: Option<usize>,
    /// A name assigned at class level is a field, not a local, however the language
    /// spells it.
    holds_members: bool,
    /// Name to index into `bindings`. A name declared twice in one scope keeps the
    /// first, which is what a reader resolves to before the second line is reached.
    names: HashMap<String, usize>,
}

/// Bindings and the references that reach them, for one file. tree-sitter has no
/// symbol table, so this is the whole of what Rqx knows about names — which is why
/// every rule reading it says `heuristic` unless the binding could not have escaped.
pub struct Scopes {
    pub bindings: Vec<Binding>,
    /// The widest scope a name is visible in, for rules that ask how far a short
    /// name travels.
    scopes: Vec<Scope>,
}

impl Scopes {
    pub fn unused(&self) -> impl Iterator<Item = &Binding> {
        self.bindings
            .iter()
            .filter(|binding| binding.references.is_empty())
    }

    pub fn of_kind(&self, kind: BindingKind) -> impl Iterator<Item = &Binding> {
        self.bindings
            .iter()
            .filter(move |binding| binding.kind == kind)
    }

    pub fn depth(&self, scope: usize) -> usize {
        let mut depth = 0;
        let mut current = Some(scope);
        while let Some(index) = current {
            depth += 1;
            current = self.scopes[index].parent;
        }
        depth
    }
}

struct Builder<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    bindings: Vec<Binding>,
    scopes: Vec<Scope>,
    /// Deferred so that a reference textually above its declaration still resolves —
    /// a hoisted function, a method calling one declared below it.
    references: Vec<(String, Span, usize)>,
}

impl<'a> Builder<'a> {
    fn open(&mut self, parent: Option<usize>, holds_members: bool) -> usize {
        self.scopes.push(Scope {
            parent,
            names: HashMap::new(),
            holds_members,
        });
        self.scopes.len() - 1
    }

    fn declare(&mut self, node: Node<'a>, kind: BindingKind, scope: usize) {
        let name = text_of(node, self.source);
        if name.is_empty() {
            return;
        }
        // Where assignment declares, the second write is a write, not a second
        // binding — otherwise every reassignment reports itself as unused.
        if !self.profile.local_declaring_kinds.is_empty()
            && kind == BindingKind::Local
            && let Some(existing) = self.scopes[scope].names.get(name).copied()
        {
            self.bindings[existing].references.push(span_of(node));
            return;
        }
        let index = self.bindings.len();
        self.bindings.push(Binding {
            name: name.to_owned(),
            span: span_of(node),
            kind,
            scope,
            references: Vec::new(),
        });
        self.scopes[scope]
            .names
            .entry(name.to_owned())
            .or_insert(index);
    }

    fn binding_kind(&self, node: Node<'a>) -> Option<BindingKind> {
        if !self.profile.is_identifier(node) {
            return None;
        }
        let parent = node.parent()?;
        if self.profile.parameter_kinds.contains(&parent.kind()) {
            return Some(BindingKind::Parameter);
        }
        if self.profile.import_binding_kinds.contains(&parent.kind()) {
            return Some(BindingKind::Import);
        }
        // Languages with no declaration keyword say "this is a new name" by assigning
        // to it, which fills a `left` field rather than a `name` one.
        let assigns = declares_through_list(
            node,
            self.profile.local_declaring_kinds,
            self.profile.expression_list_kinds,
        );
        if !assigns
            && !matches!(field_name(node), Some(field) if self.profile.binding_fields.contains(&field))
        {
            return None;
        }
        if self.profile.declarator_kinds.contains(&parent.kind()) {
            return Some(BindingKind::Local);
        }
        if declares_through_list(
            node,
            self.profile.local_declaring_kinds,
            self.profile.expression_list_kinds,
        ) {
            return Some(BindingKind::Local);
        }
        if self.profile.field_kinds.contains(&parent.kind())
            || self.profile.method_kinds.contains(&parent.kind())
        {
            return Some(BindingKind::Member);
        }
        Some(BindingKind::Declaration)
    }

    /// A property name is not a reference to anything this file declares: `a.length`
    /// says nothing about a local called `length`.
    fn is_reference(&self, node: Node<'a>) -> bool {
        if self.profile.reference_only_kinds.contains(&node.kind()) {
            return true;
        }
        if !self.profile.is_identifier(node) {
            return false;
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        if self.profile.is_member_access(parent)
            && field_name(node).is_some_and(|field| field == "property")
        {
            return false;
        }
        true
    }

    /// A parameter belongs to the scope its function opens; the function's own name
    /// belongs to the scope the function is written in, or nothing outside could
    /// call it.
    fn owning_scope(&self, node: Node<'a>, kind: BindingKind, scope: usize) -> usize {
        if kind == BindingKind::Parameter {
            return scope;
        }
        let opens_own_scope = node
            .parent()
            .is_some_and(|parent| self.profile.scope_kinds.contains(&parent.kind()));
        if opens_own_scope {
            return self.scopes[scope].parent.unwrap_or(scope);
        }
        scope
    }

    fn walk(&mut self, node: Node<'a>, scope: usize) {
        let opens = self.profile.scope_kinds.contains(&node.kind());
        // A class body opens the scope its members live in, and the body is a node of
        // its own in most grammars, so the class is the parent rather than the opener.
        let holds_members = self.profile.is_class(node)
            || node
                .parent()
                .is_some_and(|parent| self.profile.is_class(parent));
        let inner = if opens {
            self.open(Some(scope), holds_members)
        } else {
            scope
        };

        // A parameter belongs to the scope the function opens, not the one the
        // function is written in, so the function node has to be entered first.
        if let Some(mut kind) = self.binding_kind(node) {
            let target = self.owning_scope(node, kind, scope);
            if kind == BindingKind::Local && self.scopes[target].holds_members {
                kind = BindingKind::Member;
            }
            self.declare(node, kind, target);
        } else if self.is_reference(node) {
            self.references
                .push((text_of(node, self.source).to_owned(), span_of(node), inner));
        }

        let mut cursor = node.walk();
        let children: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
        for child in children {
            self.walk(child, inner);
        }
    }

    fn resolve(mut self) -> Scopes {
        for (name, span, mut scope) in std::mem::take(&mut self.references) {
            loop {
                if let Some(index) = self.scopes[scope].names.get(&name) {
                    self.bindings[*index].references.push(span);
                    break;
                }
                match self.scopes[scope].parent {
                    Some(parent) => scope = parent,
                    None => break,
                }
            }
        }
        Scopes {
            bindings: self.bindings,
            scopes: self.scopes,
        }
    }
}

pub fn resolve(profile: &'static LanguageProfile, source: &str, root: Node<'_>) -> Scopes {
    let mut builder = Builder {
        profile,
        source,
        bindings: Vec::new(),
        scopes: Vec::new(),
        references: Vec::new(),
    };
    let file = builder.open(None, false);
    builder.walk(root, file);
    builder.resolve()
}

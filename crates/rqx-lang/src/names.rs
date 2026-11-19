use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{find_within, kind_of, text_of};

/// Reads the names a node is written with: the dotted callee of a call, the receiver
/// it is called on, a function's own name. Shared by the walkers that each ask what a
/// call is, so that every one of them spells `db.user.find` the same way.
#[derive(Clone, Copy)]
pub struct Names<'a> {
    pub profile: &'static LanguageProfile,
    pub source: &'a str,
}

impl<'a> Names<'a> {
    pub const fn new(profile: &'static LanguageProfile, source: &'a str) -> Self {
        Self { profile, source }
    }

    pub fn text(&self, node: Node<'a>) -> &'a str {
        text_of(node, self.source)
    }

    pub fn see_through(&self, node: Node<'a>) -> Node<'a> {
        let mut current = node;
        while self.profile.transparent_kinds.contains(&kind_of(current)) {
            match current.named_child(0) {
                Some(inner) => current = inner,
                None => break,
            }
        }
        current
    }

    /// The dotted name a call is written with, so `describe.skip` and `t.Skip` are
    /// one string a vocabulary can match.
    pub fn callee_name(&self, node: Node<'a>) -> Option<String> {
        self.callee_name_with(node, &mut |inner| self.callee_name(inner))
    }

    /// As [`Names::callee_name`], asking `inner_name` for the name of the receiver or
    /// callee a node is written on, so a caller can remember the names it has already
    /// worked out: along a chain of calls, every call's name holds the one before it.
    pub fn callee_name_with(
        &self,
        node: Node<'a>,
        inner_name: &mut dyn FnMut(Node<'a>) -> Option<String>,
    ) -> Option<String> {
        let node = self.see_through(node);
        if self.profile.is_identifier(node) {
            return Some(self.text(node).to_owned());
        }
        // `super` and `this` are keywords with nodes of their own in most grammars,
        // and both name the receiver a call is written on.
        let word = self.text(node);
        if self.profile.super_names.contains(&word) || self.profile.self_names.contains(&word) {
            return Some(word.to_owned());
        }
        if self.profile.is_member_access(node)
            && let Some(object) = node
                .child_by_field_name(self.profile.object_field)
                .and_then(&mut *inner_name)
            && let Some(property) = self.member_property(node)
        {
            return Some(format!("{object}.{}", self.text(property)));
        }
        if self.profile.qualified_name_kinds.contains(&kind_of(node)) {
            return Some(self.text(node).replace("::", "."));
        }
        // A call written on the answer of another call. Read from the callee rather
        // than from the whole call, so a grammar that gives a call and a member reach
        // the same node kind cannot send this round in a circle.
        if self.profile.is_call(node)
            && let Some(callee) = node.child_by_field_name(self.profile.callee_field)
            && callee.id() != node.id()
        {
            return inner_name(callee);
        }
        None
    }

    /// The member half of a reach: `a.b` and `a.b()` name it in different fields in
    /// some grammars, and both mean the same thing here.
    pub fn member_property(&self, node: Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name(self.profile.property_field)
            .or_else(|| node.child_by_field_name(self.profile.callee_field))
    }

    pub fn called_name(&self, call: Node<'a>) -> Option<String> {
        // A grammar that splits the receiver from the method name leaves the call
        // node itself as the only thing spelling the whole dotted name — but the same
        // node kind is used for a call with no receiver at all, which spells nothing.
        let callee = call.child_by_field_name(self.profile.callee_field)?;
        if self.profile.is_member_access(call)
            && let Some(dotted) = self.callee_name(call)
        {
            return Some(dotted);
        }
        self.callee_name(callee)
    }

    pub fn receiver_of<'d>(&self, dotted: &'d str) -> Option<&'d str> {
        dotted.rsplit_once('.').map(|(head, _)| head)
    }

    /// A vocabulary entry spelling a receiver has to match one; an entry that is a
    /// bare name matches any call whose last segment is it.
    pub fn answers_to(&self, dotted: &str, entry: &str) -> bool {
        if entry.contains('.') {
            return dotted == entry || dotted.ends_with(&format!(".{entry}"));
        }
        dotted.rsplit('.').next() == Some(entry)
    }

    pub fn matches_any(&self, dotted: &str, entries: &[&str]) -> bool {
        entries.iter().any(|entry| self.answers_to(dotted, entry))
    }

    /// A call that holds the thread until the work is done, where the vocabulary
    /// names it on a receiver or bare.
    pub fn is_blocking(&self, dotted: &str) -> bool {
        let perf = &self.profile.perf;
        let tail = dotted.rsplit('.').next().unwrap_or(dotted);
        let receiver = self.receiver_of(dotted).unwrap_or("");
        perf.blocking.iter().any(|entry| {
            if entry.contains('.') {
                self.answers_to(dotted, entry)
            } else {
                tail == *entry
                    && (receiver.is_empty()
                        || perf
                            .blocking_receivers
                            .contains(&receiver.rsplit('.').next().unwrap_or(receiver)))
            }
        })
    }

    /// The expressions a call is given, in order. Empty for a call with no list.
    pub fn arguments(&self, call: Node<'a>) -> Vec<Node<'a>> {
        let Some(list) = (0..call.named_child_count())
            .filter_map(|index| call.named_child(index))
            .find(|child| self.profile.argument_list_kinds.contains(&kind_of(child)))
        else {
            return Vec::new();
        };
        let mut cursor = list.walk();
        list.named_children(&mut cursor).collect()
    }

    /// A function may name itself directly or through a declarator wrapper.
    pub fn function_name(&self, node: Node<'a>) -> Option<Node<'a>> {
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

    /// The braces, or the expression an arrow function is written as.
    pub fn function_body(&self, node: Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body").or_else(|| {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find(|child| self.profile.is_block(*child))
        })
    }
}

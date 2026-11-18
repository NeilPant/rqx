//! Rules about calls and loops whose cost grows with their input.

use rqx_core::facts::Spot;
use tree_sitter::Node;

use crate::tree::{descendants, kind_of, operator_of, span_of};
use crate::vocabulary::{
    HASHED_HINTS, IO_BARE, KEYED_CONSTRUCTIONS, asks_for_data, mentions_any, reaches_outside,
};

use super::Walker;

impl<'a> Walker<'a> {
    /// Building a string one piece at a time inside a loop: every pass copies
    /// everything written so far.
    pub(super) fn note_loop_concatenation(&mut self, assignment: Node<'a>) {
        if self.loop_depth() == 0 {
            return;
        }
        let profile = self.profile;
        let Some(operator) = operator_of(assignment, self.source) else {
            return;
        };
        if operator != "+=" {
            return;
        }
        let Some(target) = assignment.child_by_field_name("left") else {
            return;
        };
        let name = self.text(target);
        // Either the declaration named a text type, or the value it started with was
        // a piece of text — a language with no type annotation only offers the second.
        let holds_text = self.declared.get(name).is_some_and(|introduced| {
            let lowered = introduced.to_lowercase();
            lowered.contains("string")
                || lowered.contains("str")
                || introduced.trim_start().starts_with(['"', '\'', '`'])
        }) || assignment
            .child_by_field_name("right")
            .is_some_and(|value| profile.string_kinds.contains(&kind_of(value)));
        if !holds_text {
            return;
        }
        self.out.loop_concatenations.push(Spot {
            span: span_of(assignment),
            detail: name.to_owned(),
            count: self.loop_depth(),
        });
    }

    /// What a call is written on: `orders.items.includes` answers `orders.items`.
    /// Go writes both sides of an assignment as a list even when it assigns one
    /// thing, so the single element is what the rules are about.
    fn unwrapped(&self, node: Node<'a>) -> Node<'a> {
        let node = self.see_through(node);
        if self.profile.expression_list_kinds.contains(&kind_of(node)) {
            return node.named_child(0).unwrap_or(node);
        }
        node
    }

    fn receiver_of<'d>(&self, dotted: &'d str) -> Option<&'d str> {
        self.names().receiver_of(dotted)
    }

    fn matches_any(&self, dotted: &str, entries: &[&str]) -> bool {
        self.names().matches_any(dotted, entries)
    }

    /// Everything known about how a name was introduced: its declared type and the
    /// head of whatever it was first set to.
    fn declaration_of(&self, name: &str) -> &str {
        self.declared.get(name).map_or("", String::as_str)
    }

    /// A value that already answers by key. Searching one costs nothing, so a scan
    /// over it is not the shape this rule is about.
    fn already_keyed(&self, receiver: &str) -> bool {
        let last = receiver.rsplit('.').next().unwrap_or(receiver);
        mentions_any(last, &HASHED_HINTS)
            || mentions_any(self.declaration_of(last), &KEYED_CONSTRUCTIONS)
            || mentions_any(self.declaration_of(receiver), &KEYED_CONSTRUCTIONS)
    }

    /// Searching a string is not searching a collection: `line.indexOf(":")` walks a
    /// handful of characters and answers a parsing question.
    fn holds_text(&self, receiver: &str) -> bool {
        let last = receiver.rsplit('.').next().unwrap_or(receiver);
        let declaration = self.declaration_of(last);
        declaration.starts_with('"')
            || declaration.starts_with('\'')
            || declaration.starts_with('`')
            || self
                .profile
                .perf
                .string_types
                .iter()
                .any(|kind| declaration.split_whitespace().any(|word| word == *kind))
    }

    /// A name in capitals is a fixed table somebody wrote out, and a literal list is
    /// as long as the source says it is. Neither grows with the input.
    fn fixed_size(&self, receiver: &str) -> bool {
        let last = receiver.rsplit('.').next().unwrap_or(receiver);
        let capitals = last.chars().any(|c| c.is_ascii_uppercase())
            && !last.chars().any(|c| c.is_ascii_lowercase());
        capitals || self.declaration_of(last).starts_with('[')
    }

    /// `Buffer.concat` and `Array.from` are called on a type rather than on something
    /// holding elements, so they walk their argument once instead of once per pass.
    fn names_a_type(&self, receiver: &str) -> bool {
        receiver
            .rsplit('.')
            .next()
            .unwrap_or(receiver)
            .starts_with(|first: char| first.is_ascii_uppercase())
    }

    fn worth_scanning(&self, receiver: &str) -> bool {
        !receiver.is_empty()
            && !self.already_keyed(receiver)
            && !self.holds_text(receiver)
            && !self.fixed_size(receiver)
            && !self.names_a_type(receiver)
    }

    fn arguments(&self, call: Node<'a>) -> Vec<Node<'a>> {
        self.names().arguments(call)
    }

    fn is_literal(&self, node: Node<'a>) -> bool {
        let kind = kind_of(node);
        self.profile.string_kinds.contains(&kind)
            || self.profile.number_kinds.contains(&kind)
            || kind == "regex"
    }

    /// The collection a free-function scan is given, which is its first argument
    /// however many iterators it was spelled with.
    fn scanned_argument(&self, call: Node<'a>) -> Option<String> {
        let first = self.arguments(call).into_iter().next()?;
        let named = self.callee_name(first).or_else(|| {
            self.profile
                .is_call(first)
                .then(|| self.called_name(first))
                .flatten()
        })?;
        Some(
            named
                .rsplit_once('.')
                .map_or(named.clone(), |(head, _)| head.to_owned()),
        )
    }

    /// Rules 1, 3, 5 and 8 all ask what a call is: one walk of the vocabulary rather
    /// than four.
    pub(super) fn note_perf_call(&mut self, node: Node<'a>) {
        let Some(dotted) = self
            .called_name(node)
            .or_else(|| self.constructed_name(node))
        else {
            return;
        };
        let perf = &self.profile.perf;
        let tail = dotted.rsplit('.').next().unwrap_or(&dotted).to_owned();
        let receiver = self.receiver_of(&dotted).unwrap_or("").to_owned();
        let span = span_of(node);
        let inside_loop = self.loop_depth() > 0;

        if inside_loop
            && self.matches_any(&dotted, perf.regex_constructors)
            && !self.arguments(node).is_empty()
            && self
                .arguments(node)
                .iter()
                .all(|argument| self.is_literal(*argument))
        {
            self.note(|out| &mut out.invariants_in_loops, span, dotted.clone());
            return;
        }

        if inside_loop {
            let scanned = if perf.scans.contains(&tail.as_str()) && !receiver.is_empty() {
                Some(receiver.clone())
            } else if self.matches_any(&dotted, perf.scan_calls) {
                self.scanned_argument(node)
            } else {
                None
            };
            if let Some(collection) = scanned
                && self.worth_scanning(&collection)
                && !self
                    .arguments(node)
                    .first()
                    .is_some_and(|first| self.profile.string_kinds.contains(&kind_of(first)))
            {
                self.note(|out| &mut out.scans_in_loops, span, dotted.clone());
                return;
            }
        }

        if inside_loop
            && !self.already_keyed(&receiver)
            && ((reaches_outside(&receiver, &self.extra.receivers)
                && asks_for_data(&tail, &self.extra.methods))
                || (receiver.is_empty() && IO_BARE.contains(&tail.to_ascii_lowercase().as_str())))
        {
            self.note(|out| &mut out.queries_in_loops, span, dotted.clone());
            return;
        }

        if self.in_async() && self.names().is_blocking(&dotted) {
            self.out.blocking_in_async.push(Spot {
                span,
                detail: dotted,
                count: 1,
            });
        }
    }

    /// `std::regex digits("…")` is a declaration rather than a call, so the call walk
    /// never sees the construction C++ spells this way.
    pub(super) fn note_regex_declaration(&mut self, node: Node<'a>) {
        if self.loop_depth() == 0 {
            return;
        }
        let Some(kind) = node.child_by_field_name("type") else {
            return;
        };
        let named = self.text(kind).replace("::", ".");
        if !self
            .profile
            .perf
            .regex_types
            .iter()
            .any(|entry| named.split('.').next_back() == entry.split('.').next_back())
        {
            return;
        }
        let built = descendants(node).into_iter().any(|child| {
            self.profile.argument_list_kinds.contains(&kind_of(child))
                && child
                    .named_child(0)
                    .is_some_and(|first| self.is_literal(first))
        });
        if built {
            self.note(|out| &mut out.invariants_in_loops, span_of(node), named);
        }
    }

    /// `if row.id in wanted` is a walk of `wanted` where a set would answer at once,
    /// but only when `wanted` is a list: membership in a dict is already a lookup.
    pub(super) fn note_membership_scan(&mut self, node: Node<'a>) {
        if self.loop_depth() == 0 || !self.text(node).contains(" in ") {
            return;
        }
        let Some(right) = node.named_child(node.named_child_count().saturating_sub(1)) else {
            return;
        };
        let Some(named) = self.callee_name(right) else {
            return;
        };
        let declaration = self.declaration_of(named.rsplit('.').next().unwrap_or(&named));
        if declaration.starts_with('[') || declaration.starts_with("list(") {
            self.note(|out| &mut out.scans_in_loops, span_of(node), named);
        }
    }

    /// An accumulator rebuilt from itself: every pass copies what the passes before
    /// it built.
    pub(super) fn note_copy_accumulation(&mut self, node: Node<'a>) {
        if self.loop_depth() == 0 {
            return;
        }
        let Some(target) = node.child_by_field_name("left") else {
            return;
        };
        let target = self.unwrapped(target);
        if !self.profile.is_identifier(target) {
            return;
        }
        let name = self.text(target);
        let Some(value) = node.child_by_field_name("right") else {
            return;
        };
        let value = self.unwrapped(value);
        let text = self.text(value);
        let head = text.trim_start();
        let rebuilt = head.starts_with(&format!("[...{name}"))
            || head.starts_with(&format!("{{...{name}"))
            || head.starts_with(&format!("[*{name}"))
            || head.starts_with(&format!("{name}.concat("))
            || head.starts_with(&format!("{name} + "))
            || (self.text(node).contains("+=") && self.grows_by_copy(name, value));
        if rebuilt && self.grows_by_copy(name, value) {
            if self.text_declared.contains(name) {
                self.note(
                    |out| &mut out.typed_text_accumulators,
                    span_of(node),
                    name.to_owned(),
                );
            } else {
                self.note(
                    |out| &mut out.copied_accumulators,
                    span_of(node),
                    name.to_owned(),
                );
            }
        }
    }

    /// Adding to a string or a list is a copy; adding to a number is not.
    fn grows_by_copy(&self, name: &str, value: Node<'a>) -> bool {
        let text = self.text(value).trim_start();
        self.holds_text(name)
            || self.declaration_of(name).starts_with('[')
            || text.starts_with('[')
            || text.starts_with('{')
            || descendants(value)
                .into_iter()
                .any(|child| self.profile.string_kinds.contains(&kind_of(child)))
    }
}

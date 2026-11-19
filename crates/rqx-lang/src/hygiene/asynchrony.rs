//! Rules about awaits, callbacks, sleeps and work left running.

use rqx_core::facts::Spot;
use tree_sitter::Node;

use crate::tree::{descendants, kind_of, span_of};

use super::{AsyncFunction, Walker};

impl<'a> Walker<'a> {
    /// A function literal written inside a loop and reading the name the loop moves.
    /// Whatever runs it later sees whichever value the loop stopped on.
    pub(super) fn note_loop_capture(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(loop_node) = self.enclosing_loop(function) else {
            return;
        };
        // A callback the loop itself calls, once per element, closes over nothing it
        // will outlive.
        if profile.is_loop_callback(function, self.source)
            || self.runs_in_place(function)
            || self.binds_per_iteration(loop_node)
        {
            return;
        }
        let moved: Vec<String> = self.loop_names(loop_node);
        if moved.is_empty() {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        let Some(captured) = descendants(body)
            .into_iter()
            .filter(|node| profile.is_identifier(*node))
            .map(|node| self.text(node))
            .find(|name| moved.iter().any(|moved| moved == name))
        else {
            return;
        };
        self.out.loop_variable_captures.push(Spot {
            span: span_of(function),
            detail: captured.to_owned(),
            count: 1,
        });
    }

    /// A function literal handed to a call that runs it before returning: a sort's
    /// comparison, `strings.Map`, a Go subtest. It is done with the loop variable
    /// before the loop moves on. A subtest that calls `t.Parallel()` is the exception:
    /// it waits until the loop has finished.
    fn runs_in_place(&self, function: Node<'a>) -> bool {
        let profile = self.profile;
        let callers: &[&str] = match profile.language {
            rqx_core::Language::Go => &[
                "Run",
                "Slice",
                "SliceStable",
                "SortFunc",
                "SortStableFunc",
                "BinarySearchFunc",
                "IndexFunc",
                "ContainsFunc",
                "DeleteFunc",
                "CompactFunc",
                "EqualFunc",
                "MinFunc",
                "MaxFunc",
                "Map",
                "FieldsFunc",
                "TrimFunc",
                "TrimLeftFunc",
                "TrimRightFunc",
                "Walk",
                "WalkDir",
                "Do",
            ],
            rqx_core::Language::TypeScript | rqx_core::Language::JavaScript => &[
                "sort",
                "toSorted",
                "findLastIndex",
                "replace",
                "replaceAll",
                "from",
            ],
            rqx_core::Language::Python => &[
                "sorted", "sort", "min", "max", "filter", "map", "any", "all", "reduce", "sub",
            ],
            rqx_core::Language::Java => &[
                "sort",
                "removeIf",
                "replaceAll",
                "computeIfAbsent",
                "computeIfPresent",
                "compute",
                "merge",
            ],
            rqx_core::Language::Cpp => &[
                "sort",
                "stable_sort",
                "find_if",
                "count_if",
                "any_of",
                "all_of",
                "none_of",
                "remove_if",
                "accumulate",
            ],
        };
        // Python passes a key function by keyword: `sorted(rows, key=lambda row: …)`.
        let mut list = function.parent();
        if list.is_some_and(|parent| kind_of(parent) == "keyword_argument") {
            list = list.and_then(|parent| parent.parent());
        }
        let Some(call) = list
            .filter(|list| profile.argument_list_kinds.contains(&kind_of(*list)))
            .and_then(|list| list.parent())
            .filter(|call| profile.is_call(*call))
        else {
            return false;
        };
        let Some(tail) = profile.call_tail(call, self.source) else {
            return false;
        };
        callers.contains(&tail) && !(tail == "Run" && self.text(function).contains(".Parallel()"))
    }

    /// A JavaScript loop that declares its variable with `let` or `const` gives every
    /// pass a binding of its own, so a closure keeps the value of its own pass. Only
    /// `var`, or a variable declared before the loop, is shared between passes.
    fn binds_per_iteration(&self, loop_node: Node<'a>) -> bool {
        if !matches!(
            self.profile.language,
            rqx_core::Language::TypeScript | rqx_core::Language::JavaScript
        ) {
            return false;
        }
        let lexical = |node: Node<'a>| matches!(self.text(node), "let" | "const");
        match kind_of(loop_node) {
            "for_in_statement" => loop_node.child_by_field_name("kind").is_some_and(lexical),
            "for_statement" => loop_node
                .child_by_field_name("initializer")
                .is_some_and(|initializer| kind_of(initializer) == "lexical_declaration"),
            _ => false,
        }
    }

    fn enclosing_loop(&self, node: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        let mut parent = node.parent();
        while let Some(current) = parent {
            if profile.loop_kinds.contains(&kind_of(current)) {
                return Some(current);
            }
            if profile.is_function(current) {
                return None;
            }
            parent = current.parent();
        }
        None
    }

    /// The names a loop moves from one pass to the next.
    fn loop_names(&self, loop_node: Node<'a>) -> Vec<String> {
        let profile = self.profile;
        let Some(body) = self.loop_statements(loop_node) else {
            return Vec::new();
        };
        // A `for … in` names what it moves in a field of its own; a counted loop
        // declares it in the header like any other local.
        let named = ["left", "name", "index"]
            .into_iter()
            .find_map(|field| self.loop_header(loop_node, field));
        if let Some(header) = named {
            return descendants(header)
                .into_iter()
                .chain(std::iter::once(header))
                .filter(|node| profile.is_identifier(*node))
                .map(|node| self.text(node).to_owned())
                .collect();
        }
        descendants(loop_node)
            .into_iter()
            .filter(|node| node.start_byte() < body.start_byte())
            .filter(|node| {
                profile.is_identifier(*node)
                    && crate::tree::is_binding_position(
                        *node,
                        profile.parameter_kinds,
                        profile.borrowed_name_kinds,
                        profile.local_declaring_kinds,
                        profile.expression_list_kinds,
                    )
            })
            .map(|node| self.text(node).to_owned())
            .collect()
    }

    /// Two waits in a row on calls that do not read each other's answers. They could
    /// have run together and the reader is paying for both in series.
    pub(super) fn note_sequential_await(&mut self, function: Node<'a>) {
        let profile = self.profile;
        // A test waits in order on purpose: each wait is the step it is checking.
        if !profile.is_async(function) || self.test_depth > 0 {
            return;
        }
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        if let Some((_, second)) =
            crate::cost::indicators::independent_awaits(&self.names(), holder).first()
        {
            self.out.sequential_awaits.push(Spot {
                span: span_of(*second),
                detail: self.snippet(*second),
                count: 2,
            });
        }
    }

    /// Stopping the thread for a fixed time, outside a test. Whatever it is waiting
    /// for, it is not waiting for that.
    pub(super) fn note_sleep(&mut self, call: Node<'a>) {
        if self.test_depth > 0 {
            return;
        }
        let profile = self.profile;
        let Some(dotted) = self.called_name(call) else {
            return;
        };
        let tail = dotted.rsplit('.').next().unwrap_or(&dotted);
        if !profile.perf.sleep_calls.contains(&tail) {
            return;
        }
        self.out.sleeps.push(Spot {
            span: span_of(call),
            detail: dotted.clone(),
            count: 1,
        });
    }

    /// A callback that starts another wait. The order is real and none of it is
    /// written as an order.
    pub(super) fn note_nested_continuation(&mut self, call: Node<'a>) {
        let profile = self.profile;
        if profile.continuation_calls.is_empty() {
            return;
        }
        let Some(dotted) = self.called_name(call) else {
            return;
        };
        let tail = dotted.rsplit('.').next().unwrap_or(&dotted);
        if !profile.continuation_calls.contains(&tail) {
            return;
        }
        let inside = descendants(call).into_iter().any(|node| {
            node.id() != call.id()
                && profile.is_call(node)
                && self.called_name(node).is_some_and(|inner| {
                    let tail = inner.rsplit('.').next().unwrap_or(&inner).to_owned();
                    profile.continuation_calls.contains(&tail.as_str())
                })
        });
        if !inside {
            return;
        }
        self.out.nested_continuations.push(Spot {
            span: span_of(call),
            detail: tail.to_owned(),
            count: 1,
        });
    }

    /// Starting work that answers later and throwing the answer away. Nothing waits
    /// for it and nothing hears if it fails.
    ///
    /// Only a call that reaches one of the file's own async functions counts: `f()`
    /// for a function, `this.f()` for a method of the class it is written in, and
    /// `Type.f()` for a static one. `this.loop.stop()` reaches whatever `loop` holds,
    /// which this file cannot see, however many async `stop` methods it declares.
    pub(super) fn note_dropped_promise(&mut self, statement: Node<'a>, awaited: &[AsyncFunction]) {
        let profile = self.profile;
        let Some(call) = self.only_call(statement) else {
            return;
        };
        if descendants(statement)
            .into_iter()
            .any(|node| profile.await_kinds.contains(&kind_of(node)))
        {
            return;
        }
        let Some(dotted) = self.called_name(call) else {
            return;
        };
        let (receiver, name) = match dotted.rsplit_once('.') {
            Some((receiver, name)) => (Some(receiver), name),
            None => (None, dotted.as_str()),
        };
        let class = match receiver {
            None => None,
            Some(receiver) if profile.self_names.contains(&receiver) => {
                let Some(class) = self.enclosing_class_name(statement) else {
                    return;
                };
                Some(class)
            }
            Some(receiver) => Some(receiver.to_owned()),
        };
        if !awaited
            .iter()
            .any(|function| function.name == name && function.class == class)
        {
            return;
        }
        self.out.dropped_promises.push(Spot {
            span: span_of(statement),
            detail: dotted,
            count: 1,
        });
    }

    /// How deep a function literal sits inside other function literals. Reported on
    /// the innermost one, which is where a reader ends up.
    ///
    /// Called on entering `node`, when the walk holds one frame for the file, one for
    /// each function around `node` and one for `node` itself. Counting the frames
    /// rather than climbing with `Node::parent`, which walks down from the root, keeps
    /// a file of deeply nested callbacks from costing the cube of its depth.
    pub(super) fn note_callback_depth(&mut self, node: Node<'a>) {
        let depth = self.frames.len().saturating_sub(2) as u32;
        if depth > self.limits.callback_depth {
            self.out.callback_nesting.push(Spot {
                span: self.name_span(node),
                detail: depth.to_string(),
                count: depth,
            });
        }
    }

    /// Awaiting inside a loop makes the passes wait for each other. Awaiting inside a
    /// callback is how the same work is started together, so only written loops count.
    pub(super) fn note_await(&mut self, node: Node<'a>) {
        if !self.in_statement_loop() {
            return;
        }
        let subject = node
            .named_child(0)
            .and_then(|inner| self.called_name(inner).or_else(|| self.callee_name(inner)))
            .unwrap_or_else(|| "await".to_owned());
        self.note(|out| &mut out.awaits_in_loops, span_of(node), subject);
    }
}

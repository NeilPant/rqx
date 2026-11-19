//! Rules about control flow: branches, conditions, jumps and loops.

use std::collections::HashMap;

use rqx_core::facts::Spot;
use rqx_core::rule::RuleId;
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{descendants, kind_of, operands_of, operator_of, span_of};

use super::{ChainArm, Walker, squashed};

impl<'a> Walker<'a> {
    /// How many separate decisions one condition holds. A run of the same operator is
    /// still one decision per operand to a reader.
    fn operands(&self, node: Node<'a>) -> u32 {
        let node = self.see_through(node);
        if self.profile.binary_kinds.contains(&kind_of(node))
            && operator_of(node, self.source)
                .is_some_and(|operator| self.profile.logical_operators.contains(&operator))
        {
            let Some((left, right)) = operands_of(node) else {
                return 1;
            };
            return self.operands(left) + self.operands(right);
        }
        1
    }

    pub(super) fn note_unreachable(&mut self, block: Node<'a>) {
        let mut cursor = block.walk();
        let statements: Vec<Node<'a>> = block
            .named_children(&mut cursor)
            .filter(|child| self.profile.is_statement(*child))
            .collect();
        for (index, statement) in statements.iter().enumerate() {
            let Some((_, word)) = self
                .profile
                .exit_kinds
                .iter()
                .find(|(kind, _)| *kind == kind_of(statement))
            else {
                continue;
            };
            if let Some(next) = statements.get(index + 1)
                && !self.profile.hoisted_kinds.contains(&kind_of(next))
                && !crate::removal::still_matters(self.profile, self.source, *next)
            {
                self.out.unreachable.push(Spot {
                    span: span_of(*next),
                    detail: (*word).to_owned(),
                    count: 1,
                });
            }
            break;
        }
    }

    pub(super) fn note_condition(&mut self, branch: Node<'a>) {
        let Some(condition) = branch.child_by_field_name(self.profile.condition_field) else {
            return;
        };
        let inner = self.see_through(condition);
        let operands = self.operands(condition);
        if operands > self.limits.condition_operands {
            self.out.complex_conditions.push(Spot {
                span: span_of(inner),
                detail: String::new(),
                count: operands,
            });
        }
        let negated = operator_of(inner, self.source)
            .is_some_and(|operator| self.profile.negation_operators.contains(&operator));
        if negated
            && branch
                .child_by_field_name(self.profile.else_field)
                .is_some()
        {
            self.out.negative_conditionals.push(Spot {
                span: span_of(inner),
                detail: String::new(),
                count: 1,
            });
        }
    }

    /// The shapes an `if` takes when it is saying less than it looks like: an `else`
    /// after a branch that already left, an `if` that is the only thing inside
    /// another, two arms that do the same thing, and a branch whose whole job is to
    /// turn a condition into the boolean it already is.
    pub(super) fn note_branch_shape(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(consequence) = node
            .child_by_field_name("consequence")
            .map(|arm| self.body(arm))
        else {
            return;
        };
        let alternative = node.child_by_field_name(profile.else_field);

        if let Some(alternative) = alternative {
            if self.always_exits(consequence) {
                self.out.redundant_else.push(Spot {
                    span: span_of(alternative),
                    detail: "else".to_owned(),
                    count: 1,
                });
                if self.text(alternative).starts_with("else") {
                    let keyword = alternative.start_byte() as u32;
                    self.out.focus(
                        RuleId::RedundantElse,
                        span_of(alternative),
                        Span::new(keyword, keyword + "else".len() as u32),
                    );
                }
            }
            // Some languages make the alternative the block itself and others wrap it
            // in an `else` clause; only the wrapper is worth seeing through.
            let other = self.body(alternative);
            if kind_of(other) != profile.if_kind
                && !self.text(consequence).trim().is_empty()
                && squashed(self.text(consequence)) == squashed(self.text(other))
            {
                self.out.identical_branches.push(Spot {
                    span: span_of(node),
                    detail: "both arms".to_owned(),
                    count: 1,
                });
            }
            if let (Some(first), Some(second)) = (
                self.returned_boolean(consequence),
                self.returned_boolean(other),
            ) && first != second
            {
                self.out.boolean_from_branch.push(Spot {
                    span: span_of(node),
                    detail: "the condition itself".to_owned(),
                    count: 1,
                });
            }
        } else if let Some(inner) = self.sole_statement(consequence)
            && kind_of(inner) == profile.if_kind
            && inner.child_by_field_name(profile.else_field).is_none()
        {
            self.out.collapsible_ifs.push(Spot {
                span: span_of(inner),
                detail: "one condition".to_owned(),
                count: 1,
            });
        }
    }

    /// A block a reader was promised something in. The empty bodies a language needs
    /// — `pass`, an abstract method — are declared per language and skipped, and a
    /// catch is left to the rule that already reports it.
    pub(super) fn note_empty_block(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(parent) = node.parent() else {
            return;
        };
        // A grammar that puts a statement list inside the braces makes the block's
        // parent the braces rather than the branch, so the decision is one level up.
        let governs = |candidate: Node<'a>| {
            kind_of(candidate) == profile.if_kind
                || profile.else_clause_kinds.contains(&kind_of(candidate))
                || profile.loop_kinds.contains(&kind_of(candidate))
        };
        let decides = governs(parent)
            || (profile.structural_block_parents.contains(&kind_of(parent))
                && parent.parent().is_some_and(governs));
        // Reported once: where both the braces and a list inside them are nodes, the
        // list is the one that carries the statements.
        if profile.structural_block_parents.contains(&kind_of(node))
            && node
                .named_children(&mut node.walk())
                .any(|child| profile.is_block(child))
        {
            return;
        }
        // A comment is a named child in these grammars, so a block holding only an
        // explanation is a decision somebody documented rather than an empty one.
        if !decides || node.named_child_count() > 0 {
            return;
        }
        self.out.empty_blocks.push(Spot {
            span: span_of(node),
            detail: kind_of(parent).replace('_', " "),
            count: 1,
        });
    }

    pub(super) fn note_comparison(&mut self, node: Node<'a>) {
        let profile = self.profile;
        // Python writes a comparison as a run of operands with the operators between
        // them rather than as a left/right pair, so the fields are absent there and
        // the operator has to be read positionally.
        let operator = node.child_by_field_name("operator").map_or_else(
            || {
                (0..node.child_count())
                    .filter_map(|index| node.child(index))
                    .find(|child| !child.is_named())
            },
            Some,
        );
        let Some(operator) = operator else {
            return;
        };
        if !profile.equality_operators.contains(&self.text(operator)) {
            return;
        }
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left")
                .or_else(|| node.named_child(0)),
            node.child_by_field_name("right")
                .or_else(|| node.named_child(1)),
        ) else {
            return;
        };
        let (left_text, right_text) = (self.text(left).trim(), self.text(right).trim());
        if left_text == right_text && !left_text.is_empty() {
            self.out.self_comparisons.push(Spot {
                span: span_of(node),
                detail: left_text.to_owned(),
                count: 1,
            });
            return;
        }
        // A literal with a fractional part: `0.1`, `1.5e3`. An integer literal
        // compares exactly whatever the variable's type is, so only the decimal point
        // makes this certain without knowing the types.
        for side in [left_text, right_text] {
            if is_fractional(side) {
                self.out.float_equalities.push(Spot {
                    span: span_of(node),
                    detail: side.to_owned(),
                    count: 1,
                });
                return;
            }
        }
        for side in [left_text, right_text] {
            if profile.boolean_literals.contains(&side) {
                self.out.literal_comparisons.push(Spot {
                    span: span_of(node),
                    detail: side.to_owned(),
                    count: 1,
                });
                return;
            }
        }
    }

    /// A `continue` or `break` that control was reaching anyway. The keyword makes a
    /// reader stop and look for the condition that is not there.
    pub(super) fn note_redundant_jump(&mut self, loop_node: Node<'a>) {
        let Some(holder) = self.loop_statements(loop_node) else {
            return;
        };
        let mut cursor = holder.walk();
        let Some(last) = holder.named_children(&mut cursor).last() else {
            return;
        };
        // `break` ends a loop early and `continue` skips the rest, and at the end of
        // the body both do exactly what falling off the end does — unless they carry
        // a label, which is a jump out of somewhere else entirely.
        let bare = last.named_child_count() == 0;
        if bare && kind_of(last) == "continue_statement" {
            self.out.redundant_jumps.push(Spot {
                span: span_of(last),
                detail: "continue".to_owned(),
                count: 1,
            });
        }
    }

    /// A loop that cannot reach a second iteration: the last thing it does is leave,
    /// and nothing inside it starts the next pass.
    pub(super) fn note_single_pass_loop(&mut self, loop_node: Node<'a>) {
        let profile = self.profile;
        let Some(holder) = self.loop_statements(loop_node) else {
            return;
        };
        let Some(last) = holder.named_children(&mut holder.walk()).last() else {
            return;
        };
        let leaves = matches!(kind_of(last), "return_statement" | "break_statement");
        // A labelled break leaves an outer loop, so this one is not the loop that
        // stops.
        if !leaves || last.named_child_count() > 0 && kind_of(last) == "break_statement" {
            return;
        }
        let restarts = descendants(loop_node).into_iter().any(|node| {
            kind_of(node) == "continue_statement"
                && !Self::inside_nested_loop(node, loop_node, profile)
        });
        if restarts {
            return;
        }
        self.out.single_pass_loops.push(Spot {
            span: span_of(last),
            detail: kind_of(last).trim_end_matches("_statement").to_owned(),
            count: 1,
        });
    }

    fn inside_nested_loop(node: Node<'a>, outer: Node<'a>, profile: &LanguageProfile) -> bool {
        let mut parent = node.parent();
        while let Some(current) = parent {
            if current.id() == outer.id() {
                return false;
            }
            if profile.loop_kinds.contains(&kind_of(current)) || profile.is_function(current) {
                return true;
            }
            parent = current.parent();
        }
        false
    }

    /// Reported on the inner conditional, which is where a reader has already lost
    /// track of the first condition.
    pub(super) fn note_nested_ternary(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let mut parent = node.parent();
        while let Some(current) = parent {
            if profile.ternary_kinds.contains(&kind_of(current)) {
                self.out.nested_ternaries.push(Spot {
                    span: span_of(node),
                    detail: self.snippet(node),
                    count: 1,
                });
                return;
            }
            if profile.is_statement(current) || profile.is_function(current) {
                return;
            }
            parent = current.parent();
        }
    }

    /// Two different logical operators in one condition with nothing written to say
    /// which binds tighter.
    pub(super) fn note_ambiguous_precedence(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(outer) = operator_of(node, self.source) else {
            return;
        };
        if !profile.logical_operators.contains(&outer) {
            return;
        }
        let Some((left, right)) = operands_of(node) else {
            return;
        };
        for side in [left, right] {
            if !profile.binary_kinds.contains(&kind_of(side)) {
                continue;
            }
            let Some(inner) = operator_of(side, self.source) else {
                continue;
            };
            if profile.logical_operators.contains(&inner) && inner != outer {
                self.out.ambiguous_precedence.push(Spot {
                    span: span_of(node),
                    detail: format!("{inner} inside {outer}"),
                    count: 1,
                });
                return;
            }
        }
    }

    pub(super) fn note_negated_comparison(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(operator) = operator_of(node, self.source) else {
            return;
        };
        if !profile.negation_operators.contains(&operator) {
            return;
        }
        let Some(operand) = node.named_child(0) else {
            return;
        };
        let inner = self.see_through(operand);
        if !profile.binary_kinds.contains(&kind_of(inner)) {
            return;
        }
        let Some(compared) = operator_of(inner, self.source) else {
            return;
        };
        if !profile.equality_operators.contains(&compared) {
            return;
        }
        self.out.negated_comparisons.push(Spot {
            span: span_of(node),
            detail: compared.to_owned(),
            count: 1,
        });
    }

    /// A switch wide enough to be worth writing as a switch, with nothing said about
    /// the values it does not name.
    pub(super) fn note_missing_default(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let mut cursor = node.walk();
        let mut stack: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
        let mut arms = 0_u32;
        let mut catches_the_rest = false;
        while let Some(current) = stack.pop() {
            if profile.case_kinds.contains(&kind_of(current)) {
                arms += 1;
                let text = self.text(current).trim_start();
                if profile
                    .default_case_markers
                    .iter()
                    .any(|marker| text.starts_with(marker))
                {
                    catches_the_rest = true;
                }
                continue;
            }
            if profile.switch_kinds.contains(&kind_of(current)) {
                continue;
            }
            let mut inner = current.walk();
            stack.extend(current.named_children(&mut inner));
        }
        if arms >= self.limits.min_cases && !catches_the_rest {
            self.out.defaultless_switches.push(Spot {
                span: span_of(node),
                detail: arms.to_string(),
                count: arms,
            });
        }
    }

    /// A function that answers with a value on one path and with nothing on another.
    pub(super) fn note_mixed_return(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let (mut valued, mut bare) = (0_u32, 0_u32);
        let mut cursor = body.walk();
        let mut stack: Vec<Node<'a>> = body.children(&mut cursor).collect();
        let mut first_bare = None;
        while let Some(node) = stack.pop() {
            // A callback answers for itself, not for the function it was written in.
            if profile.is_function(node) {
                continue;
            }
            if kind_of(node) == "return_statement" {
                if node.named_child_count() == 0 {
                    bare += 1;
                    first_bare = Some(node);
                } else {
                    valued += 1;
                }
            }
            let mut inner = node.walk();
            stack.extend(node.children(&mut inner));
        }
        if let Some(node) = first_bare
            && valued > 0
            && bare > 0
        {
            self.out.mixed_returns.push(Spot {
                span: span_of(node),
                detail: format!("{valued} with a value, {bare} without"),
                count: valued + bare,
            });
        }
    }

    /// A run of parameter checks at the top of a body, each answering the same way.
    /// The body has not started yet and the reader has read four decisions.
    pub(super) fn note_guard_cascade(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let holder = body
            .named_children(&mut body.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(body);
        let mut cursor = holder.walk();
        let guards = holder
            .named_children(&mut cursor)
            .take_while(|statement| self.is_bare_guard(*statement))
            .count() as u32;
        if guards >= self.limits.guard_cascade {
            self.out.guard_cascades.push(Spot {
                span: self.name_span(function),
                detail: guards.to_string(),
                count: guards,
            });
        }
    }

    /// An `if` whose whole body is one way out and nothing else.
    fn is_bare_guard(&self, statement: Node<'a>) -> bool {
        let profile = self.profile;
        if kind_of(statement) != profile.if_kind {
            return false;
        }
        let Some(taken) = statement.child_by_field_name("consequence") else {
            return false;
        };
        let leaves = |node: Node<'a>| {
            profile.throw_kinds.contains(&kind_of(node)) || kind_of(node) == "return_statement"
        };
        // A guard is often written without braces, in which case the consequence is
        // the way out rather than a block holding it.
        if leaves(taken) {
            return true;
        }
        let holder = taken
            .named_children(&mut taken.walk())
            .find(|child| profile.is_block(*child))
            .unwrap_or(taken);
        let mut cursor = holder.walk();
        let mut inside = holder.named_children(&mut cursor);
        let Some(only) = inside.next() else {
            return false;
        };
        inside.next().is_none() && leaves(only)
    }

    /// Two arms of one chain asking the same question. The second can never be
    /// reached, and a reader has to compare them to find that out.
    ///
    /// The chain is read once, from its head, and what each later arm finds is kept
    /// for when the walk reaches that arm: reading the rest of the chain again at
    /// every arm made a long `else if` chain cost the square of its length.
    pub(super) fn note_duplicate_condition(&mut self, branch: Node<'a>) {
        match self.chain_arms.remove(&branch.id()) {
            // Only from the head of a chain, so a chain of four is read once.
            Some(ChainArm::Tail) => return,
            Some(ChainArm::Reads(found)) => {
                self.out.duplicate_conditions.extend(found);
                return;
            }
            None => {}
        }
        let profile = self.profile;
        let source = self.source;
        let mut asked: Vec<(String, Node<'a>)> = Vec::new();
        let ask = |node: Node<'a>, asked: &mut Vec<(String, Node<'a>)>| {
            if let Some(condition) = node.child_by_field_name(profile.condition_field) {
                asked.push((squashed(crate::tree::text_of(condition, source)), condition));
            }
        };
        ask(branch, &mut asked);
        // The later arms reached through an `else` clause, which read the chain again
        // from where they stand, with where in `asked` they start reading.
        let mut rereading: Vec<(usize, usize)> = Vec::new();
        if profile.else_is_sibling_list {
            // The clauses hang off the `if` as a list rather than nesting inside one
            // another, so the whole chain is one node's children.
            let mut cursor = branch.walk();
            let clauses: Vec<Node<'a>> = branch
                .named_children(&mut cursor)
                .filter(|child| profile.else_clause_kinds.contains(&kind_of(child)))
                .collect();
            for clause in clauses {
                ask(clause, &mut asked);
            }
        } else {
            let mut current = branch.child_by_field_name(profile.else_field);
            while let Some(node) = current {
                // Some grammars hang the next `if` straight off the field; others put
                // an `else` clause in between and nest the `if` inside that.
                let arm = if kind_of(node) == profile.if_kind {
                    self.chain_arms.insert(node.id(), ChainArm::Tail);
                    node
                } else if profile.else_clause_kinds.contains(&kind_of(node)) {
                    match node
                        .named_children(&mut node.walk())
                        .find(|child| kind_of(child) == profile.if_kind)
                    {
                        Some(nested) => {
                            rereading.push((nested.id(), asked.len()));
                            nested
                        }
                        None => break,
                    }
                } else {
                    break;
                };
                ask(arm, &mut asked);
                current = arm.child_by_field_name(profile.else_field);
            }
        }
        // For each arm, the next arm asking the same question; then, for each place
        // to start reading, the first arm that repeats one asked since that place.
        let mut next_same = vec![None; asked.len()];
        let mut last_seen: HashMap<&str, usize> = HashMap::new();
        for (at, (text, _)) in asked.iter().enumerate().rev() {
            next_same[at] = last_seen.insert(text.as_str(), at);
        }
        let mut first_repeat = vec![None; asked.len() + 1];
        for at in (0..asked.len()).rev() {
            first_repeat[at] = match (next_same[at], first_repeat[at + 1]) {
                (Some(here), Some(later)) => Some(usize::min(here, later)),
                (here, later) => here.or(later),
            };
        }
        let spot = |from: usize| -> Option<Spot> {
            let (text, node) = &asked[first_repeat[from]?];
            Some(Spot {
                span: span_of(*node),
                detail: text.clone(),
                count: 1,
            })
        };
        for (arm, from) in rereading {
            self.chain_arms.insert(arm, ChainArm::Reads(spot(from)));
        }
        self.out.duplicate_conditions.extend(spot(0));
    }

    /// A switch arm that does work and then runs into the next one.
    pub(super) fn note_silent_fallthrough(&mut self, switch: Node<'a>) {
        let profile = self.profile;
        let mut cursor = switch.walk();
        let mut stack: Vec<Node<'a>> = switch.named_children(&mut cursor).collect();
        let mut arms: Vec<Node<'a>> = Vec::new();
        while let Some(current) = stack.pop() {
            if profile.case_kinds.contains(&kind_of(current)) {
                arms.push(current);
                continue;
            }
            if profile.switch_kinds.contains(&kind_of(current)) {
                continue;
            }
            let mut inner = current.walk();
            stack.extend(current.named_children(&mut inner));
        }
        arms.sort_by_key(|arm| arm.start_byte());
        let Some((_, earlier)) = arms.split_last() else {
            return;
        };
        for arm in earlier {
            let mut inside = arm.walk();
            let statements: Vec<Node<'a>> = arm
                .named_children(&mut inside)
                .filter(|child| profile.is_statement(*child) || self.leaves_the_arm(*child))
                .collect();
            let Some(last) = statements.last() else {
                continue;
            };
            if statements.is_empty() || self.leaves_the_arm(*last) {
                continue;
            }
            self.out.silent_fallthroughs.push(Spot {
                span: span_of(*arm),
                detail: self.snippet(*arm),
                count: 1,
            });
            return;
        }
    }

    fn leaves_the_arm(&self, node: Node<'a>) -> bool {
        let profile = self.profile;
        matches!(
            kind_of(node),
            "break_statement" | "return_statement" | "continue_statement" | "goto_statement"
        ) || profile.throw_kinds.contains(&kind_of(node))
    }

    /// Growing or shrinking the very collection a loop is walking, in a way the
    /// language's own iteration does not survive.
    pub(super) fn note_mutation_while_iterating(&mut self, loop_node: Node<'a>) {
        let Some(walked) = self.iterated_name(loop_node) else {
            return;
        };
        let Some(body) = self.loop_statements(loop_node) else {
            return;
        };
        let Some(changed) = descendants(body).into_iter().find(|node| {
            self.calls_a_changer(*node, &walked)
                || self.rewrites_walked(*node, &walked)
                || self.deletes_from(*node, &walked)
        }) else {
            return;
        };
        self.out.mutated_while_iterated.push(Spot {
            span: span_of(changed),
            detail: walked,
            count: 1,
        });
    }

    /// A method that adds or removes elements, called on the collection being walked.
    fn calls_a_changer(&self, node: Node<'a>, walked: &str) -> bool {
        const CHANGERS: [&str; 16] = [
            "push",
            "pop",
            "shift",
            "unshift",
            "splice",
            "append",
            "remove",
            "add",
            "insert",
            "delete",
            "push_back",
            "pop_back",
            "emplace_back",
            "erase",
            "clear",
            "removeIf",
        ];
        // A JavaScript Map or Set defines removal during a walk: an entry removed
        // before the walk reaches it is skipped and the rest are visited once. An
        // array has neither method.
        const SAFE_IN_JAVASCRIPT: [&str; 2] = ["delete", "clear"];
        let profile = self.profile;
        let javascript = matches!(
            profile.language,
            rqx_core::Language::TypeScript | rqx_core::Language::JavaScript
        );
        profile.is_call(node)
            && self.called_name(node).is_some_and(|dotted| {
                let mut parts = dotted.rsplitn(2, '.');
                let tail = parts.next().unwrap_or_default();
                let receiver = parts.next().unwrap_or_default();
                receiver == walked
                    && CHANGERS.contains(&tail)
                    && !(javascript && SAFE_IN_JAVASCRIPT.contains(&tail))
            })
    }

    /// An assignment to the walked name that changes what the walk sees. Giving the
    /// name a new value does not: the loop keeps the collection it started with.
    fn rewrites_walked(&self, node: Node<'a>, walked: &str) -> bool {
        let profile = self.profile;
        let kind = kind_of(node);
        let assigns_walked = (profile.assignment_kinds.contains(&kind)
            || profile.local_declaring_kinds.contains(&kind))
            && node
                .child_by_field_name("left")
                .is_some_and(|target| self.text(target) == walked);
        if !assigns_walked {
            return false;
        }
        match profile.language {
            // `items += more` extends a list, set or dict in place.
            rqx_core::Language::Python => profile.compound_assignment_kinds.contains(&kind),
            // A range-for holds a reference to the container, so assigning to it
            // invalidates the iterators the loop is using.
            rqx_core::Language::Cpp => true,
            // A range reads the slice it was given; only an append onto a slice of
            // it, `s = append(s[:i], s[i+1:]...)`, moves the elements it reads.
            rqx_core::Language::Go => node
                .child_by_field_name("right")
                .is_some_and(|value| self.shifts_elements_of(value, walked)),
            _ => false,
        }
    }

    /// `append(s[:i], …)`: writing into the array behind `s` from inside it.
    fn shifts_elements_of(&self, value: Node<'a>, walked: &str) -> bool {
        let profile = self.profile;
        let mut value = self.see_through(value);
        while profile.expression_list_kinds.contains(&kind_of(value))
            && value.named_child_count() == 1
            && let Some(only) = value.named_child(0)
        {
            value = self.see_through(only);
        }
        profile.is_call(value)
            && self.called_name(value).as_deref() == Some("append")
            && self.names().arguments(value).first().is_some_and(|first| {
                kind_of(*first) == "slice_expression"
                    && first
                        .child_by_field_name("operand")
                        .is_some_and(|operand| self.text(operand) == walked)
            })
    }

    /// Python's `del items[key]`: a dict raises when its size changes during the
    /// walk, and a list skips the element after the one removed.
    fn deletes_from(&self, node: Node<'a>, walked: &str) -> bool {
        let profile = self.profile;
        if profile.language != rqx_core::Language::Python || kind_of(node) != "delete_statement" {
            return false;
        }
        // `del a[k], b[k]` lists its targets; `del cache[items[0]]` deletes from
        // `cache`, so only the targets themselves are read.
        let targets: Vec<Node<'a>> = node
            .named_children(&mut node.walk())
            .flat_map(|target| {
                if profile.expression_list_kinds.contains(&kind_of(target)) {
                    target.named_children(&mut target.walk()).collect()
                } else {
                    vec![target]
                }
            })
            .collect();
        targets.into_iter().any(|target| {
            kind_of(target) == "subscript"
                && target
                    .child_by_field_name("value")
                    .is_some_and(|value| self.text(value) == walked)
        })
    }

    /// What a loop is walking, where the loop names it.
    fn iterated_name(&self, loop_node: Node<'a>) -> Option<String> {
        let over = self
            .loop_header(loop_node, "right")
            .or_else(|| self.loop_header(loop_node, "value"))?;
        let over = self.see_through(over);
        self.profile
            .is_identifier(over)
            .then(|| self.text(over).to_owned())
    }

    pub(super) fn note_exits(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let guards = (0..body.named_child_count())
            .filter_map(|index| body.named_child(index))
            .take_while(|child| {
                kind_of(child) == profile.if_kind
                    || profile.exit_kinds.iter().any(|(k, _)| *k == kind_of(child))
            })
            .count();

        let mut cursor = body.walk();
        let mut stack: Vec<Node<'a>> = body.children(&mut cursor).collect();
        let mut exits: u32 = 0;
        while let Some(node) = stack.pop() {
            // A nested function has its own ways out; counting them here would blame
            // this body for a callback's shape.
            if profile.is_function(node) {
                continue;
            }
            if profile.exit_kinds.iter().any(|(k, _)| *k == kind_of(node)) {
                exits += 1;
            }
            let mut inner = node.walk();
            stack.extend(node.children(&mut inner));
        }

        let past_guards = exits.saturating_sub(guards as u32);
        if past_guards > self.limits.max_exits {
            self.out.many_exits.push(Spot {
                span: self.name_span(function),
                detail: past_guards.to_string(),
                count: past_guards,
            });
        }
    }

    pub(super) fn note_switch_width(&mut self, node: Node<'a>) {
        let profile = self.profile;
        // The arms sit inside the switch's body in most grammars and directly under
        // the statement in the rest, so both are counted rather than one guessed at.
        let mut cursor = node.walk();
        let mut stack: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
        let mut arms = 0_u32;
        while let Some(current) = stack.pop() {
            if profile.case_kinds.contains(&kind_of(current)) {
                arms += 1;
                continue;
            }
            if profile.switch_kinds.contains(&kind_of(current)) {
                continue;
            }
            let mut inner = current.walk();
            stack.extend(current.named_children(&mut inner));
        }
        if arms > 0 && arms < self.limits.min_cases {
            self.out.tiny_switches.push(Spot {
                span: span_of(node),
                detail: arms.to_string(),
                count: arms,
            });
        }
        if arms > self.limits.max_cases {
            self.out.wide_switches.push(Spot {
                span: span_of(node),
                detail: arms.to_string(),
                count: arms,
            });
        }
    }

    /// An assignment where a condition belongs. It reads as a comparison and behaves
    /// as a write, which is why it survives review.
    pub(super) fn note_assignment_in_condition(&mut self, node: Node<'a>) {
        let profile = self.profile;
        let Some(condition) = node.child_by_field_name("condition") else {
            return;
        };
        let mut cursor = condition.walk();
        let mut stack: Vec<Node<'a>> = condition.children(&mut cursor).collect();
        stack.push(condition);
        while let Some(current) = stack.pop() {
            if profile.assignment_kinds.contains(&kind_of(current)) {
                self.out.assignments_in_conditions.push(Spot {
                    span: span_of(current),
                    detail: self.text(current).trim().to_owned(),
                    count: 1,
                });
                return;
            }
            let mut inner = current.walk();
            stack.extend(current.children(&mut inner));
        }
    }

    /// Whether every path out of a block leaves the function or the loop, which is
    /// what makes a following `else` a level of indentation nobody needs.
    fn always_exits(&self, block: Node<'a>) -> bool {
        let profile = self.profile;
        let exits = |node: Node<'a>| {
            profile
                .exit_kinds
                .iter()
                .any(|(kind, _)| *kind == kind_of(node))
        };
        let last = (0..block.named_child_count())
            .filter_map(|index| block.named_child(index))
            .rfind(|child| !profile.is_comment(*child));
        match last {
            Some(node) if exits(node) => true,
            Some(node) if kind_of(node) == profile.if_kind => {
                let consequence = node.child_by_field_name("consequence");
                let alternative = node.child_by_field_name(profile.else_field);
                match (consequence, alternative) {
                    (Some(one), Some(other)) => self.always_exits(one) && self.always_exits(other),
                    _ => false,
                }
            }
            Some(node) if profile.is_block(node) => self.always_exits(node),
            _ => exits(block),
        }
    }

    /// The node actually holding a body's statements. Some grammars make an arm the
    /// block itself, some wrap it in an `else` clause, and Go puts a statement list
    /// inside the braces — comparing two arms means comparing the same thing.
    fn body(&self, node: Node<'a>) -> Node<'a> {
        if self.profile.is_block(node) {
            return node;
        }
        let mut cursor = node.walk();
        let inner: Vec<Node<'a>> = node
            .named_children(&mut cursor)
            .filter(|child| self.profile.is_block(*child))
            .collect();
        match inner.as_slice() {
            [only] => *only,
            _ => node.named_child(0).map_or(node, |first| {
                if self.profile.is_block(first) {
                    first
                } else {
                    node
                }
            }),
        }
    }

    fn sole_statement(&self, block: Node<'a>) -> Option<Node<'a>> {
        let profile = self.profile;
        let mut statements = (0..block.named_child_count())
            .filter_map(|index| block.named_child(index))
            .filter(|child| !profile.is_comment(*child));
        let first = statements.next()?;
        statements.next().is_none().then_some(first)
    }

    /// The boolean a block returns, when returning one is all it does.
    fn returned_boolean(&self, block: Node<'a>) -> Option<bool> {
        let profile = self.profile;
        let statement = self.sole_statement(block).unwrap_or(block);
        if !profile
            .exit_kinds
            .iter()
            .any(|(kind, _)| *kind == kind_of(statement))
        {
            return None;
        }
        let value = statement.named_child(0)?;
        let text = self.text(value).trim();
        profile
            .boolean_literals
            .contains(&text)
            .then_some(matches!(text, "true" | "True"))
    }
}

/// A numeric literal a reader would call a decimal. Exponent form counts, because
/// `1e-3` is stored the same way `0.001` is; an integer does not, because comparing
/// one is exact whatever it is compared against.
fn is_fractional(text: &str) -> bool {
    let body = text.trim_start_matches(['-', '+']);
    if body.is_empty() || !body.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return false;
    }
    // A hex or binary literal has no fraction, and its `e` is a digit.
    if body.starts_with("0x") || body.starts_with("0X") || body.starts_with("0b") {
        return false;
    }
    let digits = body.trim_end_matches(['f', 'F', 'd', 'D', 'l', 'L']);
    digits.contains('.') || digits.contains(['e', 'E'])
}

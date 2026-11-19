//! Rules about comments, docstrings, waivers and code that narrates itself.

use rqx_core::facts::{Hygiene, Limits, Spot};
use rqx_core::span::Span;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{Nodes, descendants, kind_of, span_of, text_of};
use crate::vocabulary::{journal_marker, looks_like_code, redundant_subject};

use super::modules::note_twin_functions;
use super::testing::note_unused_fixtures;
use super::{Walker, split_words};

impl<'a> Walker<'a> {
    /// A body that says more about what it is doing than it does.
    pub(super) fn note_over_logging(&mut self, function: Node<'a>) {
        let profile = self.profile;
        let Some(body) = self.function_body(function) else {
            return;
        };
        let inside = descendants(body);
        let statements = inside
            .iter()
            .filter(|node| profile.is_statement(**node))
            .count() as u32;
        if statements < 4 {
            return;
        }
        let logged = inside
            .iter()
            .filter(|node| profile.is_call(**node))
            .filter(|node| {
                self.called_name(**node).is_some_and(|dotted| {
                    let tail = dotted.rsplit('.').next().unwrap_or(&dotted).to_owned();
                    dotted.contains('.') && profile.log_calls.contains(&tail.as_str())
                })
            })
            .count() as u32;
        if logged * 100 >= statements * self.limits.log_percent && logged > 1 {
            self.out.over_logging.push(Spot {
                span: self.name_span(function),
                detail: format!("{logged} of {statements}"),
                count: logged,
            });
        }
    }
}

pub(super) fn scan_comments(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    limits: Limits,
    out: &mut Hygiene,
) {
    let mut runs: Vec<(Span, u32)> = Vec::new();
    // A divider drawn inside a licence header is part of the notice.
    let notices = crate::notices::protected_comments(
        source,
        nodes.iter().filter(|node| profile.is_comment(*node)),
    );
    for comment in nodes.iter().filter(|node| profile.is_comment(*node)) {
        let span = span_of(comment);
        let text = text_of(comment, source);
        if crate::vocabulary::emoji(text) {
            out.emoji.push(Spot {
                span,
                detail: String::new(),
                count: 1,
            });
        }
        // Recorded outside the chain below: how old a note is has nothing to do with
        // which of the comment rules claims it.
        if let Some(marker) = crate::vocabulary::todo_marker(text) {
            out.todos.push(Spot {
                span,
                detail: marker.to_owned(),
                count: 1,
            });
        }
        // Checked before the suppression-directive branch: rqx's own waiver is not
        // one of the diagnostics `suppressed-diagnostic` is meant to report.
        if let Some(waiver) = rqx_core::suppress::Suppression::parse(text, span) {
            out.waivers.push(waiver);
        } else if let Some(directive) = profile
            .suppression_directives
            .iter()
            .find(|marker| text.contains(*marker))
        {
            out.suppressions.push(Spot {
                span,
                detail: (*directive).to_owned(),
                count: 1,
            });
        } else if let Some(opening) = crate::vocabulary::chat_residue(text) {
            out.chat_residue.push(Spot {
                span,
                detail: opening.to_owned(),
                count: 1,
            });
        } else if let Some(regret) = crate::vocabulary::apology(text) {
            out.apologies.push(Spot {
                span,
                detail: regret.to_owned(),
                count: 1,
            });
        } else if crate::vocabulary::banner(text) && notices.binary_search(&span).is_err() {
            out.banner_comments.push(Spot {
                span,
                detail: String::new(),
                count: 1,
            });
        } else if let Some(marker) = crate::vocabulary::unattached_todo(text) {
            out.orphan_todos.push(Spot {
                span,
                detail: marker.to_owned(),
                count: 1,
            });
        } else if let Some(marker) = journal_marker(text) {
            out.journal_comments.push(Spot {
                span,
                detail: marker.to_owned(),
                count: 1,
            });
        } else if let Some(subject) = source
            .get(span.end as usize..)
            .and_then(|rest| rest.lines().nth(1))
            .and_then(|line| redundant_subject(text, line, profile.declaration_keywords))
        {
            out.redundant_comments.push(Spot {
                span,
                detail: subject,
                count: 1,
            });
        } else if looks_like_code(
            text,
            profile.code_line_endings,
            profile.code_markers,
            profile.code_operators,
        ) {
            runs.push((span, text.lines().count() as u32));
        }
    }

    note_narration(profile, source, nodes, limits, out);
    note_twin_functions(profile, source, nodes, out);
    note_unused_fixtures(profile, source, nodes, out);
    note_body_docstrings(profile, source, nodes, out);

    // Consecutive commented-out lines are one decision, not one finding per line.
    let mut index = 0;
    while index < runs.len() {
        let start = runs[index].0;
        let mut end = runs[index].0;
        let mut lines = runs[index].1;
        while index + 1 < runs.len()
            && source[end.end as usize..runs[index + 1].0.start as usize]
                .chars()
                .all(char::is_whitespace)
        {
            index += 1;
            end = runs[index].0;
            lines += runs[index].1;
        }
        if lines >= limits.commented_code_lines {
            out.commented_code.push(Spot {
                span: Span::new(start.start, end.end),
                detail: String::new(),
                count: lines,
            });
        }
        index += 1;
    }
}

/// The three rules about a comment's relationship to the code under it. They share
/// one walk because each needs the same pairing of a comment with what it precedes.
fn note_narration(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    limits: Limits,
    out: &mut Hygiene,
) {
    let index = rqx_core::LineIndex::new(source);
    let comments: Vec<Node<'_>> = nodes
        .iter()
        .filter(|node| profile.is_comment(*node))
        .collect();
    let leads = crate::tree::comment_leads(
        nodes.root(),
        |node| profile.is_comment(node),
        |node| profile.is_block(node),
    );
    let leads = |comment: Node<'_>| leads.get(&comment.id()).copied();

    // A comment, a line of code, a comment, a line of code: the author is reading the
    // code aloud. A run of comment lines with no code between them is a paragraph.
    let mut run = 0_u32;
    let mut opened = None;
    let mut previous: Option<Span> = None;
    for comment in &comments {
        let span = span_of(*comment);
        // Code between two comments continues the narration; a blank line ends it,
        // and so does a second comment written directly underneath the first.
        // A comment above a field or a property signature is documentation of a
        // shape, not narration of a run of work.
        let over_a_statement = leads(*comment).is_some_and(|node| {
            let kind = kind_of(node);
            !profile.field_kinds.contains(&kind)
                && !profile.signature_kinds.contains(&kind)
                && !profile.property_definition_kinds.contains(&kind)
        });
        let narrates = over_a_statement
            && previous.is_some_and(|last| {
                let between = &source[last.end as usize..span.start as usize];
                // The first and last fragments are the tails of the two comment lines
                // themselves, so only the lines between them can be blank.
                let lines: Vec<&str> = between.lines().collect();
                let blank = lines
                    .iter()
                    .enumerate()
                    .any(|(at, line)| at > 0 && at + 1 < lines.len() && line.trim().is_empty());
                between.chars().any(|c| !c.is_whitespace()) && !blank
            });
        if narrates {
            run += 1;
        } else {
            run = if over_a_statement { 1 } else { 0 };
            opened = Some(*comment);
        }
        if run == limits.narrated_run
            && let Some(first) = opened
        {
            out.narrated_runs.push(Spot {
                span: Span::new(span_of(first).start, span.end),
                detail: run.to_string(),
                count: run,
            });
        }
        previous = Some(span);
    }

    // Each `//` line is a node of its own, so the block above a function has to be
    // put back together before it can be measured against the body.
    let mut blocks: Vec<(Span, u32)> = Vec::new();
    for comment in &comments {
        let span = span_of(*comment);
        let lines = text_of(*comment, source).lines().count() as u32;
        match blocks.last_mut() {
            Some((last, held))
                if source[last.end as usize..span.start as usize]
                    .chars()
                    .all(char::is_whitespace)
                    && index.locate(span.start).0 == index.locate(last.end).0 + 1 =>
            {
                *last = Span::new(last.start, span.end);
                *held += lines;
            }
            _ => blocks.push((span, lines)),
        }
    }

    for (span, explanation) in blocks {
        let Some(comment) = comments
            .iter()
            .copied()
            .find(|node| span_of(*node).start == span.start)
        else {
            continue;
        };
        let Some(declared) = leads(comment).map(|node| unwrap_export(profile, node)) else {
            continue;
        };
        if !profile.is_function(declared) {
            continue;
        }
        // A comment opening the file describes the file, and the first declaration
        // under it is not what it was written about.
        if nodes
            .root()
            .named_child(0)
            .is_some_and(|first| span_of(first).start == span.start)
        {
            continue;
        }
        // Lines rather than statements: a body that is one long expression — a
        // returned tree of markup, a chained pipeline — has almost no statements in
        // it and is not thereby short.
        let body = declared
            .child_by_field_name("body")
            .or_else(|| {
                declared
                    .named_children(&mut declared.walk())
                    .find(|child| profile.is_block(*child))
            })
            .map(|body| {
                let extent = span_of(body);
                index
                    .locate(extent.end)
                    .0
                    .saturating_sub(index.locate(extent.start).0)
                    + 1
            })
            .unwrap_or(0);
        if body > 0 && explanation > body * limits.explanation_ratio {
            out.long_explanations.push(Spot {
                span,
                detail: format!("{explanation} lines above {body}"),
                count: explanation,
            });
        }
        if let Some(name) = restates_signature(profile, source, comment, declared) {
            out.restated_docs.push(Spot {
                span,
                detail: name,
                count: 1,
            });
        }
    }
}

/// A language whose doc comment is a string inside the body rather than a comment
/// above the declaration.
fn note_body_docstrings(
    profile: &'static LanguageProfile,
    source: &str,
    nodes: &Nodes<'_>,
    out: &mut Hygiene,
) {
    for declared in nodes.iter().filter(|node| profile.is_function(*node)) {
        let Some(body) = declared.child_by_field_name("body") else {
            continue;
        };
        let Some(first) = body.named_child(0) else {
            continue;
        };
        let doc = if profile.string_kinds.contains(&kind_of(first)) {
            first
        } else if kind_of(first) == "expression_statement"
            && first
                .named_child(0)
                .is_some_and(|value| profile.string_kinds.contains(&kind_of(value)))
        {
            first.named_child(0).unwrap_or(first)
        } else {
            continue;
        };
        if let Some(name) = restates_signature(profile, source, doc, declared) {
            out.restated_docs.push(Spot {
                span: span_of(doc),
                detail: name,
                count: 1,
            });
        }
    }
}

/// A declaration wearing an export wrapper is still the declaration.
fn unwrap_export<'tree>(profile: &'static LanguageProfile, node: Node<'tree>) -> Node<'tree> {
    if profile.export_kinds.contains(&kind_of(node)) {
        return node.named_child(0).unwrap_or(node);
    }
    node
}

/// Whether a doc comment says only what the signature already says. Every word it
/// adds beyond the name and the parameters is the reason to keep it.
fn restates_signature(
    profile: &'static LanguageProfile,
    source: &str,
    comment: Node<'_>,
    declared: Node<'_>,
) -> Option<String> {
    const FILLER: [&str; 18] = [
        "the", "a", "an", "of", "to", "for", "and", "or", "this", "that", "is", "it", "with",
        "from", "in", "on", "by", "as",
    ];
    const SIGNATURE_WORDS: [&str; 6] = ["return", "returns", "param", "params", "parameter", "arg"];
    let parameters =
        crate::tree::find_within(declared, profile.parameter_list_kinds, profile.block_kinds)
            .into_iter()
            .next_back();
    // C++ keeps the name under a declarator rather than in a field of its own, so
    // the last name written before the parameter list is the one.
    let name = declared
        .child_by_field_name("name")
        .or_else(|| {
            let stop = parameters.map_or(u32::MAX as usize, |list| list.start_byte());
            descendants(declared)
                .into_iter()
                .rfind(|node| node.start_byte() < stop && profile.is_identifier(*node))
        })
        .map(|node| text_of(node, source).to_owned())?;
    let mut known: Vec<String> = split_words(&name);
    if let Some(list) = parameters {
        for parameter in list.named_children(&mut list.walk()) {
            for word in split_words(text_of(parameter, source)) {
                known.push(word);
            }
        }
    }
    let body: Vec<String> = split_words(text_of(comment, source))
        .into_iter()
        .filter(|word| word.len() > 1)
        .filter(|word| !FILLER.contains(&word.as_str()))
        .filter(|word| !SIGNATURE_WORDS.contains(&word.as_str()))
        .collect();
    // Nothing said at all is `empty-block`'s business, not this rule's.
    if body.is_empty() || body.iter().any(|word| !known.contains(word)) {
        return None;
    }
    Some(name)
}

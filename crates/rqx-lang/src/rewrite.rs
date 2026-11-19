//! Turns a finding into the edit that resolves it, for the rules whose fix is
//! decided by the finding alone. A rule that is not here has no mechanical answer,
//! and `edit` says so by returning `None` rather than guessing at one.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use rqx_core::config::Fix;
use rqx_core::edit::{Edit, Resolution, apply, resolve};
use rqx_core::finding::{Confidence, Evidence, Finding};
use rqx_core::rule::{Autofix, RuleId};
use rqx_core::span::Span;
use rqx_core::{Identity, Language, RqxConfig};
use tree_sitter::{Node, Tree};

use crate::invariants::Invariants;
use crate::profile::LanguageProfile;
use crate::tree::kind_of;
use crate::{AnalysisError, support};

/// One parse of one file, reused across every finding in it. Parsing per finding
/// would re-read a large file once per fix, and the tree is the same either way.
pub struct Rewriter<'src> {
    language: Language,
    /// Whether this file's extension asks for the markup grammar, so a re-parse of
    /// the edited text reads it the same way the original was read.
    markup: bool,
    source: &'src str,
    profile: &'static LanguageProfile,
    tree: Tree,
    /// Licence notices, comments holding a URL and tool directives, which no edit may
    /// remove.
    protected: OnceCell<Vec<Span>>,
    /// What the file's edits have to leave in place, read on the first edit.
    invariants: OnceCell<Invariants>,
}

/// Why a file's edits were not written. Every variant is a reason the reader gets
/// verbatim, because a silent refusal reads as a clean bill of health.
#[derive(Clone, Debug)]
pub enum Withheld {
    /// An edit reached past the source or split a code point.
    OutOfRange,
    DoesNotParse(ParseFault),
    Unanalysable(String),
    /// A finding the plan set out to remove is still reported after the edit.
    Survives(Vec<Finding>),
    /// A finding absent before the edit is reported after it.
    Introduces(Vec<Finding>),
    /// The edit removes or renames a name another file still uses.
    StillReferenced {
        name: String,
        by: String,
    },
    /// The edit changes a function's signature where callers in other files of the
    /// package cannot be checked.
    ChangesSignature(Vec<String>),
    /// The build check (`--verify-build`) failed after the edit, or could not run.
    Build(String),
    /// The edit takes away something the program, its build or its tools depend on,
    /// in the words of `Invariants::broken_by`.
    Breaks(String),
}

impl fmt::Display for Withheld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange => f.write_str("an edit reached outside the file"),
            Self::DoesNotParse(fault) => write!(f, "the result does not parse: {fault}"),
            Self::Unanalysable(reason) => write!(f, "the result could not be analysed: {reason}"),
            Self::Survives(findings) => write!(
                f,
                "{} still reported after the edit",
                findings
                    .iter()
                    .map(|finding| finding.rule.slug())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Introduces(findings) => write!(
                f,
                "would introduce {}",
                findings
                    .iter()
                    .map(|finding| finding.rule.slug())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::StillReferenced { name, by } => {
                write!(f, "removes `{name}`, which {by} still uses")
            }
            Self::ChangesSignature(functions) => write!(
                f,
                "changes the signature of {}, which other files in the package may call",
                functions
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Build(reason) => write!(f, "the build check failed: {reason}"),
            Self::Breaks(reason) => f.write_str(reason),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ParseFault {
    pub span: Span,
    pub missing: bool,
}

impl fmt::Display for ParseFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = if self.missing { "missing" } else { "error" };
        write!(
            f,
            "{what} node at bytes {}..{}",
            self.span.start, self.span.end
        )
    }
}

/// The edited file, proven: it parses, and re-analysis reports nothing it did not
/// before, minus what the edits set out to remove.
#[derive(Clone, Debug)]
pub struct Verified {
    pub source: String,
    pub findings: Vec<Finding>,
}

/// The first `ERROR` or `MISSING` node in a parse of `source`, if any. Tree-sitter
/// recovers from a bad edit by inventing one rather than failing, so a tree that
/// parsed is not yet a tree that parsed clean.
pub fn parses_clean(language: Language, markup: bool, source: &str) -> Result<(), ParseFault> {
    let support = support(language);
    let tree = crate::parse(language, support.profile, markup, source).map_err(|_| ParseFault {
        span: Span::new(0, source.len() as u32),
        missing: false,
    })?;
    let root = tree.root_node();
    if !root.has_error() {
        return Ok(());
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            return Err(ParseFault {
                span: Span::new(node.start_byte() as u32, node.end_byte() as u32),
                missing: node.is_missing(),
            });
        }
        if node.has_error() {
            stack.extend(node.children(&mut cursor));
        }
    }
    Ok(())
}

impl<'src> Rewriter<'src> {
    pub fn new(language: Language, markup: bool, source: &'src str) -> Result<Self, AnalysisError> {
        let support = support(language);
        let tree = crate::parse(language, support.profile, markup, source)?;
        Ok(Self {
            language,
            markup,
            source,
            profile: support.profile,
            tree,
            protected: OnceCell::new(),
            invariants: OnceCell::new(),
        })
    }

    /// Applies the plan in memory and proves the result before anyone writes it.
    /// `display` and `config` must be the ones the original analysis used: the
    /// display string becomes every finding's file, and the config decides which
    /// stages run, so a different one makes the two finding sets incomparable.
    ///
    /// Rules the whole project decides are left out of the comparison on both
    /// sides; one file's re-analysis cannot see them, and the caller's project pass
    /// is what answers for those. A new finding that is itself mechanically fixable
    /// is tolerated rather than withheld, because deleting the last use of a binding
    /// makes the binding unused, and that is the next round's edit, not a regression.
    pub fn verify(
        &self,
        display: &str,
        config: &RqxConfig,
        before: &[Finding],
        resolution: &Resolution,
    ) -> Result<Verified, Withheld> {
        let source = apply(self.source, resolution).ok_or(Withheld::OutOfRange)?;
        parses_clean(self.language, self.markup, &source).map_err(Withheld::DoesNotParse)?;
        let after = crate::analyze_as(self.language, display, &source, config)
            .map_err(|error| Withheld::Unanalysable(error.to_string()))?
            .findings;

        let targeted: BTreeSet<Identity> =
            resolution.kept.iter().map(|edit| edit.answers).collect();
        // Counted, not collected into a set: two findings can share an identity (two
        // identical statements in one function), and an edit that adds another copy
        // has introduced a finding even though its identity was already known.
        let mut known: BTreeMap<Identity, usize> = BTreeMap::new();
        for finding in before
            .iter()
            .filter(|finding| !finding.rule.is_project_scoped())
        {
            *known
                .entry(Identity::resolved(finding, self.source))
                .or_default() += 1;
        }

        let mut survives = Vec::new();
        let mut introduced = Vec::new();
        let mut seen: BTreeMap<Identity, usize> = BTreeMap::new();
        for finding in after
            .iter()
            .filter(|finding| !finding.rule.is_project_scoped())
        {
            let identity = Identity::resolved(finding, &source);
            if targeted.contains(&identity) {
                survives.push(finding.clone());
                continue;
            }
            let count = seen.entry(identity).or_default();
            *count += 1;
            if *count > known.get(&identity).copied().unwrap_or(0)
                && !finding.rule.spec().autofix.is_automatic()
            {
                introduced.push(finding.clone());
            }
        }
        if !survives.is_empty() {
            return Err(Withheld::Survives(survives));
        }
        if !introduced.is_empty() {
            return Err(Withheld::Introduces(introduced));
        }
        Ok(Verified {
            source,
            findings: after,
        })
    }

    pub fn profile(&self) -> &'static LanguageProfile {
        self.profile
    }

    /// Whether the file rqx analysed is still the file being edited. A span whose
    /// first byte starts no node came from a stale scan, and applying it would edit
    /// bytes no rule ever looked at. Anchored on the start alone because a span may
    /// deliberately cover a run of siblings, which no single node spans.
    fn anchored(&self, span: Span) -> Option<Node<'_>> {
        let node = self
            .tree
            .root_node()
            .descendant_for_byte_range(span.start as usize, span.start as usize)?;
        (node.start_byte() as u32 == span.start).then_some(node)
    }

    /// Every edit one finding asks for. More than one where a fix has to change two
    /// places at once — removing a parameter also removes the argument at every call
    /// site — because applying half of that pair is worse than applying neither.
    pub fn edit(&self, finding: &Finding) -> Vec<Edit> {
        let edits = self.edits_for(finding);
        // All of a finding's edits or none of them, as everywhere else.
        if edits
            .iter()
            .any(|edit| crate::notices::touches_protected(self.protected(), edit.span))
            || !self.keeps_invariants(&edits)
        {
            return Vec::new();
        }
        edits
    }

    fn protected(&self) -> &[Span] {
        self.protected.get_or_init(|| {
            crate::notices::protected_in(self.profile, self.source, self.tree.root_node())
        })
    }

    /// Whether one finding's edits, applied alone, leave in place everything
    /// `Invariants` names. Checked per finding so one edit that would break the build
    /// does not cost the file every other fix; `rqx fix` checks the combined result
    /// again before writing.
    fn keeps_invariants(&self, edits: &[Edit]) -> bool {
        if edits.is_empty() {
            return true;
        }
        let Some(after) = apply(self.source, &resolve(edits.to_vec())) else {
            return false;
        };
        let before = self
            .invariants
            .get_or_init(|| Invariants::of_tree(self.profile, self.source, self.tree.root_node()));
        Invariants::of(self.language, self.markup, &after)
            .is_ok_and(|after| before.broken_by(&after).is_none())
    }

    fn edits_for(&self, finding: &Finding) -> Vec<Edit> {
        if !finding.rule.spec().autofix.is_automatic() {
            return Vec::new();
        }
        let span = Span::new(finding.location.start, finding.location.end);
        let Some(node) = self.anchored(span) else {
            return Vec::new();
        };
        match finding.rule {
            RuleId::UnreachableCode
                if crate::removal::still_matters(
                    self.profile,
                    self.source,
                    spanning(node, span),
                ) =>
            {
                Vec::new()
            }
            // The span is already the extent to remove.
            RuleId::UnreachableCode
            | RuleId::JournalComment
            | RuleId::RedundantComment
            | RuleId::BannerComment
            | RuleId::ChatResidue
            | RuleId::RedundantJump
            | RuleId::CommentedOutCode => {
                vec![self.deletion(finding, whole_lines(self.source, span))]
            }
            RuleId::SkippedTest | RuleId::FocusedTest => {
                self.unfocus(finding, node).into_iter().collect()
            }
            RuleId::UnusedLocal => self.unused_local(finding, node).into_iter().collect(),
            RuleId::UnusedPrivateMember => self.unused_member(finding, node).into_iter().collect(),
            RuleId::UnusedParameter => self.unused_parameter(finding, node),
            _ => Vec::new(),
        }
    }

    /// The parameter and the argument at every call site, or nothing. The fix is only
    /// offered at `Verified`, which the rule restricts to a trailing parameter of a
    /// function no other file can call — so every caller is in this file and can be
    /// found. A mention that is not one of those calls means the function is passed
    /// around, or called through a receiver that may be something else, and its shape
    /// is not ours to change.
    fn unused_parameter(&self, finding: &Finding, name: Node<'_>) -> Vec<Edit> {
        if finding.confidence != Confidence::Verified {
            return Vec::new();
        }
        let Evidence::UnusedParameter { position, .. } = finding.evidence else {
            return Vec::new();
        };
        let Some(parameter) = climb(name, |node| {
            self.profile.parameter_kinds.contains(&kind_of(node))
        }) else {
            return Vec::new();
        };
        let Some(list) = parameter
            .parent()
            .filter(|node| self.profile.parameter_list_kinds.contains(&kind_of(node)))
        else {
            return Vec::new();
        };
        let Some(function) = climb(list, |node| self.profile.is_function(node)) else {
            return Vec::new();
        };
        // Go writes results and receivers as parameter lists too. Only the list the
        // grammar names `parameters` is the one the call sites pass arguments to.
        if function
            .child_by_field_name("parameters")
            .is_some_and(|parameters| parameters.id() != list.id())
        {
            return Vec::new();
        }
        let Some(called) = function
            .child_by_field_name("name")
            .map(|node| text_of(node, self.source))
        else {
            return Vec::new();
        };

        if self.body_reads(function, parameter) {
            return Vec::new();
        }
        let Some(calls) = self.every_call_of(function, called) else {
            return Vec::new();
        };

        let mut edits =
            vec![self.deletion(finding, with_separator(self.source, span_of(parameter)))];
        for call in calls {
            let mut cursor = call.walk();
            let Some(arguments) = call
                .named_children(&mut cursor)
                .find(|node| self.profile.argument_list_kinds.contains(&kind_of(node)))
            else {
                return Vec::new();
            };
            let mut inner = arguments.walk();
            let passed: Vec<Node<'_>> = arguments
                .named_children(&mut inner)
                .filter(|node| !self.profile.is_comment(*node))
                .collect();
            // A named or spread argument does not sit at the parameter's position.
            if passed
                .iter()
                .any(|argument| UNPOSITIONED_ARGUMENTS.contains(&kind_of(*argument)))
            {
                return Vec::new();
            }
            // Fewer arguments than parameters means a default is in play, and removing
            // nothing is the correct edit for this site.
            // `position` counts from one, the way the report says "parameter 2 of 2".
            if let Some(argument) = passed.get(position.saturating_sub(1) as usize) {
                edits.push(self.deletion(finding, with_separator(self.source, span_of(*argument))));
            }
        }
        edits
    }

    /// Every call of `function` in this file, when those calls are every mention of
    /// its name outside comments. A method counts as called only through `this` or,
    /// where the language allows it, by its bare name; a function only by its bare
    /// name. Anything else — the name passed as a value, a call through another
    /// receiver, an overload, a shadowing local — leaves a caller unaccounted for.
    fn every_call_of(&self, function: Node<'_>, called: &str) -> Option<Vec<Node<'_>>> {
        let method = self.is_member(function);
        let mut calls = Vec::new();
        for call in crate::tree::descendants(self.tree.root_node())
            .into_iter()
            .filter(|node| self.profile.is_call(*node))
        {
            let Some((receiver, name)) = self.callee_of(call) else {
                continue;
            };
            if name != called {
                continue;
            }
            let resolved = match receiver {
                None => !method || matches!(self.language, Language::Java | Language::Cpp),
                Some(receiver) => method && text_of(receiver, self.source) == "this",
            };
            if !resolved {
                return None;
            }
            calls.push(call);
        }
        let word = called.trim_start_matches('#');
        (self.mentions_outside_comments(word) == 1 + calls.len()).then_some(calls)
    }

    /// What a call calls, as the receiver it goes through, if any, and the name.
    fn callee_of<'tree>(&self, call: Node<'tree>) -> Option<(Option<Node<'tree>>, &'src str)> {
        let callee = call.child_by_field_name(self.profile.callee_field)?;
        if self.profile.is_member_access(callee) {
            let property = callee.child_by_field_name(self.profile.property_field)?;
            return Some((callee.named_child(0), text_of(property, self.source)));
        }
        if self.profile.is_identifier(callee) {
            // Java names the method and the receiver as two fields of the call itself.
            return Some((
                call.child_by_field_name("object"),
                text_of(callee, self.source),
            ));
        }
        None
    }

    /// Whether `function` is declared in a class body rather than at the top level or
    /// inside another function.
    fn is_member(&self, function: Node<'_>) -> bool {
        std::iter::successors(function.parent(), Node::parent)
            .find(|node| self.profile.is_class(*node) || self.profile.is_function(*node))
            .is_some_and(|node| self.profile.is_class(node))
    }

    /// How many times `word` appears as a whole word outside comments, strings
    /// included: a name in a string may be looked up by reflection.
    fn mentions_outside_comments(&self, word: &str) -> usize {
        self.mentions_within(self.tree.root_node(), word)
    }

    /// `mentions_outside_comments`, counted inside `node` only.
    fn mentions_within(&self, node: Node<'_>, word: &str) -> usize {
        let comments: Vec<Span> = crate::tree::descendants(node)
            .into_iter()
            .filter(|node| self.profile.is_comment(*node))
            .map(span_of)
            .collect();
        let in_comment = |at: usize| {
            comments
                .iter()
                .any(|span| span.start as usize <= at && at < span.end as usize)
        };
        let is_word =
            |character: char| character.is_alphanumeric() || character == '_' || character == '$';
        let start = node.start_byte();
        let text = &self.source[start..node.end_byte()];
        text.match_indices(word)
            .map(|(at, _)| start + at)
            .filter(|at| {
                let before = self.source[..*at].chars().next_back();
                let after = self.source[at + word.len()..].chars().next();
                !before.is_some_and(is_word) && !after.is_some_and(is_word) && !in_comment(*at)
            })
            .count()
    }

    /// Whether the function's body mentions any name the parameter declares. The
    /// rule names the parameter by the first identifier it finds, which in some
    /// grammars is the type (`CheckPoints* check_points`), so the body is read again
    /// for every name outside the parameter's type and default value.
    fn body_reads(&self, function: Node<'_>, parameter: Node<'_>) -> bool {
        let Some(body) = function.child_by_field_name("body") else {
            return true;
        };
        let skipped: Vec<Span> = ["type", "value", "default_value"]
            .iter()
            .filter_map(|field| parameter.child_by_field_name(field))
            .map(span_of)
            .collect();
        crate::tree::descendants(parameter)
            .into_iter()
            .filter(|node| self.profile.is_identifier(*node))
            .filter(|node| {
                let span = span_of(*node);
                !skipped
                    .iter()
                    .any(|outer| outer.start <= span.start && span.end <= outer.end)
            })
            .any(|name| self.mentions_within(body, text_of(name, self.source)) > 0)
    }

    fn deletion(&self, finding: &Finding, span: Span) -> Edit {
        Edit::deletion(finding, Identity::resolved(finding, self.source), span)
    }

    fn replacement(&self, finding: &Finding, span: Span, text: &str) -> Edit {
        Edit::new(
            finding,
            Identity::resolved(finding, self.source),
            span,
            text,
        )
    }

    /// The finding points at the name; the edit has to remove what declares it. A
    /// declaration whose value calls something is left alone — the binding is unused,
    /// but the call may be why the line is there. So is a Python name a nested
    /// function declares `nonlocal` or `global`: that statement needs the binding,
    /// and Python refuses to compile it without one.
    fn unused_local(&self, finding: &Finding, name: Node<'_>) -> Option<Edit> {
        if crate::invariants::declared_nonlocal(self.source, self.tree.root_node())
            .contains(text_of(name, self.source))
        {
            return None;
        }
        let declarator = climb(name, |node| {
            self.profile.declarator_kinds.contains(&kind_of(node))
                || self.profile.local_declaring_kinds.contains(&kind_of(node))
        })?;
        // A declaration outside a function belongs to the module or package, and
        // another file may read it.
        climb(declarator, |node| self.profile.is_function(node))?;
        if self.has_effect(declarator, name) {
            return None;
        }

        let statement = climb(declarator, |node| self.profile.is_statement(node))?;
        let siblings = self.declarators_in(statement);
        if siblings > 1 {
            // Removing one of several takes its separator with it, or the list is
            // left with a hole a parser will not accept.
            let span = with_separator(self.source, span_of(declarator));
            return Some(self.deletion(finding, span));
        }
        Some(self.deletion(finding, whole_lines(self.source, span_of(statement))))
    }

    /// A member whose finding names the whole declaration can go as it stands; the
    /// variant reported on an assignment inside a constructor names a narrower span
    /// that is a statement in someone else's body, so it is left to a person.
    fn unused_member(&self, finding: &Finding, node: Node<'_>) -> Option<Edit> {
        let kind = kind_of(node);
        let declares = self.profile.field_kinds.contains(&kind)
            || self.profile.function_kinds.contains(&kind)
            || self.profile.decorated_kinds.contains(&kind);
        declares.then(|| self.deletion(finding, whole_lines(self.source, span_of(node))))
    }

    /// `it.only(...)` becomes `it(...)`: the focus is the property, so removing it
    /// leaves the call the author meant to write.
    fn unfocus(&self, finding: &Finding, node: Node<'_>) -> Option<Edit> {
        if self.profile.annotation_kinds.contains(&kind_of(node)) {
            return Some(self.deletion(finding, whole_lines(self.source, span_of(node))));
        }
        let call = if self.profile.is_call(node) {
            node
        } else {
            climb(node, |parent| self.profile.is_call(parent))?
        };
        let callee = call.child_by_field_name("function")?;
        if !self.profile.is_member_access(callee) {
            return None;
        }
        let object = callee.child_by_field_name("object")?;
        Some(self.replacement(finding, span_of(callee), text_of(object, self.source)))
    }

    /// Whether removing this declaration would also remove work. The name being
    /// declared is skipped, so `const handler = () => {}` does not read as a call. An
    /// index can raise or run a lookup method, and in CPython a binding that is never
    /// read still keeps its value alive until the function returns, which
    /// `functools.lru_cache` relies on to hold off `__del__` mid-update.
    fn has_effect(&self, declarator: Node<'_>, name: Node<'_>) -> bool {
        let mut cursor = declarator.walk();
        let mut stack: Vec<Node<'_>> = declarator.children(&mut cursor).collect();
        while let Some(node) = stack.pop() {
            if node.id() == name.id() {
                continue;
            }
            if self.profile.is_call(node)
                || self.profile.is_function(node)
                || INDEXING.contains(&kind_of(node))
            {
                return true;
            }
            let mut inner = node.walk();
            stack.extend(node.children(&mut inner));
        }
        false
    }

    fn declarators_in(&self, statement: Node<'_>) -> usize {
        let mut cursor = statement.walk();
        let mut stack: Vec<Node<'_>> = statement.children(&mut cursor).collect();
        let mut found = 0;
        while let Some(node) = stack.pop() {
            if self.profile.declarator_kinds.contains(&kind_of(node)) {
                found += 1;
                continue;
            }
            let mut inner = node.walk();
            stack.extend(node.children(&mut inner));
        }
        found.max(1)
    }

    /// Every edit this file's findings ask for, with the conflicts already settled.
    /// `allowed` is the project's `[fix]` table: a rule it refuses is left to a
    /// person even though the edit is decided.
    pub fn plan(&self, findings: &[Finding], allowed: &Fix) -> Resolution {
        resolve(
            findings
                .iter()
                .filter(|finding| allowed.admits(finding))
                .flat_map(|finding| self.edit(finding))
                .collect(),
        )
    }
}

/// An element read by index or key, in each grammar's spelling.
const INDEXING: &[&str] = &[
    "subscript",
    "subscript_expression",
    "index_expression",
    "array_access",
];

/// Arguments that bind by name or spread into several parameters, so their place in
/// the list says nothing about which parameter they fill.
const UNPOSITIONED_ARGUMENTS: &[&str] = &[
    "keyword_argument",
    "list_splat",
    "dictionary_splat",
    "spread_element",
    "variadic_argument",
];

/// The nearest ancestor the test accepts, starting with the node itself.
fn climb<'tree>(from: Node<'tree>, accept: impl Fn(Node<'tree>) -> bool) -> Option<Node<'tree>> {
    let mut node = from;
    loop {
        if accept(node) {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// The widest node that starts where `node` does and stays inside `span`: the
/// statement a finding names rather than the first token of it.
fn spanning(node: Node<'_>, span: Span) -> Node<'_> {
    let mut widest = node;
    while let Some(parent) = widest.parent() {
        if parent.start_byte() != widest.start_byte() || parent.end_byte() as u32 > span.end {
            break;
        }
        widest = parent;
    }
    widest
}

fn span_of(node: Node<'_>) -> Span {
    Span::new(node.start_byte() as u32, node.end_byte() as u32)
}

fn text_of<'src>(node: Node<'_>, source: &'src str) -> &'src str {
    span_of(node).text(source)
}

/// Widens to the separator that joins this item to the next, so removing one item
/// from a list does not leave the comma that introduced it.
fn with_separator(source: &str, span: Span) -> Span {
    let after = source[span.end as usize..]
        .char_indices()
        .find(|(_, character)| !character.is_whitespace());
    if let Some((offset, ',')) = after {
        let end = span.end as usize + offset + 1;
        let trailing = source[end..]
            .char_indices()
            .find(|(_, character)| !character.is_whitespace() && *character != '\n')
            .map_or(0, |(offset, _)| offset);
        return Span::new(span.start, (end + trailing) as u32);
    }
    let before = source[..span.start as usize]
        .char_indices()
        .rev()
        .find(|(_, character)| !character.is_whitespace());
    if let Some((offset, ',')) = before {
        return Span::new(offset as u32, span.end);
    }
    span
}

/// Which rules `edit` can answer, for callers that want to say so before parsing.
pub fn is_fixable(rule: RuleId) -> bool {
    rule.spec().autofix.is_automatic()
}

pub fn kind(rule: RuleId) -> Autofix {
    rule.spec().autofix
}

fn line_start(source: &str, at: usize) -> usize {
    source[..at].rfind('\n').map_or(0, |index| index + 1)
}

fn line_end(source: &str, at: usize) -> usize {
    source[at..]
        .find('\n')
        .map_or(source.len(), |index| at + index)
}

/// Widens a deletion to the lines it sits on when it has them to itself, so removing
/// a statement does not leave its indentation and a blank line behind. A span sharing
/// a line with live code keeps that line and gives up only the whitespace leading
/// into it.
fn whole_lines(source: &str, span: Span) -> Span {
    let start = span.start as usize;
    let end = span.end as usize;
    let head = line_start(source, start);
    let tail = line_end(source, end);

    if source[head..start].trim().is_empty() && source[end..tail].trim().is_empty() {
        let past_newline = if tail < source.len() { tail + 1 } else { tail };
        return Span::new(head as u32, past_newline as u32);
    }

    let lead = &source[head..start];
    let trimmed = lead.trim_end().len();
    Span::new((start - (lead.len() - trimmed)) as u32, span.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rqx_core::RqxConfig;
    use rqx_core::edit::apply;

    /// Every confidence, so each rule's edit is exercised; the floor has its own tests.
    fn every_confidence() -> RqxConfig {
        let mut config = RqxConfig::default();
        config.fix.min_confidence = Confidence::Low;
        config
    }

    fn fixed(language: Language, source: &str) -> String {
        let config = every_confidence();
        let analysis = crate::analyze_as(language, "fixture", source, &config).expect("analysis");
        let rewriter = Rewriter::new(language, false, source).expect("parse");
        let plan = rewriter.plan(&analysis.findings, &config.fix);
        apply(source, &plan).expect("edits within the source")
    }

    #[test]
    fn unreachable_code_loses_its_lines_and_its_indentation() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n";
        assert_eq!(
            fixed(Language::TypeScript, source),
            "function f() {\n  return 1\n}\n"
        );
    }

    #[test]
    fn a_commented_out_block_leaves_no_blank_lines() {
        let source = "\
const live = 1;
// const a = 1;
// const b = 2;
// const c = 3;
export const used = live;
";
        assert_eq!(
            fixed(Language::TypeScript, source),
            "const live = 1;\nexport const used = live;\n"
        );
    }

    #[test]
    fn a_trailing_comment_keeps_the_code_it_sits_beside() {
        let widened = whole_lines("call() // TODO: later\n", Span::new(7, 21));
        assert_eq!(widened, Span::new(6, 21));
    }

    #[test]
    fn the_last_line_of_a_file_without_a_newline_is_still_removable() {
        let source = "const a = 1\n// TODO: fix";
        let widened = whole_lines(source, Span::new(12, source.len() as u32));
        assert_eq!(widened, Span::new(12, source.len() as u32));
    }

    #[test]
    fn a_crlf_line_loses_both_of_its_endings() {
        let source = "const a = 1\r\n// TODO: fix\r\nconst b = 2\r\n";
        let widened = whole_lines(source, Span::new(13, 25));
        assert_eq!(
            &source[widened.start as usize..widened.end as usize],
            "// TODO: fix\r\n"
        );
    }

    fn verified(language: Language, source: &str) -> Result<Verified, Withheld> {
        let config = every_confidence();
        let analysis = crate::analyze_as(language, "fixture", source, &config).expect("analysis");
        let rewriter = Rewriter::new(language, false, source).expect("parse");
        let plan = rewriter.plan(&analysis.findings, &config.fix);
        rewriter.verify("fixture", &config, &analysis.findings, &plan)
    }

    #[test]
    fn a_clean_deletion_is_verified_and_its_finding_is_gone() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n";
        let verified = verified(Language::TypeScript, source).expect("verified");
        assert_eq!(verified.source, "function f() {\n  return 1\n}\n");
        assert!(
            verified
                .findings
                .iter()
                .all(|finding| finding.rule != RuleId::UnreachableCode),
            "{:?}",
            verified.findings
        );
    }

    #[test]
    fn a_deletion_that_makes_a_binding_unused_is_still_verified() {
        let source = "function f() {\n  const a = 1\n  return 1\n  console.log(a)\n}\n";
        let verified = verified(Language::TypeScript, source).expect("the next round's edit");
        assert!(
            verified
                .findings
                .iter()
                .any(|finding| finding.rule == RuleId::UnusedLocal),
            "{:?}",
            verified.findings
        );
    }

    #[test]
    fn an_edit_that_breaks_the_parse_is_withheld() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        let finding = analysis
            .findings
            .iter()
            .find(|finding| finding.rule == RuleId::UnreachableCode)
            .expect("an unreachable finding");
        let closing = source.rfind('}').unwrap() as u32;
        let broken = resolve(vec![Edit::deletion(
            finding,
            Identity::resolved(finding, source),
            Span::new(closing, closing + 1),
        )]);
        let rewriter = Rewriter::new(Language::TypeScript, false, source).unwrap();
        assert!(matches!(
            rewriter.verify("fixture", &config, &analysis.findings, &broken),
            Err(Withheld::DoesNotParse(_))
        ));
    }

    #[test]
    fn an_edit_that_leaves_its_finding_in_place_is_withheld() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        let finding = analysis
            .findings
            .iter()
            .find(|finding| finding.rule == RuleId::UnreachableCode)
            .expect("an unreachable finding");
        let last = source.len() as u32;
        let elsewhere = resolve(vec![Edit::deletion(
            finding,
            Identity::resolved(finding, source),
            Span::new(last - 1, last),
        )]);
        let rewriter = Rewriter::new(Language::TypeScript, false, source).unwrap();
        assert!(matches!(
            rewriter.verify("fixture", &config, &analysis.findings, &elsewhere),
            Err(Withheld::Survives(_))
        ));
    }

    /// The second `any` has the same rule, file and subject as the first, so a set
    /// of identities sees nothing new. Counting them sees one more.
    #[test]
    fn an_edit_that_repeats_a_known_finding_is_withheld() {
        let source = "export function f(a: any): number {\n  return 1\n  console.log(a)\n}\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        assert_eq!(
            analysis
                .findings
                .iter()
                .filter(|finding| finding.rule == RuleId::ImpreciseType)
                .count(),
            1
        );
        let dead = analysis
            .findings
            .iter()
            .find(|finding| finding.rule == RuleId::UnreachableCode)
            .expect("an unreachable finding");
        let answers = Identity::resolved(dead, source);
        let parameter_end = source.find("a: any").unwrap() as u32 + "a: any".len() as u32;
        let repeating = resolve(vec![
            Edit::deletion(
                dead,
                answers,
                whole_lines(source, Span::new(dead.location.start, dead.location.end)),
            ),
            Edit::new(
                dead,
                answers,
                Span::new(parameter_end, parameter_end),
                ", b: any",
            ),
        ]);
        let rewriter = Rewriter::new(Language::TypeScript, false, source).unwrap();
        match rewriter.verify("fixture", &config, &analysis.findings, &repeating) {
            Err(Withheld::Introduces(found)) => assert!(
                found
                    .iter()
                    .all(|finding| finding.rule == RuleId::ImpreciseType),
                "{found:?}"
            ),
            other => panic!("expected the repeated finding to be refused: {other:?}"),
        }
    }

    #[test]
    fn the_default_floor_leaves_heuristic_fixes_to_a_person() {
        let source = "const live = 1;\n// const a = 1;\n// const b = 2;\n// const c = 3;\nexport const used = live;\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        let rewriter = Rewriter::new(Language::TypeScript, false, source).unwrap();
        assert!(rewriter.plan(&analysis.findings, &config.fix).is_empty());
        assert_eq!(
            fixed(Language::TypeScript, source),
            "const live = 1;\nexport const used = live;\n"
        );
    }

    /// Deleting the divider is still the fix; the notice and the link are not part of
    /// it, whichever comment rule asks.
    #[test]
    fn a_licence_header_and_a_link_survive_every_comment_fix() {
        let source = "\
// Copyright 2013-2023 The Cobra Authors
//
// Licensed under the Apache License, Version 2.0 (the \"License\");
//      http://www.apache.org/licenses/LICENSE-2.0
// ======================================================

package cobra

// ---------------- helpers ----------------

func f() int {
\t// https://github.com/spf13/cobra/issues/1257
\t// \tflagName += \"=\"
\treturn 1
}
";
        let after = fixed(Language::Go, source);
        assert!(after.starts_with("// Copyright 2013-2023 The Cobra Authors\n//\n// Licensed under the Apache License, Version 2.0 (the \"License\");\n//      http://www.apache.org/licenses/LICENSE-2.0\n// ======================================================\n"), "{after}");
        assert!(
            after.contains("// https://github.com/spf13/cobra/issues/1257"),
            "{after}"
        );
        assert!(!after.contains("---- helpers ----"), "{after}");
    }

    /// `var preExecHookFn` in cobra's `command_win.go` is read by `command.go`, and a
    /// C++ global by every file that includes its `extern` declaration.
    #[test]
    fn a_package_level_declaration_is_never_an_unused_local() {
        for (language, source) in [
            (
                Language::Go,
                "package cobra\n\nvar preExecHookFn = func(c int) {}\n",
            ),
            (Language::Cpp, "int retry_limit = 3;\n"),
            (Language::Python, "retry_limit = 3\n"),
            (Language::TypeScript, "let retryLimit = 3\n"),
        ] {
            let config = RqxConfig::default();
            let analysis = crate::analyze_as(language, "fixture", source, &config).unwrap();
            assert!(
                !analysis
                    .findings
                    .iter()
                    .any(|finding| finding.rule == RuleId::UnusedLocal),
                "{language:?}: {:?}",
                analysis.findings
            );
            assert_eq!(fixed(language, source), source, "{language:?}");
        }
    }

    /// `flush` reads `counter` between the two writes, so the first is not dead. Only
    /// a name the function declares is judged.
    #[test]
    fn a_package_level_name_written_twice_is_not_a_dead_store() {
        let source = "\
package p

var counter int

func flush() { println(counter) }

func run() {
\tcounter = 1
\tflush()
\tcounter = 2
\tlocal := 1
\tlocal = 2
\tprintln(local)
}
";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::Go, "fixture", source, &config).unwrap();
        let stores: Vec<&str> = analysis
            .findings
            .iter()
            .filter(|finding| finding.rule == RuleId::DeadStore)
            .filter_map(|finding| match &finding.evidence {
                Evidence::Name { identifier, .. } => Some(identifier.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(stores, ["local"], "{:?}", analysis.findings);
    }

    /// The shape behind cobra's `completions.go:187: too many return values`: Go's
    /// results are a parameter list too, and the fixer deleted them as if they were
    /// unused parameters, with the "matching" argument at every call site.
    #[test]
    fn a_go_result_list_is_not_a_parameter_list() {
        let source = "\
package cobra

type Func func()

type Command struct{}

func (c *Command) get(name string) (Func, bool) {
\treturn nil, false
}

func check(args []string, lastArg string) (*Command, []string, string, error) {
\treturn nil, args, \"\", nil
}

func use(c *Command, args []string, last string) {
\tc.get(\"x\")
\t_, _, _, _ = check(args, last)
}
";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::Go, "fixture", source, &config).unwrap();
        let named: Vec<&Finding> = analysis
            .findings
            .iter()
            .filter(|finding| finding.rule == RuleId::UnusedParameter)
            .collect();
        assert!(
            named.iter().all(|finding| matches!(
                &finding.evidence,
                Evidence::UnusedParameter { name, .. } if name == "name" || name == "lastArg"
            )),
            "{named:?}"
        );
        let after = fixed(Language::Go, source);
        assert!(after.contains(") (Func, bool) {"), "{after}");
        assert!(
            after.contains("(*Command, []string, string, error) {"),
            "{after}"
        );
    }

    /// One source per language carrying all four syntactic-span rules, and the file
    /// each should become. Fixing the result again must change nothing.
    const CORPUS: [(Language, &str, &str); 5] = [
        (
            Language::TypeScript,
            "// @author someone\n// get user name\nfunction getUserName() {\n  return 'x'\n  console.log('dead')\n}\n// const a = 1;\n// const b = 2;\n// const c = 3;\n",
            "function getUserName() {\n  return 'x'\n}\n",
        ),
        (
            Language::Python,
            "# @author someone\n# get user name\ndef get_user_name():\n    return 'x'\n    print('dead')\n# a = 1\n# b = 2\n# c = 3\n",
            "def get_user_name():\n    return 'x'\n",
        ),
        (
            Language::Go,
            "package main\n\n// @author someone\n// get user name\nfunc getUserName() string {\n\treturn \"x\"\n\tprintln(\"dead\")\n}\n\n// a := first()\n// b := second()\n// c := third()\n",
            "package main\n\nfunc getUserName() string {\n\treturn \"x\"\n}\n\n",
        ),
        (
            Language::Java,
            "// @author someone\nclass Hygiene {\n    // get user name\n    String getUserName() {\n        return \"x\";\n        System.out.println(\"dead\");\n    }\n    // int a = 1;\n    // int b = 2;\n    // int c = 3;\n}\n",
            "class Hygiene {\n    String getUserName() {\n        return \"x\";\n    }\n}\n",
        ),
        (
            Language::Cpp,
            "// @author someone\n// get user name\nint getUserName() {\n    return 1;\n    printf(\"dead\");\n}\n// int a = 1;\n// int b = 2;\n// int c = 3;\n",
            "int getUserName() {\n    return 1;\n}\n",
        ),
    ];

    #[test]
    fn every_language_fixes_the_four_syntactic_rules_and_converges() {
        for (language, source, expected) in CORPUS {
            let once = verified(language, source)
                .unwrap_or_else(|withheld| panic!("{language:?}: {withheld}"));
            assert_eq!(once.source, expected, "{language:?}");
            let again = verified(language, &once.source).expect("a fixed file verifies");
            assert_eq!(again.source, expected, "{language:?} did not converge");
        }
    }

    #[test]
    fn an_unused_local_takes_its_declaration_with_it() {
        for (language, source, expected) in [
            (
                Language::TypeScript,
                "export function f(): number {\n  const spare = 2\n  return 1\n}\n",
                "export function f(): number {\n  return 1\n}\n",
            ),
            (
                Language::Python,
                "def f():\n    spare = 2\n    return 1\n",
                "def f():\n    return 1\n",
            ),
        ] {
            assert_eq!(fixed(language, source), expected, "{language:?}");
        }
    }

    #[test]
    fn a_declaration_that_calls_something_is_left_alone() {
        let source = "export function f(): number {\n  const spare = compute()\n  return 1\n}\n";
        assert_eq!(fixed(Language::TypeScript, source), source);
    }

    #[test]
    fn one_of_several_declarators_takes_its_comma() {
        let source = "export function f(): number {\n  let kept = 1, spare = 2\n  return kept\n}\n";
        assert_eq!(
            fixed(Language::TypeScript, source),
            "export function f(): number {\n  let kept = 1\n  return kept\n}\n"
        );
    }

    #[test]
    fn a_focused_test_loses_only_its_focus() {
        let source =
            "describe('s', () => {\n  it.only('works', () => {\n    expect(1).toBe(1)\n  })\n})\n";
        assert_eq!(
            fixed(Language::TypeScript, source),
            "describe('s', () => {\n  it('works', () => {\n    expect(1).toBe(1)\n  })\n})\n"
        );
    }

    #[test]
    fn a_dead_trailing_parameter_goes_from_the_signature_and_every_call() {
        let source = "\
function work(used: number, spare: number) {
  return used
}
export const total = work(1, 2) + work(3, 4)
";
        assert_eq!(
            fixed(Language::TypeScript, source),
            "\
function work(used: number) {
  return used
}
export const total = work(1) + work(3)
"
        );
    }

    /// The function is handed to something rather than called, so its shape is not
    /// this file's to change.
    #[test]
    fn a_function_passed_as_a_value_keeps_its_parameters() {
        let source = "\
function work(used: number, spare: number) {
  return used
}
export const deferred = [work]
";
        assert_eq!(fixed(Language::TypeScript, source), source);
    }

    /// `autofix: Deletion` is a promise that rqx can make the edit. A rule declaring
    /// one with no arm in `edit` reads as fixable everywhere it is shown — in the
    /// editor's UNNECESSARY tag, in `rqx explain`, in the `[fix] apply` set — and then
    /// silently produces nothing.
    #[test]
    fn every_rule_that_claims_a_mechanical_fix_has_one() {
        let answered = [
            RuleId::UnreachableCode,
            RuleId::JournalComment,
            RuleId::RedundantComment,
            RuleId::BannerComment,
            RuleId::ChatResidue,
            RuleId::RedundantJump,
            RuleId::CommentedOutCode,
            RuleId::UnusedLocal,
            RuleId::UnusedPrivateMember,
            RuleId::FocusedTest,
            RuleId::SkippedTest,
            RuleId::UnusedParameter,
        ];
        let claimed: Vec<&str> = RuleId::ALL
            .iter()
            .copied()
            .filter(|rule| rule.spec().autofix.is_automatic())
            .filter(|rule| !answered.contains(rule))
            .map(|rule| rule.slug())
            .collect();
        assert!(
            claimed.is_empty(),
            "these declare a mechanical fix and have no fixer: {claimed:?}"
        );
    }

    #[test]
    fn parses_clean_names_the_first_broken_node() {
        assert_eq!(
            parses_clean(Language::TypeScript, false, "const a = 1\n"),
            Ok(())
        );
        let fault = parses_clean(Language::TypeScript, false, "function f( {\n").unwrap_err();
        assert!(fault.span.end <= 14, "{fault}");
    }

    #[test]
    fn a_span_that_names_no_node_is_refused() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        let mut stale = analysis
            .findings
            .into_iter()
            .find(|finding| finding.rule == RuleId::UnreachableCode)
            .expect("an unreachable finding");
        stale.location.start += 3;

        let rewriter = Rewriter::new(Language::TypeScript, false, source).unwrap();
        assert!(rewriter.edit(&stale).is_empty());
    }

    #[test]
    fn a_rule_with_no_mechanical_answer_produces_nothing() {
        let source = "function f(a: number) {\n  if (a > 1) { if (a > 2) { if (a > 3) { return 1 } } }\n  return 0\n}\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        let rewriter = Rewriter::new(Language::TypeScript, false, source).unwrap();
        for finding in &analysis.findings {
            if !is_fixable(finding.rule) {
                assert!(rewriter.edit(finding).is_empty(), "{}", finding.rule);
            }
        }
    }
}

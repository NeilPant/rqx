//! Rules a project wrote, as tree-sitter queries. Every grammar rqx parses with is
//! already in the binary; this is the only thing that was missing to let a team ask
//! a question the catalogue will never ship.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rqx_core::config::CustomRule;
use rqx_core::finding::{Evidence, Finding};
use rqx_core::rule::RuleId;
use rqx_core::rules::FindingBuilder;
use rqx_core::span::Span;
use rqx_core::Language;
use tree_sitter::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor};

/// The capture that says where to report. Without one the match's first capture is
/// used, which is usually the outermost pattern and is what a reader expects.
const ANCHOR: &str = "site";

thread_local! {
    /// Compiling a query costs more than running it, and a worker runs every rule
    /// against every file it parses. Keyed by language too, because the same text
    /// compiles against five different grammars.
    static COMPILED: RefCell<HashMap<(Language, String), Rc<Query>>> = RefCell::new(HashMap::new());
}

/// Compile every rule once, so a typo in a query is a config error rather than a
/// silent rule that never fires.
pub fn validate(rules: &[CustomRule]) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::new();
    for rule in rules {
        if rule.id.trim().is_empty() {
            return Err("a `[[rule]]` has no `id`".to_owned());
        }
        if seen.contains(&rule.id.as_str()) {
            return Err(format!("two `[[rule]]` blocks are both called `{}`", rule.id));
        }
        seen.push(&rule.id);
        if rqx_core::rule::RuleId::ALL
            .iter()
            .any(|shipped| shipped.slug() == rule.id)
        {
            return Err(format!(
                "`{}` is already a rule rqx ships; pick another id",
                rule.id
            ));
        }
        if rule.languages.is_empty() {
            return Err(format!("`{}` names no languages", rule.id));
        }
        for language in &rule.languages {
            compile(*language, &rule.query).map_err(|error| {
                format!(
                    "`{}` will not compile against {}: {error}",
                    rule.id,
                    language.slug()
                )
            })?;
        }
    }
    Ok(())
}

fn compile(language: Language, source: &str) -> Result<Rc<Query>, String> {
    COMPILED.with_borrow_mut(|compiled| {
        if let Some(query) = compiled.get(&(language, source.to_owned())) {
            return Ok(Rc::clone(query));
        }
        let query = Query::new(&crate::grammar(language), source).map_err(|error| {
            format!(
                "{} at row {}, column {}",
                error.message, error.row, error.column
            )
        })?;
        let query = Rc::new(query);
        compiled.insert((language, source.to_owned()), Rc::clone(&query));
        Ok(query)
    })
}

pub fn run(
    language: Language,
    source: &str,
    root: Node<'_>,
    rules: &[CustomRule],
    builder: &FindingBuilder<'_>,
    out: &mut Vec<Finding>,
) {
    for rule in rules {
        if !rule.languages.contains(&language) || !builder.config.rules.enabled_custom(rule) {
            continue;
        }
        // Already compiled by `validate` before the scan started, so a failure here
        // cannot happen and reporting it per file would report it per file.
        let Ok(query) = compile(language, &rule.query) else {
            continue;
        };
        let anchor = query.capture_index_for_name(ANCHOR);
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source.as_bytes());
        while let Some(found) = matches.next() {
            let Some(at) = anchor
                .and_then(|wanted| {
                    found
                        .captures
                        .iter()
                        .find(|capture| capture.index == wanted)
                })
                .or_else(|| found.captures.first())
            else {
                continue;
            };
            let span = Span::new(at.node.start_byte() as u32, at.node.end_byte() as u32);
            out.push(Finding {
                rule: RuleId::Custom,
                location: builder.range(span),
                severity: rule.severity,
                confidence: rule.confidence,
                evidence: Evidence::Custom {
                    id: rule.id.as_str().into(),
                    message: rule.message.clone(),
                    family: rule.family,
                },
                fix_hint: rule.hint.clone(),
                related: Vec::new(),
                known: false,
            });
        }
    }
}

/// One match of an ad-hoc query, flattened so a caller that has no grammar can print
/// it. Used by `rqx query`, which exists so a rule can be tried before it is written
/// into a config.
pub struct Match {
    pub line: u32,
    pub column: u32,
    pub captures: Vec<Capture>,
}

pub struct Capture {
    pub name: String,
    pub kind: &'static str,
    pub text: String,
}

/// Run a query over one file's text. The query is compiled per call rather than
/// cached: this runs once per file for one query, and a cache keyed on the text
/// would outlive its usefulness immediately.
pub fn probe(language: Language, query: &str, source: &str) -> Result<Vec<Match>, String> {
    let query = compile(language, query)?;
    let profile = crate::support(language).profile;
    let tree = crate::parse(language, profile, source).map_err(|error| error.to_string())?;
    let index = rqx_core::line_index::LineIndex::new(source);
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut found = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut out = Vec::new();
    while let Some(one) = found.next() {
        let Some(first) = one.captures.first() else {
            continue;
        };
        let (line, column) = index.locate(first.node.start_byte() as u32);
        out.push(Match {
            line,
            column,
            captures: one
                .captures
                .iter()
                .map(|capture| Capture {
                    name: names[capture.index as usize].to_owned(),
                    kind: capture.node.kind(),
                    text: capture
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or_default()
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                })
                .collect(),
        });
    }
    Ok(out)
}

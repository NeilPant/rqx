use std::collections::BTreeSet;
use std::path::PathBuf;

use lsp_server::RequestId;
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Command, CreateFile,
    CreateFileOptions, DocumentChangeOperation, DocumentChanges, ExecuteCommandParams, OneOf,
    OptionalVersionedTextDocumentIdentifier, Position, Range, ResourceOp, TextDocumentEdit,
    TextEdit, WorkspaceEdit,
};
use rqx_core::config::CONFIG_FILE_NAME;
use rqx_core::rule::{Category, RuleId};
use serde_json::{Value, json};

use super::Server;
use super::protocol::{range_at, uri_of};
use crate::report::projection;

pub const SCAN: &str = "rqx.scan";
pub const CHECK: &str = "rqx.check";
pub const EXPLAIN: &str = "rqx.explain";
pub const START_HERE: &str = "rqx.startHere";
pub const CATALOGUE: &str = "rqx.catalogue";
pub const DISABLE_RULE: &str = "rqx.disableRule";
pub const DISABLE_CATEGORY: &str = "rqx.disableCategory";
pub const TOGGLE_PATTERNS: &str = "rqx.togglePatterns";
pub const TOGGLE_CODE_LENS: &str = "rqx.toggleCodeLens";

pub fn names() -> Vec<String> {
    [
        SCAN,
        CHECK,
        EXPLAIN,
        START_HERE,
        CATALOGUE,
        DISABLE_RULE,
        DISABLE_CATEGORY,
        TOGGLE_PATTERNS,
        TOGGLE_CODE_LENS,
    ]
    .iter()
    .map(|name| (*name).to_owned())
    .collect()
}

pub enum Outcome {
    Answered(Value),
    /// The worker will send the response once its scan finishes.
    Deferred,
    /// The worker could not take the job, and the caller owes an answer now.
    Refused(super::Refused),
    Unknown,
}

/// A refusal is an error rather than a success carrying an `error` field, because a
/// client cannot tell the second one from a real answer without knowing our shape.
pub fn refusal(refused: super::Refused) -> lsp_server::Response {
    match refused {
        super::Refused::Coalesced(id) => {
            lsp_server::Response::new_ok(id, json!({ "coalesced": true }))
        }
        super::Refused::Dead(id) => lsp_server::Response::new_err(
            id,
            lsp_types::error_codes::REQUEST_FAILED as i32,
            "rqx is busy with a different job, or its scanner stopped".to_owned(),
        ),
    }
}

pub fn execute(server: &mut Server, params: &ExecuteCommandParams, id: RequestId) -> Outcome {
    let first = params.arguments.first();
    match params.command.as_str() {
        SCAN => match server.scan(Some(id)) {
            Some(refused) => Outcome::Refused(refused),
            None => Outcome::Deferred,
        },
        CHECK => {
            let base = as_str(first).unwrap_or("origin/HEAD").to_owned();
            match server.review(&base, Some(id)) {
                Some(refused) => Outcome::Refused(refused),
                None => Outcome::Deferred,
            }
        }
        EXPLAIN => Outcome::Answered(explain(first)),
        START_HERE => Outcome::Answered(json!(server.start_here())),
        CATALOGUE => Outcome::Answered(projection::catalogue()),
        DISABLE_RULE => Outcome::Answered(disable(server, first, false)),
        DISABLE_CATEGORY => Outcome::Answered(disable(server, first, true)),
        TOGGLE_PATTERNS => {
            let on = server.toggle_patterns();
            Outcome::Answered(json!({ "patterns": on }))
        }
        TOGGLE_CODE_LENS => {
            let on = server.toggle_code_lens();
            Outcome::Answered(json!({ "codeLens": on }))
        }
        _ => Outcome::Unknown,
    }
}

fn as_str(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

fn explain(argument: Option<&Value>) -> Value {
    let Some(slug) = as_str(argument) else {
        return json!({ "error": "rqx.explain needs a rule slug" });
    };
    projection::rule(slug).unwrap_or_else(|| json!({ "error": format!("no rule called `{slug}`") }))
}

/// The command path has to ask the client to apply the edit itself. A code action can
/// hand one back for the client to apply, but the result of `executeCommand` is just a
/// value — returning an edit there writes nothing.
fn disable(server: &Server, argument: Option<&Value>, family: bool) -> Value {
    let Some(name) = as_str(argument) else {
        return json!({ "error": "nothing named to disable" });
    };
    let known = if family {
        Category::ALL.iter().any(|entry| entry.slug() == name)
    } else {
        RuleId::ALL.iter().any(|rule| rule.spec().slug == name)
    };
    if !known {
        return json!({ "error": format!("`{name}` is not a rule or a family") });
    }
    let Some(edit) = config_edit(&server.root().join(CONFIG_FILE_NAME), name) else {
        return json!({ "error": format!("`{name}` is already disabled, or RQX.toml has a shape rqx will not edit blind") });
    };
    server.apply_edit(format!("rqx: disable `{name}`"), edit);
    json!({ "disabled": name })
}

/// An insertion rather than a re-serialisation: `rqx init` writes a line of prose per
/// table, and round-tripping the file through a TOML writer would delete all of it.
fn config_edit(path: &PathBuf, name: &str) -> Option<WorkspaceEdit> {
    let uri = uri_of(path)?;
    let existing = std::fs::read_to_string(path).ok();
    let edit = insertion(existing.as_deref().unwrap_or_default(), name)?;

    // `changes` cannot create a file, so a project with no RQX.toml — the default —
    // would silently apply nothing.
    let mut operations = Vec::new();
    if existing.is_none() {
        operations.push(DocumentChangeOperation::Op(ResourceOp::Create(
            CreateFile {
                uri: uri.clone(),
                options: Some(CreateFileOptions {
                    overwrite: Some(false),
                    ignore_if_exists: Some(true),
                }),
                annotation_id: None,
            },
        )));
    }
    operations.push(DocumentChangeOperation::Edit(TextDocumentEdit {
        text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
        edits: vec![OneOf::Left(edit)],
    }));

    Some(WorkspaceEdit {
        document_changes: Some(DocumentChanges::Operations(operations)),
        ..WorkspaceEdit::default()
    })
}

/// Finds `disabled = [...]` under `[rules]` and inserts into it, adds the key under an
/// existing `[rules]`, or appends the whole table — in that order, because a second
/// `[rules]` header is a TOML error rather than a merge. A shape it cannot read safely
/// yields nothing, since a wrong edit leaves the project with an `RQX.toml` that no
/// longer parses.
fn insertion(source: &str, name: &str) -> Option<TextEdit> {
    let mut in_rules = false;
    let mut rules_header: Option<u32> = None;
    for (number, line) in source.lines().enumerate() {
        let body = line.trim_start();
        if body.starts_with('[') {
            in_rules = body.trim_end().starts_with("[rules]");
            if in_rules {
                if rules_header.is_some() {
                    return None;
                }
                rules_header = Some(number as u32);
            }
            continue;
        }
        if !in_rules {
            continue;
        }
        let Some(rest) = body.strip_prefix("disabled") else {
            continue;
        };
        let Some(open) = rest.find('[') else { continue };
        // An array spread over several lines cannot be edited by looking at one of
        // them, and falling through to the append branch would write a second
        // `disabled` key — a duplicate key, which stops the whole file parsing.
        let close = rest.find(']')?;

        let entries = &rest[open + 1..close];
        if entries
            .split(',')
            .any(|entry| entry.trim().trim_matches(['"', '\'']) == name)
        {
            return None;
        }

        let indent = line.len() - body.len();
        let column = (indent + "disabled".len() + open + 1) as u32;
        let text = if entries.trim().is_empty() {
            format!("\"{name}\"")
        } else {
            format!("\"{name}\", ")
        };
        return Some(TextEdit {
            range: at(number as u32, column),
            new_text: text,
        });
    }

    match rules_header {
        Some(line) => Some(TextEdit {
            range: at(line + 1, 0),
            new_text: format!("disabled = [\"{name}\"]\n"),
        }),
        None => {
            let lines = source.lines().count() as u32;
            let separator = if source.is_empty() || source.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            Some(TextEdit {
                range: at(lines, 0),
                new_text: format!("{separator}\n[rules]\ndisabled = [\"{name}\"]\n"),
            })
        }
    }
}

fn at(line: u32, character: u32) -> Range {
    let position = Position { line, character };
    Range {
        start: position,
        end: position,
    }
}

/// Config actions for the rules whose fix is a decision, and the edit itself for the
/// ones it is not. A source edit is offered only after the same verification `rqx fix`
/// runs, so accepting one from an editor is no less proven than accepting one from the
/// command line.
pub fn actions_for(server: &Server, params: &CodeActionParams) -> Vec<CodeActionOrCommand> {
    let config = server.root().join(CONFIG_FILE_NAME);
    let mut actions = source_fixes(server, params);
    // The client asks for actions on every cursor move, and the same rule usually
    // appears on several of the diagnostics in range.
    let mut seen = BTreeSet::new();
    let configuration = params
        .context
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.source.as_deref() == Some(super::diagnostics::SOURCE))
        .filter_map(|diagnostic| {
            let data = diagnostic.data.as_ref()?;
            let rule = data.get("rule")?.as_str()?;
            let category = data.get("category")?.as_str()?;
            seen.insert((rule.to_owned(), category.to_owned()))
                .then(|| (rule.to_owned(), category.to_owned()))
        })
        .flat_map(|(rule, category)| {
            [
                config_edit(&config, &rule)
                    .map(|edit| action(format!("rqx: disable `{rule}`"), edit)),
                config_edit(&config, &category)
                    .map(|edit| action(format!("rqx: disable the `{category}` family"), edit)),
                Some(CodeActionOrCommand::Command(Command {
                    title: format!("rqx: explain `{rule}`"),
                    command: EXPLAIN.to_owned(),
                    arguments: Some(vec![json!(rule)]),
                })),
            ]
            .into_iter()
            .flatten()
        })
        .collect::<Vec<_>>();
    actions.extend(configuration);
    actions
}

/// One parse and one verification of the open buffer, so an editor never offers an
/// edit the command line would withhold.
fn source_fixes(server: &Server, params: &CodeActionParams) -> Vec<CodeActionOrCommand> {
    let Some(path) = super::protocol::path_from_uri(&params.text_document.uri) else {
        return Vec::new();
    };
    let Some(language) = rqx_core::Language::of_path(&path) else {
        return Vec::new();
    };
    let Some(source) = server.text_of(&path) else {
        return Vec::new();
    };
    let settings = server.settings();
    let fixable: Vec<_> = server
        .findings_of(&path, &source)
        .into_iter()
        .filter(|finding| settings.for_file(&path).fix.allows(finding.rule))
        .collect();
    if fixable.is_empty() {
        return Vec::new();
    }
    let Ok(rewriter) = rqx_lang::rewrite::Rewriter::new(language, &source) else {
        return Vec::new();
    };

    let index = rqx_core::LineIndex::new(&source);
    let requested = params.range;
    let display = path
        .strip_prefix(server.root())
        .unwrap_or(&path)
        .display()
        .to_string();

    let mut actions = Vec::new();
    for finding in &fixable {
        let range = range_at(&index, server.encoding(), &finding.location);
        if range.end < requested.start || range.start > requested.end {
            continue;
        }
        let plan = rewriter.plan(std::slice::from_ref(finding), &settings.for_file(&path).fix);
        if plan.is_empty()
            || rewriter
                .verify(&display, settings.for_file(&path), &fixable, &plan)
                .is_err()
        {
            continue;
        }
        let edits: Vec<TextEdit> = plan
            .kept
            .iter()
            .map(|edit| TextEdit {
                range: range_at(
                    &index,
                    server.encoding(),
                    &span_range(&index, &finding.location.file, edit.span),
                ),
                new_text: edit.replacement.clone(),
            })
            .collect();
        actions.push(source_action(
            format!("rqx: fix `{}`", finding.rule.slug()),
            WorkspaceEdit {
                changes: Some(
                    [(params.text_document.uri.clone(), edits)]
                        .into_iter()
                        .collect(),
                ),
                ..WorkspaceEdit::default()
            },
        ));
    }
    actions
}

fn span_range(
    index: &rqx_core::LineIndex<'_>,
    file: &std::sync::Arc<str>,
    span: rqx_core::span::Span,
) -> rqx_core::finding::SourceRange {
    let (line, column) = index.locate(span.start);
    let (end_line, _) = index.locate(span.end);
    rqx_core::finding::SourceRange {
        file: std::sync::Arc::clone(file),
        start: span.start,
        end: span.end,
        line,
        column,
        end_line,
    }
}

fn action(title: String, edit: WorkspaceEdit) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        edit: Some(edit),
        ..CodeAction::default()
    })
}

/// Tagged `source.fixAll` as well as quick fix, so an editor can run every one of
/// them on save without offering the config actions alongside.
fn source_action(title: String, edit: WorkspaceEdit) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(CodeActionKind::SOURCE_FIX_ALL),
        edit: Some(edit),
        ..CodeAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies the edit the way a client would, then parses the result — an edit that
    /// produces a file `RQX.toml` cannot read is worse than no quick fix at all.
    fn applied(source: &str, name: &str) -> String {
        let edit = insertion(source, name).expect("an edit");
        let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
        let line = edit.range.start.line as usize;
        while lines.len() <= line {
            lines.push(String::new());
        }
        let column = (edit.range.start.character as usize).min(lines[line].len());
        lines[line].insert_str(column, &edit.new_text);
        let out = lines.join("\n");
        out.parse::<toml::Table>()
            .unwrap_or_else(|error| panic!("the edit broke the file: {error}\n{out}"));
        out
    }

    #[test]
    fn an_empty_array_gains_the_first_entry() {
        let out = applied("[rules]\ndisabled = []\n", "magic-number");
        assert!(out.contains("disabled = [\"magic-number\"]"), "{out}");
    }

    #[test]
    fn a_populated_array_keeps_what_was_there() {
        let out = applied("[rules]\ndisabled = [\"long-function\"]\n", "magic-number");
        assert!(
            out.contains("disabled = [\"magic-number\", \"long-function\"]"),
            "{out}"
        );
    }

    /// The template writes a line of prose above every key, so an edit that dropped
    /// comments would silently delete the explanation of the setting being changed.
    #[test]
    fn comments_survive_the_edit() {
        let source = "# why this table exists\n[rules]\n# slugs or families\ndisabled = []\n";
        let out = applied(source, "magic-number");
        assert!(out.contains("# why this table exists"), "{out}");
        assert!(out.contains("# slugs or families"), "{out}");
    }

    #[test]
    fn a_file_without_the_table_gains_one() {
        let out = applied("[scan]\nthreads = 4\n", "magic-number");
        assert!(out.contains("[rules]"), "{out}");
        assert!(out.contains("disabled = [\"magic-number\"]"), "{out}");
    }

    #[test]
    fn a_rule_already_disabled_is_not_added_twice() {
        assert!(insertion("[rules]\ndisabled = [\"magic-number\"]\n", "magic-number").is_none());
    }

    /// The name appearing anywhere in the file used to count as already-disabled, so a
    /// commented-out example silently swallowed the quick fix.
    #[test]
    fn a_name_mentioned_elsewhere_does_not_count_as_disabled() {
        let source = "# e.g. disabled = [\"magic-number\"]\n[rules]\ndisabled = []\n";
        let out = applied(source, "magic-number");
        assert!(out.contains("disabled = [\"magic-number\"]"), "{out}");
    }

    /// A multi-line array cannot be edited from one line, and falling through to the
    /// append branch wrote a second `disabled` key — a duplicate that stops `RQX.toml`
    /// parsing at all.
    #[test]
    fn a_multi_line_array_is_left_alone_rather_than_duplicated() {
        let source = "[rules]\ndisabled = [\n  \"long-function\",\n]\n";
        assert!(insertion(source, "magic-number").is_none());
    }

    /// The insert column came from a two-sided trim, so a trailing space moved it past
    /// the closing bracket and produced `disabled = []\"magic-number\"`.
    #[test]
    fn trailing_whitespace_does_not_move_the_insert_point() {
        let out = applied("[rules]\n  disabled = []   \n", "magic-number");
        assert!(out.contains("disabled = [\"magic-number\"]"), "{out}");
    }

    #[test]
    fn a_second_rules_table_is_refused_rather_than_guessed_at() {
        assert!(insertion("[rules]\nenabled = []\n[a]\nx = 1\n[rules]\n", "x").is_none());
    }

    #[test]
    fn a_file_with_no_trailing_newline_keeps_its_last_line() {
        let out = applied("[scan]\nthreads = 4", "magic-number");
        assert!(out.contains("threads = 4"), "{out}");
        assert!(out.contains("[rules]"), "{out}");
    }
}

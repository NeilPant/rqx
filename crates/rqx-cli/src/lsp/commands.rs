use std::collections::BTreeSet;
use std::path::PathBuf;

use lsp_server::RequestId;
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Command, CreateFile,
    CreateFileOptions, DocumentChangeOperation, DocumentChanges, ExecuteCommandParams, OneOf,
    OptionalVersionedTextDocumentIdentifier, Position, Range, ResourceOp, TextDocumentEdit,
    TextEdit, WorkspaceEdit,
};
use rqx_core::rule::{Category, RuleId};
use serde_json::{Value, json};

use super::Server;
use super::protocol::{range_at, uri_of};
use super::worker::Task;
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
pub const FIX: &str = "rqx.fix";
pub const PLAN: &str = "rqx.plan";
pub const VERIFY: &str = "rqx.verify";
pub const BASELINE: &str = "rqx.baseline";
pub const SUPPRESS: &str = "rqx.suppress";
pub const CONFIG: &str = "rqx.config";
pub const FINDINGS: &str = "rqx.findings";

static FIX_ALL: std::sync::LazyLock<CodeActionKind> =
    std::sync::LazyLock::new(|| CodeActionKind::new("source.fixAll.rqx"));

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
        FIX,
        PLAN,
        VERIFY,
        BASELINE,
        SUPPRESS,
        CONFIG,
        FINDINGS,
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
        FIX => deferred(server.run(
            Task::Fix {
                write: as_bool(first).unwrap_or(false),
                only: strings(params.arguments.get(1)).into_iter().collect(),
            },
            Some(id),
        )),
        VERIFY => deferred(server.run(
            Task::Verify {
                base: as_str(first).unwrap_or("origin/HEAD").to_owned(),
                expect: strings(params.arguments.get(1)),
            },
            Some(id),
        )),
        BASELINE => deferred(server.run(
            Task::Baseline {
                update: as_bool(first).unwrap_or(false),
            },
            Some(id),
        )),
        PLAN => Outcome::Answered(plan(server, first, params.arguments.get(1))),
        SUPPRESS => Outcome::Answered(suppress(server, first)),
        CONFIG => Outcome::Answered(projection::config(server.settings())),
        FINDINGS => Outcome::Answered(findings(server, first)),
        _ => Outcome::Unknown,
    }
}

fn deferred(refused: Option<super::Refused>) -> Outcome {
    match refused {
        Some(refused) => Outcome::Refused(refused),
        None => Outcome::Deferred,
    }
}

fn as_bool(value: Option<&Value>) -> Option<bool> {
    value.and_then(Value::as_bool)
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn field<'a>(object: Option<&'a Value>, name: &str) -> Option<&'a Value> {
    object.and_then(|object| object.get(name))
}

/// Answered from the last scan rather than by scanning: a plan is a ranking of what
/// is already known, and asking for one should not cost a rescan.
fn plan(server: &Server, budget: Option<&Value>, limit: Option<&Value>) -> Value {
    let Some(outcome) = server.last_outcome() else {
        return json!({ "error": "nothing scanned yet" });
    };
    let budget = budget.and_then(Value::as_u64).map(|value| value as u32);
    let limit = limit
        .and_then(Value::as_u64)
        .map_or(20, |value| value as usize);
    projection::plan(&outcome, budget, limit)
}

fn findings(server: &Server, query: Option<&Value>) -> Value {
    let Some(outcome) = server.last_outcome() else {
        return json!({ "error": "nothing scanned yet" });
    };
    // Serde already knows the lowercase labels the CLI and the report use.
    let severity = field(query, "severity").and_then(|value| {
        serde_json::from_value::<rqx_core::finding::Severity>(value.clone()).ok()
    });
    let mut paths = strings(field(query, "glob"));
    if let Some(file) = as_str(field(query, "file")) {
        paths.push(file.to_owned());
    }
    let filter = match crate::report::FindingFilter::new(
        severity,
        &strings(field(query, "rule")),
        &strings(field(query, "category")),
        &paths,
    ) {
        Ok(filter) => filter,
        Err(error) => return json!({ "error": format!("{error:#}") }),
    };
    projection::findings(&outcome, &filter, server.settings())
}

/// A waiver is a comment in the source, so like disabling a rule it is an edit the
/// client applies rather than a file this server writes behind the editor's back.
fn suppress(server: &Server, request: Option<&Value>) -> Value {
    let Some(file) = as_str(field(request, "file")) else {
        return json!({ "error": "rqx.suppress needs a file" });
    };
    let Some(rule) = as_str(field(request, "rule")) else {
        return json!({ "error": "rqx.suppress needs a rule" });
    };
    let Some(reason) = as_str(field(request, "reason")).filter(|reason| !reason.trim().is_empty())
    else {
        return json!({ "error": "a waiver needs a reason, or rqx will not honour it" });
    };
    let line = field(request, "line")
        .and_then(Value::as_u64)
        .map_or(1, |line| line as u32);
    let whole_file = as_bool(field(request, "wholeFile")).unwrap_or(false);
    let until = as_str(field(request, "until"));

    let path = server.root().join(file);
    let Some(language) = rqx_core::Language::of_path(&path) else {
        return json!({ "error": format!("`{file}` is not a language rqx reads") });
    };
    let Some(source) = server.text_of(&path) else {
        return json!({ "error": format!("cannot read `{file}`") });
    };
    let Some(waiver) =
        rqx_core::suppress::waiver(&source, language, line, rule, reason, until, whole_file)
    else {
        return json!({ "error": "the waiver could not be written; check the date is YYYY-MM-DD" });
    };
    let Some(uri) = uri_of(&path) else {
        return json!({ "error": format!("`{file}` has no URI") });
    };
    let edit = WorkspaceEdit {
        changes: Some(
            [(
                uri,
                vec![TextEdit {
                    range: at(waiver.line - 1, 0),
                    new_text: waiver.text,
                }],
            )]
            .into_iter()
            .collect(),
        ),
        ..WorkspaceEdit::default()
    };
    server.apply_edit(format!("rqx: waive `{rule}`"), edit);
    json!({ "suppressed": rule, "file": file, "at": waiver.line })
}

fn as_str(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

/// A rule or a pattern: an editor asking about the slug on a pattern hint gets the
/// pattern's page rather than an error nobody sees.
fn explain(argument: Option<&Value>) -> Value {
    let Some(slug) = as_str(argument) else {
        return json!({ "error": "rqx.explain needs a rule slug" });
    };
    projection::rule(slug)
        .or_else(|| projection::pattern(slug))
        .unwrap_or_else(|| json!({ "error": format!("no rule or pattern called `{slug}`") }))
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
    let Some(edit) = config_edit(&crate::config::config_path_in(server.root()), name) else {
        return json!({ "error": format!("`{name}` is already disabled, or rqx.yaml has a shape rqx will not edit blind") });
    };
    server.apply_edit(format!("rqx: disable `{name}`"), edit);
    json!({ "disabled": name })
}

/// An insertion rather than a re-serialisation: `rqx init` writes a line of prose per
/// table, and round-tripping the file through a YAML writer would delete all of it.
fn config_edit(path: &PathBuf, name: &str) -> Option<WorkspaceEdit> {
    let uri = uri_of(path)?;
    let existing = std::fs::read_to_string(path).ok();
    let insertion = crate::config::disable_entry(existing.as_deref().unwrap_or_default(), name)?;
    let edit = TextEdit {
        range: at(insertion.line, insertion.column),
        new_text: insertion.text,
    };

    // `changes` cannot create a file, so a project with no rqx.yaml — the default —
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
    let config = crate::config::config_path_in(server.root());
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
    let markup = rqx_lang::holds_markup(&path.to_string_lossy());
    let Ok(rewriter) = rqx_lang::rewrite::Rewriter::new(language, markup, &source) else {
        return Vec::new();
    };

    let index = rqx_core::LineIndex::new(&source);
    let requested = params.range;
    let display = path
        .strip_prefix(server.root())
        .unwrap_or(&path)
        .display()
        .to_string();

    let edits_of = |plan: &rqx_core::edit::Resolution| -> Vec<TextEdit> {
        plan.kept
            .iter()
            .map(|edit| TextEdit {
                range: range_at(
                    &index,
                    server.encoding(),
                    &span_range(&index, &fixable[0].location.file, edit.span),
                ),
                new_text: edit.replacement.clone(),
            })
            .collect()
    };
    let proven = |findings: &[rqx_core::finding::Finding]| -> Option<Vec<TextEdit>> {
        let plan = rewriter.plan(findings, &settings.for_file(&path).fix);
        if plan.is_empty()
            || rewriter
                .verify(&display, settings.for_file(&path), &fixable, &plan)
                .is_err()
        {
            return None;
        }
        Some(edits_of(&plan))
    };
    let edit_for = |edits: Vec<TextEdit>| WorkspaceEdit {
        changes: Some(
            [(params.text_document.uri.clone(), edits)]
                .into_iter()
                .collect(),
        ),
        ..WorkspaceEdit::default()
    };

    // A fix-all request wants the whole file done in one round trip — the same
    // per-file round `rqx fix` runs — not one action per finding for the client to
    // apply first and re-request.
    if wants_fix_all(params) {
        return proven(&fixable)
            .map(|edits| {
                vec![source_action(
                    "rqx: fix everything in this file".to_owned(),
                    edit_for(edits),
                )]
            })
            .unwrap_or_default();
    }

    let mut actions = Vec::new();
    for finding in &fixable {
        let range = range_at(&index, server.encoding(), &finding.location);
        if range.end < requested.start || range.start > requested.end {
            continue;
        }
        let Some(edits) = proven(std::slice::from_ref(finding)) else {
            continue;
        };
        actions.push(source_action(
            format!("rqx: fix `{}`", finding.slug()),
            edit_for(edits),
        ));
    }
    actions
}

/// A client asking with `only` set to a fix-all kind — on save, or from a "fix this
/// file" command — is asking for the file, not for the cursor's line.
fn wants_fix_all(params: &CodeActionParams) -> bool {
    params.context.only.as_ref().is_some_and(|kinds| {
        kinds
            .iter()
            .any(|kind| FIX_ALL.as_str().starts_with(kind.as_str()))
    })
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

/// A child of `source.fixAll`, so `codeActionsOnSave` matches it under either name while
/// a client asking for `source.fixAll.rqx` gets rqx's edits and nobody else's.
fn source_action(title: String, edit: WorkspaceEdit) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(FIX_ALL.clone()),
        edit: Some(edit),
        ..CodeAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asking_for(kinds: &[&str]) -> CodeActionParams {
        CodeActionParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///tmp/a.ts".parse().expect("a uri"),
            },
            range: Range::default(),
            context: lsp_types::CodeActionContext {
                diagnostics: Vec::new(),
                only: (!kinds.is_empty()).then(|| {
                    kinds
                        .iter()
                        .map(|kind| CodeActionKind::from((*kind).to_owned()))
                        .collect()
                }),
                trigger_kind: None,
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        }
    }

    /// `source.fixAll` is the parent kind an editor asks for on save and
    /// `source.fixAll.rqx` is what a "fix this file" command asks for. Both mean the
    /// whole file; a request with no `only` is the cursor asking about its own line.
    #[test]
    fn a_fix_all_request_is_recognised_under_either_name() {
        assert!(wants_fix_all(&asking_for(&["source.fixAll"])));
        assert!(wants_fix_all(&asking_for(&["source.fixAll.rqx"])));
        assert!(wants_fix_all(&asking_for(&["source"])));
        assert!(!wants_fix_all(&asking_for(&["quickfix"])));
        assert!(!wants_fix_all(&asking_for(&[])));
    }

    /// Answering another extension's fix-all with our edits would apply changes the
    /// client never asked for.
    #[test]
    fn another_extensions_fix_all_is_not_ours() {
        assert!(!wants_fix_all(&asking_for(&["source.fixAll.eslint"])));
    }

    /// Every command the server advertises has to be one `execute` can dispatch, or
    /// a client binds a key to a method-not-found.
    #[test]
    fn every_advertised_command_is_namespaced() {
        let names = names();
        assert!(
            names.iter().all(|name| name.starts_with("rqx.")),
            "{names:?}"
        );
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "a command is advertised twice");
    }
}

mod commands;
mod diagnostics;
mod protocol;
mod worker;

use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::{
    CodeLens, CodeLensParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, Hover, HoverContents, HoverParams,
    InitializeParams, MarkupContent, MarkupKind, Position, Range, ServerInfo, Uri,
};
use rqx_core::finding::Finding;
use serde::Deserialize;

use crate::config::Resolved;
use diagnostics::Publisher;
use protocol::{Encoding, path_from_uri};
use worker::{Job, Refused, Worker};

/// Kept because an editor built against it before the server grew standard commands
/// still sends it, and a request that stops being answered looks like a crash.
const SCAN_WORKSPACE: &str = "rqx/scanWorkspace";

/// What a client may set without a client-side plugin. Every one of these is a display
/// choice, so a wrong value narrows what is shown rather than changing what is found.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Options {
    config_file: Option<PathBuf>,
    scan_on_startup: Option<bool>,
    scan_on_save: Option<bool>,
    code_lens: Option<bool>,
    patterns: Option<bool>,
}

pub struct Server {
    root: PathBuf,
    config: Resolved,
    explicit_config: Option<PathBuf>,
    /// Open buffers by path relative to the root. Analysis reads these rather than the
    /// file on disk, because an editor restoring unsaved work would otherwise be shown
    /// findings about text nobody can see.
    documents: HashMap<String, String>,
    patterns: bool,
    code_lens: bool,
    scan_on_startup: bool,
    scan_on_save: bool,
    publisher: Arc<Publisher>,
    worker: Worker,
}

impl Server {
    pub const fn root(&self) -> &PathBuf {
        &self.root
    }

    /// Ranked from the last scan's grades rather than by rescanning: the answer to
    /// "what should I open" cannot be fresher than the scan it came from anyway.
    pub fn start_here(&self) -> Vec<serde_json::Value> {
        self.publisher
            .priority(20)
            .into_iter()
            .map(|(file, facts)| {
                serde_json::json!({
                    "file": file,
                    "grade": facts.grade.label(),
                    "findings": facts.findings,
                    "importers": facts.importers,
                    "commits": facts.commits,
                })
            })
            .collect()
    }

    /// The result of `executeCommand` is a value, not an edit the client applies — so a
    /// command that changes a file has to ask for it.
    pub fn apply_edit(&self, label: String, edit: lsp_types::WorkspaceEdit) {
        self.publisher.request(
            "workspace/applyEdit",
            serde_json::json!({ "label": label, "edit": edit }),
        );
    }

    pub fn review(&self, base: &str, reply_to: Option<RequestId>) -> Option<Refused> {
        self.worker.request(Job {
            config: self.config.clone(),
            reply_to,
            patterns: self.patterns,
            base: Some(base.to_owned()),
            dirty: self.dirty(),
        })
    }

    /// Files the editor holds unsaved, so a background scan does not republish them
    /// from the stale copy on disk.
    fn dirty(&self) -> Vec<String> {
        self.documents.keys().cloned().collect()
    }

    /// Rescanned rather than just flipped: advice is published as diagnostics, so
    /// turning it on or off only shows up once the files are published again.
    pub fn toggle_patterns(&mut self) -> bool {
        self.patterns = !self.patterns;
        self.scan(None);
        self.patterns
    }

    /// The client caches lenses until it is told they are stale, so without the refresh
    /// the toggle does nothing visible until the file is edited.
    pub fn toggle_code_lens(&mut self) -> bool {
        self.code_lens = !self.code_lens;
        self.publisher.refresh_lenses();
        self.code_lens
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root).map_or_else(
            |_| path.display().to_string(),
            |rest| rest.display().to_string(),
        )
    }

    fn analyze(&self, path: &Path, source: &str) -> Vec<Finding> {
        // A file no language claims, or one the workspace has switched off, is not a
        // file with no findings — it is a file this server has nothing to say about.
        let Some(language) = rqx_core::Language::of_path(path) else {
            return Vec::new();
        };
        if !self.config.config.languages.includes(language) {
            return Vec::new();
        }
        let relative = self.relative(path);
        rqx_lang::analyze(path, &relative, source, self.config.for_file(path))
            .map(|analysis| analysis.findings)
            .unwrap_or_default()
    }

    fn refresh(&mut self, uri: &Uri) {
        let Some(path) = path_from_uri(uri) else {
            return;
        };
        let relative = self.relative(&path);
        let source = match self.documents.get(&relative) {
            Some(text) => text.clone(),
            None => match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(_) => return,
            },
        };
        let findings = self.analyze(&path, &source);
        self.publisher.publish(&relative, &source, findings, &[]);
    }

    /// Cross-file rules cannot be answered from one open file, so the workspace is
    /// scanned on its own thread and every file republished at once. Without this a
    /// client that does not know rqx's custom request sees 52 of the 60 rules and is
    /// told nothing about the other eight.
    fn scan(&self, reply_to: Option<RequestId>) -> Option<Refused> {
        self.worker.request(Job {
            config: self.config.clone(),
            reply_to,
            patterns: self.patterns,
            base: None,
            dirty: self.dirty(),
        })
    }

    fn hover(&self, params: &HoverParams) -> Option<Hover> {
        let uri = &params.text_document_position_params.text_document.uri;
        let relative = self.relative(&path_from_uri(uri)?);
        let (range, finding) = self
            .publisher
            .at(&relative, params.text_document_position_params.position)?;
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: diagnostics::hover_markdown(&finding),
            }),
            range: Some(range),
        })
    }

    /// One lens per file rather than one per function: the per-finding measurement is
    /// already on the squiggle and in the hover, while a file's grade and blast radius
    /// have nowhere else in an editor to be.
    fn code_lens(&self, params: &CodeLensParams) -> Vec<CodeLens> {
        if !self.code_lens {
            return Vec::new();
        }
        let Some(path) = path_from_uri(&params.text_document.uri) else {
            return Vec::new();
        };
        let relative = self.relative(&path);
        let Some(facts) = self.publisher.facts(&relative) else {
            return Vec::new();
        };
        let top = Range {
            start: Position::default(),
            end: Position::default(),
        };
        vec![CodeLens {
            range: top,
            command: Some(lsp_types::Command {
                title: format!(
                    "rqx · {} · {} findings · {} importers · {} commits/90d",
                    facts.grade.label(), facts.findings, facts.importers, facts.commits
                ),
                command: commands::SCAN.to_owned(),
                arguments: None,
            }),
            data: None,
        }]
    }

    fn reload(&mut self) {
        if let Ok(settings) = crate::config::resolve(&self.root, self.explicit_config.as_deref()) {
            self.config = settings;
        }
        self.scan(None);
    }
}

pub fn serve(root: &Path, config: Resolved) -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    // The encoding has to be chosen from what the client offers before capabilities are
    // sent, and `Connection::initialize` sends them for us — so the handshake is driven
    // by hand to get the client's half first.
    let (id, params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(params)?;
    let encoding = Encoding::negotiate(&params.capabilities);
    let options: Options = params
        .initialization_options
        .clone()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();

    connection.initialize_finish(
        id,
        serde_json::json!({
            "capabilities": protocol::capabilities(encoding, commands::names()),
            "serverInfo": ServerInfo {
                name: "rqx".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            },
        }),
    )?;

    // The client's workspace root wins over the process's directory: an editor is
    // rarely launched from the project it has open.
    let root = params
        .workspace_folders
        .and_then(|folders| folders.first().and_then(|folder| path_from_uri(&folder.uri)))
        .unwrap_or_else(|| root.to_path_buf());

    // Re-resolved against the client's root rather than the directory rqx was launched
    // from, or a monorepo package's own RQX.toml would be read from the wrong place.
    let config = crate::config::resolve(&root, options.config_file.as_deref()).unwrap_or(config);

    let publisher = Arc::new(Publisher::new(
        connection.sender.clone(),
        root.clone(),
        encoding,
    ));
    let mut server = Server {
        root,
        config,
        explicit_config: options.config_file,
        documents: HashMap::new(),
        patterns: options.patterns.unwrap_or(false),
        code_lens: options.code_lens.unwrap_or(false),
        scan_on_startup: options.scan_on_startup.unwrap_or(true),
        scan_on_save: options.scan_on_save.unwrap_or(true),
        worker: Worker::spawn(Arc::clone(&publisher)),
        publisher,
    };

    // `initialize_finish` returns only once the client has sent `initialized`, and it
    // consumes that notification — so this is the first point at which the client is
    // ready to be sent anything, and the message loop will never see it.
    if server.scan_on_startup {
        server.scan(None);
    }

    // Not `?`, because the teardown below has to run on the error path too: the worker
    // and the delivery thread both hold a clone of the connection's sender, and the
    // writer stops only once every sender is dropped.
    let outcome = main_loop(&connection, &mut server);
    drop(server);
    drop(connection);
    io_threads.join()?;
    outcome
}

fn main_loop(
    connection: &Connection,
    server: &mut Server,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    for message in &connection.receiver {
        match message {
            Message::Request(request) if request.method == "shutdown" => {
                let _ = connection
                    .sender
                    .send(Message::Response(Response::new_ok(request.id, ())));
                drain_until_exit(connection);
                return Ok(());
            }
            Message::Request(request) => handle_request(connection, server, request),
            Message::Notification(notification) => handle_notification(server, &notification),
            Message::Response(_) => {}
        }
    }
    Ok(())
}

/// `Connection::handle_shutdown` treats anything but `exit` as a protocol violation, and
/// a client's answer to the progress request a running scan sent is exactly that — so
/// using it turns an ordinary shutdown mid-scan into a reported crash.
fn drain_until_exit(connection: &Connection) {
    let deadline = Duration::from_secs(30);
    while let Ok(message) = connection.receiver.recv_timeout(deadline) {
        match message {
            Message::Notification(notification) if notification.method == "exit" => return,
            // A request arriving after shutdown must be refused rather than served, but
            // leaving it unanswered would hang a client waiting on it.
            Message::Request(request) => {
                let _ = connection.sender.send(Message::Response(Response::new_err(
                    request.id,
                    ErrorCode::InvalidRequest as i32,
                    "rqx is shutting down".to_owned(),
                )));
            }
            _ => {}
        }
    }
}

fn handle_request(connection: &Connection, server: &mut Server, request: Request) {
    let id = request.id.clone();
    let reply = |response: Response| {
        let _ = connection.sender.send(Message::Response(response));
    };

    match request.method.as_str() {
        SCAN_WORKSPACE => {
            // The worker answers once the scan finishes, unless it could not take the
            // job — in which case this request is owed an answer now rather than one
            // that never comes.
            if let Some(refused) = server.scan(Some(id)) {
                reply(commands::refusal(refused));
            }
        }
        lsp_types::request::HoverRequest::METHOD => {
            let hover = parse::<HoverParams>(&request).and_then(|params| server.hover(&params));
            reply(Response::new_ok(id, hover));
        }
        lsp_types::request::CodeLensRequest::METHOD => {
            let lenses = parse::<CodeLensParams>(&request)
                .map(|params| server.code_lens(&params))
                .unwrap_or_default();
            reply(Response::new_ok(id, lenses));
        }
        lsp_types::request::CodeActionRequest::METHOD => {
            let actions = parse::<lsp_types::CodeActionParams>(&request)
                .map(|params| commands::actions_for(server, &params))
                .unwrap_or_default();
            reply(Response::new_ok(id, actions));
        }
        lsp_types::request::ExecuteCommand::METHOD => {
            match parse::<lsp_types::ExecuteCommandParams>(&request) {
                Some(params) => match commands::execute(server, &params, id.clone()) {
                    commands::Outcome::Answered(value) => reply(Response::new_ok(id, value)),
                    commands::Outcome::Deferred => {}
                    commands::Outcome::Refused(refused) => reply(commands::refusal(refused)),
                    commands::Outcome::Unknown => reply(unknown(id, &params.command)),
                },
                None => reply(Response::new_err(
                    id,
                    ErrorCode::InvalidParams as i32,
                    "executeCommand needs a command name".to_owned(),
                )),
            }
        }
        // Answering an unimplemented method with `null` tells a client the capability
        // is there and returned nothing, which is a lie it will keep asking on.
        method => reply(unknown(id, method)),
    }
}

fn unknown(id: RequestId, method: &str) -> Response {
    Response::new_err(
        id,
        ErrorCode::MethodNotFound as i32,
        format!("rqx does not handle `{method}`"),
    )
}

fn parse<P: serde::de::DeserializeOwned>(request: &Request) -> Option<P> {
    serde_json::from_value(request.params.clone()).ok()
}

fn handle_notification(server: &mut Server, notification: &Notification) {
    let params = notification.params.clone();
    match notification.method.as_str() {
        lsp_types::notification::DidOpenTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(params)
                && let Some(path) = path_from_uri(&params.text_document.uri)
            {
                let relative = server.relative(&path);
                server.documents.insert(relative, params.text_document.text);
                server.refresh(&params.text_document.uri);
            }
        }
        // The buffer is tracked but not analysed: a half-typed function is a work in
        // progress rather than a finding, and the next save is what asks the question.
        lsp_types::notification::DidChangeTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(params)
                && let Some(path) = path_from_uri(&params.text_document.uri)
                && let Some(change) = params.content_changes.into_iter().next_back()
            {
                let relative = server.relative(&path);
                server.documents.insert(relative, change.text);
            }
        }
        lsp_types::notification::DidSaveTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<DidSaveTextDocumentParams>(params) {
                server.refresh(&params.text_document.uri);
                // The saved file may have made an export live or an import cycle, and
                // neither is visible from the file that changed — but on a very large
                // repository a scan per save is more than a reader wants.
                if server.scan_on_save {
                    server.scan(None);
                }
            }
        }
        lsp_types::notification::DidCloseTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(params)
                && let Some(path) = path_from_uri(&params.text_document.uri)
            {
                let relative = server.relative(&path);
                server.documents.remove(&relative);
            }
        }
        lsp_types::notification::DidChangeWatchedFiles::METHOD => server.reload(),
        _ => {}
    }
}

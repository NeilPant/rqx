use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidOpenTextDocument, DidSaveTextDocument, Notification as _, PublishDiagnostics,
};
use lsp_types::request::HoverRequest;
use lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Hover, HoverContents,
    HoverParams, HoverProviderCapability, Location, MarkupContent, MarkupKind, NumberOrString,
    Position, PublishDiagnosticsParams, Range, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Uri,
};
use rqx_core::config::RqxConfig;
use rqx_core::finding::{Confidence, Finding, Severity};

use crate::scan;

/// Custom request the extension uses to scan the whole workspace, since LSP has no
/// notion of "analyze everything and tell me a score".
const SCAN_WORKSPACE: &str = "rqx/scanWorkspace";

fn severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    }
}

/// LSP positions are zero-based in both axes while findings carry them one-based,
/// so an off-by-one here puts every squiggle on the wrong line.
fn position(line: u32, column: u32) -> Position {
    Position { line: line.saturating_sub(1), character: column.saturating_sub(1) }
}

fn range_of(finding: &Finding) -> Range {
    let start = position(finding.location.line, finding.location.column);
    let end = Position {
        line: finding.location.end_line.saturating_sub(1).max(start.line),
        character: if finding.location.end_line == finding.location.line {
            start.character + (finding.location.end - finding.location.start).max(1)
        } else {
            start.character + 1
        },
    };
    Range { start, end }
}

fn uri_for(root: &Path, relative: &str) -> Option<Uri> {
    let absolute = root.join(relative);
    format!("file://{}", absolute.display()).parse().ok()
}

fn message_for(finding: &Finding) -> String {
    let spec = finding.rule.spec();
    let confidence = match finding.confidence {
        Confidence::Verified => "",
        Confidence::Heuristic => " (measured threshold)",
        Confidence::Low => " (low confidence)",
    };
    format!("{}{confidence}\n\n{}\n\nFix: {}", finding.evidence, spec.why, finding.fix_hint)
}

fn diagnostic(root: &Path, finding: &Finding) -> Diagnostic {
    let spec = finding.rule.spec();
    let related: Vec<DiagnosticRelatedInformation> = finding
        .related
        .iter()
        .filter_map(|site| {
            Some(DiagnosticRelatedInformation {
                location: Location {
                    uri: uri_for(root, &site.file)?,
                    range: Range {
                        start: position(site.line, site.column),
                        end: position(site.line, site.column + 1),
                    },
                },
                message: "same shape here".to_owned(),
            })
        })
        .collect();

    Diagnostic {
        range: range_of(finding),
        severity: Some(severity(finding.severity)),
        code: Some(NumberOrString::String(spec.slug.to_owned())),
        code_description: None,
        source: Some("rqx".to_owned()),
        message: message_for(finding),
        related_information: (!related.is_empty()).then_some(related),
        tags: None,
        data: None,
    }
}

struct Server {
    root: PathBuf,
    config: RqxConfig,
    /// Findings per file, kept so hover can answer without rescanning and so a file
    /// that becomes clean can be published with an empty list rather than keeping
    /// stale squiggles.
    published: HashMap<String, Vec<Finding>>,
}

impl Server {
    fn analyze_file(&mut self, path: &Path) -> Vec<Finding> {
        let Ok(source) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        let relative = path
            .strip_prefix(&self.root)
            .map_or_else(|_| path.display().to_string(), |rest| rest.display().to_string());
        rqx_ts::analyze(path, &relative, &source, &self.config)
            .map(|analysis| analysis.findings)
            .unwrap_or_default()
    }

    fn publish(&mut self, connection: &Connection, relative: String, findings: Vec<Finding>) {
        let Some(uri) = uri_for(&self.root, &relative) else {
            return;
        };
        let diagnostics =
            findings.iter().map(|finding| diagnostic(&self.root, finding)).collect();
        let params = PublishDiagnosticsParams { uri, diagnostics, version: None };
        let _ = connection.sender.send(Message::Notification(Notification {
            method: PublishDiagnostics::METHOD.to_owned(),
            params: serde_json::to_value(params).unwrap_or_default(),
        }));
        self.published.insert(relative, findings);
    }

    fn refresh(&mut self, connection: &Connection, uri: &Uri) {
        let Some(path) = path_from_uri(uri) else { return };
        let findings = self.analyze_file(&path);
        let relative = path
            .strip_prefix(&self.root)
            .map_or_else(|_| path.display().to_string(), |rest| rest.display().to_string());
        self.publish(connection, relative, findings);
    }

    /// Cross-file rules cannot be answered from one open file, so the whole project
    /// is scanned and every file's diagnostics are republished at once.
    fn scan_workspace(&mut self, connection: &Connection) -> serde_json::Value {
        let Ok(outcome) = scan::scan(&self.root, &self.config) else {
            return serde_json::json!({ "error": "scan failed" });
        };

        let mut by_file: HashMap<String, Vec<Finding>> = HashMap::new();
        for finding in &outcome.findings {
            by_file.entry(finding.location.file.to_string()).or_default().push(finding.clone());
        }
        // A file that had findings and now has none still needs an empty publish, or
        // the editor keeps showing the squiggles it was last told about.
        for stale in self.published.keys().cloned().collect::<Vec<_>>() {
            by_file.entry(stale).or_default();
        }
        for (file, findings) in by_file {
            self.publish(connection, file, findings);
        }

        serde_json::json!({
            "health": outcome
                .counts
                .weighted_health_score(outcome.code_lines, &outcome.impact),
            "findings": outcome.counts.total,
            "files": outcome.files_scanned,
            "elapsedMs": outcome.elapsed_ms,
        })
    }

    fn hover(&self, params: &HoverParams) -> Option<Hover> {
        let uri = &params.text_document_position_params.text_document.uri;
        let path = path_from_uri(uri)?;
        let relative = path
            .strip_prefix(&self.root)
            .map_or_else(|_| path.display().to_string(), |rest| rest.display().to_string());
        let line = params.text_document_position_params.position.line + 1;

        let findings = self.published.get(&relative)?;
        let finding = findings.iter().find(|finding| {
            line >= finding.location.line && line <= finding.location.end_line.max(finding.location.line)
        })?;

        let spec = finding.rule.spec();
        let value = format!(
            "**{}**\n\n{}\n\n{}\n\n**Fix — {}**\n\n{}\n\n---\n\n*{} · {}*",
            spec.slug,
            finding.evidence,
            spec.why,
            spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>().join(" / "),
            finding.fix_hint,
            spec.clean_code,
            spec.guru_smell.unwrap_or("no catalog entry")
        );
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value }),
            range: Some(range_of(finding)),
        })
    }
}

fn path_from_uri(uri: &Uri) -> Option<PathBuf> {
    let text = uri.as_str();
    let rest = text.strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(rest)))
}

/// URIs arrive percent-encoded and a path with a space is common enough that
/// ignoring it would silently break diagnostics for those files.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        // Diagnostics are recomputed on open and save rather than on every keystroke:
        // a half-typed function is not a finding, it is a work in progress.
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::NONE),
            save: Some(TextDocumentSyncSaveOptions::Supported(true)),
            ..TextDocumentSyncOptions::default()
        })),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        ..ServerCapabilities::default()
    }
}

fn cast<R: lsp_types::request::Request>(
    request: Request,
) -> Result<(RequestId, R::Params), ExtractError<Request>> {
    request.extract(<R as lsp_types::request::Request>::METHOD)
}

pub fn serve(root: &Path, config: RqxConfig) -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();
    let initialize = serde_json::to_value(capabilities())?;
    let params = connection.initialize(initialize)?;

    // The client's workspace root wins over the process's directory: an editor is
    // rarely launched from the project it has open.
    let root = serde_json::from_value::<lsp_types::InitializeParams>(params)
        .ok()
        .and_then(|params| {
            params
                .workspace_folders
                .and_then(|folders| folders.first().and_then(|f| path_from_uri(&f.uri)))
        })
        .unwrap_or_else(|| root.to_path_buf());

    let server = Server { root, config, published: HashMap::new() };
    // The connection owns the sender, and the writer thread only stops once every
    // sender is dropped — so it must go out of scope before joining, or a clean
    // editor shutdown hangs the process forever.
    main_loop(&connection, server)?;
    drop(connection);
    io_threads.join()?;
    Ok(())
}

fn main_loop(
    connection: &Connection,
    mut server: Server,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                let id = request.id.clone();
                if request.method == SCAN_WORKSPACE {
                    let result = server.scan_workspace(connection);
                    let _ = connection
                        .sender
                        .send(Message::Response(Response::new_ok(id, result)));
                    continue;
                }
                match cast::<HoverRequest>(request) {
                    Ok((id, params)) => {
                        let hover = server.hover(&params);
                        let _ = connection
                            .sender
                            .send(Message::Response(Response::new_ok(id, hover)));
                    }
                    Err(_) => {
                        let _ = connection.sender.send(Message::Response(Response::new_ok(
                            id,
                            serde_json::Value::Null,
                        )));
                    }
                }
            }
            Message::Notification(notification) => {
                let method = notification.method.as_str();
                if (method == DidOpenTextDocument::METHOD
                    || method == DidSaveTextDocument::METHOD)
                    && let Some(uri) = notification
                        .params
                        .get("textDocument")
                        .and_then(|document| document.get("uri"))
                        .and_then(|uri| uri.as_str())
                        .and_then(|uri| uri.parse::<Uri>().ok())
                {
                    server.refresh(connection, &uri);
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_positions_are_zero_based() {
        assert_eq!(position(1, 1), Position { line: 0, character: 0 });
        assert_eq!(position(12, 5), Position { line: 11, character: 4 });
    }

    /// A synthetic span for a whole-file finding carries line 0, and subtracting one
    /// from that would wrap a u32 into a position past the end of the universe.
    #[test]
    fn a_synthetic_position_does_not_underflow() {
        assert_eq!(position(0, 0), Position { line: 0, character: 0 });
    }

    #[test]
    fn file_uris_decode_back_into_paths() {
        let uri: Uri = "file:///tmp/my%20project/src/a.ts".parse().unwrap();
        assert_eq!(path_from_uri(&uri), Some(PathBuf::from("/tmp/my project/src/a.ts")));
    }

    #[test]
    fn a_non_file_uri_is_ignored_rather_than_guessed_at() {
        let uri: Uri = "untitled:Untitled-1".parse().unwrap();
        assert_eq!(path_from_uri(&uri), None);
    }
}

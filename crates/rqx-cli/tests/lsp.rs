use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

mod common;
use common::Scratch;
use common::{Watchdog, file_uri};

/// The server speaks JSON-RPC over a pipe, so the only test that proves it works is one
/// that speaks it back. Every capability it advertises is exercised here rather than in
/// each of the editors that consume it.
struct Session {
    child: Child,
    _watchdog: Watchdog,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: i64,
}

impl Session {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rqx"))
            .args(["lsp"])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start `rqx lsp`");
        let watchdog = Watchdog::arm(child.id(), Duration::from_secs(120));
        let input = child.stdin.take().expect("stdin");
        let output = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            _watchdog: watchdog,
            input,
            output,
            next_id: 0,
        }
    }

    fn send(&mut self, message: &Value) {
        let body = serde_json::to_string(message).expect("serialisable");
        write!(self.input, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("write");
        self.input.flush().expect("flush");
    }

    fn request(&mut self, method: &str, params: Value) -> i64 {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        id
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn read(&mut self) -> Value {
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            let read = self.output.read_line(&mut line).expect("header");
            assert!(read > 0, "the server closed the connection");
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                length = value.trim().parse().expect("a length");
            }
            if line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0; length];
        self.output.read_exact(&mut body).expect("body");
        serde_json::from_slice(&body).expect("json")
    }

    /// Diagnostics arrive unprompted and interleave with responses, so every wait is
    /// for a message matching a shape rather than for the next message.
    fn wait_for(&mut self, matches: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            let message = self.read();
            if matches(&message) {
                return message;
            }
        }
        panic!("timed out waiting for a message");
    }

    fn response_to(&mut self, id: i64) -> Value {
        self.wait_for(|message| message.get("id").and_then(Value::as_i64) == Some(id))
    }

    fn initialize(&mut self, root: &Path) -> Value {
        let uri = file_uri(root);
        let id = self.request(
            "initialize",
            json!({
                "processId": null,
                "rootUri": uri,
                "capabilities": { "general": { "positionEncodings": ["utf-16"] } },
                "initializationOptions": { "scanOnStartup": true },
                "workspaceFolders": [{ "uri": uri, "name": "fixture" }],
            }),
        );
        let answer = self.response_to(id);
        self.notify("initialized", json!({}));
        answer
    }

    fn shutdown(&mut self) {
        let id = self.request("shutdown", Value::Null);
        let _ = self.response_to(id);
        self.notify("exit", Value::Null);
        // The startup scan is still writing diagnostics, and a client that stops reading
        // fills the pipe and blocks the server's writer thread — so the process would
        // never exit and `wait` would never return.
        let mut rest = Vec::new();
        let _ = self.output.read_to_end(&mut rest);
        let _ = self.child.wait();
    }
}

fn diagnostics_for<'a>(message: &'a Value, file: &str) -> Option<&'a Vec<Value>> {
    if message.get("method")?.as_str()? != "textDocument/publishDiagnostics" {
        return None;
    }
    let uri = message.get("params")?.get("uri")?.as_str()?;
    if !uri.ends_with(file) {
        return None;
    }
    message.get("params")?.get("diagnostics")?.as_array()
}

fn codes(diagnostics: &[Value]) -> Vec<String> {
    diagnostics
        .iter()
        .filter_map(|entry| Some(entry.get("code")?.as_str()?.to_owned()))
        .collect()
}

/// The fixtures are copied under a directory with a space in its name, because building
/// a URI from a path used to produce an unparseable one and every publish for the whole
/// project silently did nothing. One directory per test, because cargo runs them in
/// parallel and a shared tree would have them deleting each other's fixtures mid-scan.
fn workspace_with_a_space(test: &str) -> Scratch {
    let root = Scratch::new(&format!("lsp {test}"));
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/typescript");
    copy(&source, &root);
    root
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create");
    for entry in std::fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("kind").is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

#[test]
fn the_handshake_states_what_the_server_is_and_what_it_can_do() {
    let root = workspace_with_a_space("the_handshake_states_what_the_server_is_and_what_it_can_do");
    let mut session = Session::start(&root);
    let answer = session.initialize(&root);
    let result = &answer["result"];

    assert_eq!(result["serverInfo"]["name"], "rqx");
    assert!(
        result["serverInfo"]["version"].is_string(),
        "a client cannot warn about a version it is never told"
    );
    assert_eq!(result["capabilities"]["positionEncoding"], "utf-16");
    assert!(result["capabilities"]["hoverProvider"].as_bool().unwrap());
    assert!(result["capabilities"]["codeActionProvider"].is_object());

    let commands = result["capabilities"]["executeCommandProvider"]["commands"]
        .as_array()
        .expect("a command list");
    for expected in ["rqx.scan", "rqx.check", "rqx.explain", "rqx.startHere"] {
        assert!(
            commands.iter().any(|name| name == expected),
            "`{expected}` is not advertised, so no editor can bind it"
        );
    }

    session.shutdown();
}

#[test]
fn an_opened_file_under_a_path_with_a_space_gets_diagnostics() {
    let root = workspace_with_a_space("an_opened_file_under_a_path_with_a_space_gets_diagnostics");
    let mut session = Session::start(&root);
    session.initialize(&root);

    let file = root.join("hygiene.ts");
    let text = std::fs::read_to_string(&file).expect("the fixture");
    session.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": file_uri(&file),
                "languageId": "typescript",
                "version": 1,
                "text": text,
            }
        }),
    );

    let message = session.wait_for(|message| {
        diagnostics_for(message, "hygiene.ts").is_some_and(|found| !found.is_empty())
    });
    let diagnostics = diagnostics_for(&message, "hygiene.ts").expect("diagnostics");
    let first = &diagnostics[0];
    assert_eq!(first["source"], "rqx");
    assert!(first["code"].is_string());
    assert!(
        first["codeDescription"]["href"]
            .as_str()
            .unwrap()
            .contains("docs/rules/"),
        "a rule code with no reference is inert text in the Problems panel"
    );
    assert!(
        first["data"]["rule"].is_string(),
        "a code action has nothing to act on"
    );

    session.shutdown();
}

/// A developer's first rqx quick fix used to be one that turned the rule off. The
/// mechanical rules now offer the edit itself, verified before it is offered.
#[test]
fn a_mechanical_finding_offers_the_edit_and_not_only_a_way_to_silence_it() {
    let root = workspace_with_a_space(
        "a_mechanical_finding_offers_the_edit_and_not_only_a_way_to_silence_it",
    );
    let file = root.join("fixable.ts");
    let text = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";
    std::fs::write(&file, text).expect("the fixture");

    let mut session = Session::start(&root);
    session.initialize(&root);
    let uri = file_uri(&file);
    session.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "typescript",
                "version": 1,
                "text": text,
            }
        }),
    );

    let message = session.wait_for(|message| {
        diagnostics_for(message, "fixable.ts").is_some_and(|found| !found.is_empty())
    });
    let diagnostics = diagnostics_for(&message, "fixable.ts").expect("diagnostics");

    let id = session.request(
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": { "start": { "line": 2, "character": 2 }, "end": { "line": 2, "character": 2 } },
            "context": { "diagnostics": diagnostics },
        }),
    );
    let answer = session.wait_for(|message| message["id"] == json!(id));
    let actions = answer["result"].as_array().expect("actions");

    // A client matches code action kinds by prefix, so `source.fixAll.rqx` answers a
    // request for `source.fixAll` while letting one client ask for rqx's edits alone.
    let fix = actions
        .iter()
        .find(|action| {
            action["kind"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("source.fixAll"))
        })
        .unwrap_or_else(|| panic!("no source edit offered: {actions:#?}"));
    assert!(
        fix["title"].as_str().unwrap().contains("unreachable-code"),
        "{fix}"
    );
    let edits = fix["edit"]["changes"][&uri].as_array().expect("edits");
    assert_eq!(edits.len(), 1, "{fix}");
    assert_eq!(edits[0]["newText"], "");

    session.shutdown();
}

/// Eight of the sixty rules cannot be answered from one open file. Before the server
/// scanned on its own, an editor that did not know rqx's custom request never saw them.
#[test]
fn cross_file_rules_arrive_without_being_asked_for() {
    let root = workspace_with_a_space("cross_file_rules_arrive_without_being_asked_for");
    let mut session = Session::start(&root);
    session.initialize(&root);

    let deadline = Instant::now() + Duration::from_secs(120);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        let message = session.read();
        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && let Some(diagnostics) = message["params"]["diagnostics"].as_array()
        {
            seen.extend(codes(diagnostics));
        }
        if message.get("method").and_then(Value::as_str) == Some("rqx/report") {
            break;
        }
    }

    assert!(
        seen.iter().any(|code| code == "unused-export"),
        "no cross-file finding was published; saw {seen:?}"
    );

    session.shutdown();
}

#[test]
fn the_scan_reports_health_and_a_ranked_place_to_start() {
    let root = workspace_with_a_space("the_scan_reports_health_and_a_ranked_place_to_start");
    let mut session = Session::start(&root);
    session.initialize(&root);

    let report = session
        .wait_for(|message| message.get("method").and_then(Value::as_str) == Some("rqx/report"));
    let params = &report["params"];
    assert!(params["health"].as_u64().unwrap() <= 100);
    assert!(params["grades"].is_object());

    let id = session.request(
        "workspace/executeCommand",
        json!({ "command": "rqx.startHere", "arguments": [] }),
    );
    let ranked = session.response_to(id);
    let first = &ranked["result"][0];
    assert!(
        first["file"].is_string() && first["grade"].is_string(),
        "start-here answered nothing useful: {ranked}"
    );

    session.shutdown();
}

#[test]
fn explain_answers_with_the_catalogue_entry() {
    let root = workspace_with_a_space("explain_answers_with_the_catalogue_entry");
    let mut session = Session::start(&root);
    session.initialize(&root);

    let id = session.request(
        "workspace/executeCommand",
        json!({ "command": "rqx.explain", "arguments": ["unused-export"] }),
    );
    let answer = session.response_to(id);
    assert_eq!(answer["result"]["slug"], "unused-export");
    assert!(
        answer["result"]["markdown"]
            .as_str()
            .unwrap()
            .contains("unused-export")
    );

    session.shutdown();
}

/// `Connection::handle_shutdown` refuses anything but `exit`, and the client's answer to
/// the progress request an in-flight scan sent is exactly that — so shutting down during
/// a scan reported itself as a server crash.
#[test]
fn shutting_down_mid_scan_is_not_a_protocol_error() {
    let root = workspace_with_a_space("shutting_down_mid_scan_is_not_a_protocol_error");
    let mut session = Session::start(&root);
    session.initialize(&root);

    // The startup scan asks the client to create a progress token; answering it is what
    // a real client does, and what used to arrive at the wrong moment.
    let create = session.wait_for(|message| {
        message.get("method").and_then(Value::as_str) == Some("window/workDoneProgress/create")
    });
    let id = create["id"].clone();

    let shutdown = session.request("shutdown", Value::Null);
    session.send(&json!({ "jsonrpc": "2.0", "id": id, "result": null }));
    let answer = session.response_to(shutdown);
    assert!(answer.get("error").is_none(), "shutdown errored: {answer}");

    session.notify("exit", Value::Null);
    let mut rest = Vec::new();
    let _ = session.output.read_to_end(&mut rest);
    let status = session.child.wait().expect("the server exits");
    assert!(status.success(), "the server exited with {status}");
}

/// Answering an unimplemented method with `null` tells a client the capability is there
/// and returned nothing, which is a lie it keeps asking on.
#[test]
fn an_unimplemented_method_is_refused_rather_than_answered_with_nothing() {
    let root = workspace_with_a_space(
        "an_unimplemented_method_is_refused_rather_than_answered_with_nothing",
    );
    let mut session = Session::start(&root);
    session.initialize(&root);

    let id = session.request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": "file:///nowhere.ts" },
            "position": { "line": 0, "character": 0 },
        }),
    );
    let answer = session.response_to(id);
    assert_eq!(
        answer["error"]["code"], -32601,
        "expected MethodNotFound: {answer}"
    );

    session.shutdown();
}

/// One bad frame from a client used to end the connection with `disconnected channel`.
/// It is answered with a JSON-RPC error now, and the session carries on.
#[test]
fn a_malformed_message_is_answered_and_the_session_carries_on() {
    let root = workspace_with_a_space("a_malformed_message_is_answered_and_the_session_carries_on");
    let mut session = Session::start(&root);
    write!(session.input, "Content-Length: 5\r\n\r\n{{{{{{{{{{").expect("write");
    session.input.flush().expect("flush");
    let refusal = session.wait_for(|message| message.get("error").is_some());
    assert_eq!(refusal["error"]["code"], -32700, "{refusal}");

    let answer = session.initialize(&root);
    assert!(answer["result"]["capabilities"].is_object(), "{answer}");
    session.shutdown();
}

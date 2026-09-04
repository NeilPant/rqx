use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// The server speaks JSON-RPC over a pipe, so the only test that proves it works is one
/// that speaks it back. MCP frames each message as a line rather than with the
/// `Content-Length` header the language server uses.
struct Session {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    next_id: i64,
}

impl Session {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rqx"))
            .args(["mcp"])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start `rqx mcp`");
        // Reading a pipe blocks with no deadline, so without this a server that stops
        // answering hangs the suite forever rather than failing it.
        let pid = child.id();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(120));
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
        });
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().expect("stdout"));
        let mut session = Self {
            child,
            input,
            output,
            next_id: 0,
        };
        session.initialize();
        session
    }

    fn send(&mut self, message: &Value) {
        let body = serde_json::to_string(message).expect("serialisable");
        let input = self.input.as_mut().expect("an open server");
        writeln!(input, "{body}").expect("write");
        input.flush().expect("flush");
    }

    fn request(&mut self, method: &str, params: Value) -> i64 {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        id
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        let read = self.output.read_line(&mut line).expect("a line");
        assert!(read > 0, "the server closed the connection");
        serde_json::from_str(&line).expect("json")
    }

    /// A server may send notifications of its own, so every wait is for a message
    /// matching a shape rather than for the next message.
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

    fn initialize(&mut self) -> Value {
        let id = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "rqx tests", "version": "0" },
            }),
        );
        let answer = self.response_to(id);
        self.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        answer
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let id = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        self.response_to(id)
    }

    fn structured(&mut self, tool: &str, arguments: Value) -> Value {
        let answer = self.call(tool, arguments);
        assert!(
            answer.get("error").is_none(),
            "{tool} failed: {}",
            answer["error"]
        );
        answer["result"]["structuredContent"].clone()
    }

    fn text(&mut self, tool: &str, arguments: Value) -> String {
        let answer = self.call(tool, arguments);
        answer["result"]["content"][0]["text"]
            .as_str()
            .expect("a text block")
            .to_owned()
    }

    /// Closing stdin is what asks the server to stop; draining what it has left to say
    /// keeps a full pipe from blocking its writer, which would leave `wait` hanging.
    fn shutdown(&mut self) {
        self.input.take();
        let mut rest = Vec::new();
        let _ = self.output.read_to_end(&mut rest);
        let _ = self.child.wait();
    }
}

/// One directory per test, because cargo runs them in parallel and a shared tree would
/// have them deleting each other's fixtures mid-scan. The name carries a space for the
/// same reason the language server's fixtures do: a path that needs quoting is the one
/// that breaks first.
fn workspace(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx mcp {test}"));
    let _ = std::fs::remove_dir_all(&root);
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
fn the_handshake_advertises_every_tool_with_a_schema() {
    let root = workspace("handshake");
    let mut session = Session::start(&root);

    let id = session.request("tools/list", json!({}));
    let answer = session.response_to(id);
    let tools = answer["result"]["tools"].as_array().expect("tools");

    let mut names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "analyze", "check", "config", "explain", "findings", "patterns", "rules", "scan"
        ]
    );

    for tool in tools {
        let name = tool["name"].as_str().expect("a name");
        assert!(
            tool["inputSchema"].is_object(),
            "{name} has no input schema, so no agent can call it"
        );
        // rqx never edits source, and an agent that believed otherwise would ask before
        // every call.
        assert_eq!(
            tool["annotations"]["readOnlyHint"],
            json!(true),
            "{name} is not marked read-only"
        );
    }
    session.shutdown();
}

/// The whole design is that a scan answers with a summary. A findings array here is the
/// regression that puts a project's entire report back into an agent's context.
#[test]
fn a_scan_summarises_and_does_not_dump() {
    let root = workspace("summary");
    let mut session = Session::start(&root);

    let scan = session.structured("scan", json!({}));
    assert!(scan["findings"]["total"].as_u64().expect("a total") > 0);
    assert!(
        scan["findings"]["by_rule"].is_object(),
        "the per-rule counts are the summary"
    );
    assert!(
        scan["findings"].get("findings").is_none() && !scan["findings"].is_array(),
        "a scan must not return the findings themselves: {}",
        scan["findings"]
    );
    assert!(scan["start_here"].as_array().expect("a ranking").len() <= 10);
    assert!(scan["health"].is_number());

    let digest = session.text("scan", json!({}));
    assert!(digest.starts_with("## rqx — health"), "{digest}");
    session.shutdown();
}

#[test]
fn paging_returns_the_same_findings_whatever_the_page_size() {
    let root = workspace("cursor");
    let mut session = Session::start(&root);
    session.structured("scan", json!({}));

    let (small, matched) = walk(&mut session, 7);
    let (large, again) = walk(&mut session, 100);

    assert_eq!(matched, again, "the scan changed between walks");
    assert_eq!(small.len() as u64, matched, "the walk skipped or stopped early");
    assert_eq!(
        small, large,
        "the page size changed which findings came back"
    );
    session.shutdown();
}

fn walk(session: &mut Session, limit: u64) -> (Vec<Value>, u64) {
    let mut collected = Vec::new();
    let mut cursor = Value::Null;
    loop {
        let mut arguments = json!({ "limit": limit });
        if let Some(cursor) = cursor.as_str() {
            arguments["cursor"] = json!(cursor);
        }
        let page = session.structured("findings", arguments);
        let matched = page["matched"].as_u64().expect("a count");
        collected.extend(page["findings"].as_array().expect("findings").iter().cloned());
        cursor = page["next_cursor"].clone();
        if cursor.is_null() {
            return (collected, matched);
        }
    }
}

/// A drill-down belongs to the scan that was actually run. Keying the cache on the
/// server root instead had `findings` quietly rescan the whole project after a scan of
/// one subdirectory, and answer from findings the cursor was never issued against.
#[test]
fn a_drill_down_reads_the_subdirectory_that_was_scanned() {
    let root = workspace("subdirectory");
    std::fs::create_dir_all(root.join("nested")).expect("a subdirectory");
    std::fs::write(
        root.join("nested/wide.ts"),
        "export function wide(a, b, c, d, e, f) { return a + b + c + d + e + f; }\n",
    )
    .expect("a file with something wrong in it");
    let mut session = Session::start(&root);

    let whole = session.structured("scan", json!({}));
    let part = session.structured("scan", json!({ "path": "nested" }));
    assert!(
        part["files_scanned"].as_u64().expect("a count")
            < whole["files_scanned"].as_u64().expect("a count"),
        "the fixture has no subdirectory to narrow to"
    );

    let page = session.structured("findings", json!({ "limit": 100 }));
    assert_eq!(
        page["scan_id"], part["scan_id"],
        "the drill-down read a different scan than the one just run"
    );
    // Findings name files relative to what was scanned, so narrowing to `nested`
    // leaves only the one file in it.
    for finding in page["findings"].as_array().expect("findings") {
        assert_eq!(
            finding["location"]["file"], "wide.ts",
            "a finding came from outside the scanned subdirectory"
        );
    }
    session.shutdown();
}

/// Offset 20 of a different scan is not the page the agent asked for, and answering it
/// anyway is wrong in a way nothing downstream can detect.
#[test]
fn a_cursor_from_an_earlier_scan_is_refused() {
    let root = workspace("stale-cursor");
    let mut session = Session::start(&root);

    session.structured("scan", json!({}));
    let page = session.structured("findings", json!({ "limit": 1 }));
    let stale = page["next_cursor"].as_str().expect("a cursor").to_owned();

    session.structured("scan", json!({}));
    let answer = session.call("findings", json!({ "cursor": stale }));
    let message = answer["error"]["message"]
        .as_str()
        .expect("a refusal")
        .to_owned();
    assert!(message.contains("scanned again"), "{message}");
    session.shutdown();
}

#[test]
fn a_path_outside_the_root_is_refused() {
    let root = workspace("containment");
    let mut session = Session::start(&root);

    for asked in ["..", "../..", "/etc", "src/../.."] {
        let answer = session.call("scan", json!({ "path": asked }));
        let message = answer["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("`{asked}` was not refused: {answer}"));
        assert!(message.contains("not inside"), "{message}");
    }
    session.shutdown();
}

#[test]
fn a_buffer_is_analysed_without_touching_the_disk() {
    let root = workspace("buffer");
    let mut session = Session::start(&root);

    let analysis = session.structured(
        "analyze",
        json!({
            "filename": "nowhere/on/disk.ts",
            "source": "export function wide(a, b, c, d, e, f) { return a + b + c + d + e + f; }",
        }),
    );
    assert_eq!(analysis["language"], json!("typescript"));
    let rules: Vec<&str> = analysis["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| finding["rule"].as_str().expect("a slug"))
        .collect();
    assert!(rules.contains(&"long-parameter-list"), "{rules:?}");
    assert_eq!(analysis["scope"], json!("buffer"));
    assert!(
        analysis["note"].as_str().expect("a note").contains("52 of the 60"),
        "an agent reading silence as a clean bill of health is the failure this prevents"
    );
    assert!(!root.join("nowhere").exists(), "the buffer reached the disk");
    session.shutdown();
}

/// A rule an agent is told about and a rule the terminal explains have to be the same
/// rule, or the two surfaces disagree about what rqx measures.
#[test]
fn explain_answers_from_the_catalogue() {
    let root = workspace("explain");
    let mut session = Session::start(&root);

    let rule = session.structured("explain", json!({ "name": "long-function" }));
    assert_eq!(rule["slug"], json!("long-function"));
    assert!(!rule["why"].as_str().expect("a reason").is_empty());
    assert!(!rule["fixes"].as_array().expect("fixes").is_empty());

    let pattern = session.structured("explain", json!({ "name": "decorator" }));
    assert_eq!(pattern["name"], json!("Decorator"));

    let answer = session.call("explain", json!({ "name": "no-such-thing" }));
    assert!(answer["error"]["message"].as_str().is_some());
    session.shutdown();
}

/// The reason the projections are shared: an agent and a CI job reading the same scan
/// must not be told different numbers.
#[test]
fn a_scan_over_mcp_matches_the_json_reporter() {
    let root = workspace("agreement");

    let reported = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", ".", "--reporter", "json"])
        .current_dir(&root)
        .output()
        .expect("the reporter ran");
    let summary: Value = serde_json::from_str(
        String::from_utf8(reported.stdout)
            .expect("utf-8")
            .lines()
            .next_back()
            .expect("a summary line"),
    )
    .expect("json");

    let mut session = Session::start(&root);
    let scan = session.structured("scan", json!({}));

    assert_eq!(scan["health"], summary["health_score"]);
    assert_eq!(scan["files_scanned"], summary["files_scanned"]);
    assert_eq!(scan["findings"]["total"], summary["counts"]["total"]);
    session.shutdown();
}

/// A server that binds a public address with no token hands every file it can parse to
/// whoever reaches the port, so it refuses to start rather than warning.
#[test]
fn a_public_bind_without_a_token_is_refused() {
    let root = workspace("bind");
    let refused = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["mcp", "--http", "0.0.0.0:0"])
        .current_dir(&root)
        .env_remove("RQX_MCP_TOKEN")
        .output()
        .expect("the process ran");

    assert_eq!(refused.status.code(), Some(2));
    let complaint = String::from_utf8_lossy(&refused.stderr);
    assert!(complaint.contains("RQX_MCP_TOKEN"), "{complaint}");
}

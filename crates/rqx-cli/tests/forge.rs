//! Posting to a merge request, against a server standing in for one.
//!
//! The sticky comment is the part worth holding: a pipeline that runs ten times has
//! to leave one comment, and the only way to know it does is to run it twice and
//! count. A hand-rolled listener rather than a mock, because what is being tested is
//! the request rqx actually makes — the verb, the path, the header and the body.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;

/// One request as the server saw it.
#[derive(Clone, Debug)]
struct Seen {
    verb: String,
    path: String,
    headers: Vec<String>,
    body: String,
}

/// A comment as the forge is holding it.
#[derive(Clone)]
struct Note {
    id: usize,
    body: String,
}

struct Forge {
    port: u16,
    seen: mpsc::Receiver<Seen>,
}

/// Answers the four calls rqx makes — list, create, edit and status — and nothing
/// else, so a request rqx should not be making shows up as a 404 rather than as a
/// pass.
fn stand_in() -> Forge {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let (told, seen) = mpsc::channel();

    thread::spawn(move || {
        let mut notes: Vec<Note> = Vec::new();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Some(request) = read_request(&mut stream) else {
                continue;
            };
            let _ = told.send(request.clone());
            if request.path == "/stop" {
                let _ = reply(&mut stream, 200, "{}");
                return;
            }
            let answer = answer(&request, &mut notes);
            let _ = reply(&mut stream, answer.0, &answer.1);
        }
    });
    Forge { port, seen }
}

fn read_request(stream: &mut TcpStream) -> Option<Seen> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut start = String::new();
    reader.read_line(&mut start).ok()?;
    let mut parts = start.split_whitespace();
    let verb = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();

    let mut headers = Vec::new();
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let line = line.trim_end().to_owned();
        if line.is_empty() {
            break;
        }
        if let Some(said) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = said.trim().parse().unwrap_or(0);
        }
        headers.push(line);
    }
    let mut body = vec![0; length];
    if length > 0 {
        reader.read_exact(&mut body).ok()?;
    }
    Some(Seen {
        verb,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn answer(request: &Seen, notes: &mut Vec<Note>) -> (u16, String) {
    let body_field = |raw: &str| {
        serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|value| value["body"].as_str().map(str::to_owned))
            .unwrap_or_default()
    };
    let listing = request.path.contains("/notes") || request.path.contains("/comments");
    let paged_past_one = request.path.contains("page=2") || request.path.contains("page=3");

    // A project called `refuse` is how a test asks for the unhappy path: rqx has to
    // report the code and what the forge said, not "curl exited 22".
    if request.path.contains("refuse") {
        return (403, r#"{"message":"insufficient scope"}"#.to_owned());
    }

    match request.verb.as_str() {
        "GET" if listing && paged_past_one => (200, "[]".to_owned()),
        "GET" if listing => {
            let items: Vec<String> = notes
                .iter()
                .map(|note| {
                    format!(
                        r#"{{"id":{},"body":{}}}"#,
                        note.id,
                        serde_json::to_string(&note.body).unwrap_or_default()
                    )
                })
                .collect();
            (200, format!("[{}]", items.join(",")))
        }
        "POST" if request.path.contains("/statuses/") => (201, "{}".to_owned()),
        "POST" if listing => {
            let id = notes.len() + 1;
            notes.push(Note {
                id,
                body: body_field(&request.body),
            });
            (201, format!(r#"{{"id":{id}}}"#))
        }
        "PUT" | "PATCH" => {
            let which: usize = request
                .path
                .rsplit('/')
                .next()
                .and_then(|last| last.parse().ok())
                .unwrap_or(0);
            match notes.iter_mut().find(|note| note.id == which) {
                Some(note) => {
                    note.body = body_field(&request.body);
                    (200, format!(r#"{{"id":{which}}}"#))
                }
                None => (404, r#"{"message":"no such note"}"#.to_owned()),
            }
        }
        _ => (
            404,
            r#"{"message":"not a call rqx should make"}"#.to_owned(),
        ),
    }
}

fn reply(stream: &mut TcpStream, code: u16, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-forge-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    std::fs::write(root.join("a.ts"), "export const kept = 1\n").expect("a file");
    Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("init")
        .arg("-q")
        .output()
        .expect("git init");
    for (key, value) in [
        ("user.email", "t@t"),
        ("user.name", "Tester"),
        ("core.autocrlf", "false"),
    ] {
        let _ = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["config", key, value])
            .output();
    }
    let _ = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["add", "-A"])
        .output();
    let _ = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["commit", "-qm", "one"])
        .output();
    // Left uncommitted, so `--base HEAD` has a change to review. A review of nothing
    // passes, which would make every assertion here about a passing gate.
    std::fs::write(
        root.join("b.ts"),
        "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n",
    )
    .expect("a change to review");
    root
}

fn comment(root: &Path, port: u16, extra: &[(&str, &str)], args: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rqx"));
    command
        .arg("comment")
        .arg(root)
        .args(["--base", "HEAD", "--fail-on", "error"])
        .args(args)
        .env_remove("CI_MERGE_REQUEST_IID")
        .env_remove("GITHUB_REPOSITORY")
        .env("RQX_NO_CACHE", "1");
    for (key, value) in extra {
        command.env(key, value.replace("{port}", &port.to_string()));
    }
    command.output().expect("failed to run rqx")
}

fn drain(forge: &Forge) -> Vec<Seen> {
    let _ = std::net::TcpStream::connect(("127.0.0.1", forge.port)).map(|mut stream| {
        let _ = write!(stream, "GET /stop HTTP/1.1\r\nHost: x\r\n\r\n");
    });
    forge.seen.try_iter().collect()
}

const GITLAB: &[(&str, &str)] = &[
    ("CI_MERGE_REQUEST_IID", "42"),
    ("CI_PROJECT_ID", "1234"),
    ("CI_API_V4_URL", "http://127.0.0.1:{port}/api/v4"),
    ("GITLAB_TOKEN", "secret"),
    ("CI_COMMIT_SHA", "abc123"),
];

/// Ten pipeline runs, one comment. The whole point of the hidden marker.
#[test]
fn a_second_run_edits_the_comment_the_first_one_left() {
    let root = project("sticky");
    let forge = stand_in();

    let first = comment(&root, forge.port, GITLAB, &[]);
    let second = comment(&root, forge.port, GITLAB, &[]);
    let calls = drain(&forge);

    assert!(
        String::from_utf8_lossy(&first.stderr).contains("commented on"),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("updated the comment"),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );

    let creates = calls
        .iter()
        .filter(|call| call.verb == "POST" && call.path.contains("/notes"))
        .count();
    let edits = calls.iter().filter(|call| call.verb == "PUT").count();
    assert_eq!(creates, 1, "a second comment was created");
    assert_eq!(edits, 1, "the first comment was not edited");
    let _ = std::fs::remove_dir_all(&root);
}

/// The token goes in the header GitLab reads, and the grade lands on the commit.
#[test]
fn gitlab_gets_its_header_and_its_commit_status() {
    let root = project("gitlab");
    let forge = stand_in();
    comment(&root, forge.port, GITLAB, &[]);
    let calls = drain(&forge);

    assert!(
        calls.iter().any(|call| call
            .headers
            .iter()
            .any(|header| header == "PRIVATE-TOKEN: secret")),
        "the token was not sent as GitLab reads it"
    );
    let status = calls
        .iter()
        .find(|call| call.path.contains("/statuses/abc123"))
        .expect("no commit status");
    assert!(status.body.contains("\"failed\""), "{}", status.body);
    assert!(status.body.contains("health"), "{}", status.body);
    let _ = std::fs::remove_dir_all(&root);
}

/// GitHub edits with `PATCH` at a different path, which is the one thing that could
/// be copied wrong from the GitLab side and still look right in review.
#[test]
fn github_uses_its_own_verbs_and_paths() {
    let root = project("github");
    let event = root.join("event.json");
    // `GITHUB_SHA` on a pull_request event is the merge commit the runner built,
    // which belongs to no branch: a status posted there shows on nothing. The head
    // is only in the payload, and that is the one the status has to name.
    std::fs::write(
        &event,
        r#"{"pull_request":{"number":7,"head":{"sha":"beadfeed"}}}"#,
    )
    .expect("an event");
    let github: Vec<(&str, &str)> = vec![
        ("GITHUB_REPOSITORY", "acme/widgets"),
        ("GITHUB_API_URL", "http://127.0.0.1:{port}"),
        ("GITHUB_TOKEN", "secret"),
        ("GITHUB_SHA", "deadbeef"),
        ("GITHUB_EVENT_PATH", event.to_str().expect("utf-8")),
    ];

    let forge = stand_in();
    comment(&root, forge.port, &github, &[]);
    comment(&root, forge.port, &github, &[]);
    let calls = drain(&forge);

    assert!(
        calls
            .iter()
            .any(|call| call.verb == "POST" && call.path == "/repos/acme/widgets/issues/7/comments"),
        "wrong create path: {:?}",
        calls.iter().map(|c| &c.path).collect::<Vec<_>>()
    );
    assert!(
        calls
            .iter()
            .any(|call| call.verb == "PATCH" && call.path.contains("/issues/comments/")),
        "GitHub was not edited with PATCH: {:?}",
        calls.iter().map(|c| (&c.verb, &c.path)).collect::<Vec<_>>()
    );
    assert!(
        calls
            .iter()
            .any(|call| call.path == "/repos/acme/widgets/statuses/beadfeed"),
        "the status did not name the pull request's head: {:?}",
        calls.iter().map(|c| &c.path).collect::<Vec<_>>()
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.path.contains("statuses/deadbeef")),
        "the status named the merge commit"
    );
    assert!(
        calls.iter().any(|call| {
            call.headers
                .iter()
                .any(|header| header == "Authorization: Bearer secret")
        }),
        "the token was not sent as GitHub reads it"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Two jobs on one merge request each keep their own comment.
#[test]
fn a_different_marker_keeps_a_different_comment() {
    let root = project("markers");
    let forge = stand_in();
    comment(&root, forge.port, GITLAB, &[]);
    comment(&root, forge.port, GITLAB, &["--marker", "api"]);
    let calls = drain(&forge);

    let creates = calls
        .iter()
        .filter(|call| call.verb == "POST" && call.path.contains("/notes"))
        .count();
    assert_eq!(creates, 2, "the second marker reused the first's comment");
    let _ = std::fs::remove_dir_all(&root);
}

/// A dry run is a dry run: it prints the comment and touches nothing.
#[test]
fn a_dry_run_posts_nothing() {
    let root = project("dry");
    let forge = stand_in();
    let output = comment(&root, forge.port, GITLAB, &["--dry-run"]);
    let calls = drain(&forge);

    let body = String::from_utf8_lossy(&output.stdout);
    assert!(body.contains("### "), "no comment was printed:\n{body}");
    assert!(body.contains("<details>"), "no detail was printed");
    assert!(
        calls.iter().all(|call| call.path == "/stop"),
        "a dry run reached the forge: {:?}",
        calls.iter().map(|c| &c.path).collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Outside a pipeline there is no merge request, and saying so is more use than a
/// stack trace about a missing variable.
#[test]
fn no_merge_request_is_explained_rather_than_crashed() {
    let root = project("nowhere");
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg("comment")
        .arg(&root)
        .args(["--base", "HEAD"])
        .env_remove("CI_MERGE_REQUEST_IID")
        .env_remove("GITHUB_REPOSITORY")
        .env("RQX_NO_CACHE", "1")
        .output()
        .expect("failed to run rqx");
    let said = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(said.contains("no merge request"), "{said}");
    let _ = std::fs::remove_dir_all(&root);
}

/// A forge that says no says why. The token is scoped wrong more often than anything
/// else goes wrong here, and "curl exited 22" sends nobody anywhere.
#[test]
fn a_refusal_carries_the_code_and_what_the_forge_said() {
    let root = project("refused");
    let forge = stand_in();
    let refusing: Vec<(&str, &str)> = vec![
        ("CI_MERGE_REQUEST_IID", "42"),
        ("CI_PROJECT_ID", "refuse"),
        ("CI_API_V4_URL", "http://127.0.0.1:{port}/api/v4"),
        ("GITLAB_TOKEN", "secret"),
        ("CI_COMMIT_SHA", "abc123"),
    ];

    let output = comment(&root, forge.port, &refusing, &[]);
    let _ = drain(&forge);
    let said = String::from_utf8_lossy(&output.stderr).into_owned();

    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(said.contains("403"), "{said}");
    assert!(said.contains("insufficient scope"), "{said}");
    let _ = std::fs::remove_dir_all(&root);
}

/// The token is a secret on a shared runner, where `ps` is readable by every other
/// job. It goes to curl in a file, never in an argument.
#[test]
fn the_token_never_reaches_the_command_line() {
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/forge.rs"))
            .expect("forge.rs");
    let request = source
        .split_once("fn request(")
        .expect("a request function")
        .1;
    let request = request.split("\n    }\n").next().expect("its body");

    assert!(
        !request.contains("\"--header\""),
        "a header is being passed as an argument"
    );
    assert!(
        request.contains("--config"),
        "headers should go through a curl config file"
    );
}

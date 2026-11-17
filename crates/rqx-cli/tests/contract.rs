//! The machine formats other tools read: the JSON stream, SARIF, JUnit, the GitLab
//! Code Quality report, the GitHub annotations, and the `check` verdict. Each is
//! compared against a checked-in golden under `tests/contract/`.
//!
//! The comparison is a superset check, not an equality check. Everything the golden
//! holds must still be there with the same value; anything new may sit beside it.
//! Formats only ever grow, because a consumer that reads a field today reads it
//! after the next release too.
//!
//! JSON, SARIF and GitLab: every key path and value in the golden is present and
//! equal in the output, arrays compared by index. The JSON stream is compared per
//! finding line, with the summary line matched on its own. JUnit: every element and
//! attribute in the golden is present, children matched by index. GitHub: every
//! golden line appears in the output.
//!
//! Volatile values (elapsed time, the version, the temporary directory) are
//! normalised before comparing and before a golden is written.
//!
//! A deliberate change to a format, or a new rule that fires on the fixtures, moves a
//! golden. Regenerate them with `RQX_BLESS=1 cargo test -p rqx --test contract` and
//! review the diff: a golden may gain lines, and losing one is a breaking change.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const VERSION_PLACEHOLDER: &str = "<version>";
const ROOT_PLACEHOLDER: &str = "<root>";

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/contract")
        .join(name)
}

fn blessing() -> bool {
    std::env::var_os("RQX_BLESS").is_some()
}

/// A copy of the TypeScript fixtures outside any repository, so the churn and
/// authorship rules (which read git history) stay silent and the output depends on
/// the fixture files alone.
struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("rqx-contract-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a temporary workspace");
        Self { root }
    }

    fn copy_fixtures_into(&self, destination: &Path) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/typescript");
        fs::create_dir_all(destination).expect("a destination");
        for entry in fs::read_dir(source).expect("the fixtures") {
            let entry = entry.expect("an entry");
            fs::copy(entry.path(), destination.join(entry.file_name())).expect("a fixture");
        }
    }

    fn rqx(&self, directory: &Path, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
            .current_dir(directory)
            .env("RQX_NO_UPDATE_CHECK", "1")
            .args(args)
            .arg("--no-tui")
            .output()
            .expect("failed to run rqx");
        let code = output.status.code().unwrap_or(-1);
        assert!(
            code == 0 || code == 1,
            "rqx {args:?} could not run ({code}): {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf-8 output")
    }

    fn git(&self, directory: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .expect("git")
            .status;
        assert!(status.success(), "git {args:?} failed");
    }

    fn scrub(&self, text: &str) -> String {
        text.replace(self.root.to_string_lossy().as_ref(), ROOT_PLACEHOLDER)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn scan_fixtures(reporter: &str) -> (Workspace, String) {
    let workspace = Workspace::new(&format!("scan-{reporter}"));
    workspace.copy_fixtures_into(&workspace.root.join("typescript"));
    let output = workspace.rqx(
        &workspace.root,
        &[
            "scan",
            "typescript",
            "--max-findings",
            "0",
            "--reporter",
            reporter,
        ],
    );
    let output = workspace.scrub(&output);
    (workspace, output)
}

// Normalisation -----------------------------------------------------------------

fn normalise_value(value: &mut Value) {
    match value {
        Value::String(text) if text == env!("CARGO_PKG_VERSION") => {
            *text = VERSION_PLACEHOLDER.to_owned();
        }
        Value::Array(items) => items.iter_mut().for_each(normalise_value),
        Value::Object(fields) => {
            if let Some(elapsed) = fields.get_mut("elapsed_ms") {
                *elapsed = Value::from(0);
            }
            fields.values_mut().for_each(normalise_value);
        }
        _ => {}
    }
}

fn parse_json(text: &str) -> Value {
    let mut value: Value = serde_json::from_str(text).expect("valid JSON");
    normalise_value(&mut value);
    value
}

/// One JSON document per line. The findings come first and the summary closes the
/// stream, so the two are held apart: a finding added mid-stream moves the summary's
/// line number but not its identity.
struct Stream {
    findings: Vec<Value>,
    summary: Value,
}

fn parse_stream(text: &str) -> Stream {
    let mut findings = Vec::new();
    let mut summary = None;
    for line in text.lines().filter(|line| !line.is_empty()) {
        let value = parse_json(line);
        if value["kind"] == "summary" {
            summary = Some(value);
        } else {
            findings.push(value);
        }
    }
    Stream {
        findings,
        summary: summary.expect("a summary line"),
    }
}

fn write_stream(stream: &Stream) -> String {
    let mut text = String::new();
    for line in stream.findings.iter().chain([&stream.summary]) {
        text.push_str(&serde_json::to_string(line).expect("serialisable"));
        text.push('\n');
    }
    text
}

/// A time attribute is a measurement, not a contract.
fn normalise_times(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find(" time=\"") {
        let value_start = start + " time=\"".len();
        out.push_str(&rest[..value_start]);
        out.push('0');
        let value_end = rest[value_start..].find('"').expect("a closing quote");
        rest = &rest[value_start + value_end..];
    }
    out.push_str(rest);
    out
}

// Superset comparison -------------------------------------------------------------

/// Everything in `golden` is in `actual`. Objects may gain keys and arrays may gain
/// trailing elements; a value that was there must be unchanged.
fn assert_superset(golden: &Value, actual: &Value) {
    if let Err(difference) = superset(golden, actual, "$") {
        panic!("{difference}");
    }
}

fn superset(golden: &Value, actual: &Value, path: &str) -> Result<(), String> {
    match (golden, actual) {
        (Value::Object(expected), Value::Object(found)) => {
            for (key, expected_value) in expected {
                let at = format!("{path}.{key}");
                let found_value = found
                    .get(key)
                    .ok_or_else(|| format!("{at} is gone from the output"))?;
                superset(expected_value, found_value, &at)?;
            }
            Ok(())
        }
        (Value::Array(expected), Value::Array(found)) => {
            if found.len() < expected.len() {
                return Err(format!(
                    "{path} lost elements: the golden has {}, the output {}",
                    expected.len(),
                    found.len()
                ));
            }
            for (index, (expected_item, found_item)) in expected.iter().zip(found).enumerate() {
                superset(expected_item, found_item, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        _ if golden == actual => Ok(()),
        _ => Err(format!("{path} changed: golden {golden}, output {actual}")),
    }
}

fn assert_stream_superset(golden: &Stream, actual: &Stream) {
    assert!(
        actual.findings.len() >= golden.findings.len(),
        "the stream lost findings: golden {}, output {}",
        golden.findings.len(),
        actual.findings.len()
    );
    for (index, (expected, found)) in golden.findings.iter().zip(&actual.findings).enumerate() {
        if let Err(difference) = superset(expected, found, &format!("finding {index}")) {
            panic!("{difference}");
        }
    }
    if let Err(difference) = superset(&golden.summary, &actual.summary, "summary") {
        panic!("{difference}");
    }
}

// A minimal XML tree --------------------------------------------------------------

#[derive(Debug)]
struct Element {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<Element>,
}

/// Reads the elements and attributes of a document and skips text, comments and the
/// declaration. Enough for what the JUnit reporter writes, nothing more.
fn parse_xml(text: &str) -> Element {
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        rest = &rest[open..];
        if rest.starts_with("<?") {
            rest = &rest[rest.find("?>").expect("a closed declaration") + 2..];
        } else if rest.starts_with("<!--") {
            rest = &rest[rest.find("-->").expect("a closed comment") + 3..];
        } else if let Some(closing) = rest.strip_prefix("</") {
            let end = closing.find('>').expect("a closed tag");
            let finished = stack.pop().expect("an open element");
            assert_eq!(finished.name, closing[..end].trim(), "mismatched tags");
            rest = &closing[end + 1..];
            attach(&mut stack, &mut root, finished);
        } else {
            let end = tag_end(rest);
            let self_closing = rest[..end].ends_with('/');
            let body = rest[1..end].trim_end_matches('/');
            let element = parse_tag(body);
            rest = &rest[end + 1..];
            if self_closing {
                attach(&mut stack, &mut root, element);
            } else {
                stack.push(element);
            }
        }
    }
    root.expect("a root element")
}

fn attach(stack: &mut [Element], root: &mut Option<Element>, finished: Element) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(finished),
        None => *root = Some(finished),
    }
}

/// The index of the `>` that closes a tag, ignoring any inside an attribute value.
fn tag_end(tag: &str) -> usize {
    let mut quote = None;
    for (index, character) in tag.char_indices() {
        match (quote, character) {
            (None, '"' | '\'') => quote = Some(character),
            (Some(open), _) if open == character => quote = None,
            (None, '>') => return index,
            _ => {}
        }
    }
    panic!("an unclosed tag: {tag:.80}");
}

fn parse_tag(body: &str) -> Element {
    let (name, mut rest) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
    let mut attributes = Vec::new();
    loop {
        rest = rest.trim_start();
        let Some((key, after)) = rest.split_once('=') else {
            break;
        };
        let after = after.trim_start();
        let quote = after.chars().next().expect("a quoted value");
        let value_end = after[1..].find(quote).expect("a closing quote") + 1;
        attributes.push((key.trim().to_owned(), after[1..value_end].to_owned()));
        rest = &after[value_end + 1..];
    }
    Element {
        name: name.to_owned(),
        attributes,
        children: Vec::new(),
    }
}

fn element_superset(golden: &Element, actual: &Element, path: &str) -> Result<(), String> {
    let here = format!("{path}/{}", golden.name);
    if golden.name != actual.name {
        return Err(format!(
            "{path}: expected <{}>, found <{}>",
            golden.name, actual.name
        ));
    }
    for (key, value) in &golden.attributes {
        let found = actual
            .attributes
            .iter()
            .find(|(name, _)| name == key)
            .ok_or_else(|| format!("{here} lost its `{key}` attribute"))?;
        if &found.1 != value {
            return Err(format!(
                "{here}@{key} changed: golden `{value}`, output `{}`",
                found.1
            ));
        }
    }
    if actual.children.len() < golden.children.len() {
        return Err(format!(
            "{here} lost children: the golden has {}, the output {}",
            golden.children.len(),
            actual.children.len()
        ));
    }
    for (index, (expected, found)) in golden.children.iter().zip(&actual.children).enumerate() {
        element_superset(expected, found, &format!("{here}[{index}]"))?;
    }
    Ok(())
}

// Golden handling -------------------------------------------------------------------

/// Writes the golden when blessing; otherwise returns what is checked in.
fn golden_text(name: &str, normalised_output: &str) -> Option<String> {
    let path = golden_path(name);
    if blessing() {
        fs::create_dir_all(path.parent().expect("a directory")).expect("the golden directory");
        fs::write(&path, normalised_output).expect("a golden");
        return None;
    }
    Some(fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "no golden at {}; generate it with RQX_BLESS=1",
            path.display()
        )
    }))
}

fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("serialisable");
    text.push('\n');
    text
}

fn check_json_document(name: &str, output: &str) {
    let actual = parse_json(output);
    if let Some(golden) = golden_text(name, &pretty(&actual)) {
        assert_superset(&parse_json(&golden), &actual);
    }
}

fn check_json_stream(name: &str, output: &str) {
    let actual = parse_stream(output);
    if let Some(golden) = golden_text(name, &write_stream(&actual)) {
        assert_stream_superset(&parse_stream(&golden), &actual);
    }
}

#[test]
fn the_json_stream_only_grows() {
    let (_workspace, output) = scan_fixtures("json");
    check_json_stream("scan.json.ndjson", &output);
}

#[test]
fn sarif_only_grows() {
    let (_workspace, output) = scan_fixtures("sarif");
    check_json_document("scan.sarif.json", &output);
}

#[test]
fn the_gitlab_code_quality_report_only_grows() {
    let (_workspace, output) = scan_fixtures("gitlab");
    check_json_document("scan.gitlab.json", &output);
}

#[test]
fn junit_only_grows() {
    let (_workspace, output) = scan_fixtures("junit");
    let output = normalise_times(&output);
    if let Some(golden) = golden_text("scan.junit.xml", &output)
        && let Err(difference) = element_superset(&parse_xml(&golden), &parse_xml(&output), "")
    {
        panic!("{difference}");
    }
}

#[test]
fn the_github_annotations_only_grow() {
    let (_workspace, output) = scan_fixtures("github");
    if let Some(golden) = golden_text("scan.github.txt", &output) {
        let lines: Vec<&str> = output.lines().collect();
        for line in golden.lines() {
            assert!(
                lines.contains(&line),
                "an annotation is gone from the output: {line:.200}"
            );
        }
    }
}

/// `check` reports the same stream as `scan` and adds a `review` object to the
/// summary. The fixtures are committed on top of a one-file commit, so every fixture
/// counts as changed and the verdict has something to say.
#[test]
fn the_check_verdict_only_grows() {
    let workspace = Workspace::new("check");
    let repository = workspace.root.join("repository");
    fs::create_dir_all(&repository).expect("a repository");
    workspace.git(&repository, &["init", "-q"]);
    fs::write(repository.join("seed.ts"), "export const seed = 1\n").expect("a seed");
    workspace.git(&repository, &["add", "seed.ts"]);
    workspace.git(&repository, &["commit", "-qm", "base"]);
    workspace.copy_fixtures_into(&repository);
    workspace.git(&repository, &["add", "."]);
    workspace.git(&repository, &["commit", "-qm", "branch"]);

    let output = workspace.rqx(
        &repository,
        &["check", ".", "--base", "HEAD~1", "--reporter", "json"],
    );
    check_json_stream("check.json.ndjson", &workspace.scrub(&output));
}

// The comparison itself ------------------------------------------------------------

#[test]
fn a_new_key_is_allowed_and_a_missing_or_changed_one_is_not() {
    let golden: Value = serde_json::json!({"a": 1, "list": [{"x": 1}], "nested": {"k": "v"}});
    let grown = serde_json::json!({"a": 1, "b": 2, "list": [{"x": 1, "y": 2}, {"x": 3}], "nested": {"k": "v", "more": true}});
    assert!(superset(&golden, &grown, "$").is_ok());

    let missing = serde_json::json!({"list": [{"x": 1}], "nested": {"k": "v"}});
    assert!(
        superset(&golden, &missing, "$")
            .unwrap_err()
            .contains("$.a")
    );

    let changed = serde_json::json!({"a": 1, "list": [{"x": 2}], "nested": {"k": "v"}});
    assert!(
        superset(&golden, &changed, "$")
            .unwrap_err()
            .contains("$.list[0].x")
    );

    let shorter = serde_json::json!({"a": 1, "list": [], "nested": {"k": "v"}});
    assert!(superset(&golden, &shorter, "$").is_err());
}

#[test]
fn an_element_may_gain_attributes_and_children_but_not_lose_them() {
    let golden = parse_xml("<?xml version=\"1.0\"?><a n=\"1\"><b k=\"v\" /></a>");
    let grown =
        parse_xml("<a n=\"1\" extra=\"2\"><b k=\"v\" more=\"x\">text &gt; here</b><c/></a>");
    assert!(element_superset(&golden, &grown, "").is_ok());

    let lost = parse_xml("<a><b k=\"v\"/></a>");
    assert!(element_superset(&golden, &lost, "").is_err());
    let changed = parse_xml("<a n=\"1\"><b k=\"w\"/></a>");
    assert!(element_superset(&golden, &changed, "").is_err());
}

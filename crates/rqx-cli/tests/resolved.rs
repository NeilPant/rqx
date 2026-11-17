//! The credit a change earns: findings present at the base that it removed, and how
//! the health score moved.

mod common;
use std::process::Command;

use common::Repository;

const DEAD: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";
const FIXED: &str = "export function keep(): number {\n  return 1\n}\n";
const OTHER: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
/// `OTHER` with a second function: a change must add one for the base to be scanned.
const OTHER_AND_MORE: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\nexport function sub(a: number, b: number): number {\n  return a - b\n}\n";

fn repository(name: &str, after: &str) -> Repository {
    let repository = Repository::new(name);
    repository.commit(
        "base",
        &[("src/keep.ts", Some(DEAD)), ("src/add.ts", Some(OTHER))],
    );
    repository.commit(
        "change",
        &[
            ("src/keep.ts", Some(after)),
            ("src/add.ts", Some(OTHER_AND_MORE)),
        ],
    );
    repository
}

fn summary(lines: &[serde_json::Value]) -> &serde_json::Value {
    lines
        .iter()
        .find(|line| line["kind"] == "summary")
        .expect("a summary line")
}

#[test]
fn a_finding_the_change_removed_is_credited_with_the_health_it_gained() {
    let repository = repository("resolved-fixed", FIXED);
    let lines = repository.rqx_lines(&["check", ".", "--base", "HEAD~1", "--reporter", "json"]);
    let summary = summary(&lines);

    let resolved = summary["resolved"].as_array().expect("a resolved array");
    assert!(
        resolved
            .iter()
            .any(|entry| entry["rule"] == "unreachable-code" && entry["file"] == "src/keep.ts"),
        "{resolved:#?}"
    );
    assert!(summary["health_delta"].as_i64().expect("a delta") > 0);
}

#[test]
fn a_change_that_fixes_nothing_credits_nothing() {
    let repository = Repository::new("resolved-unchanged");
    repository.commit(
        "base",
        &[("src/keep.ts", Some(DEAD)), ("src/add.ts", Some(OTHER))],
    );
    repository.commit("touch", &[("src/add.ts", Some(OTHER_AND_MORE))]);
    let lines = repository.rqx_lines(&["check", ".", "--base", "HEAD~1", "--reporter", "json"]);
    let summary = summary(&lines);

    assert_eq!(summary["resolved"], serde_json::json!([]));
}

#[test]
fn a_scan_and_a_review_without_a_revision_carry_no_credit() {
    let repository = repository("resolved-absent", FIXED);
    let scan = repository.rqx_lines(&["scan", ".", "--reporter", "json"]);
    assert!(summary(&scan).get("resolved").is_none());
    assert!(summary(&scan).get("health_delta").is_none());

    let listed = repository.rqx_lines(&["check", ".", "--files-from", "-", "--reporter", "json"]);
    assert!(summary(&listed).get("resolved").is_none());
}

#[test]
fn the_comment_has_a_resolved_section() {
    let repository = repository("resolved-comment", FIXED);
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["comment", ".", "--base", "HEAD~1", "--dry-run"])
        .current_dir(&repository.root)
        .env("HOME", &repository.root)
        .env("XDG_CONFIG_HOME", &repository.root)
        .output()
        .expect("rqx runs");
    let body = String::from_utf8_lossy(&output.stdout);

    assert!(body.contains("### Resolved"), "{body}");
    assert!(
        body.contains("`src/keep.ts:3` **unreachable-code**"),
        "{body}"
    );
    assert!(body.contains("Resolves 1 finding."), "{body}");
}

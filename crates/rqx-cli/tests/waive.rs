//! `rqx waive` writes the comment the scan honours, and refuses what the scan would
//! ignore.

mod common;
use common::Scratch;

use std::path::Path;
use std::process::{Command, Output};

const DIRTY: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";

fn project(name: &str) -> Scratch {
    let root = Scratch::new(&format!("waive-{name}"));
    std::fs::write(root.join("a.ts"), DIRTY).expect("a file");
    root
}

fn rqx(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .current_dir(root)
        .args(args)
        .output()
        .expect("failed to run rqx")
}

fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn a_waiver_it_wrote_is_one_the_scan_lists() {
    let root = project("round-trip");
    let waived = rqx(
        &root,
        &[
            "waive",
            "a.ts:3",
            "unreachable-code",
            "--reason",
            "kept for the demo",
        ],
    );
    assert_eq!(waived.status.code(), Some(0), "{}", said(&waived));

    let text = std::fs::read_to_string(root.join("a.ts")).expect("the file");
    assert!(
        text.contains("  // rqx-ignore unreachable-code: kept for the demo\n  console.log('dead')"),
        "{text}"
    );

    let listed = said(&rqx(&root, &["suppressions", "."]));
    assert!(listed.contains("unreachable-code"), "{listed}");
    assert!(listed.contains("kept for the demo"), "{listed}");
}

#[test]
fn an_unknown_rule_is_refused_with_the_one_it_was_close_to() {
    let root = project("unknown");
    let refused = rqx(
        &root,
        &["waive", "a.ts:3", "unreachable-cod", "--reason", "x"],
    );
    assert_eq!(refused.status.code(), Some(2));
    let message = said(&refused);
    assert!(message.contains("`unreachable-code`"), "{message}");
    assert_eq!(
        std::fs::read_to_string(root.join("a.ts")).expect("file"),
        DIRTY
    );
}

#[test]
fn a_waiver_without_a_reason_is_refused() {
    let root = project("no-reason");
    let blank = rqx(
        &root,
        &["waive", "a.ts:3", "unreachable-code", "--reason", "  "],
    );
    assert_eq!(blank.status.code(), Some(2));
    let absent = rqx(&root, &["waive", "a.ts:3", "unreachable-code"]);
    assert_eq!(absent.status.code(), Some(2));
    assert_eq!(
        std::fs::read_to_string(root.join("a.ts")).expect("file"),
        DIRTY
    );
}

#[test]
fn a_dry_run_prints_the_diff_and_changes_nothing() {
    let root = project("dry-run");
    let dry = rqx(
        &root,
        &[
            "waive",
            "a.ts:3",
            "unreachable-code",
            "--reason",
            "demo",
            "--dry-run",
        ],
    );
    assert_eq!(dry.status.code(), Some(0), "{}", said(&dry));
    let diff = said(&dry);
    assert!(
        diff.contains("+  // rqx-ignore unreachable-code: demo"),
        "{diff}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("a.ts")).expect("file"),
        DIRTY
    );
}

#[test]
fn a_whole_file_waiver_needs_no_line() {
    let root = project("whole-file");
    let waived = rqx(
        &root,
        &[
            "waive",
            "a.ts",
            "unreachable-code",
            "--reason",
            "generated",
            "--whole-file",
        ],
    );
    assert_eq!(waived.status.code(), Some(0), "{}", said(&waived));
    let text = std::fs::read_to_string(root.join("a.ts")).expect("the file");
    assert!(
        text.starts_with("// rqx-ignore-file unreachable-code: generated\n"),
        "{text}"
    );
}

/// The scan's findings by rule, from its JSON output.
fn scanned_rules(root: &Path) -> Vec<String> {
    let scanned = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .current_dir(root)
        .env("RQX_NO_CACHE", "1")
        .args(["scan", ".", "--reporter", "json", "--max-findings", "0"])
        .output()
        .expect("failed to run rqx");
    String::from_utf8_lossy(&scanned.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|record| record["kind"] == "finding")
        .map(|record| {
            format!(
                "{}:{}",
                record["rule"].as_str().unwrap_or_default(),
                record["evidence"]["directive"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

#[test]
fn a_waiver_below_a_finding_does_not_waive_it() {
    let root = project("below");
    std::fs::write(
        root.join("a.ts"),
        format!("{DIRTY}// rqx-ignore unreachable-code: placed at the end of the file\n"),
    )
    .expect("a file");
    let rules = scanned_rules(&root);
    assert!(
        rules
            .iter()
            .any(|rule| rule.starts_with("unreachable-code:")),
        "{rules:?}"
    );
}

#[test]
fn a_waiver_that_names_no_rule_is_reported() {
    let root = project("misnamed");
    std::fs::write(
        root.join("a.ts"),
        DIRTY.replace(
            "  console.log",
            "  // rqx-ignore unreachable-cod: kept for the demo\n  console.log",
        ),
    )
    .expect("a file");
    let rules = scanned_rules(&root);
    assert!(
        rules.contains(
            &"unjustified-suppression:rqx-ignore names `unreachable-cod`, which is not a rule \
              or a family; did you mean `unreachable-code`?"
                .to_owned()
        ),
        "{rules:?}"
    );
    assert!(
        rules
            .iter()
            .any(|rule| rule.starts_with("unreachable-code:")),
        "a misnamed waiver waives nothing: {rules:?}"
    );
}

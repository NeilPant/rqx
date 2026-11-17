use std::path::{Path, PathBuf};
use std::process::Command;

/// A repository with two commits: the files as they were, then as they are. Verifying
/// needs real history, so these tests build one rather than mocking git.
fn repository(name: &str, before: &[(&str, &str)], after: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-verify-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp repository");

    git(&root, &["init", "-q"]);
    git(&root, &["config", "core.autocrlf", "false"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "user.name", "test"]);

    write(&root, before);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "before"]);

    for (path, _) in before {
        if !after.iter().any(|(name, _)| name == path) {
            let _ = std::fs::remove_file(root.join(path));
        }
    }
    write(&root, after);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "after"]);
    root
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        std::fs::write(root.join(path), text).expect("a file");
    }
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git");
    assert!(
        status.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

struct Verdict {
    changes: Vec<(String, String, String)>,
    passed: bool,
}

impl Verdict {
    fn has(&self, outcome: &str, rule: &str) -> bool {
        self.changes
            .iter()
            .any(|(kind, name, _)| kind == outcome && name == rule)
    }
}

fn verify(root: &Path, extra: &[&str]) -> Verdict {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args([
            "verify",
            root.to_str().expect("utf-8"),
            "--base",
            "HEAD~1",
            "--json",
        ])
        .args(extra)
        .output()
        .expect("failed to run rqx");
    let body = String::from_utf8(output.stdout).expect("utf-8 output");

    let mut changes = Vec::new();
    let mut passed = false;
    for line in body.lines() {
        let value: serde_json::Value = serde_json::from_str(line).expect("a json object");
        if value["kind"] == "change" {
            changes.push((
                value["outcome"].as_str().unwrap_or_default().to_owned(),
                value["rule"].as_str().unwrap_or_default().to_owned(),
                value["subject"].as_str().unwrap_or_default().to_owned(),
            ));
        } else {
            passed = value["passed"].as_bool().unwrap_or(false);
        }
    }
    Verdict { changes, passed }
}

#[test]
fn a_finding_that_was_removed_is_resolved() {
    let root = repository(
        "resolved",
        &[(
            "a.ts",
            "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n",
        )],
        &[("a.ts", "export function keep(): number {\n  return 1\n}\n")],
    );
    let verdict = verify(&root, &[]);

    assert!(
        verdict.has("resolved", "unreachable-code"),
        "{:?}",
        verdict.changes
    );
    assert!(verdict.passed);
    let _ = std::fs::remove_dir_all(&root);
}

/// The finding that matters most: a body moved into a helper takes its problem with
/// it, and a tool that called that "resolved" would be endorsing the wrong diff.
#[test]
fn a_finding_that_moved_house_is_relocated_rather_than_resolved() {
    let root = repository(
        "relocated",
        &[
            ("index.ts", "export const entry = 1\n"),
            (
                "a.ts",
                "import { entry } from './index'\nexport function keep() {\n  const spare = 1\n  return entry\n}\n",
            ),
        ],
        &[
            ("index.ts", "export const entry = 1\n"),
            (
                "a.ts",
                "import { entry } from './index'\nexport function keep() {\n  return entry\n}\n",
            ),
            (
                "b.ts",
                "export function other() {\n  const spare = 1\n  return 2\n}\n",
            ),
        ],
    );
    let verdict = verify(&root, &[]);

    assert!(
        verdict.has("relocated", "unused-local"),
        "the local moved file and was not reported as relocated: {:?}",
        verdict.changes
    );
    assert!(!verdict.passed, "a relocation is not a pass");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_new_finding_is_introduced_and_fails_the_run() {
    let root = repository(
        "introduced",
        &[("a.ts", "export function keep(): number {\n  return 1\n}\n")],
        &[(
            "a.ts",
            "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n",
        )],
    );
    let verdict = verify(&root, &[]);

    assert!(
        verdict.has("introduced", "unreachable-code"),
        "{:?}",
        verdict.changes
    );
    assert!(!verdict.passed);
    let _ = std::fs::remove_dir_all(&root);
}

/// A metric that shrank without reaching the threshold is neither done nor untouched,
/// and saying so is the difference between a grader and a pass/fail stamp.
#[test]
fn a_metric_that_only_shrank_is_improved() {
    let long = |count: usize| {
        let body = (0..count)
            .map(|index| format!("  const value{index} = {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("export function big(): number {{\n{body}\n  return 1\n}}\n")
    };
    let root = repository("improved", &[("a.ts", &long(60))], &[("a.ts", &long(48))]);
    let verdict = verify(&root, &[]);

    assert!(
        verdict.has("improved", "long-function"),
        "{:?}",
        verdict.changes
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_expectation_that_was_not_met_fails_the_run() {
    let root = repository(
        "expect",
        &[(
            "a.ts",
            "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n",
        )],
        &[(
            "a.ts",
            "export function keep(): number {\n  return 1\n  console.log('still dead')\n}\n",
        )],
    );
    let verdict = verify(&root, &["--expect", "unreachable-code"]);
    assert!(!verdict.passed, "{:?}", verdict.changes);

    let met = verify(
        &repository(
            "expect-met",
            &[(
                "a.ts",
                "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n",
            )],
            &[("a.ts", "export function keep(): number {\n  return 1\n}\n")],
        ),
        &["--expect", "unreachable-code"],
    );
    assert!(met.passed);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_verify_summary_carries_the_schema_version() {
    let root = repository(
        "schema-version",
        &[("a.ts", "export const one = 1\n")],
        &[("a.ts", "export const one = 2\n")],
    );
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["verify"])
        .arg(&root)
        .args(["--base", "HEAD~1", "--json"])
        .output()
        .expect("failed to run rqx");
    let body = String::from_utf8(output.stdout).expect("utf-8 output");
    let summary: serde_json::Value =
        serde_json::from_str(body.lines().last().expect("a summary")).expect("a json object");
    assert_eq!(summary["kind"], "summary");
    assert_eq!(summary["schema_version"], 1);
    let _ = std::fs::remove_dir_all(&root);
}

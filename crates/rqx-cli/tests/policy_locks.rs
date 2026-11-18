//! A key the policy locks cannot be moved by a flag or an environment variable either:
//! a run that asks for a different value exits 2 and names the key, the policy and
//! what asked. Asking for the locked value is allowed.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const POLICY: &str = "\
gate:
  fail_on: warning
  max_findings: 3
  min_health: 95
  min_grade: fair
  fail_on_incomplete: false
scan:
  threads: 2
  max_findings: 50
  churn_days: 90
  coverage: [locked.lcov]
  profile: [locked.cpuprofile]
  sarif_in: [locked.sarif]
locked:
  keys:
    - gate.fail_on
    - gate.max_findings
    - gate.min_health
    - gate.min_grade
    - gate.fail_on_incomplete
    - scan.threads
    - scan.max_findings
    - scan.churn_days
    - scan.coverage
    - scan.profile
    - scan.sarif_in
";

/// Every variable a flag below reads, so the developer's own environment cannot
/// decide a test.
const VARIABLES: [&str; 8] = [
    "RQX_FAIL_ON",
    "RQX_ALLOW_FINDINGS",
    "RQX_MIN_HEALTH",
    "RQX_MIN_GRADE",
    "RQX_FAIL_ON_INCOMPLETE",
    "RQX_THREADS",
    "RQX_LEVEL",
    "RQX_POLICY",
];

fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-locks-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    std::fs::write(root.join("rqx.yaml"), "root: true\npolicy: policy.yaml\n").expect("config");
    std::fs::write(root.join("policy.yaml"), POLICY).expect("policy");
    std::fs::write(
        root.join("a.ts"),
        "export function one(): number {\n  return 1\n}\n",
    )
    .expect("a file");
    root
}

fn rqx(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rqx"));
    command.current_dir(root).env("RQX_NO_CACHE", "1");
    for variable in VARIABLES {
        command.env_remove(variable);
    }
    command
        .args(args)
        .envs(env.iter().copied())
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

fn assert_refused(output: &Output, key: &str, asked_by: &str) {
    let message = said(output);
    assert_eq!(output.status.code(), Some(2), "{message}");
    assert!(
        message.contains(&format!("`{key}` is locked to")),
        "{message}"
    );
    assert!(message.contains("policy.yaml"), "{message}");
    assert!(
        message.contains(&format!("; {asked_by} asks for")),
        "{message}"
    );
}

#[test]
fn a_flag_that_moves_a_locked_key_is_refused() {
    let root = project("flags");
    for (key, args) in [
        ("gate.fail_on", &["scan", ".", "--fail-on", "never"][..]),
        (
            "gate.max_findings",
            &["scan", ".", "--allow-findings", "99"],
        ),
        ("gate.min_health", &["scan", ".", "--min-health", "0"]),
        ("gate.min_grade", &["scan", ".", "--min-grade", "never"]),
        (
            "gate.fail_on_incomplete",
            &["scan", ".", "--fail-on-incomplete"],
        ),
        ("scan.threads", &["scan", ".", "--threads", "7"]),
        ("scan.max_findings", &["scan", ".", "--max-findings", "0"]),
        ("scan.coverage", &["scan", ".", "--coverage", "other.lcov"]),
        (
            "scan.profile",
            &["scan", ".", "--profile", "other.cpuprofile"],
        ),
        ("scan.sarif_in", &["scan", ".", "--sarif-in", "other.sarif"]),
        (
            "scan.profile",
            &["perf", ".", "--profile", "other.cpuprofile"],
        ),
        ("scan.churn_days", &["hotspots", ".", "--since", "10"]),
        ("gate.fail_on", &["check", ".", "--fail-on=never"]),
    ] {
        let flag = args
            .iter()
            .find(|arg| arg.starts_with("--"))
            .and_then(|arg| arg.split('=').next())
            .expect("a flag");
        assert_refused(&rqx(&root, args, &[]), key, flag);
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_variable_that_moves_a_locked_key_is_refused_and_named() {
    let root = project("variables");
    for (key, variable, value) in [
        ("gate.fail_on", "RQX_FAIL_ON", "never"),
        ("gate.max_findings", "RQX_ALLOW_FINDINGS", "99"),
        ("gate.min_health", "RQX_MIN_HEALTH", "0"),
        ("gate.min_grade", "RQX_MIN_GRADE", "never"),
        ("gate.fail_on_incomplete", "RQX_FAIL_ON_INCOMPLETE", "true"),
        ("scan.threads", "RQX_THREADS", "7"),
    ] {
        let output = rqx(&root, &["scan", "."], &[(variable, value)]);
        assert_refused(&output, key, variable);
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_flag_is_named_when_both_the_flag_and_its_variable_are_given() {
    let root = project("both");
    let output = rqx(
        &root,
        &["scan", ".", "--min-health", "0"],
        &[("RQX_MIN_HEALTH", "10")],
    );
    assert_refused(&output, "gate.min_health", "--min-health");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn asking_for_the_locked_value_is_allowed() {
    let root = project("same");
    let output = rqx(
        &root,
        &[
            "scan",
            ".",
            "--fail-on",
            "warning",
            "--allow-findings",
            "3",
            "--min-health",
            "95",
            "--min-grade",
            "fair",
            "--threads",
            "2",
            "--max-findings",
            "50",
        ],
        &[("RQX_FAIL_ON", "warning")],
    );
    let message = said(&output);
    assert!(!message.contains("is locked to"), "{message}");
    assert_ne!(output.status.code(), Some(2), "{message}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn without_a_lock_a_flag_still_moves_the_setting() {
    let root = project("unlocked");
    std::fs::write(root.join("policy.yaml"), "gate:\n  fail_on: warning\n").expect("policy");
    let output = rqx(&root, &["scan", ".", "--fail-on", "never"], &[]);
    let message = said(&output);
    assert_eq!(output.status.code(), Some(0), "{message}");
    let _ = std::fs::remove_dir_all(root);
}

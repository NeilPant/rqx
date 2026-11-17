//! `--sarif-in`: another tool's SARIF results, read as findings in the `external` family.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CLEAN: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
const ESLINT: &str = include_str!("sarif/eslint.sarif");

fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-sarif-in-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("a temp project");
    std::fs::write(root.join("src/a.js"), "var unused = 1;\nconsole.log(2)\n").expect("a file");
    std::fs::write(root.join("src/clean.ts"), CLEAN).expect("a file");
    std::fs::write(
        root.join("eslint.sarif"),
        ESLINT.replace("{root}", &root.to_string_lossy()),
    )
    .expect("a SARIF file");
    root
}

fn rqx(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .current_dir(root)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .env_remove("RQX_CONFIG")
        .env_remove("RQX_POLICY")
        .env_remove("RQX_LEVEL")
        .output()
        .expect("failed to run rqx")
}

fn lines_of(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn external_rules(lines: &[serde_json::Value]) -> Vec<&str> {
    lines
        .iter()
        .filter(|line| line["kind"] == "finding" && line["category"] == "external")
        .filter_map(|line| line["rule"].as_str())
        .collect()
}

fn summary(lines: &[serde_json::Value]) -> &serde_json::Value {
    lines
        .iter()
        .find(|line| line["kind"] == "summary")
        .expect("a summary line")
}

#[test]
fn nothing_external_appears_without_the_flag() {
    let root = project("absent");
    let lines = lines_of(&rqx(&root, &["scan", ".", "--reporter", "json"]));
    assert!(external_rules(&lines).is_empty());
}

#[test]
fn results_become_findings_and_leave_the_health_score_alone() {
    let root = project("score");
    let without = lines_of(&rqx(&root, &["scan", ".", "--reporter", "json"]));
    let with = lines_of(&rqx(
        &root,
        &[
            "scan",
            ".",
            "--sarif-in",
            "eslint.sarif",
            "--reporter",
            "json",
        ],
    ));
    assert_eq!(
        external_rules(&with),
        ["eslint/no-unused-vars", "eslint/no-console"]
    );
    assert_eq!(
        summary(&with)["health_score"],
        summary(&without)["health_score"]
    );
    assert_eq!(
        summary(&with)["counts"]["total"].as_u64().unwrap(),
        summary(&without)["counts"]["total"].as_u64().unwrap() + 2
    );
}

#[test]
fn an_external_error_fails_the_gate() {
    let root = project("gate");
    let output = rqx(
        &root,
        &[
            "scan",
            ".",
            "--sarif-in",
            "eslint.sarif",
            "--fail-on",
            "error",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn an_unreadable_file_is_skipped_with_a_notice() {
    let root = project("missing");
    let output = rqx(
        &root,
        &[
            "scan",
            ".",
            "--sarif-in",
            "nowhere.sarif",
            "--reporter",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("skipping SARIF input"));
}

#[test]
fn a_baseline_taken_with_the_files_configured_absorbs_their_findings() {
    let root = project("baseline");
    let configure = |text: &str| std::fs::write(root.join("rqx.yaml"), text).expect("a file");
    configure("scan:\n  sarif_in: [eslint.sarif]\n");
    assert_eq!(rqx(&root, &["baseline"]).status.code(), Some(0));
    configure("scan:\n  sarif_in: [eslint.sarif]\ngate:\n  baseline: .rqx/baseline.json\n");

    let output = rqx(&root, &["scan", ".", "--fail-on", "error"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

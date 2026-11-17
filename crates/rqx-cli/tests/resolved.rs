//! The credit a change earns: findings present at the base that it removed.
//!
//! A change that adds a function has its base scanned whole, so its credit covers the
//! project and carries a health delta. Any other change is credited from the files it
//! touched, analysed one at a time, with no delta.

mod common;
use std::process::Command;

use common::Repository;

const DEAD: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";
const FIXED: &str = "export function keep(): number {\n  return 1\n}\n";
const OTHER: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
/// `OTHER` with a second function, which is what makes a change add one.
const OTHER_AND_MORE: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\nexport function sub(a: number, b: number): number {\n  return a - b\n}\n";

fn base() -> Repository {
    let repository = Repository::new("resolved");
    repository.commit(
        "base",
        &[("src/keep.ts", Some(DEAD)), ("src/add.ts", Some(OTHER))],
    );
    repository
}

fn summary(lines: &[serde_json::Value]) -> &serde_json::Value {
    lines
        .iter()
        .find(|line| line["kind"] == "summary")
        .expect("a summary line")
}

fn review(repository: &Repository) -> Vec<serde_json::Value> {
    repository.rqx_lines(&["check", ".", "--base", "HEAD~1", "--reporter", "json"])
}

fn rules_resolved(summary: &serde_json::Value) -> Vec<(String, String)> {
    summary["resolved"]
        .as_array()
        .expect("a resolved array")
        .iter()
        .map(|entry| {
            (
                entry["rule"].as_str().unwrap().to_owned(),
                entry["file"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_cleanup_that_adds_no_function_is_credited_from_the_files_it_touched() {
    let repository = base();
    repository.commit("change", &[("src/keep.ts", Some(FIXED))]);
    let lines = review(&repository);
    let summary = summary(&lines);

    assert!(rules_resolved(summary).contains(&("unreachable-code".into(), "src/keep.ts".into())));
    assert_eq!(summary["resolved_scope"], "changed-files");
    assert!(
        summary.get("health_delta").is_none(),
        "a score needs the whole base"
    );
}

#[test]
fn a_change_that_adds_a_function_is_credited_over_the_project_with_its_health_delta() {
    let repository = base();
    repository.commit(
        "change",
        &[
            ("src/keep.ts", Some(FIXED)),
            ("src/add.ts", Some(OTHER_AND_MORE)),
        ],
    );
    let lines = review(&repository);
    let summary = summary(&lines);

    assert!(rules_resolved(summary).contains(&("unreachable-code".into(), "src/keep.ts".into())));
    assert_eq!(summary["resolved_scope"], "project");
    assert!(summary["health_delta"].as_i64().expect("a delta") > 0);
}

#[test]
fn a_deleted_file_is_credited_with_the_findings_it_held() {
    let repository = base();
    repository.commit("change", &[("src/keep.ts", None)]);
    let lines = review(&repository);

    assert!(
        rules_resolved(summary(&lines))
            .contains(&("unreachable-code".into(), "src/keep.ts".into()))
    );
}

#[test]
fn a_finding_that_needs_the_whole_project_is_not_credited_from_one_file() {
    // `keep.ts` is unused at the base and imported at the head, which resolves
    // `unused-file`, but one file's analysis cannot tell that, so it is not claimed.
    let repository = base();
    repository.commit(
        "change",
        &[(
            "src/add.ts",
            Some(&format!(
                "import {{ keep }} from './keep'\n{OTHER}export const used = keep\n"
            )),
        )],
    );
    let lines = review(&repository);
    let rules: Vec<String> = rules_resolved(summary(&lines))
        .into_iter()
        .map(|(rule, _)| rule)
        .collect();

    assert!(!rules.contains(&"unused-file".to_owned()), "{rules:?}");
    assert!(!rules.contains(&"unused-export".to_owned()), "{rules:?}");
}

#[test]
fn a_change_that_fixes_nothing_credits_nothing() {
    let repository = base();
    repository.commit("touch", &[("src/add.ts", Some(&format!("{OTHER}\n")))]);
    let lines = review(&repository);

    assert_eq!(summary(&lines)["resolved"], serde_json::json!([]));
}

#[test]
fn a_scan_and_a_review_without_a_revision_carry_no_credit() {
    let repository = base();
    repository.commit("change", &[("src/keep.ts", Some(FIXED))]);
    let scan = repository.rqx_lines(&["scan", ".", "--reporter", "json"]);
    for key in ["resolved", "resolved_scope", "health_delta"] {
        assert!(summary(&scan).get(key).is_none(), "{key}");
    }

    let listed = repository.rqx_lines(&["check", ".", "--files-from", "-", "--reporter", "json"]);
    assert!(summary(&listed).get("resolved").is_none());
}

#[test]
fn the_comment_has_a_resolved_section() {
    let repository = base();
    repository.commit("change", &[("src/keep.ts", Some(FIXED))]);
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
    assert!(
        body.contains("Resolves 1 finding in the files it changed."),
        "{body}"
    );
    assert!(!body.contains("Health went from"), "{body}");
}

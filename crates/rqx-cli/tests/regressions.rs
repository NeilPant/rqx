//! Complexity regressions in a review: a function that costs more after the change than
//! it did at the base revision, found without scanning the base.

mod common;
use common::Repository;

const CALLEE_CONSTANT: &str = "export function lookup(items: string[], key: string): boolean {\n  return items.length > 0 && key.length > 0;\n}\n";

const CALLEE_LINEAR: &str = "export function lookup(items: string[], key: string): boolean {\n  for (const item of items) {\n    if (item === key) {\n      return true;\n    }\n  }\n  return false;\n}\n";

/// Calls `lookup` once per item, so it costs what `lookup` costs times the items.
const CALLER: &str = "import { lookup } from \"./callee\";\n\nexport function reconcileBatch(items: string[]): number {\n  let seen = 0;\n  for (const item of items) {\n    if (lookup(items, item)) {\n      seen += 1;\n    }\n  }\n  return seen;\n}\n";

const SCAN_LINEAR: &str = "export function scanAll(items: string[]): number {\n  let seen = 0;\n  for (const item of items) {\n    seen += item.length;\n  }\n  return seen;\n}\n";

const SCAN_QUADRATIC: &str = "export function scanAll(items: string[]): number {\n  let seen = 0;\n  for (const item of items) {\n    for (const other of items) {\n      seen += item.length + other.length;\n    }\n  }\n  return seen;\n}\n";

/// A repository whose last commit swaps `lookup` from constant to linear time, which
/// makes `reconcileBatch` quadratic without touching its file.
fn callee_got_slower(name: &str) -> Repository {
    let repository = Repository::new(name);
    repository.commit(
        "base",
        &[
            ("src/callee.ts", Some(CALLEE_CONSTANT)),
            ("src/caller.ts", Some(CALLER)),
        ],
    );
    repository.commit("slower", &[("src/callee.ts", Some(CALLEE_LINEAR))]);
    repository
}

fn summary_of(repository: &Repository, command: &[&str]) -> serde_json::Value {
    repository
        .rqx_lines(command)
        .into_iter()
        .find(|line| line["kind"] == "summary")
        .expect("a summary line")
}

fn summary(repository: &Repository, args: &[&str]) -> serde_json::Value {
    let mut command = vec![
        "check",
        ".",
        "--base",
        "HEAD~1",
        "--reporter",
        "json",
        "--quiet",
    ];
    command.extend_from_slice(args);
    summary_of(repository, &command)
}

fn regressions(summary: &serde_json::Value) -> Vec<&serde_json::Value> {
    summary["regressions"]
        .as_array()
        .expect("a regressions array")
        .iter()
        .collect()
}

#[test]
fn a_caller_nobody_edited_regresses_when_its_callee_gets_slower() {
    let repository = callee_got_slower("regress-caller");

    let summary = summary(&repository, &["--regressions"]);

    let regression = regressions(&summary)
        .into_iter()
        .find(|regression| regression["qual"] == "reconcileBatch")
        .expect("the caller regressed");
    assert_eq!(regression["file"], "src/caller.ts");
    assert_eq!(regression["line"], 3);
    assert_eq!(regression["before"], "O(n)");
    assert_eq!(regression["after"], "O(n²)");
    let trace = regression["trace"].as_array().unwrap();
    assert!(!trace.is_empty(), "the way to O(n²) is not shown");
    assert!(
        trace
            .iter()
            .any(|step| step["what"] == "lookup" && step["file"] == "src/callee.ts"),
        "{trace:?}"
    );
    assert!(summary.get("regressions_skipped").is_none());
}

#[test]
fn the_callee_that_went_from_constant_to_linear_is_a_regression_too() {
    let repository = callee_got_slower("regress-callee");

    let summary = summary(&repository, &["--regressions"]);

    let found = regressions(&summary);
    let callee = found
        .iter()
        .find(|regression| regression["qual"] == "lookup")
        .expect("the callee regressed");
    assert_eq!(callee["before"], "O(1)");
    assert_eq!(callee["after"], "O(n)");
}

#[test]
fn a_review_that_was_not_asked_to_look_says_nothing_about_regressions() {
    let repository = callee_got_slower("regress-silent");

    let summary = summary(&repository, &[]);

    assert!(summary.get("regressions").is_none(), "{summary}");
    assert!(summary.get("regressions_skipped").is_none());
}

#[test]
fn the_gate_setting_asks_for_them_without_the_flag() {
    let repository = Repository::new("regress-configured");
    repository.commit(
        "base",
        &[
            (
                "rqx.yaml",
                Some("gate:\n  complexity_regression: warning\n"),
            ),
            ("src/callee.ts", Some(CALLEE_CONSTANT)),
            ("src/caller.ts", Some(CALLER)),
        ],
    );
    repository.commit("slower", &[("src/callee.ts", Some(CALLEE_LINEAR))]);

    let summary = summary(&repository, &[]);

    assert_eq!(regressions(&summary).len(), 2, "{summary}");
}

#[test]
fn a_renamed_function_is_new_and_not_a_regression() {
    let repository = Repository::new("regress-renamed");
    repository.commit("base", &[("src/scan.ts", Some(SCAN_LINEAR))]);
    let renamed = SCAN_QUADRATIC.replace("scanAll", "scanEverything");
    repository.commit("renamed", &[("src/scan.ts", Some(&renamed))]);

    let summary = summary(&repository, &["--regressions"]);

    assert!(regressions(&summary).is_empty(), "{summary}");
}

#[test]
fn a_function_that_keeps_its_name_and_grows_is_a_regression() {
    let repository = Repository::new("regress-grown");
    repository.commit("base", &[("src/scan.ts", Some(SCAN_LINEAR))]);
    repository.commit("grown", &[("src/scan.ts", Some(SCAN_QUADRATIC))]);

    let summary = summary(&repository, &["--regressions"]);

    let found = regressions(&summary);
    assert_eq!(found.len(), 1, "{summary}");
    assert_eq!(found[0]["before"], "O(n)");
    assert_eq!(found[0]["after"], "O(n²)");
}

#[test]
fn a_function_that_was_constant_time_and_not_recorded_is_compared_as_constant() {
    let repository = Repository::new("regress-constant");
    let constant = "export function total(items: string[]): number {\n  return items.length;\n}\n";
    let linear = "export function total(items: string[]): number {\n  let sum = 0;\n  for (const item of items) {\n    sum += item.length;\n  }\n  return sum;\n}\n";
    repository.commit("base", &[("src/total.ts", Some(constant))]);
    repository.commit("grown", &[("src/total.ts", Some(linear))]);

    let summary = summary(&repository, &["--regressions"]);

    let found = regressions(&summary);
    assert_eq!(found.len(), 1, "{summary}");
    assert_eq!(found[0]["qual"], "total");
    assert_eq!(found[0]["before"], "O(1)");
    assert_eq!(found[0]["after"], "O(n)");
}

#[test]
fn a_function_that_got_faster_is_not_reported() {
    let repository = Repository::new("regress-faster");
    repository.commit("base", &[("src/scan.ts", Some(SCAN_QUADRATIC))]);
    repository.commit("faster", &[("src/scan.ts", Some(SCAN_LINEAR))]);

    let summary = summary(&repository, &["--regressions"]);

    assert!(regressions(&summary).is_empty(), "{summary}");
}

#[test]
fn a_review_of_listed_files_has_no_base_and_says_so() {
    let repository = callee_got_slower("regress-listed");
    let listed = repository.root.join("listed.txt");
    std::fs::write(&listed, "src/callee.ts\0").expect("a list");

    let summary = summary_of(
        &repository,
        &[
            "check",
            ".",
            "--files-from",
            listed.to_str().unwrap(),
            "--regressions",
            "--reporter",
            "json",
            "--quiet",
        ],
    );

    assert!(regressions(&summary).is_empty());
    assert_eq!(
        summary["regressions_skipped"],
        "there is no base revision to compare with"
    );
}

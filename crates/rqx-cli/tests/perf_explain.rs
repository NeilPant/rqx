//! `rqx perf --explain`: the derivation of one function's cost, from its loop through
//! the function it calls to the library entry that priced the call. The derivation is
//! text a reader follows, so it is held as a golden file.
//!
//! `RQX_BLESS=1 cargo test -p rqx --test perf_explain` rewrites the golden file.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PRICES: &str = "\
export function hasPrice(prices: string[], wanted: string): boolean {
  return prices.includes(wanted);
}
";

const RECONCILE: &str = "\
import { hasPrice } from \"./prices\";

export function reconcile(orders: string[], prices: string[]) {
  const missing: string[] = [];
  for (const order of orders) {
    if (!hasPrice(prices, order)) {
      missing.push(order);
    }
  }
  return missing;
}

export function fibonacci(n: number): number {
  if (n < 2) return n;
  return fibonacci(n - 1) + fibonacci(n - 2);
}
";

const FIRST_FINDER: &str = "export function find(items: string[], wanted: string) {\n  for (const item of items) {\n    if (item === wanted) return item;\n  }\n}\n";
const SECOND_FINDER: &str = "export function find(rows: number[]) {\n  return rows.length;\n}\n";

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-explain-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    for (file, text) in files {
        std::fs::write(root.join(file), text).expect("the source");
    }
    root
}

fn explain(root: &Path, target: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg("perf")
        .arg(root)
        .args(["--explain", target])
        .args(extra)
        .output()
        .expect("failed to run rqx")
}

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/perf_explain")
        .join(name)
}

fn check_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("RQX_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("the directory");
        std::fs::write(&path, actual).expect("write the golden file");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no {}; generate it with RQX_BLESS=1", path.display()));
    assert_eq!(actual, expected, "{name} differs; RQX_BLESS=1 rewrites it");
}

#[test]
fn the_derivation_runs_from_the_loop_through_the_callee_to_the_library_entry() {
    let root = project(
        "golden",
        &[("prices.ts", PRICES), ("reconcile.ts", RECONCILE)],
    );
    let output = explain(&root, "reconcile", &[]);
    let _ = std::fs::remove_dir_all(&root);
    assert!(output.status.success(), "{output:?}");
    check_golden(
        "reconcile.golden",
        &String::from_utf8(output.stdout).expect("utf-8"),
    );
}

#[test]
fn a_recursion_is_described_rather_than_listed_as_a_call_to_itself() {
    let root = project("recursion", &[("reconcile.ts", RECONCILE)]);
    let output = explain(&root, "fibonacci", &[]);
    let _ = std::fs::remove_dir_all(&root);
    check_golden(
        "fibonacci.golden",
        &String::from_utf8(output.stdout).expect("utf-8"),
    );
}

#[test]
fn a_name_that_two_functions_share_lists_them_and_fails() {
    let root = project(
        "ambiguous",
        &[("a.ts", FIRST_FINDER), ("b.ts", SECOND_FINDER)],
    );
    let output = explain(&root, "find", &[]);
    let by_place = explain(&root, "b.ts:1", &[]);
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert!(
        stderr.contains("a.ts:1") && stderr.contains("b.ts:1"),
        "{stderr}"
    );
    assert!(by_place.status.success(), "a place names one function");
    assert!(
        String::from_utf8(by_place.stdout)
            .expect("utf-8")
            .contains("b.ts:1")
    );
}

#[test]
fn a_name_that_matches_nothing_fails_and_says_how_to_name_one() {
    let root = project("missing", &[("prices.ts", PRICES)]);
    let output = explain(&root, "nowhere", &[]);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert!(stderr.contains("`file:line`"), "{stderr}");
}

const STREAM: &str = "\
export function drain(source: Source, wanted: string[]) {
  const seen: string[] = [];
  while (source.hasMore()) {
    const item = source.next();
    if (wanted.indexOf(item) >= 0) {
      seen.push(item);
    }
  }
  return seen;
}
";

#[test]
fn a_bound_it_could_not_read_lowers_the_confidence_and_names_the_loop() {
    let root = project("unread", &[("stream.ts", STREAM)]);
    let output = explain(&root, "drain", &["--json"]);
    let _ = std::fs::remove_dir_all(&root);
    let explained: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON object");

    assert_eq!(explained["confidence"], "heuristic");
    let reasons = explained["reasons"].as_array().expect("reasons");
    assert!(
        reasons.iter().any(|reason| reason
            .as_str()
            .is_some_and(|text| text.contains("stream.ts:3"))),
        "{reasons:?}"
    );
}

#[test]
fn json_carries_the_same_derivation() {
    let root = project(
        "json",
        &[("prices.ts", PRICES), ("reconcile.ts", RECONCILE)],
    );
    let output = explain(&root, "reconcile", &["--json"]);
    let _ = std::fs::remove_dir_all(&root);
    let explained: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON object");

    assert_eq!(explained["name"], "reconcile");
    assert_eq!(explained["time"], "O(orders\u{b7}prices)");
    let time = &explained["derivation"][0];
    assert_eq!(time["dimension"], "time");
    let call = &time["steps"][0]["children"][0];
    assert_eq!(call["kind"], "call");
    assert_eq!(call["children"][0]["kind"], "library");
    assert!(
        call["children"][0]["what"]
            .as_str()
            .is_some_and(|what| what.contains("js.array.includes")),
        "{call}"
    );
}

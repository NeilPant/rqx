//! The base revision is scanned only when the change adds a function. A review that
//! carries `resolved` on its summary line scanned the base, and one that does not did
//! not, so the key is what shows which happened.

mod common;
use common::Repository;

const ADD: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
const ADD_AND_SUB: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\nexport function sub(a: number, b: number): number {\n  return a - b\n}\n";
const DEAD: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";
const FIXED: &str = "export function keep(): number {\n  return 1\n}\n";

fn scanned_base(name: &str, after: &[(&str, Option<&str>)]) -> bool {
    let repository = Repository::new(name);
    repository.commit(
        "base",
        &[
            ("src/add.ts", Some(ADD)),
            ("src/keep.ts", Some(DEAD)),
            ("README.md", Some("# project\n")),
        ],
    );
    repository.commit("change", after);
    let lines = repository.rqx_lines(&["check", ".", "--base", "HEAD~1", "--reporter", "json"]);
    lines
        .iter()
        .find(|line| line["kind"] == "summary")
        .expect("a summary line")
        .get("resolved")
        .is_some()
}

#[test]
fn a_change_that_removes_code_and_adds_no_function_does_not_scan_the_base() {
    assert!(!scanned_base("base-fix", &[("src/keep.ts", Some(FIXED))]));
}

#[test]
fn an_edit_to_a_function_that_keeps_its_name_does_not_scan_the_base() {
    let edited = ADD.replace("a + b", "b + a");
    assert!(!scanned_base("base-edit", &[("src/add.ts", Some(&edited))]));
}

#[test]
fn a_change_to_a_file_that_holds_no_code_does_not_scan_the_base() {
    assert!(!scanned_base(
        "base-docs",
        &[("README.md", Some("# project\n\nmore\n"))]
    ));
}

#[test]
fn a_new_function_in_an_existing_file_scans_the_base() {
    assert!(scanned_base(
        "base-added",
        &[("src/add.ts", Some(ADD_AND_SUB))]
    ));
}

#[test]
fn a_new_file_with_a_function_scans_the_base() {
    assert!(scanned_base(
        "base-new-file",
        &[("src/sub.ts", Some(ADD_AND_SUB))]
    ));
}

#[test]
fn a_renamed_function_counts_as_added() {
    let renamed = ADD.replace("add", "plus");
    assert!(scanned_base(
        "base-renamed",
        &[("src/add.ts", Some(&renamed))]
    ));
}

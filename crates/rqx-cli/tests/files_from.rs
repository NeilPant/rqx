//! `--files-from`: the files a pipeline names, in place of the ones git says changed.
//! The projects here are not repositories, which is the point of the flag.

mod common;
use common::Scratch;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const CLEAN: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
const DIRTY: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";

fn project(name: &str, files: &[(&str, &str)]) -> Scratch {
    let root = Scratch::new(&format!("files-from-{name}"));
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        std::fs::write(file, text).expect("a file");
    }
    root
}

/// Runs rqx in `root` with `input` on standard input.
fn rqx_with(root: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .current_dir(root)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .env_remove("RQX_CONFIG")
        .env_remove("RQX_POLICY")
        .env_remove("RQX_LEVEL")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run rqx");
    child
        .stdin
        .take()
        .expect("a stdin pipe")
        .write_all(input)
        .expect("the list");
    child.wait_with_output().expect("rqx to finish")
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn files_with_findings(output: &Output) -> Vec<String> {
    let mut files: Vec<String> = stdout(output)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|line| line["kind"] == "finding")
        .filter_map(|line| line["location"]["file"].as_str().map(str::to_owned))
        .collect();
    files.sort();
    files.dedup();
    files
}

#[test]
fn scan_reports_only_the_listed_files() {
    let root = project("scan", &[("a.ts", DIRTY), ("b.ts", DIRTY)]);
    let everything = rqx_with(&root, &["scan", ".", "--reporter", "json"], b"");
    assert_eq!(files_with_findings(&everything), ["a.ts", "b.ts"]);

    let listed = rqx_with(
        &root,
        &["scan", ".", "--files-from", "-", "--reporter", "json"],
        b"a.ts\n",
    );
    assert_eq!(code(&listed), 0, "{listed:?}");
    assert_eq!(files_with_findings(&listed), ["a.ts"]);
}

#[test]
fn scan_reads_the_list_from_a_file_too() {
    let root = project("scan-file", &[("a.ts", DIRTY), ("b.ts", DIRTY)]);
    std::fs::write(root.join("list.txt"), "b.ts\n").expect("a list");
    let listed = rqx_with(
        &root,
        &[
            "scan",
            ".",
            "--files-from",
            "list.txt",
            "--reporter",
            "json",
        ],
        b"",
    );
    assert_eq!(files_with_findings(&listed), ["b.ts"]);
}

/// The scan still reads the whole project, so a file another one imports is not
/// reported as unused just because only it was listed.
#[test]
fn scan_still_analyses_the_whole_project() {
    let root = project(
        "scan-whole",
        &[
            ("a.ts", CLEAN),
            (
                "index.ts",
                "import { add } from './a'\nexport const three = add(1, 2)\n",
            ),
        ],
    );
    let listed = rqx_with(
        &root,
        &["scan", ".", "--files-from", "-", "--reporter", "json"],
        b"a.ts\n",
    );
    assert!(
        files_with_findings(&listed).is_empty(),
        "a.ts is imported by index.ts, which only the whole project knows: {}",
        stdout(&listed)
    );
    let everything = rqx_with(&root, &["scan", ".", "--reporter", "json"], b"");
    assert_eq!(files_with_findings(&everything), ["index.ts"]);
}

#[test]
fn paths_that_do_not_exist_are_said_once_and_ignored() {
    let root = project("missing", &[("a.ts", DIRTY)]);
    let listed = rqx_with(
        &root,
        &["scan", ".", "--files-from", "-", "--reporter", "json"],
        b"a.ts\ngone.ts\nalso-gone.ts\n",
    );
    assert_eq!(code(&listed), 0);
    assert_eq!(files_with_findings(&listed), ["a.ts"]);
    let said = stderr(&listed);
    assert_eq!(said.matches("--files-from: ignored 2").count(), 1, "{said}");
    assert!(said.contains("gone.ts, also-gone.ts"), "{said}");
}

#[test]
fn absolute_paths_are_read_against_the_scan_root() {
    let root = project("absolute", &[("a.ts", DIRTY), ("b.ts", DIRTY)]);
    let absolute = root.join("b.ts");
    let listed = rqx_with(
        &root,
        &["scan", ".", "--files-from", "-", "--reporter", "json"],
        absolute.display().to_string().as_bytes(),
    );
    assert_eq!(files_with_findings(&listed), ["b.ts"]);
}

#[test]
fn check_without_git_history_reviews_the_listed_files_and_gates_on_them() {
    let root = project("check", &[("clean.ts", CLEAN), ("dirty.ts", DIRTY)]);
    let without = rqx_with(&root, &["check", ".", "--fail-on", "warning"], b"");
    assert_eq!(
        code(&without),
        2,
        "with no list and no repository there is nothing to review"
    );

    let gated = rqx_with(
        &root,
        &["check", ".", "--files-from", "-", "--fail-on", "error"],
        b"dirty.ts\n",
    );
    assert_eq!(code(&gated), 1, "{}", stdout(&gated));
    assert!(stdout(&gated).contains("dirty.ts"));

    let passing = rqx_with(
        &root,
        &["check", ".", "--files-from", "-", "--fail-on", "error"],
        b"clean.ts\n",
    );
    assert_eq!(
        code(&passing),
        0,
        "dirty.ts was not listed, so it is not under review: {}",
        stdout(&passing)
    );
}

#[test]
fn a_display_cap_on_check_hides_findings_without_loosening_the_gate() {
    let root = project("check-cap", &[("a.ts", DIRTY), ("b.ts", DIRTY)]);
    let arguments = |cap: &'static str| {
        vec![
            "check",
            ".",
            "--files-from",
            "-",
            "--fail-on",
            "info",
            "--allow-findings",
            "1",
            "--reporter",
            "json",
            "--threads",
            "1",
            "--max-findings",
            cap,
        ]
    };
    let listed = b"a.ts\nb.ts\n";

    let capped = rqx_with(&root, &arguments("1"), listed);
    let uncapped = rqx_with(&root, &arguments("0"), listed);

    assert_eq!(code(&capped), 1, "{}", stdout(&capped));
    assert_eq!(code(&uncapped), 1, "{}", stdout(&uncapped));
    assert!(files_with_findings(&capped).len() < files_with_findings(&uncapped).len());
}

#[test]
fn check_reads_a_nul_separated_list() {
    let root = project("check-nul", &[("clean.ts", CLEAN), ("dirty.ts", DIRTY)]);
    let gated = rqx_with(
        &root,
        &["check", ".", "--files-from", "-", "--fail-on", "error"],
        b"clean.ts\0dirty.ts\0",
    );
    assert_eq!(code(&gated), 1, "{}", stdout(&gated));
}

/// What imports a listed file is reviewed as well, as it is for a diff: deleting an
/// export is only a problem in the file that was importing it.
#[test]
fn check_still_reviews_the_files_that_import_the_listed_ones() {
    let root = project(
        "check-importer",
        &[
            ("lib.ts", CLEAN),
            (
                "dirty.ts",
                "import { add } from './lib'\nexport function keep(): number {\n  return add(1, 2)\n  console.log('dead')\n}\n",
            ),
        ],
    );
    let gated = rqx_with(
        &root,
        &["check", ".", "--files-from", "-", "--fail-on", "error"],
        b"lib.ts\n",
    );
    assert_eq!(code(&gated), 1, "{}", stdout(&gated));
    assert!(stdout(&gated).contains("dirty.ts"));
}

#[test]
fn a_base_revision_and_a_list_are_two_answers_to_one_question() {
    let root = project("conflict", &[("a.ts", CLEAN)]);
    let both = rqx_with(
        &root,
        &["check", ".", "--base", "main", "--files-from", "-"],
        b"a.ts\n",
    );
    assert_eq!(code(&both), 2);
    assert!(
        stderr(&both).contains("cannot be used with"),
        "{}",
        stderr(&both)
    );
}

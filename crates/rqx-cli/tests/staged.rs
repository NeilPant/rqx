//! `check --staged`: the verdict is about the index, whatever the working tree holds.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CLEAN: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
const DIRTY: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";

fn repository(name: &str, committed: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-staged-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp repository");
    git(&root, &["init", "-q"]);
    git(&root, &["config", "core.autocrlf", "false"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "user.name", "test"]);
    write(&root, committed);
    git(&root, &["add", "--", "."]);
    git(&root, &["commit", "-qm", "start", "--allow-empty"]);
    root
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        std::fs::write(file, text).expect("a file");
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("failed to run rqx")
}

fn check_staged(root: &Path) -> Output {
    rqx(root, &["check", ".", "--staged", "--fail-on", "error"])
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn report(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn staged_bad_code_fails_the_check() {
    let root = repository("fails", &[("clean.ts", CLEAN)]);
    write(&root, &[("dirty.ts", DIRTY)]);
    git(&root, &["add", "--", "dirty.ts"]);

    let output = check_staged(&root);
    assert_eq!(code(&output), 1, "{}", report(&output));
    assert!(report(&output).contains("dirty.ts"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_same_bad_code_left_unstaged_passes() {
    let root = repository("unstaged", &[("clean.ts", CLEAN), ("kept.ts", CLEAN)]);
    write(&root, &[("dirty.ts", DIRTY), ("kept.ts", DIRTY)]);

    let output = check_staged(&root);
    assert_eq!(code(&output), 0, "{}", report(&output));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_staged_fix_passes_though_the_working_tree_file_is_dirty_again() {
    let root = repository("fixed", &[("a.ts", DIRTY)]);
    write(&root, &[("a.ts", CLEAN)]);
    git(&root, &["add", "--", "a.ts"]);
    write(&root, &[("a.ts", DIRTY)]);

    let output = check_staged(&root);
    assert_eq!(code(&output), 0, "{}", report(&output));
    assert_eq!(
        std::fs::read_to_string(root.join("a.ts")).expect("a file"),
        DIRTY,
        "the working tree is left as it was"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_unstaged_fix_does_not_rescue_staged_bad_code() {
    let root = repository("not-rescued", &[("a.ts", CLEAN)]);
    write(&root, &[("a.ts", DIRTY)]);
    git(&root, &["add", "--", "a.ts"]);
    write(&root, &[("a.ts", CLEAN)]);

    let output = check_staged(&root);
    assert_eq!(code(&output), 1, "{}", report(&output));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn nothing_staged_passes() {
    let root = repository("empty", &[("a.ts", CLEAN)]);
    write(&root, &[("a.ts", DIRTY)]);

    let output = check_staged(&root);
    assert_eq!(code(&output), 0, "{}", report(&output));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_subdirectory_is_reviewed_with_the_paths_it_names() {
    let root = repository("subdirectory", &[("pkg/clean.ts", CLEAN)]);
    write(&root, &[("pkg/dirty.ts", DIRTY)]);
    git(&root, &["add", "--", "pkg/dirty.ts"]);

    let output = rqx(&root, &["check", "pkg", "--staged", "--fail-on", "error"]);
    assert_eq!(code(&output), 1, "{}", report(&output));
    assert!(report(&output).contains("dirty.ts"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn staging_conflicts_with_the_other_ways_to_name_the_files() {
    let root = repository("conflicts", &[("a.ts", CLEAN)]);
    for other in [
        ["--base", "HEAD"].as_slice(),
        ["--files-from", "-"].as_slice(),
        ["--fix"].as_slice(),
    ] {
        let mut arguments = vec!["check", ".", "--staged"];
        arguments.extend_from_slice(other);
        let output = rqx(&root, &arguments);
        assert_eq!(code(&output), 2, "{other:?}: {}", report(&output));
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn outside_a_repository_it_says_so() {
    let root = std::env::temp_dir().join(format!("rqx-staged-bare-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a directory");
    write(&root, &[("a.ts", CLEAN)]);

    let output = check_staged(&root);
    assert_eq!(code(&output), 2, "{}", report(&output));
    assert!(report(&output).contains("git repository"));
    let _ = std::fs::remove_dir_all(root);
}

/// The hook definition names a command line, and a flag renamed here would leave every
/// repository that uses the hook failing its commits.
#[test]
fn the_pre_commit_hook_runs_a_command_that_works() {
    let definition = include_str!("../../../.pre-commit-hooks.yaml");
    for setting in [
        "language: system",
        "pass_filenames: false",
        "always_run: true",
    ] {
        assert!(definition.contains(setting), "{setting}");
    }
    let entry = definition
        .lines()
        .find_map(|line| line.trim().strip_prefix("entry: rqx "))
        .expect("an entry that runs rqx");

    let root = repository("hook", &[("a.ts", CLEAN)]);
    let arguments: Vec<&str> = entry.split_whitespace().collect();
    let output = rqx(&root, &arguments);
    assert_eq!(code(&output), 0, "{}", report(&output));
    let _ = std::fs::remove_dir_all(root);
}

//! `rqx setup agent`: nothing is written without `--write`, and writing never replaces
//! what the project already holds.

mod common;
use common::Scratch;

use std::path::Path;
use std::process::{Command, Output};

fn directory(name: &str) -> Scratch {
    Scratch::new(&format!("agent-{name}"))
}

fn setup(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["setup", "agent"])
        .args(arguments)
        .current_dir(root)
        .env("HOME", root)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run rqx")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn without_write_it_prints_and_changes_nothing() {
    let root = directory("print");

    let output = setup(&root, &["--for", "claude"]);

    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("rqx check --staged"));
    assert!(text.contains(".claude/settings.json"));
    assert_eq!(std::fs::read_dir(&root).expect("a directory").count(), 0);
}

#[test]
fn write_keeps_the_existing_instructions_and_replaces_only_its_own_block() {
    let root = directory("write");
    std::fs::write(root.join("AGENTS.md"), "# Notes\n\nKeep functions short.\n").expect("a file");

    assert!(setup(&root, &["--write"]).status.success());
    let first = std::fs::read_to_string(root.join("AGENTS.md")).expect("a file");
    assert!(first.starts_with("# Notes\n\nKeep functions short.\n\n<!-- rqx:begin -->"));

    assert!(
        setup(&root, &["--write", "--for", "codex"])
            .status
            .success()
    );
    let second = std::fs::read_to_string(root.join("AGENTS.md")).expect("a file");
    assert_eq!(first, second);
}

#[test]
fn write_for_claude_merges_the_stop_hook_into_existing_settings() {
    let root = directory("claude");
    std::fs::create_dir_all(root.join(".claude")).expect("a directory");
    std::fs::write(
        root.join(".claude/settings.json"),
        r#"{ "model": "opus", "hooks": { "PreToolUse": [] } }"#,
    )
    .expect("a file");

    assert!(
        setup(&root, &["--write", "--for", "claude"])
            .status
            .success()
    );

    let settings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".claude/settings.json")).expect("a file"),
    )
    .expect("json");
    assert_eq!(settings["model"], "opus");
    assert!(settings["hooks"]["PreToolUse"].is_array());
    assert!(
        settings["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .is_some_and(|command| command.contains("rqx check --staged"))
    );
    assert!(root.join("CLAUDE.md").is_file());
}

#[test]
fn a_settings_file_that_is_not_json_is_an_error_and_is_left_alone() {
    let root = directory("broken");
    std::fs::create_dir_all(root.join(".claude")).expect("a directory");
    std::fs::write(root.join(".claude/settings.json"), "{ nope").expect("a file");

    let output = setup(&root, &["--write", "--for", "claude"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        std::fs::read_to_string(root.join(".claude/settings.json")).expect("a file"),
        "{ nope"
    );
}

#[test]
fn naming_an_editor_still_works() {
    let root = directory("editor");
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["setup", "neovim"])
        .current_dir(&root)
        .env("HOME", &root)
        .output()
        .expect("failed to run rqx");
    assert!(output.status.success());
    assert!(stdout(&output).contains("lspconfig"));
}

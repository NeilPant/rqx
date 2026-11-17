//! Health trends, end to end: recording a snapshot from a scan, and reading the
//! series back. The projects are not repositories, so the snapshot has no commit.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DIRTY: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";

fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-trend-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    std::fs::write(root.join("a.ts"), DIRTY).expect("a file");
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

fn snapshots(file: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(file)
        .expect("a trend file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect()
}

#[test]
fn record_without_a_path_appends_to_the_default_file() {
    let root = project("default");
    for _ in 0..2 {
        let output = rqx(&root, &["scan", ".", "--record", "--quiet"]);
        assert!(output.status.code().is_some_and(|code| code < 2));
    }

    let lines = snapshots(&root.join(".rqx/trend.ndjson"));
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["kind"], "snapshot");
    assert_eq!(lines[0]["schema_version"], 1);
    assert_eq!(lines[0]["files"], 1);
    assert!(lines[0]["counts"]["total"].as_u64() > Some(0));
    assert!(lines[0]["commit"].is_null());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn record_takes_a_path_and_does_not_change_the_report() {
    let root = project("path");
    let plain = rqx(&root, &["scan", ".", "--reporter", "json"]);
    let recorded = rqx(
        &root,
        &[
            "scan",
            ".",
            "--reporter",
            "json",
            "--record",
            "history.ndjson",
        ],
    );

    assert_eq!(snapshots(&root.join("history.ndjson")).len(), 1);
    let summary = |output: &Output| {
        let mut value: serde_json::Value = String::from_utf8_lossy(&output.stdout)
            .lines()
            .last()
            .and_then(|line| serde_json::from_str(line).ok())
            .expect("a summary");
        value["elapsed_ms"] = 0.into();
        value
    };
    assert_eq!(summary(&plain), summary(&recorded));
    let _ = std::fs::remove_dir_all(&root);
}

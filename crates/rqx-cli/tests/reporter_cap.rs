//! The display cap is for people. A report a program reads lists every finding unless
//! `--max-findings` asks otherwise, and a capped SARIF report says what it left out.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Twelve empty handlers, each one `empty-catch`, under a cap of three.
fn project() -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-reporter-cap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a project");
    let handlers: String = (0..12)
        .map(|n| format!("export function f{n}() {{ try {{ run(); }} catch (e) {{}} }}\n"))
        .collect();
    std::fs::write(root.join("a.ts"), handlers).expect("a file");
    std::fs::write(root.join("rqx.yaml"), "scan:\n  max_findings: 3\n").expect("a config");
    root
}

fn scan(root: &Path, extra: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", ".", "--no-tui", "--reporter", "terminal"])
        .args(extra)
        .current_dir(root)
        .env("RQX_NO_UPDATE_CHECK", "1")
        .env("HOME", root)
        .output()
        .expect("rqx runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn sarif(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("a report")).expect("json")
}

#[test]
fn machine_reports_list_every_finding_and_the_terminal_keeps_its_cap() {
    let root = project();
    let terminal = scan(
        &root,
        &[
            "--reporter",
            "sarif=all.sarif",
            "--reporter",
            "json=all.json",
        ],
    );
    let run = &sarif(&root.join("all.sarif"))["runs"][0];
    let results = run["results"].as_array().expect("results");
    assert!(results.len() >= 12, "{} results", results.len());
    assert!(run["properties"]["rqx"].get("omitted").is_none(), "{run}");

    let json = std::fs::read_to_string(root.join("all.json")).expect("json");
    let listed = json
        .lines()
        .filter(|line| line.contains("\"kind\":\"finding\""))
        .count();
    assert_eq!(listed, results.len());
    assert!(json.contains("\"omitted\":0"), "{json}");

    assert!(terminal.contains("not shown"), "{terminal}");
}

#[test]
fn an_explicit_cap_applies_to_sarif_and_says_how_many_it_left_out() {
    let root = project();
    scan(
        &root,
        &["--max-findings", "2", "--reporter", "sarif=capped.sarif"],
    );
    let run = &sarif(&root.join("capped.sarif"))["runs"][0];
    assert_eq!(run["results"].as_array().expect("results").len(), 2);
    let omitted = run["properties"]["rqx"]["omitted"]
        .as_u64()
        .expect("a count");
    assert!(omitted >= 10, "{omitted}");
    let notice = &run["invocations"][0]["toolExecutionNotifications"][0];
    assert_eq!(notice["level"], "warning");
}

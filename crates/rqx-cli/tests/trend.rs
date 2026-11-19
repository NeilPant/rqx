//! Health trends, end to end: recording a snapshot from a scan, and reading the
//! series back. The projects are not repositories, so the snapshot has no commit.

mod common;
use common::Scratch;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DIRTY: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";

fn project(name: &str) -> Scratch {
    let root = Scratch::new(&format!("trend-it-{name}"));
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
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A trend file whose health rose and whose findings fell, written by hand so the
/// series is known.
fn history(root: &Path) -> PathBuf {
    let file = root.join("trend.ndjson");
    let line = |time: &str, health: u32, findings: u32| {
        serde_json::json!({
            "kind": "snapshot", "schema_version": 1, "commit": null, "time": time,
            "rqx_version": "0.4.0", "health": health, "grade": "fair", "files": 10,
            "counts": {"error": 0, "warning": findings, "info": 0, "total": findings},
            "rules": {"long-function": findings},
            "owners": {"@web": health},
        })
        .to_string()
    };
    let body = [
        line("2026-11-01T10:00:00Z", 50, 12),
        line("2026-11-08T10:00:00Z", 62, 7),
        line("2026-11-15T10:00:00Z", 71, 4),
    ]
    .join("\n");
    std::fs::write(&file, format!("{body}\n")).expect("a trend file");
    file
}

#[test]
fn trend_prints_the_series_with_direction_and_movers() {
    let root = project("series");
    let file = history(&root);
    let text = stdout(&rqx(&root, &["trend", file.to_str().expect("utf-8")]));

    assert!(text.contains("3 snapshots"), "{text}");
    assert!(text.contains("50 \u{2192} 71"), "{text}");
    assert!(text.contains("+21 improving"), "{text}");
    assert!(text.contains("Biggest movers by rule"), "{text}");
    assert!(text.contains("long-function"), "{text}");
}

#[test]
fn trend_limits_to_the_last_snapshots_and_can_break_down_by_owner() {
    let root = project("last");
    let file = history(&root);
    let path = file.to_str().expect("utf-8");
    let text = stdout(&rqx(
        &root,
        &["trend", path, "--last", "2", "--by", "owner"],
    ));

    assert!(text.contains("2 snapshots"), "{text}");
    assert!(text.contains("62 \u{2192} 71"), "{text}");
    assert!(
        text.contains("Biggest movers by owner") && text.contains("@web"),
        "{text}"
    );
}

#[test]
fn trend_json_is_one_object_per_series() {
    let root = project("json");
    let file = history(&root);
    let output = rqx(
        &root,
        &["trend", file.to_str().expect("utf-8"), "--reporter", "json"],
    );
    let lines: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str(line).expect("json"))
        .collect();

    assert_eq!(lines[0]["name"], "health");
    assert_eq!(lines[0]["points"], serde_json::json!([50, 62, 71]));
    assert_eq!(lines.last().expect("a summary")["kind"], "summary");
}

#[test]
fn trend_without_a_file_fails_and_says_how_to_make_one() {
    let root = project("absent");
    let output = rqx(&root, &["trend"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--record"));
}

#[test]
fn recording_twice_then_reading_it_back_gives_two_snapshots() {
    let root = project("roundtrip");
    for _ in 0..2 {
        rqx(&root, &["scan", ".", "--record", "--quiet"]);
    }
    let text = stdout(&rqx(&root, &["trend"]));
    assert!(text.contains("2 snapshots"), "{text}");
}

#[test]
fn the_html_report_charts_the_trend_file_it_is_given() {
    let root = project("html");
    let file = history(&root);
    let output = rqx(
        &root,
        &[
            "scan",
            ".",
            "--reporter",
            "html",
            "--trend",
            file.to_str().expect("utf-8"),
        ],
    );
    let page = stdout(&output);
    assert!(page.contains("<section id=\"trend\""), "{page}");
    assert!(page.contains("class=\"trend\""), "{page}");

    let plain = stdout(&rqx(&root, &["scan", ".", "--reporter", "html"]));
    assert!(!plain.contains("<section id=\"trend\""));
}

#[test]
fn a_trend_file_that_does_not_exist_yet_is_not_an_error() {
    let root = project("html-first-run");
    let output = rqx(
        &root,
        &[
            "scan",
            ".",
            "--reporter",
            "html",
            "--trend",
            "absent.ndjson",
        ],
    );
    assert!(output.status.code().is_some_and(|code| code < 2));
    assert!(stdout(&output).contains("A chart needs two snapshots"));
}

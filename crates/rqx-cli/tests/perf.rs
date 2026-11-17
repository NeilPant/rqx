//! What a profile does to a static finding, end to end. A shape that costs is a
//! claim about how work grows; a run is the only thing that can say whether anything
//! reached it, and the two have to disagree visibly rather than silently.

use std::path::{Path, PathBuf};
use std::process::Command;

const SOURCE: &str = "\
export function sumCube(grid: number[][][]): number {
  let total = 0;
  for (const plane of grid) {
    for (const line of plane) {
      for (const cell of line) {
        total += cell;
      }
    }
  }
  return total;
}

export function keepWanted(rows: number[], wanted: number[]): number[] {
  const kept: number[] = [];
  for (const row of rows) {
    if (wanted.indexOf(row) >= 0) {
      kept.push(row);
    }
  }
  return kept;
}
";

/// Two functions, and a profile that only ever saw the first. `lineNumber` is
/// zero-based and names the line a function was declared on, which is how V8 writes
/// it and why the reader adds one.
const PROFILE: &str = r#"{
  "nodes": [
    {"id": 1, "callFrame": {"functionName": "(root)", "url": "", "lineNumber": -1}, "children": [2]},
    {"id": 2, "callFrame": {"functionName": "sumCube", "url": "file:///hot.ts", "lineNumber": 0}}
  ],
  "samples": [2, 2, 2, 2],
  "timeDeltas": [1000, 1000, 1000, 1000]
}"#;

fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-perf-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    std::fs::write(root.join("hot.ts"), SOURCE).expect("the source");
    std::fs::write(root.join("run.cpuprofile"), PROFILE).expect("the profile");
    root
}

fn perf(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg("perf")
        .arg(root)
        .args(args)
        .arg("--json")
        .output()
        .expect("failed to run rqx")
}

fn rows(root: &Path, args: &[&str]) -> Vec<serde_json::Value> {
    String::from_utf8(perf(root, args).stdout)
        .expect("utf-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a row"))
        .collect()
}

#[test]
fn a_profile_ranks_what_ran_above_what_did_not() {
    let root = project("ranks");
    let without = rows(&root, &[]);
    assert_eq!(without.len(), 2, "both functions carry a finding");
    assert!(without.iter().all(|row| row["sampled"].is_null()));

    let with = rows(&root, &["--profile", "run.cpuprofile"]);
    assert_eq!(with[0]["name"], "sumCube");
    assert_eq!(with[0]["sampled"]["inclusive"], 1000);
    assert_eq!(with[1]["name"], "keepWanted");
    assert_eq!(
        with[1]["sampled"],
        serde_json::Value::Null,
        "a function the run never entered has no share to report"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The same contract coverage has with dead code: the finding stays visible and
/// stops counting, rather than disappearing on the strength of one run.
#[test]
fn a_finding_the_profile_never_reached_drops_to_low() {
    let root = project("demotes");
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg("scan")
        .arg(&root)
        .args(["--profile", "run.cpuprofile", "--reporter", "json"])
        .output()
        .expect("failed to run rqx");
    let findings: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .expect("utf-8")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("a finding"))
        .filter(|finding: &serde_json::Value| finding["category"] == "performance")
        .collect();

    let scan = findings
        .iter()
        .find(|finding| finding["rule"] == "scan-in-loop")
        .expect("the scan inside a loop");
    assert_eq!(scan["confidence"], "low");
    assert_eq!(scan["evidence"]["sampled"], 0);

    let nested = findings
        .iter()
        .find(|finding| finding["rule"] == "deep-loop-nesting")
        .expect("the nested loops");
    assert_ne!(
        nested["confidence"], "low",
        "the profile put this one on the stack"
    );
    assert_eq!(nested["evidence"]["sampled"], 1000);
    let _ = std::fs::remove_dir_all(&root);
}

/// A speedscope file names a frame by file and line, and a file that stops halfway is
/// reported and skipped without costing the scan the profiles that did read.
#[test]
fn a_speedscope_profile_is_read_and_a_truncated_one_is_skipped() {
    let root = project("speedscope");
    std::fs::write(
        root.join("run.speedscope.json"),
        r#"{"$schema": "https://www.speedscope.app/file-format-schema.json",
        "shared": {"frames": [{"name": "sumCube", "file": "hot.ts", "line": 5}]},
        "profiles": [{"type": "sampled", "unit": "none", "samples": [[0], [0]]}]}"#,
    )
    .expect("the speedscope profile");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/profiles/truncated.speedscope.json"),
        root.join("truncated.speedscope.json"),
    )
    .expect("the truncated profile");

    let output = perf(
        &root,
        &[
            "--profile",
            "truncated.speedscope.json",
            "--profile",
            "run.speedscope.json",
        ],
    );
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert!(
        stderr.contains("skipping profile") && stderr.contains("truncated.speedscope.json"),
        "{stderr}"
    );
    let with = rows(
        &root,
        &[
            "--profile",
            "truncated.speedscope.json",
            "--profile",
            "run.speedscope.json",
        ],
    );
    assert_eq!(with[0]["name"], "sumCube");
    assert_eq!(with[0]["sampled"]["inclusive"], 1000);
    let _ = std::fs::remove_dir_all(&root);
}

//! What a profile does to a static finding, end to end. A shape that costs is a
//! claim about how work grows; a run is the only thing that can say whether anything
//! reached it, and the two have to disagree visibly rather than silently.

mod common;
use common::Scratch;

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

fn project(name: &str) -> Scratch {
    let root = Scratch::new(&format!("perf-{name}"));
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
}

fn profile_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/profiles")
        .join(name)
}

/// `jfr print` names a frame by class and line, and the class is what says which
/// file the samples belong to.
#[test]
fn a_jfr_text_dump_places_samples_in_the_java_file_its_class_came_from() {
    let java = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/java");
    let dump = profile_fixture("java.jfr.txt");
    let with = rows(&java, &["--profile", dump.to_str().expect("a path")]);
    let sum_cube = with
        .iter()
        .find(|row| row["name"] == "sumCube")
        .expect("the cubic loop");
    assert_eq!(sum_cube["sampled"]["inclusive"], 333);
}

#[test]
fn a_binary_jfr_recording_is_skipped_with_the_conversion_that_reads_it() {
    let root = project("native-jfr");
    std::fs::write(root.join("run.jfr"), b"FLR\0\0\x02\0\x01").expect("the recording");
    let output = perf(
        &root,
        &["--profile", "run.jfr", "--profile", "run.cpuprofile"],
    );
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert!(
        stderr.contains("skipping profile")
            && stderr.contains("jfr print --events jdk.ExecutionSample"),
        "{stderr}"
    );
    assert!(
        output.status.success(),
        "the profile that did read is still used"
    );
}

/// Three loops over constants nest as deep as three over the input, and cost nothing
/// like it; a search inside a loop is one loop deep and costs a pass over two lists.
const SHAPES: &str = "\
export function tableOfCubes(): number {
  let total = 0;
  for (let i = 0; i < 4; i++) {
    for (let j = 0; j < 4; j++) {
      for (let k = 0; k < 4; k++) {
        total += i * j * k;
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

fn shapes(name: &str) -> Scratch {
    let root = Scratch::new(&format!("perf-{name}"));
    std::fs::write(root.join("shapes.ts"), SHAPES).expect("the source");
    root
}

fn row_named<'a>(rows: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    rows.iter()
        .find(|row| row["name"] == name)
        .unwrap_or_else(|| panic!("no row for {name}"))
}

/// `order` and `loops` are what readers of the JSON have always been given, and they
/// describe the loop nesting of the text. The composed cost arrives beside them.
#[test]
fn a_row_keeps_its_syntactic_order_and_gains_the_composed_cost() {
    let root = shapes("composed");
    let all = rows(&root, &[]);

    let cubes = row_named(&all, "tableOfCubes");
    assert_eq!(cubes["order"], "O(n\u{b3}+)");
    assert_eq!(cubes["loops"], 3);
    assert_eq!(cubes["time"], "O(1)", "the bounds are constants");

    let keep = row_named(&all, "keepWanted");
    assert_eq!(keep["order"], "O(n)");
    assert_eq!(keep["loops"], 1);
    assert_eq!(keep["time"], "O(n\u{b7}m)");
    assert_eq!(keep["space"], "O(n)");
    assert_eq!(keep["confidence"], "verified");
    assert_eq!(keep["cost"]["expr"], "O(rows\u{b7}wanted)");
    let sizes: Vec<(&str, &str)> = keep["cost"]["vars"]
        .as_array()
        .expect("the variables")
        .iter()
        .map(|var| (var["id"].as_str().unwrap(), var["name"].as_str().unwrap()))
        .collect();
    assert_eq!(sizes, [("n", "rows"), ("m", "wanted")]);
    assert_eq!(keep["cost"]["trace"][0]["what"], "loop");
    for key in ["alloc", "io_order"] {
        assert!(!keep[key].is_null(), "{key}");
    }
    assert_eq!(keep["io"], 0, "`io` is still the count of round trips");
}

/// `--sort order` ranks by what the function was composed to cost, so three loops over
/// constants no longer outrank a search inside a loop.
#[test]
fn sorting_by_order_time_or_space_ranks_by_the_composed_cost() {
    let root = shapes("sorted");
    let by_order = rows(&root, &["--sort", "order"]);
    let by_time = rows(&root, &["--sort", "time"]);
    let by_space = rows(&root, &["--sort", "space"]);

    assert_eq!(by_order[0]["name"], "keepWanted");
    assert_eq!(by_time[0]["name"], "keepWanted");
    assert_eq!(by_space[0]["name"], "keepWanted");
}

fn html_report(root: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", "--reporter", "html"])
        .current_dir(root)
        .output()
        .expect("failed to run rqx");
    String::from_utf8(output.stdout).expect("utf-8")
}

/// The page is read with nothing beside it and no script, so the derivation is a native
/// `<details>` under its row, and the row links to the file's findings further down.
#[test]
fn the_html_report_ranks_the_slow_functions_with_their_derivations_and_no_script() {
    let root = shapes("html");
    let page = html_report(&root);

    assert!(page.contains("<section id=\"performance\""), "a section");
    assert!(page.contains("<a href=\"#performance\">Performance</a>"));
    let keep = page
        .lines()
        .find(|line| line.contains("<th scope=\"row\">keepWanted</th>"))
        .expect("a row for the function");
    assert!(keep.contains("<td>O(n\u{b7}m)</td>"), "{keep}");
    assert!(keep.contains("verified"), "{keep}");
    assert!(
        keep.contains("<a href=\"#shapes.ts\">shapes.ts:13</a>"),
        "{keep}"
    );
    assert!(
        page.contains("<details class=\"derivation\"><summary>How the cost was derived</summary><pre>keepWanted"),
        "the derivation sits in a details element"
    );
    assert!(!page.contains("<script"), "the page carries no script");
}

#[test]
fn a_project_with_nothing_slow_has_no_performance_section() {
    let root = Scratch::new("perf-quiet");
    std::fs::write(
        root.join("quiet.ts"),
        "export const one = (): number => 1;\n",
    )
    .expect("source");
    let page = html_report(&root);

    assert!(page.contains("<section id=\"summary\""));
    assert!(!page.contains("id=\"performance\""));
}

#[test]
fn the_terminal_table_shows_the_syntactic_order_beside_the_composed_columns() {
    let root = shapes("table");
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg("perf")
        .arg(&root)
        .output()
        .expect("failed to run rqx");
    let text = String::from_utf8(output.stdout).expect("utf-8");
    let header = text
        .lines()
        .find(|line| line.contains("order") && line.contains("where"))
        .expect("a header");
    let columns: Vec<&str> = header.split_whitespace().collect();
    assert_eq!(
        columns,
        [
            "order", "time", "space", "io", "alloc", "conf", "loops", "trips", "reach", "where"
        ]
    );
    let keep = text
        .lines()
        .find(|line| line.contains("keepWanted"))
        .expect("a row");
    assert!(
        keep.contains("O(n\u{b7}m)") && keep.contains("verified"),
        "{keep}"
    );
}

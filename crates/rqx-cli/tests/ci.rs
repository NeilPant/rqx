//! What a CI system depends on: the exit code, and the shape of each report it feeds
//! to a viewer. These are the contracts a pipeline breaks silently when they drift —
//! a gate that stops failing, or a report a viewer quietly ignores because one field
//! changed name.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CLEAN: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
const DIRTY: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-ci-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    for (path, text) in files {
        std::fs::write(root.join(path), text).expect("a file");
    }
    root
}

fn rqx(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg(args[0])
        .arg(root)
        .args(&args[1..])
        .output()
        .expect("failed to run rqx")
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

#[test]
fn a_clean_project_exits_zero_and_a_gated_dirty_one_exits_one() {
    let clean = project(
        "clean",
        &[("a.ts", CLEAN), ("index.ts", "export * from './a'\n")],
    );
    assert_eq!(code(&rqx(&clean, &["scan", "--fail-on", "warning"])), 0);

    let dirty = project(
        "dirty",
        &[("a.ts", DIRTY), ("index.ts", "export * from './a'\n")],
    );
    assert_eq!(
        code(&rqx(&dirty, &["scan", "--fail-on", "warning"])),
        1,
        "a gate that stops failing is a pipeline that stops working"
    );
    for root in [clean, dirty] {
        let _ = std::fs::remove_dir_all(root);
    }
}

/// The distinction a pipeline needs most: "your code is bad" and "rqx could not run"
/// have to be different exits, or a broken config reads as a passing build.
#[test]
fn being_unable_to_run_is_a_different_exit_from_failing() {
    let missing = std::env::temp_dir().join("rqx-ci-nowhere");
    let _ = std::fs::remove_dir_all(&missing);
    assert_eq!(code(&rqx(&missing, &["scan"])), 2);

    let broken = project(
        "broken-config",
        &[("RQX.toml", "[rules]\ncategories = [\"nope\"]\n")],
    );
    assert_eq!(code(&rqx(&broken, &["scan"])), 2);
    let _ = std::fs::remove_dir_all(broken);
}

/// A bare scan reports; it gates only when asked. A pipeline that added a config file
/// above its checkout would otherwise start failing without anyone changing a rule.
#[test]
fn a_bare_scan_never_gates() {
    let root = project(
        "ungated",
        &[
            ("a.ts", DIRTY),
            ("index.ts", "export * from './a'\n"),
            ("RQX.toml", "[gate]\nfail_on = \"info\"\nmax_findings = 0\n"),
        ],
    );
    assert_eq!(code(&rqx(&root, &["scan"])), 0);
    assert_eq!(code(&rqx(&root, &["scan", "--fail-on", "info"])), 1);
    let _ = std::fs::remove_dir_all(root);
}

/// Every field GitLab reads before it will draw a Code Quality annotation. A report
/// missing one of these is accepted by the pipeline and shown to nobody.
#[test]
fn the_gitlab_report_carries_what_gitlab_reads() {
    let root = project(
        "gitlab",
        &[("a.ts", DIRTY), ("index.ts", "export * from './a'\n")],
    );
    let out = root.join("gl-code-quality-report.json");
    rqx(
        &root,
        &["scan", "--reporter", &format!("gitlab={}", out.display())],
    );

    let body = std::fs::read_to_string(&out).expect("the report");
    let items: Vec<serde_json::Value> = serde_json::from_str(&body).expect("an array");
    assert!(!items.is_empty(), "no findings reached the report");

    for item in &items {
        for field in ["description", "check_name", "fingerprint", "severity"] {
            assert!(item[field].is_string(), "{field} missing from {item}");
        }
        assert!(item["location"]["path"].is_string(), "{item}");
        assert!(item["location"]["lines"]["begin"].is_number(), "{item}");
        assert!(
            ["info", "minor", "major", "critical", "blocker"]
                .contains(&item["severity"].as_str().unwrap_or_default()),
            "GitLab rejects an unknown severity: {item}"
        );
    }
    let _ = std::fs::remove_dir_all(root);
}

/// GitLab shows one annotation per fingerprint. A file holding the same finding three
/// times has to produce three, or two of them are dropped on the way to the merge
/// request without anything saying so.
#[test]
fn every_entry_in_the_gitlab_report_has_its_own_fingerprint() {
    let repeated = "export function f(a: number) {\n  try { g() } catch (e) { }\n  try { g() } catch (e) { }\n  try { g() } catch (e) { }\n  return a\n}\nfunction g() {}\n";
    let root = project(
        "prints",
        &[("a.ts", repeated), ("index.ts", "export * from './a'\n")],
    );
    let out = root.join("report.json");
    rqx(
        &root,
        &["scan", "--reporter", &format!("gitlab={}", out.display())],
    );

    let body = std::fs::read_to_string(&out).expect("the report");
    let items: Vec<serde_json::Value> = serde_json::from_str(&body).expect("an array");
    let prints: std::collections::BTreeSet<&str> = items
        .iter()
        .map(|item| item["fingerprint"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        prints.len(),
        items.len(),
        "{} of {} entries would be collapsed into another",
        items.len() - prints.len(),
        items.len()
    );
    let _ = std::fs::remove_dir_all(root);
}

/// A fingerprint that moved with the line would re-report every finding below an edit
/// as new, which is the whole reason the report has fingerprints at all.
#[test]
fn a_finding_that_only_moved_keeps_its_fingerprint() {
    let of = |name: &str, source: &str| -> Vec<String> {
        let root = project(
            name,
            &[("a.ts", source), ("index.ts", "export * from './a'\n")],
        );
        let out = root.join("report.json");
        rqx(
            &root,
            &["scan", "--reporter", &format!("gitlab={}", out.display())],
        );
        let body = std::fs::read_to_string(&out).expect("the report");
        let items: Vec<serde_json::Value> = serde_json::from_str(&body).expect("an array");
        let mut prints: Vec<String> = items
            .iter()
            .map(|item| item["fingerprint"].as_str().unwrap_or_default().to_owned())
            .collect();
        prints.sort();
        let _ = std::fs::remove_dir_all(root);
        prints
    };

    let before = of("print-before", DIRTY);
    let after = of("print-after", &format!("// a new line at the top\n{DIRTY}"));
    assert_eq!(before, after, "the whole file re-reported after one edit");
}

#[test]
fn the_junit_report_is_parseable_and_counts_its_failures() {
    let root = project(
        "junit",
        &[("a.ts", DIRTY), ("index.ts", "export * from './a'\n")],
    );
    let out = root.join("rqx-junit.xml");
    rqx(
        &root,
        &["scan", "--reporter", &format!("junit={}", out.display())],
    );

    let body = std::fs::read_to_string(&out).expect("the report");
    assert!(body.starts_with("<?xml"), "{body}");
    assert!(body.contains("<testsuite"), "{body}");
    assert!(
        body.contains("<failure"),
        "a dirty project reported no failure"
    );
    // Every element a viewer opens has to close, or the parser gives up on the file
    // and the job shows no tests at all.
    // The trailing space matters: `<testsuite ` must not also count `<testsuites `.
    // The header counts are what a viewer shows before anyone opens a case, so a
    // report whose totals disagree with its own elements misreports the build.
    let attribute = |name: &str| -> usize {
        body.split(&format!("{name}=\""))
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| panic!("no {name} in:\n{body}"))
    };
    assert_eq!(
        attribute("tests"),
        body.matches("<testcase ").count(),
        "the header counts cases the file does not have:\n{body}"
    );
    assert_eq!(
        attribute("failures"),
        body.matches("<failure ").count(),
        "the header counts failures the file does not have:\n{body}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_sarif_report_names_a_run_and_a_rule_for_every_result() {
    let root = project(
        "sarif",
        &[("a.ts", DIRTY), ("index.ts", "export * from './a'\n")],
    );
    let out = root.join("rqx.sarif");
    rqx(
        &root,
        &["scan", "--reporter", &format!("sarif={}", out.display())],
    );

    let body = std::fs::read_to_string(&out).expect("the report");
    let sarif: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(sarif["version"], "2.1.0");
    let run = &sarif["runs"][0];
    assert!(run["tool"]["driver"]["name"].is_string());

    let rules: Vec<&str> = run["tool"]["driver"]["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .map(|rule| rule["id"].as_str().unwrap_or_default())
        .collect();
    for result in run["results"].as_array().expect("results") {
        let id = result["ruleId"].as_str().expect("a ruleId");
        assert!(rules.contains(&id), "{id} has no rule descriptor");
    }
    let _ = std::fs::remove_dir_all(root);
}

/// One pass, several reports. Running rqx twice to get two formats doubles the slowest
/// step in the job, so the pipeline snippets all rely on this working.
#[test]
fn one_run_writes_every_report_it_was_asked_for() {
    let root = project(
        "many",
        &[("a.ts", DIRTY), ("index.ts", "export * from './a'\n")],
    );
    let gitlab = root.join("gl.json");
    let junit = root.join("junit.xml");
    let sarif = root.join("out.sarif");
    let markdown = root.join("summary.md");

    let output = rqx(
        &root,
        &[
            "scan",
            "--reporter",
            &format!("gitlab={}", gitlab.display()),
            "--reporter",
            &format!("junit={}", junit.display()),
            "--reporter",
            &format!("sarif={}", sarif.display()),
            "--reporter",
            &format!("markdown={}", markdown.display()),
        ],
    );
    assert_eq!(code(&output), 0);
    for path in [&gitlab, &junit, &sarif, &markdown] {
        assert!(path.exists(), "{} was not written", path.display());
        assert!(
            std::fs::metadata(path).expect("metadata").len() > 0,
            "{} is empty",
            path.display()
        );
    }
    let _ = std::fs::remove_dir_all(root);
}

/// `rqx scan . | head` closes the pipe, and a tool that called that a failure would
/// fail a job for being read impatiently.
#[test]
fn a_closed_pipe_is_not_a_failure() {
    let root = project(
        "pipe",
        &[("a.ts", DIRTY), ("index.ts", "export * from './a'\n")],
    );
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "{} scan {} --format json | head -1 > /dev/null",
            env!("CARGO_BIN_EXE_rqx"),
            root.display()
        ))
        .status()
        .expect("sh");
    assert!(status.success());
    let _ = std::fs::remove_dir_all(root);
}

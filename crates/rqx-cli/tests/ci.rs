//! What a CI system depends on: the exit code, and the shape of each report it feeds
//! to a viewer. These are the contracts a pipeline breaks silently when they drift —
//! a gate that stops failing, or a report a viewer quietly ignores because one field
//! changed name.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod common;
use common::{file_uri, run_with_a_reader_that_walks_away};

const CLEAN: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
const DIRTY: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-ci-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        std::fs::write(file, text).expect("a file");
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

/// A command that takes no path, so it cannot go through `rqx`.
fn rqx_bare(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .output()
        .expect("failed to run rqx");
    assert!(output.status.success(), "rqx exited with {}", output.status);
    String::from_utf8_lossy(&output.stdout).into_owned()
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
        &[("rqx.yaml", "rules:\n  categories: [nope]\n")],
    );
    assert_eq!(code(&rqx(&broken, &["scan"])), 2);
    let _ = std::fs::remove_dir_all(broken);
}

/// A TOML file that was skipped would be a gate nobody knows has changed, so it stops
/// the run; converting it brings the same gate back, with the original kept.
#[test]
fn a_toml_config_stops_the_run_until_it_is_migrated() {
    let root = project(
        "retired",
        &[
            ("a.ts", DIRTY),
            ("index.ts", "export * from './a'\n"),
            ("RQX.toml", "# the bar\n[gate]\nfail_on = \"info\"\n"),
        ],
    );
    assert_eq!(code(&rqx(&root, &["scan"])), 2);
    let refusal = String::from_utf8_lossy(&rqx(&root, &["scan"]).stderr).into_owned();
    assert!(refusal.contains("rqx config migrate"), "{refusal}");

    // The user's own config directories point at the project, so the test never
    // converts the real one.
    let migrated = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["config", "migrate"])
        .arg(&root)
        .env("HOME", &root)
        .env("XDG_CONFIG_HOME", &root)
        .env_remove("APPDATA")
        .output()
        .expect("failed to run rqx");
    assert_eq!(code(&migrated), 0, "{migrated:?}");
    assert!(root.join("rqx.yaml").exists());
    assert!(root.join("RQX.toml.bak").exists());
    assert!(!root.join("RQX.toml").exists());
    assert!(
        std::fs::read_to_string(root.join("rqx.yaml"))
            .expect("rqx.yaml")
            .contains("# the bar\ngate:\n  fail_on: info\n")
    );

    assert_eq!(code(&rqx(&root, &["scan"])), 0, "a bare scan never gates");
    assert_eq!(code(&rqx(&root, &["scan", "--fail-on", "info"])), 1);
    let _ = std::fs::remove_dir_all(root);
}

/// The schema is derived from the config types, so it can be printed without a config
/// and is the file an editor reads from `docs/`.
#[test]
fn the_config_schema_prints_without_a_config_and_matches_the_checked_in_one() {
    let printed = rqx_bare(&["config", "schema"]);
    let schema: serde_json::Value = serde_json::from_str(&printed).expect("JSON");
    assert_eq!(schema["title"], "rqx configuration");
    assert!(schema["properties"]["thresholds"].is_object(), "{printed}");
    let checked_in = std::fs::read_to_string(format!(
        "{}/../../docs/rqx.schema.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("docs/rqx.schema.json");
    assert_eq!(printed.trim_end(), checked_in.trim_end());
}

/// `unreachable-code` is an error by default and fires on `DIRTY`; these show what a
/// project can say about one rule without touching the rest of the catalogue.
fn one_rule(name: &str, config: &str) -> PathBuf {
    project(
        name,
        &[
            ("a.ts", DIRTY),
            ("index.ts", "export * from './a'\n"),
            ("rqx.yaml", config),
        ],
    )
}

#[test]
fn a_rules_severity_can_be_set_in_the_config() {
    let lowered = one_rule(
        "severity",
        "rules:\n  unreachable-code:\n    severity: info\n",
    );
    assert_eq!(code(&rqx(&lowered, &["scan", "--fail-on", "error"])), 0);
    assert_eq!(code(&rqx(&lowered, &["scan", "--fail-on", "info"])), 1);
    let _ = std::fs::remove_dir_all(lowered);
}

#[test]
fn a_rule_restricted_to_paths_stays_quiet_elsewhere() {
    let elsewhere = one_rule(
        "paths-elsewhere",
        "rules:\n  unreachable-code:\n    paths: [\"lib/**\"]\n",
    );
    assert_eq!(code(&rqx(&elsewhere, &["scan", "--fail-on", "info"])), 0);

    let here = one_rule(
        "paths-here",
        "rules:\n  unreachable-code:\n    paths: [\"*.ts\"]\n",
    );
    assert_eq!(code(&rqx(&here, &["scan", "--fail-on", "info"])), 1);
    for root in [elsewhere, here] {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn a_rule_can_be_switched_off_and_a_threshold_set_where_it_is_read() {
    let off = one_rule(
        "switch",
        "rules:\n  unreachable-code:\n    enabled: false\n",
    );
    assert_eq!(code(&rqx(&off, &["scan", "--fail-on", "info"])), 0);

    let two_parameters = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
    let tuned = project(
        "tuned",
        &[
            ("a.ts", two_parameters),
            ("index.ts", "export * from './a'\n"),
            (
                "rqx.yaml",
                "rules:\n  long-parameter-list:\n    max_params: 1\n",
            ),
        ],
    );
    assert_eq!(code(&rqx(&tuned, &["scan", "--fail-on", "info"])), 1);
    let untuned = project(
        "untuned",
        &[
            ("a.ts", two_parameters),
            ("index.ts", "export * from './a'\n"),
        ],
    );
    assert_eq!(code(&rqx(&untuned, &["scan", "--fail-on", "info"])), 0);
    for root in [off, tuned, untuned] {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn an_override_can_hold_a_subtree_to_its_own_rule_settings() {
    let created = project(
        "override-rule",
        &[
            ("legacy/a.ts", DIRTY),
            ("legacy/index.ts", "export * from './a'\n"),
            (
                "rqx.yaml",
                "overrides:\n  - paths: [\"legacy/**\"]\n    rules:\n      unreachable-code:\n        enabled: false\n",
            ),
        ],
    );
    // Override globs are matched against the canonical path of the config's directory,
    // and a temp directory on macOS is reached through a symlink.
    let root = created.canonicalize().expect("a real path");
    assert_eq!(code(&rqx(&root, &["scan", "--fail-on", "info"])), 0);
    std::fs::write(root.join("fresh.ts"), DIRTY).expect("a file outside the subtree");
    assert_eq!(code(&rqx(&root, &["scan", "--fail-on", "info"])), 1);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_unknown_rule_or_option_is_refused_with_what_it_could_have_been() {
    let rule = one_rule(
        "typo-rule",
        "rules:\n  unreachable-cod:\n    severity: info\n",
    );
    let output = rqx(&rule, &["scan"]);
    assert_eq!(code(&output), 2);
    let said = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(said.contains("Did you mean `unreachable-code`?"), "{said}");

    let option = one_rule(
        "typo-option",
        "rules:\n  unreachable-code:\n    severty: info\n",
    );
    let output = rqx(&option, &["scan"]);
    assert_eq!(code(&output), 2);
    let said = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(said.contains("Did you mean `severity`?"), "{said}");
    assert!(
        said.contains("rqx.yaml:"),
        "the file and line are named: {said}"
    );
    for root in [rule, option] {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn explain_says_what_a_setting_is_and_which_preset_or_file_set_it() {
    let root = project(
        "explain",
        &[(
            "rqx.yaml",
            "extends: [ci-gate]\nthresholds:\n  max_params: 6\n",
        )],
    );
    let said = |key: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
            .args(["config"])
            .arg(&root)
            .args(["explain", key])
            .output()
            .expect("failed to run rqx");
        (
            code(&output),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    };
    let (status, preset) = said("gate.min_health");
    assert_eq!(status, 0);
    assert!(preset.contains("current  70  preset:ci-gate"), "{preset}");
    let (_, file) = said("thresholds.max_params");
    assert!(file.contains("Parameters one function may take"), "{file}");
    assert!(file.contains("default  4"), "{file}");
    assert!(file.contains("current  6  rqx.yaml"), "{file}");
    assert_eq!(said("gate.fial_on").0, 2);
    let _ = std::fs::remove_dir_all(root);
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
            ("rqx.yaml", "gate:\n  fail_on: info\n  max_findings: 0\n"),
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

#[test]
fn a_failed_gate_is_written_into_the_sarif_run() {
    let root = project("sarif-gate", &[("a.ts", DIRTY)]);
    let output = rqx(
        &root,
        &[
            "scan",
            "--reporter",
            "sarif",
            "--fail-on",
            "warning",
            "--no-tui",
        ],
    );
    assert_eq!(code(&output), 1);

    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).expect("a document");
    let run = &sarif["runs"][0];
    assert_eq!(run["invocations"][0]["exitCode"], 1);
    assert!(
        !run["invocations"][0]["toolExecutionNotifications"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(run["results"][0].get("partialFingerprints").is_none());
    assert!(run.get("automationDetails").is_none());
}

#[test]
fn junit_and_github_carry_the_gate_only_when_there_is_one() {
    let root = project("gate-lines", &[("a.ts", DIRTY)]);
    let said = |reporter: &str, extra: &[&str]| {
        let mut args = vec!["scan", "--reporter", reporter, "--no-tui"];
        args.extend_from_slice(extra);
        String::from_utf8_lossy(&rqx(&root, &args).stdout).into_owned()
    };

    let junit = said("junit", &["--fail-on", "warning"]);
    assert!(junit.contains(r#"<testsuite name="rqx gate" tests="1" failures="1">"#));
    let github = said("github", &["--fail-on", "warning"]);
    assert!(
        github
            .lines()
            .last()
            .unwrap()
            .starts_with("::error title=rqx gate::")
    );
    let github = said("github", &["--fail-on", "error", "--allow-findings", "100"]);
    assert_eq!(
        github.lines().last(),
        Some("::notice title=rqx gate::gate passed")
    );

    assert!(!said("junit", &[]).contains("rqx gate"));
    assert!(!said("github", &[]).contains("rqx gate"));
}

#[test]
fn sarif_fingerprints_and_category_are_opt_in_through_the_environment() {
    let root = project("sarif-opt-in", &[("a.ts", DIRTY)]);
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan"])
        .arg(&root)
        .args(["--reporter", "sarif", "--no-tui"])
        .env("RQX_SARIF_FINGERPRINTS", "1")
        .env("RQX_SARIF_CATEGORY", "rqx/api")
        .output()
        .expect("failed to run rqx");
    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).expect("a document");
    let run = &sarif["runs"][0];
    assert!(run["results"][0]["partialFingerprints"]["rqxIdentity/v1"].is_string());
    assert_eq!(run["automationDetails"]["id"], "rqx/api");
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
    let output = run_with_a_reader_that_walks_away(
        Command::new(env!("CARGO_BIN_EXE_rqx"))
            .arg("scan")
            .arg(&root)
            .args(["--format", "json"]),
    );
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(root);
}

/// rqx decides dead code from the import graph, which cannot see a dynamic import or
/// a container wiring things up by name. A test run that reached the code is evidence
/// the graph is missing a reader, so the finding is kept and shown but stops counting
/// against the score.
#[test]
fn a_test_run_contradicts_a_dead_code_finding_without_hiding_it() {
    let root = project(
        "coverage",
        &[
            (
                "a.ts",
                "export function reached(): number {\n  return 41\n}\n",
            ),
            ("b.ts", "export function never(): number {\n  return 0\n}\n"),
            ("lcov.info", "SF:a.ts\nDA:1,3\nDA:2,3\nend_of_record\n"),
        ],
    );

    let of = |args: &[&str]| -> Vec<(String, String)> {
        let mut all = vec!["scan", "--format", "json", "--max-findings", "0"];
        all.extend_from_slice(args);
        let out = rqx(&root, &all);
        String::from_utf8(out.stdout)
            .expect("utf-8")
            .lines()
            .filter_map(|line| {
                let value: serde_json::Value = serde_json::from_str(line).ok()?;
                if value["kind"] == "summary" {
                    return None;
                }
                Some((
                    value["location"]["file"].as_str()?.to_owned(),
                    value["confidence"].as_str()?.to_owned(),
                ))
            })
            .collect()
    };

    let plain = of(&[]);
    assert!(
        plain.iter().all(|(_, confidence)| confidence == "verified"),
        "{plain:?}"
    );

    let covered = of(&[
        "--coverage",
        root.join("lcov.info").to_str().expect("utf-8"),
    ]);
    let ran = covered
        .iter()
        .find(|(file, _)| file == "a.ts")
        .expect("a.ts is still reported");
    assert_eq!(
        ran.1, "low",
        "a file the tests ran still counts against the score"
    );
    assert_eq!(
        covered
            .iter()
            .find(|(file, _)| file == "b.ts")
            .expect("b.ts")
            .1,
        "verified",
        "a file no test reached is still dead"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// The interface takes over the terminal. A pipeline's stdout is not a terminal, so
/// it must never see an alternate-screen escape — and if it ever did, the job would
/// hang waiting for a keypress nobody is there to send.
#[test]
fn a_pipeline_never_gets_the_interface() {
    let root = project("piped", &[("a.ts", DIRTY)]);
    // No `--no-tui`: the point is that a pipe is enough on its own.
    let output = rqx(&root, &["scan"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains('\u{1b}'), "escape sequences reached a pipe");
    assert!(
        !text.contains("[?1049h"),
        "the interface opened into a pipe"
    );
    assert!(text.contains("Findings"), "no report was printed:\n{text}");
    assert_eq!(code(&output), 0, "a bare scan never gates");
    let _ = std::fs::remove_dir_all(&root);
}

/// Runs rqx the way a shell does: no path argument, inside the project.
fn rqx_in(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("failed to run rqx")
}

/// The report minus its wall-clock line, which is the one part two runs of the same
/// scan are allowed to disagree on.
fn without_duration(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim_start().starts_with("Duration"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Bare `rqx` used to print help and exit 0 whenever it could not open the interface,
/// which made a CI job with a broken setup look green. It now runs the scan.
#[test]
fn bare_rqx_without_a_terminal_is_the_scan() {
    let root = project("bare", &[("a.ts", DIRTY)]);
    let bare = rqx_in(&root, &[]);
    let scan = rqx_in(&root, &["scan", "."]);
    let report = without_duration(&bare);
    assert!(
        report.contains("Findings"),
        "no report was printed:\n{report}"
    );
    assert_eq!(report, without_duration(&scan));
    assert_eq!(code(&bare), code(&scan));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn bare_rqx_that_cannot_run_exits_two() {
    let root = project(
        "bare-broken",
        &[("rqx.yaml", "rules:\n  categories: [nope]\n")],
    );
    assert_eq!(code(&rqx_in(&root, &[])), 2);
    let _ = std::fs::remove_dir_all(&root);
}

/// The flags that used to steer between the interface and the report stay accepted,
/// so a script that passed them keeps working.
#[test]
fn the_interface_flags_are_still_accepted_by_scan() {
    let root = project("old-flags", &[("a.ts", DIRTY)]);
    let output = rqx(
        &root,
        &["scan", "--no-tui", "--no-mouse", "--motion", "off"],
    );
    assert_eq!(code(&output), 0);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Findings"));
    let _ = std::fs::remove_dir_all(&root);
}

/// `--rule` used to choose which findings the interface opened on, and the report
/// ignored it on a terminal. It narrows the report itself now.
#[test]
fn scan_rule_narrows_the_report() {
    let root = project("rule-filter", &[("a.ts", DIRTY)]);
    let everything = String::from_utf8_lossy(&rqx(&root, &["scan"]).stdout).into_owned();
    assert!(everything.contains("\u{25cf} unused-file"));

    let narrowed =
        String::from_utf8_lossy(&rqx(&root, &["scan", "--rule", "unreachable-code"]).stdout)
            .into_owned();
    assert!(narrowed.contains("\u{25cf} unreachable-code"));
    assert!(
        !narrowed.contains("\u{25cf} unused-file"),
        "the filter did not apply:\n{narrowed}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// `CI` set is the other half: a build machine can have a terminal attached and must
/// still get the report rather than a screen it cannot answer.
#[test]
fn ci_gets_the_report_and_the_exit_code() {
    let root = project("ci-env", &[("a.ts", DIRTY)]);
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg("scan")
        .arg(&root)
        .args(["--fail-on", "error"])
        .env("CI", "true")
        .env("TERM", "xterm-256color")
        .output()
        .expect("failed to run rqx");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("[?1049h"), "the interface opened under CI");
    assert_eq!(code(&output), 1, "the gate did not fail the build");
    let _ = std::fs::remove_dir_all(&root);
}

/// A watch whose stdout is a file would otherwise fill it with clear-screen escapes
/// between the reports; a line tells the runs apart instead.
#[test]
fn a_watch_without_a_terminal_separates_runs_instead_of_clearing_the_screen() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::Duration;

    let root = project("watch", &[("a.ts", DIRTY)]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", "--watch"])
        .arg(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to run rqx");
    let _watchdog = common::Watchdog::arm(child.id(), Duration::from_secs(60));
    let mut lines = BufReader::new(child.stdout.take().expect("a piped stdout")).lines();

    let mut written = false;
    let mut seen = String::new();
    while let Some(Ok(line)) = lines.next() {
        seen.push_str(&line);
        seen.push('\n');
        if line.contains("rescanned after") {
            break;
        }
        // The first report is the signal that the watch has taken its stamps.
        if !written && seen.contains("Findings") {
            std::fs::write(root.join("a.ts"), CLEAN).expect("a changed file");
            written = true;
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    assert!(seen.contains("rescanned after"), "no separator:\n{seen}");
    assert!(
        !seen.contains('\u{1b}'),
        "an escape sequence reached a pipe"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A pipeline pointed at the wrong directory would otherwise pass, silently, with a
/// perfect score: nothing was analysed, so nothing was wrong.
#[test]
fn a_directory_with_nothing_to_analyse_says_so() {
    let root = project("nosources", &[("README.md", "# nothing to scan\n")]);
    let output = rqx(&root, &["scan", "--no-tui"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(code(&output), 0, "finding nothing is not a failure");
    assert!(
        text.contains("No files to analyse"),
        "an empty scan read as a clean one:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The root help lists the commands in groups, so a twenty-eighth command must be
/// placed in one rather than quietly falling off the page.
#[test]
fn every_command_is_in_exactly_one_help_group() {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg("--help")
        .output()
        .expect("failed to run rqx");
    let help = String::from_utf8_lossy(&output.stdout);

    // `help` is clap's own and `docs` is hidden; everything else a reader can type
    // has to appear under a heading.
    let listed = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg("completions")
        .arg("bash")
        .output()
        .expect("failed to run rqx");
    let script = String::from_utf8_lossy(&listed.stdout);
    let mut missing = Vec::new();
    for name in [
        "scan",
        "check",
        "fix",
        "verify",
        "comment",
        "complexity",
        "perf",
        "health",
        "slop",
        "graph",
        "plan",
        "patterns",
        "recommend",
        "rules",
        "explain",
        "techniques",
        "levels",
        "init",
        "config",
        "baseline",
        "suppressions",
        "cache",
        "lsp",
        "mcp",
        "setup",
        "completions",
        "query",
    ] {
        if !help.contains(&format!("  {name}")) {
            missing.push(name);
        }
        assert!(
            script.contains(name),
            "{name} is missing from the completion script"
        );
    }
    assert!(missing.is_empty(), "not in any help group: {missing:?}");
    for heading in [
        "The loop",
        "Look closer",
        "Reference",
        "Project",
        "Integrations",
    ] {
        assert!(help.contains(heading), "no {heading} heading:\n{help}");
    }
}

#[test]
fn a_first_run_is_told_what_it_has_no_way_to_know() {
    let bare = project("bare", &[("a.ts", CLEAN)]);
    let said = String::from_utf8_lossy(&rqx(&bare, &["scan", "--no-tui"]).stdout).into_owned();
    assert!(said.contains("No rqx.yaml"), "{said}");
    assert!(said.contains("Not a git repository"), "{said}");

    let settled = project("settled", &[("a.ts", CLEAN), ("rqx.yaml", "gate: {}\n")]);
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&settled)
            .arg("init")
            .output()
            .expect("git")
            .status
            .success()
    );
    let said = String::from_utf8_lossy(&rqx(&settled, &["scan", "--no-tui"]).stdout).into_owned();
    assert!(!said.contains("No rqx.yaml"), "{said}");
    assert!(!said.contains("Not a git repository"), "{said}");
}

/// The commonest CI failure there is: a runner clones one commit deep, a review
/// diffs `base...HEAD`, and the two histories share nothing. The message has to name
/// that, because nothing else about it looks like a clone-depth problem.
#[test]
fn a_shallow_clone_is_named_as_the_reason_a_review_cannot_run() {
    let origin = project("origin", &[("a.ts", CLEAN)]);
    let git = |root: &Path, args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git")
    };
    git(&origin, &["init", "-q"]);
    git(&origin, &["config", "core.autocrlf", "false"]);
    git(&origin, &["add", "-A"]);
    git(
        &origin,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "one",
        ],
    );
    std::fs::write(origin.join("b.ts"), DIRTY).expect("a file");
    git(&origin, &["add", "-A"]);
    git(
        &origin,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "two",
        ],
    );

    let shallow = origin.with_extension("clone");
    let _ = std::fs::remove_dir_all(&shallow);
    assert!(
        Command::new("git")
            .args(["-c", "core.autocrlf=false", "clone", "-q", "--depth", "1"])
            .arg(file_uri(&origin))
            .arg(&shallow)
            .output()
            .expect("git clone")
            .status
            .success()
    );

    let output = rqx(&shallow, &["check", "--base", "HEAD~1"]);
    let said = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(code(&output), 2, "{said}");
    assert!(said.contains("shallow"), "{said}");
    assert!(said.contains("fetch-depth: 0"), "{said}");

    let _ = std::fs::remove_dir_all(&origin);
    let _ = std::fs::remove_dir_all(&shallow);
}

fn parses_as_json_lines(text: &str) {
    assert!(!text.trim().is_empty(), "stdout is empty");
    for line in text.lines() {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|error| panic!("not JSON ({error}): {line:.200}"));
    }
}

#[test]
fn timings_and_advice_leave_a_json_stream_whole() {
    let root = project("json-companions", &[("a.ts", DIRTY)]);
    let output = rqx(
        &root,
        &[
            "scan",
            "--reporter",
            "json",
            "--timings",
            "--advice",
            "--no-tui",
        ],
    );
    parses_as_json_lines(&String::from_utf8_lossy(&output.stdout));
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(said.contains("total"), "the timings went missing: {said}");
}

#[test]
fn the_fix_report_leaves_a_sarif_document_whole() {
    let root = project("sarif-fix", &[("seed.ts", CLEAN)]);
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .expect("git")
            .status;
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "base"]);
    std::fs::write(root.join("b.ts"), DIRTY).expect("a file");
    git(&["add", "-A"]);
    git(&["commit", "-qm", "branch"]);

    let output = rqx(
        &root,
        &[
            "check",
            "--base",
            "HEAD~1",
            "--fix",
            "--reporter",
            "sarif",
            "--no-tui",
        ],
    );
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not a document: {error}"));
    assert_eq!(document["version"], "2.1.0");
    assert!(!output.stderr.is_empty(), "the fix report went missing");
}

fn json_lines(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("a json object"))
        .collect()
}

#[test]
fn every_json_line_says_what_it_is() {
    let root = project("json-kinds", &[("a.ts", DIRTY)]);
    let lines = json_lines(&rqx(&root, &["scan", "--reporter", "json", "--no-tui"]));

    let (summary, findings) = lines.split_last().expect("a summary line");
    assert!(!findings.is_empty(), "the fixture found nothing");
    assert!(findings.iter().all(|line| line["kind"] == "finding"));
    assert_eq!(summary["kind"], "summary");
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["rqx_version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn a_gated_scan_says_whether_it_passed_and_an_ungated_one_says_nothing() {
    let root = project("json-gate", &[("a.ts", DIRTY)]);
    let summary_of = |extra: &[&str]| {
        let mut args = vec!["scan", "--reporter", "json", "--no-tui"];
        args.extend_from_slice(extra);
        json_lines(&rqx(&root, &args)).pop().expect("a summary")
    };

    let failed = summary_of(&["--fail-on", "warning"]);
    assert_eq!(failed["gate"]["passed"], false);
    assert!(!failed["gate"]["failures"].as_array().unwrap().is_empty());

    let passed = summary_of(&["--fail-on", "error", "--allow-findings", "100"]);
    assert_eq!(passed["gate"]["passed"], true);
    assert_eq!(passed["gate"]["failures"], serde_json::json!([]));

    assert!(summary_of(&[]).get("gate").is_none());
}

#[test]
fn the_secondary_summaries_carry_the_schema_version() {
    let root = project("json-secondary", &[("a.ts", DIRTY)]);
    for command in ["fix", "plan"] {
        let lines = json_lines(&rqx(&root, &[command, "--json", "--no-tui"]));
        let summary = lines.last().expect("a summary line");
        assert_eq!(summary["kind"], "summary", "{command}");
        assert_eq!(summary["schema_version"], 1, "{command}");
    }
}

/// `--slugs` is what a script and a shell completion read. Ignoring the family or
/// language beside it hands back the whole catalogue with no sign it was narrowed.
#[test]
fn slugs_are_narrowed_the_same_way_the_listing_is() {
    let all = rqx_bare(&["rules", "--slugs"]);
    let performance = rqx_bare(&["rules", "performance", "--slugs"]);
    let go = rqx_bare(&["rules", "--language", "go", "--slugs"]);

    let count = |text: &str| text.lines().filter(|line| !line.is_empty()).count();
    assert_eq!(count(&all), 185, "the catalogue is not 185 rules");
    assert!(
        count(&performance) < count(&all) && count(&performance) > 0,
        "the family was ignored: {} of {}",
        count(&performance),
        count(&all)
    );
    assert!(
        count(&go) < count(&all) && count(&go) > 0,
        "the language was ignored: {} of {}",
        count(&go),
        count(&all)
    );
    for slug in performance.lines() {
        assert!(all.lines().any(|line| line == slug), "{slug} is not a rule");
    }
}

/// The README names the number of commands. It was wrong by one within a week of
/// being written, which is what a number in prose does unless something counts it.
#[test]
fn the_readme_counts_the_commands_correctly() {
    let readme =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md"))
            .expect("README.md");
    let help = rqx_bare(&["--help"]);

    let spelled = [
        ("Twenty-six", 26),
        ("Twenty-seven", 27),
        ("Twenty-eight", 28),
        ("Twenty-nine", 29),
        ("Thirty", 30),
    ];
    let claimed = spelled
        .iter()
        .find(|(word, _)| readme.contains(&format!("{word} of them, under five headings")))
        .map(|(_, count)| *count)
        .expect("the README no longer says how many commands there are");

    // A command row is `  <name>[, <alias>]   <Description>`: two spaces, the name,
    // then a capitalised sentence. The example block under the groups is indented the
    // same way but describes itself in lower case, which is what separates the two.
    let real = help
        .lines()
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.trim_start().split_once("  "))
        .filter(|(name, description)| {
            name.starts_with(|character: char| character.is_ascii_lowercase())
                && description
                    .trim_start()
                    .starts_with(|character: char| character.is_ascii_uppercase())
        })
        .count();

    assert_eq!(
        claimed, real,
        "the README says {claimed}, `rqx --help` lists {real}"
    );
}

/// `println!` panics when the reader closes the pipe, and `main` can only forgive a
/// broken pipe that is returned to it as an error — so the slug listing has to write
/// through a handle. `rqx rules --slugs | head` is what a shell completion does, and
/// it exited 101 until it did.
///
/// Asserted against the source rather than by piping: the whole listing is 3KB and
/// fits in a pipe buffer, so the child usually finishes before the reader closes and
/// a runtime test passes with the bug still in place. It did.
#[test]
fn the_slug_listing_writes_through_a_handle_rather_than_println() {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("main.rs");
    let listing = source
        .split_once("if slugs {")
        .expect("the --slugs branch")
        .1
        .split_once("\n            }")
        .expect("its end")
        .0;

    // Comments excluded: the one above this very code names the macro it avoids.
    let code: String = listing
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !code.contains("println!"),
        "the slug listing prints with a macro that panics on a closed pipe"
    );
    assert!(
        code.contains("writeln!"),
        "the slug listing should write through a handle so the error can propagate"
    );
}

/// A build log is not a person. The update check must not print into one, must not
/// reach the network from one, and must not change what the pipeline exits with —
/// the notice is the only thing rqx added to a report after this project's gate
/// contract was written down.
#[test]
fn a_pipeline_never_sees_the_update_notice() {
    let cache = std::env::temp_dir().join(format!("rqx-notice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    std::fs::create_dir_all(cache.join("rqx")).expect("a cache directory");
    std::fs::write(
        cache.join("rqx").join("update.json"),
        format!(
            r#"{{"checked_at": {}, "version": "9.9.9", "notes": "- something"}}"#,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock")
                .as_secs()
        ),
    )
    .expect("a cached answer");

    let project = project("notice", &[("a.ts", CLEAN)]);
    // Every one of these on its own has to be enough, because a pipeline that sets
    // only `TF_BUILD` is still a pipeline.
    for pipeline in [
        "CI",
        "JENKINS_URL",
        "TEAMCITY_VERSION",
        "TF_BUILD",
        "BUILD_NUMBER",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
            .arg("scan")
            .arg(&project)
            .args(["--no-tui"])
            .env("XDG_CACHE_HOME", &cache)
            .env_remove("CI")
            .env(pipeline, "1")
            .output()
            .expect("failed to run rqx");

        let said = String::from_utf8_lossy(&output.stderr);
        assert!(
            !said.contains("9.9.9"),
            "{pipeline} still got an update notice: {said}"
        );
        assert_eq!(code(&output), 0, "{pipeline} changed the exit code");
    }

    // And with nothing set at all: the output is a pipe here, not a terminal, which
    // is the test that does not depend on knowing every CI system's variables.
    let piped = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .arg("scan")
        .arg(&project)
        .args(["--no-tui"])
        .env("XDG_CACHE_HOME", &cache)
        .env_remove("CI")
        .output()
        .expect("failed to run rqx");
    let said = String::from_utf8_lossy(&piped.stderr);
    assert!(
        !said.contains("9.9.9"),
        "a redirected stderr got a notice: {said}"
    );
    assert!(!said.contains('\u{1b}'), "escape sequences reached a pipe");

    let _ = std::fs::remove_dir_all(&cache);
    let _ = std::fs::remove_dir_all(&project);
}

//! `rqx config validate`: the exit code a pipeline branches on, and the place a problem
//! is reported at. The config is resolved by the code a scan uses, so these also pin
//! that the two agree about what loads.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-validate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        std::fs::write(file, text).expect("a file");
    }
    root
}

/// Run from inside the project, with the user's own config out of reach, so the
/// output names `rqx.yaml` and not a path on whoever's machine this is.
fn rqx(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .current_dir(root)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .env_remove("APPDATA")
        .env_remove("RQX_CONFIG")
        .env_remove("RQX_POLICY")
        .env_remove("RQX_LEVEL")
        .output()
        .expect("failed to run rqx")
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn a_sound_config_is_valid_and_says_how_many_files_it_read() {
    let root = project("sound", &[("rqx.yaml", "thresholds:\n  max_params: 3\n")]);
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 0, "{output:?}");
    assert_eq!(stdout(&output).trim(), "\u{2713} config is valid (1 file)");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_project_with_no_config_is_valid() {
    let root = project("bare", &[]);
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 0, "{output:?}");
    assert!(stdout(&output).contains("config is valid (0 files)"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_error_is_exit_two_and_names_the_line_and_column() {
    let root = project(
        "unknown-key",
        &[("rqx.yaml", "thresholds:\n  max_params: 3\n  bogus: 1\n")],
    );
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 2, "{output:?}");
    let report = stdout(&output);
    assert!(
        report.starts_with("rqx.yaml:3:3: error: unknown field `bogus`"),
        "{report}"
    );
    assert!(report.contains("config is invalid"), "{report}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_cost_table_in_the_documented_shape_is_valid() {
    let root = project(
        "cost-table",
        &[(
            "rqx.yaml",
            "performance:\n  max_order: O(n log n)\n  costs:\n    \
             - {id: acme.contains, receiver: [AcmeQueue], method: contains, cost: n}\n    \
             - {id: acme.size, receiver: [AcmeQueue], method: size, cost: 1}\n\
             gate:\n  complexity_regression: warning\n",
        )],
    );
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 0, "{output:?}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_max_order_that_is_not_an_order_is_an_error_at_its_line() {
    let root = project(
        "max-order",
        &[("rqx.yaml", "performance:\n  max_order: quadratic\n")],
    );
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 2, "{output:?}");
    let report = stdout(&output);
    assert!(
        report.starts_with("rqx.yaml:2:14: error: `quadratic` is not an order"),
        "{report}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_unknown_regression_gate_is_an_error_at_its_line() {
    let root = project(
        "regression-gate",
        &[("rqx.yaml", "gate:\n  complexity_regression: loud\n")],
    );
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 2, "{output:?}");
    assert!(
        stdout(&output).starts_with("rqx.yaml:2:"),
        "{}",
        stdout(&output)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_cost_entry_that_names_no_call_is_an_error_in_the_file() {
    let root = project(
        "cost-entry",
        &[(
            "rqx.yaml",
            "performance:\n  costs:\n    - {id: a, cost: n}\n",
        )],
    );
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 2, "{output:?}");
    let report = stdout(&output);
    assert!(
        report.contains("`a` names neither a `method` nor an `fn`"),
        "{report}"
    );
    assert!(report.starts_with("rqx.yaml:"), "{report}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_retired_toml_file_is_an_error_that_points_at_the_migration() {
    let root = project("retired", &[("RQX.toml", "[gate]\nfail_on = \"info\"\n")]);
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 2, "{output:?}");
    let report = stdout(&output);
    assert!(report.contains("RQX.toml: error:"), "{report}");
    assert!(report.contains("rqx config migrate"), "{report}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_project_that_breaks_a_locked_key_is_an_error_at_the_line_that_sets_it() {
    let root = project(
        "locked",
        &[
            (
                "rqx.yaml",
                "policy: policy.yaml\nthresholds:\n  max_params: 9\n",
            ),
            (
                "policy.yaml",
                "thresholds:\n  max_params: 4\nlocked:\n  keys: [thresholds.max_params]\n",
            ),
        ],
    );
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 2, "{output:?}");
    let report = stdout(&output);
    assert!(
        report.starts_with("rqx.yaml:3:3: error: `thresholds.max_params` is locked to 4"),
        "{report}"
    );
    let _ = std::fs::remove_dir_all(root);
}

const WARNED: &str = "rules:\n  disabled: [nonsense, long-parameter-list]\nthresholds:\n  max_params: 3\noverrides:\n  - paths: []\n";

#[test]
fn warnings_print_and_exit_zero() {
    let root = project("warned", &[("rqx.yaml", WARNED)]);
    let output = rqx(&root, &["config", "validate"]);
    assert_eq!(code(&output), 0, "{output:?}");
    let report = stdout(&output);
    assert!(
        report.contains("rqx.yaml:2:14: warning: `nonsense` in `rules.disabled` names nothing"),
        "{report}"
    );
    assert!(
        report.contains(
            "rqx.yaml:4:3: warning: `thresholds.max_params` is set, but `long-parameter-list` is off"
        ),
        "{report}"
    );
    assert!(
        report.contains("rqx.yaml:5:1: warning: an entry of `overrides` names no `paths`"),
        "{report}"
    );
    assert!(
        report.contains("config is valid (1 file, 3 warnings)"),
        "{report}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn strict_turns_warnings_into_exit_one() {
    let root = project("strict", &[("rqx.yaml", WARNED)]);
    let output = rqx(&root, &["config", "validate", "--strict"]);
    assert_eq!(code(&output), 1, "{output:?}");
    assert!(stdout(&output).contains("--strict fails on them"));

    let sound = project(
        "strict-sound",
        &[("rqx.yaml", "gate:\n  fail_on: warning\n")],
    );
    assert_eq!(code(&rqx(&sound, &["config", "validate", "--strict"])), 0);
    for root in [root, sound] {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn an_error_stays_exit_two_under_strict() {
    let root = project("strict-error", &[("rqx.yaml", "bogus: 1\n")]);
    assert_eq!(code(&rqx(&root, &["config", "validate", "--strict"])), 2);
    let _ = std::fs::remove_dir_all(root);
}

/// The leniency belongs to `validate`. A run keeps refusing what it always refused,
/// because a name that resolves to nothing is a rule that silently never applies.
#[test]
fn a_scan_still_refuses_what_validate_only_warns_about() {
    let root = project("refused", &[("rqx.yaml", WARNED), ("a.ts", "export {}\n")]);
    assert_eq!(code(&rqx(&root, &["config", "validate"])), 0);
    assert_eq!(code(&rqx(&root, &["scan", "."])), 2);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_path_argument_and_the_config_flag_choose_what_is_validated() {
    let root = project(
        "chosen",
        &[
            ("service/rqx.yaml", "bogus: 1\n"),
            ("other.yaml", "gate:\n  fail_on: warning\n"),
        ],
    );
    assert_eq!(code(&rqx(&root, &["config", "validate", "service"])), 2);
    assert_eq!(
        code(&rqx(
            &root,
            &["config", "validate", "--config", "other.yaml"]
        )),
        0
    );
    let _ = std::fs::remove_dir_all(root);
}

fn lines(output: &Output) -> Vec<serde_json::Value> {
    stdout(output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}")))
        .collect()
}

#[test]
fn config_json_is_one_line_per_setting_then_a_summary() {
    let root = project(
        "config-json",
        &[("rqx.yaml", "thresholds:\n  max_params: 6\n")],
    );
    let output = rqx(&root, &["config", "--json"]);
    assert_eq!(code(&output), 0, "{output:?}");
    let lines = lines(&output);
    let (summary, settings) = lines.split_last().expect("a summary line");
    assert!(settings.iter().all(|line| line["kind"] == "setting"));

    let find = |key: &str| {
        settings
            .iter()
            .find(|line| line["key"] == key)
            .unwrap_or_else(|| panic!("no setting {key}"))
    };
    let tuned = find("thresholds.max_params");
    assert_eq!(tuned["value"], 6);
    assert_eq!(tuned["from"], "rqx.yaml");
    assert_eq!(tuned["default"], false);
    let untouched = find("gate.fail_on");
    assert_eq!(untouched["value"], "error");
    assert_eq!(untouched["from"], "default");
    assert_eq!(untouched["default"], true);

    assert_eq!(summary["kind"], "summary");
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["files"], serde_json::json!(["rqx.yaml"]));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn config_json_with_a_broken_config_is_still_exit_two() {
    let root = project("config-json-broken", &[("rqx.yaml", "bogus: 1\n")]);
    let output = rqx(&root, &["config", "--json"]);
    assert_eq!(code(&output), 2);
    assert!(stdout(&output).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn validate_json_is_one_line_per_problem_then_a_summary() {
    let root = project("validate-json", &[("rqx.yaml", WARNED)]);
    let output = rqx(&root, &["config", "validate", "--json"]);
    assert_eq!(code(&output), 0, "{output:?}");
    let lines = lines(&output);
    let (summary, problems) = lines.split_last().expect("a summary line");
    assert_eq!(problems.len(), 3);
    assert_eq!(problems[0]["kind"], "problem");
    assert_eq!(problems[0]["severity"], "warning");
    assert_eq!(problems[0]["file"], "rqx.yaml");
    assert_eq!(problems[0]["line"], 2);
    assert_eq!(problems[0]["column"], 14);
    assert_eq!(summary["valid"], true);
    assert_eq!(summary["warnings"], 3);

    let strict = rqx(&root, &["config", "validate", "--json", "--strict"]);
    assert_eq!(code(&strict), 1);
    assert_eq!(lines_of_summary(&strict)["valid"], false);
    let _ = std::fs::remove_dir_all(root);
}

fn lines_of_summary(output: &Output) -> serde_json::Value {
    lines(output).pop().expect("a summary line")
}

#[test]
fn validate_json_reports_an_error_with_its_place() {
    let root = project(
        "validate-json-error",
        &[("rqx.yaml", "thresholds:\n  bogus: 1\n")],
    );
    let output = rqx(&root, &["config", "validate", "--json"]);
    assert_eq!(code(&output), 2);
    let lines = lines(&output);
    assert_eq!(lines[0]["severity"], "error");
    assert_eq!(lines[0]["line"], 2);
    assert_eq!(lines[0]["column"], 3);
    assert_eq!(lines[1]["valid"], false);
    assert_eq!(lines[1]["errors"], 1);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn json_is_refused_on_the_subcommands_that_have_no_json_form() {
    let root = project("json-refused", &[]);
    assert_eq!(code(&rqx(&root, &["config", "--json", "files"])), 2);
    let _ = std::fs::remove_dir_all(root);
}

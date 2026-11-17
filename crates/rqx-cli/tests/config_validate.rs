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

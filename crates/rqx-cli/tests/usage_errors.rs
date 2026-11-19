//! Settings a run cannot honour are refused up front with exit 2, and a setting that
//! names a file the command itself writes does not stop that command.

use std::path::PathBuf;
use std::process::{Command, Output};

fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-usage-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a project");
    std::fs::write(root.join("a.ts"), "export const a = 1;\n").expect("a file");
    root
}

fn rqx(root: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .current_dir(root)
        .env("RQX_NO_UPDATE_CHECK", "1")
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .env_remove("RQX_THREADS")
        .env_remove("RQX_CONFIG")
        .output()
        .expect("rqx runs")
}

#[test]
fn a_thread_count_past_the_limit_is_refused_wherever_it_is_set() {
    let root = project("threads");
    let flag = rqx(&root, &["scan", ".", "--no-tui", "--threads", "5000"]);
    let said = String::from_utf8_lossy(&flag.stderr);
    assert_eq!(flag.status.code(), Some(2), "{said}");
    assert!(said.contains("at most 256"), "{said}");

    std::fs::write(root.join("rqx.yaml"), "scan:\n  threads: 100000\n").expect("config");
    let file = rqx(&root, &["scan", ".", "--no-tui"]);
    let said = String::from_utf8_lossy(&file.stderr);
    assert_eq!(file.status.code(), Some(2), "{said}");
    assert!(said.contains("scan.threads"), "{said}");
}

/// The error for a missing baseline tells the user to run `rqx baseline`, so that
/// command has to work while the file is missing.
#[test]
fn baseline_records_the_file_gate_baseline_names_when_it_is_missing() {
    let root = project("baseline");
    std::fs::write(
        root.join("rqx.yaml"),
        "gate:\n  baseline: .rqx/baseline.json\n",
    )
    .expect("config");

    let scan = rqx(&root, &["scan", ".", "--no-tui"]);
    assert_eq!(scan.status.code(), Some(2), "a scan still needs the file");

    let recorded = rqx(&root, &["baseline", "."]);
    assert!(
        recorded.status.success(),
        "{}",
        String::from_utf8_lossy(&recorded.stderr)
    );
    assert!(root.join(".rqx/baseline.json").exists());
    assert!(rqx(&root, &["scan", ".", "--no-tui"]).status.success());
}

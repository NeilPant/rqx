//! A command given a path that is not there exits 2 and says so. Reading no files and
//! reporting a perfect score, or writing a badge that reads `health 100`, would pass a
//! CI step run from the wrong directory.

use std::process::Command;

#[test]
fn every_command_that_reads_a_path_refuses_one_that_does_not_exist() {
    let missing = std::env::temp_dir()
        .join(format!("rqx-missing-{}", std::process::id()))
        .join("nowhere");
    let missing = missing.to_string_lossy().into_owned();
    let commands: Vec<Vec<&str>> = vec![
        vec!["health", &missing],
        vec!["badge", &missing],
        vec!["graph", &missing],
        vec!["complexity", &missing],
        vec!["hotspots", &missing],
        vec!["perf", &missing],
        vec!["slop", &missing],
        vec!["recommend", &missing],
        vec!["owners", &missing],
        vec!["patterns", &missing],
        vec!["suppressions", &missing],
        vec!["baseline", &missing],
        vec!["query", "typescript", "(identifier) @name", &missing],
        vec!["cache", "clear", &missing],
        vec!["cache", "status", &missing],
    ];
    for arguments in commands {
        let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
            .args(&arguments)
            .arg("--no-tui")
            .env("RQX_NO_UPDATE_CHECK", "1")
            .output()
            .expect("rqx runs");
        let said = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {said}");
        assert!(said.contains("does not exist"), "{arguments:?}: {said}");
    }
}

//! A pager is for a person at a terminal. Output into a pipe has to arrive exactly as
//! it did before the pager existed, with the same exit code, whatever the environment
//! names as a pager.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> String {
    format!("{}/../../fixtures/typescript", env!("CARGO_MANIFEST_DIR"))
}

/// `rqx scan` on the fixtures into a pipe, with the gate on so the exit code says
/// something, and the pager variables set as `pagers` gives them.
fn scan(extra: &[&str], pagers: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rqx"));
    command
        .args([
            "scan",
            &fixtures(),
            "--progress",
            "none",
            "--fail-on",
            "error",
        ])
        .args(extra)
        .env_remove("RQX_PAGER")
        .env_remove("PAGER")
        .env("RQX_NO_UPDATE_CHECK", "1");
    for (name, value) in pagers {
        command.env(name, value);
    }
    command.output().expect("rqx runs")
}

/// The report without the one line that differs between any two runs: how long it took.
fn report(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim_start().starts_with("Duration"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A pager that only leaves a file behind, so a test can tell whether it was started.
fn marking_pager(marker: &Path) -> String {
    format!("touch {}", marker.display())
}

fn marker(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rqx-pager-{name}-{}", std::process::id()))
}

#[cfg(unix)]
#[test]
fn a_report_into_a_pipe_never_starts_a_pager() {
    let rqx_marker = marker("rqx");
    let pager_marker = marker("pager");
    let rqx_pager = marking_pager(&rqx_marker);
    let pager = marking_pager(&pager_marker);

    let direct = scan(&["--no-pager"], &[]);
    let piped = scan(
        &[],
        &[("RQX_PAGER", rqx_pager.as_str()), ("PAGER", pager.as_str())],
    );
    let only_pager = scan(&[], &[("PAGER", pager.as_str())]);

    assert!(!rqx_marker.exists(), "RQX_PAGER was started for a pipe");
    assert!(!pager_marker.exists(), "PAGER was started for a pipe");
    assert!(!direct.stdout.is_empty());
    for output in [&piped, &only_pager] {
        assert_eq!(report(output), report(&direct), "the piped report changed");
        assert_eq!(output.status.code(), direct.status.code());
    }
}

#[test]
fn no_pager_changes_neither_the_report_nor_the_exit_code() {
    let paged_if_it_could = scan(&[], &[]);
    let refused = scan(&["--no-pager"], &[]);
    assert_eq!(report(&paged_if_it_could), report(&refused));
    assert_eq!(paged_if_it_could.status.code(), refused.status.code());
    assert_eq!(refused.status.code(), Some(1), "the fixtures fail the gate");
}

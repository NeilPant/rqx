//! The program starts, on every platform, whatever the main thread's stack is. The
//! Windows main thread has 1 MB, and a debug build once overflowed it before printing a
//! word, so these run the real binary and read what it prints.

use std::process::{Command, Output};

fn rqx(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(arguments)
        .env("NO_COLOR", "1")
        .env("RQX_NO_UPDATE_CHECK", "1")
        .output()
        .expect("failed to run rqx")
}

fn assert_prints_usage(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} exited with {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Usage:"),
        "{what} printed no usage"
    );
}

#[test]
fn the_top_level_help_prints() {
    assert_prints_usage(&rqx(&["--help"]), "rqx --help");
}

#[test]
fn the_help_of_a_subcommand_prints() {
    assert_prints_usage(&rqx(&["scan", "--help"]), "rqx scan --help");
    assert_prints_usage(&rqx(&["setup", "--help"]), "rqx setup --help");
}

/// A shell's `ulimit -s` caps the main thread's stack at 512 KB, which is below what
/// Windows gives. The program has to run anyway, because its body runs on a thread of
/// its own. Windows has no `sh` and no such limit; the tests above run against its
/// own 1 MB there.
#[cfg(unix)]
#[test]
fn the_program_runs_with_a_main_thread_stack_of_512_kilobytes() {
    let output = Command::new("sh")
        .args(["-c", "ulimit -s 512 && exec \"$0\" scan --help"])
        .arg(env!("CARGO_BIN_EXE_rqx"))
        .env("NO_COLOR", "1")
        .env("RQX_NO_UPDATE_CHECK", "1")
        .output()
        .expect("failed to run sh");
    assert_prints_usage(&output, "rqx scan --help under a 512 KB stack limit");
}

//! Links are an escape sequence in the report, so the question that matters is where
//! they must never appear: in a pipe, in a file, and on a report with no colour.

use std::process::Command;

const OSC_8: &str = "\u{1b}]8;;";

fn run(arguments: &[&str], editor_url: Option<&str>) -> String {
    let fixtures = format!("{}/../../fixtures/typescript", env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_rqx"));
    command
        .args(["scan", &fixtures, "--progress", "none"])
        .args(arguments)
        .env_remove("RQX_HYPERLINKS")
        .env_remove("RQX_EDITOR_URL")
        .env("TERM_PROGRAM", "iTerm.app");
    if let Some(template) = editor_url {
        command.env("RQX_EDITOR_URL", template);
    }
    let output = command.output().expect("failed to run rqx");
    String::from_utf8(output.stdout).expect("output was not utf-8")
}

#[test]
fn a_piped_report_has_no_links_even_on_a_terminal_that_supports_them() {
    let text = run(&[], None);

    assert!(!text.contains(OSC_8), "a pipe received a link");
}

#[test]
fn a_report_without_colour_has_no_links_even_when_asked_for() {
    let text = run(&["--hyperlinks", "always", "--color", "never"], None);

    assert!(
        !text.contains(OSC_8),
        "an uncoloured report received a link"
    );
}

#[test]
fn a_report_written_to_a_file_has_no_links() {
    let path = std::env::temp_dir().join(format!("rqx-links-{}.txt", std::process::id()));
    let reporter = format!("terminal={}", path.display());
    run(
        &[
            "--hyperlinks",
            "always",
            "--color",
            "always",
            "--reporter",
            &reporter,
        ],
        None,
    );

    let written = std::fs::read_to_string(&path).expect("the report file");
    let _ = std::fs::remove_file(&path);
    assert!(!written.contains(OSC_8), "a file received a link");
    assert!(written.contains("FAIL"), "the report was not written");
}

#[test]
fn always_with_colour_links_each_place_to_the_file() {
    let text = run(&["--hyperlinks", "always", "--color", "always"], None);

    assert!(text.contains("\u{1b}]8;;file:///"), "no link in:\n{text}");
}

#[test]
fn the_editor_template_decides_where_a_link_goes() {
    let text = run(
        &["--hyperlinks", "always", "--color", "always"],
        Some("vscode://file/{path}:{line}:{column}"),
    );

    assert!(
        text.contains("\u{1b}]8;;vscode://file/"),
        "no editor link in:\n{text}"
    );
}

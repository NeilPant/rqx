//! The JSON `skipped` list names files the way findings do.

use std::path::Path;
use std::process::Command;

mod common;
use common::Scratch;

fn skipped_file(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .current_dir(directory)
        .env("RQX_NO_UPDATE_CHECK", "1")
        .args(arguments)
        .output()
        .expect("failed to run rqx");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|line| line["kind"] == "summary")
        .expect("a summary line")["skipped"][0]
        .as_str()
        .expect("a skipped file")
        .to_owned()
}

/// A consumer joins `skipped` to findings and to `skipped_by` by path, so the same file
/// has to be spelled the same way whichever directory rqx was started from.
#[test]
fn skipped_files_are_named_relative_to_the_scanned_directory() {
    let root = Scratch::new("skipped-paths");
    std::fs::create_dir_all(root.join("app")).expect("a directory");
    std::fs::write(root.join("app/big.js"), "var a=1;".repeat(2000)).expect("a minified file");
    let parent = root.parent().expect("a parent");
    let name = root
        .file_name()
        .expect("a name")
        .to_string_lossy()
        .into_owned();

    let from_the_parent = skipped_file(parent, &["scan", &name, "--reporter", "json"]);
    let from_inside = skipped_file(&root, &["scan", ".", "--reporter", "json"]);

    assert!(
        from_the_parent.starts_with("app/big.js: "),
        "{from_the_parent}"
    );
    assert_eq!(from_inside, from_the_parent);
}

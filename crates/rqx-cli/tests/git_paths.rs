//! What rqx asks git, with the names and revisions git is careful about: paths it
//! would quote, a revision that reads as an option, and a base checkout a killed run
//! left behind.

use std::collections::BTreeSet;
use std::path::Path;

mod common;
use common::Repository;

/// An empty handler, which `empty-catch` reports in every file below.
const SWALLOWS: &str = "export function f() {\n  try {\n    run();\n  } catch (e) {}\n}\n";

/// Names git quotes in its line-per-path output: a non-ASCII letter, a tab, a double
/// quote and a newline.
fn awkward_names() -> Vec<&'static str> {
    let mut names = vec!["café.ts", "ünï/日本語.ts", "q\"uote.ts"];
    // Windows allows neither a tab nor a newline in a file name.
    if cfg!(unix) {
        names.extend(["ta\tb.ts", "new\nline.ts"]);
    }
    names
}

fn reported_files(lines: &[serde_json::Value]) -> BTreeSet<String> {
    lines
        .iter()
        .filter(|line| line["kind"] == "finding")
        .filter_map(|line| line["location"]["file"].as_str().map(str::to_owned))
        .collect()
}

fn with_awkward_names(name: &str) -> Repository {
    let repository = Repository::new(name);
    let mut files: Vec<(&str, Option<&str>)> = vec![("base.ts", Some("export const a = 1;\n"))];
    files.extend(awkward_names().into_iter().map(|name| (name, Some(""))));
    repository.commit("base", &files);
    let mut changed: Vec<(&str, Option<&str>)> = vec![("plain.ts", Some(SWALLOWS))];
    changed.extend(
        awkward_names()
            .into_iter()
            .map(|name| (name, Some(SWALLOWS))),
    );
    repository.commit("change", &changed);
    repository
}

#[test]
fn check_reviews_every_changed_file_whatever_its_name() {
    let repository = with_awkward_names("check-names");
    let lines = repository.rqx_lines(&["check", "--base", "HEAD~1", "--reporter", "json"]);
    let reported = reported_files(&lines);
    for name in awkward_names().into_iter().chain(["plain.ts"]) {
        assert!(
            reported.contains(name),
            "{name:?} was changed and not reviewed: {reported:?}"
        );
    }
}

#[test]
fn scan_since_keeps_every_changed_file_whatever_its_name() {
    let repository = with_awkward_names("since-names");
    let lines = repository.rqx_lines(&["scan", "--since", "HEAD~1", "--reporter", "json"]);
    let reported = reported_files(&lines);
    for name in awkward_names() {
        assert!(reported.contains(name), "{name:?} missing: {reported:?}");
    }
}

#[test]
fn churn_counts_files_whatever_their_name() {
    let repository = with_awkward_names("churn-names");
    let output = repository.rqx_output(&["hotspots", "--reporter", "json"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("café.ts"), "café.ts has no churn: {text}");
}

fn files_in(directory: &Path) -> BTreeSet<String> {
    std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_revision_that_reads_as_an_option_is_refused_before_git_sees_it() {
    let repository = with_awkward_names("option-revision");
    let before = files_in(&repository.root);
    for args in [
        vec!["check", "--base=--output=written.txt", "--no-tui"],
        vec!["scan", "--since=--output=written.txt", "--no-tui"],
        vec!["verify", "--base=--output=written.txt", "--no-tui"],
    ] {
        let output = repository.rqx_output(&args);
        let said = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {said}");
        assert!(
            said.contains("is not a revision"),
            "{args:?} did not say why: {said}"
        );
    }
    assert_eq!(
        files_in(&repository.root),
        before,
        "a refused revision still wrote a file"
    );
}

#[test]
fn a_failing_git_command_is_quoted_in_the_error() {
    let repository = with_awkward_names("git-said");
    let output = repository.rqx_output(&["check", "--base", "no-such-branch", "--no-tui"]);
    let said = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(
        said.contains("git diff") && said.contains("no-such-branch"),
        "the error does not say what git said: {said}"
    );
}

fn registered_checkouts(repository: &Repository) -> Vec<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .expect("git runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .filter(|path| path.contains("rqx-base-"))
        .map(str::to_owned)
        .collect()
}

/// A run killed while it held a base checkout leaves the checkout registered in the
/// repository. The next run that takes one removes it, and removes its own when done.
#[test]
fn a_base_checkout_left_by_a_killed_run_is_removed_by_the_next() {
    let repository = with_awkward_names("abandoned-checkout");
    let abandoned = std::env::temp_dir().join(format!(
        "rqx-base-{}-abandoned-by-a-test",
        std::process::id()
    ));
    repository.git(&[
        "worktree",
        "add",
        "--detach",
        "--quiet",
        &abandoned.to_string_lossy(),
        "HEAD~1",
    ]);
    // An unheld lock is what a killed run leaves beside its checkout.
    std::fs::write(abandoned.with_extension("lock"), "").expect("a lock file");
    assert_eq!(registered_checkouts(&repository).len(), 1);

    let output = repository.rqx_output(&["verify", "--base", "HEAD~1", "--no-tui"]);
    assert!(
        output.status.code() != Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        registered_checkouts(&repository).is_empty(),
        "checkouts left registered: {:?}",
        registered_checkouts(&repository)
    );
    assert!(
        !abandoned.exists(),
        "the abandoned checkout is still on disk"
    );
}

//! `rqx update`, against a release served from a directory.
//!
//! The release is a folder laid out like GitHub's download URLs and reached through
//! `RQX_RELEASE_URL`, so nothing here touches the network. What gets replaced is
//! always a copy of the binary in a temp directory: replacing the one cargo built
//! would break every other test in the run.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::file_uri;
use sha2::{Digest, Sha256};

const VERSION: &str = "9.9.9";

fn workspace(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-update-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a temp directory");
    root
}

/// What the release calls this machine, spelled out here rather than imported so a
/// change to the published names has to change a test too.
fn platform() -> String {
    let system = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(windows) {
        "win32"
    } else {
        "linux"
    };
    let architecture = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    format!("{system}-{architecture}")
}

/// `served/v9.9.9/` holding an archive of a script that prints `new`, and a
/// `SHA256SUMS` that lists `listed` for it, or the archive's real hash when `None`.
fn serve_release(served: &Path, listed: Option<&str>) {
    let staged = served.join("staged");
    fs::create_dir_all(&staged).expect("staging");
    fs::write(staged.join("rqx"), "#!/bin/sh\necho new\n").expect("the binary");

    let published = served.join(format!("v{VERSION}"));
    fs::create_dir_all(&published).expect("the release directory");
    let name = format!("rqx-{VERSION}-{}.tar.gz", platform());
    let archive = published.join(&name);
    let packed = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&staged)
        .arg("rqx")
        .status()
        .expect("tar");
    assert!(packed.success());

    let hash = listed.map_or_else(
        || {
            format!(
                "{:x}",
                Sha256::digest(fs::read(&archive).expect("the archive"))
            )
        },
        str::to_owned,
    );
    fs::write(published.join("SHA256SUMS"), format!("{hash}  {name}\n")).expect("checksums");
}

/// A second name for the same binary, so running it makes it the "installed" rqx.
/// Replacing the file at the copy's name leaves the original alone, because a rename
/// replaces a directory entry and not the inode behind another one.
fn installed_copy(directory: &Path) -> PathBuf {
    let copy = directory.join(format!("rqx{}", std::env::consts::EXE_SUFFIX));
    let original = env!("CARGO_BIN_EXE_rqx");
    if fs::hard_link(original, &copy).is_err() {
        fs::copy(original, &copy).expect("a copy of the binary");
    }
    copy
}

fn update(program: &Path, served: &Path, cargo_home: &Path, arguments: &[&str]) -> Output {
    Command::new(program)
        .arg("update")
        .args(arguments)
        .env("RQX_RELEASE_URL", file_uri(served))
        .env("CARGO_HOME", cargo_home)
        .env_remove("RQX_MANAGED")
        .output()
        .expect("rqx runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_dry_run_says_what_it_would_do_and_changes_nothing() {
    let root = workspace("dry");
    let served = root.join("served");
    let program = PathBuf::from(env!("CARGO_BIN_EXE_rqx"));
    let before = fs::metadata(&program)
        .expect("metadata")
        .modified()
        .expect("mtime");

    let output = update(
        &program,
        &served,
        &root,
        &["--version", VERSION, "--dry-run"],
    );

    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(stdout.contains("download"), "{stdout}");
    assert!(
        stdout.contains(&format!("v{VERSION}/rqx-{VERSION}-{}.tar.gz", platform())),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("v{VERSION}/SHA256SUMS")),
        "{stdout}"
    );
    assert!(stdout.contains("target"), "{stdout}");
    assert_eq!(
        fs::metadata(&program)
            .expect("metadata")
            .modified()
            .expect("mtime"),
        before
    );
    assert!(!served.exists(), "a dry run created {}", served.display());
}

#[test]
fn a_package_managed_install_is_handed_back_even_when_told_a_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["update", "--version", VERSION, "--dry-run"])
        .env("RQX_MANAGED", "npm")
        .output()
        .expect("rqx runs");

    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("hand back to npm"), "{stdout}");
}

#[test]
fn a_binary_outside_cargo_is_planned_as_a_download_and_one_inside_as_a_build() {
    let root = workspace("method");
    let cargo_home = root.join("cargo");
    let bin = cargo_home.join("bin");
    fs::create_dir_all(&bin).expect("cargo's bin");
    let inside = installed_copy(&bin);
    let outside = installed_copy(&root);

    for (program, expected) in [(&inside, "cargo install"), (&outside, "download")] {
        let output = update(
            program,
            &root.join("served"),
            &cargo_home,
            &["--version", VERSION, "--dry-run"],
        );
        let stdout = text(&output.stdout);
        assert!(stdout.contains(expected), "{}: {stdout}", program.display());
    }
}

#[cfg(unix)]
#[test]
fn an_update_replaces_a_copy_of_the_binary_with_the_release() {
    let root = workspace("install");
    let served = root.join("served");
    serve_release(&served, None);
    let copy = installed_copy(&root);
    let original = fs::read(env!("CARGO_BIN_EXE_rqx")).expect("the real binary");

    let output = update(&copy, &served, &root, &["--version", VERSION]);

    assert!(
        output.status.success(),
        "{}{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&copy).expect("the new file"),
        "#!/bin/sh\necho new\n"
    );
    assert_eq!(
        fs::read(env!("CARGO_BIN_EXE_rqx")).expect("the real binary"),
        original,
        "the binary cargo built was touched"
    );
    let ran = Command::new(&copy).output().expect("the new binary runs");
    assert_eq!(text(&ran.stdout), "new\n");
}

#[cfg(unix)]
#[test]
fn a_checksum_that_does_not_match_installs_nothing_and_exits_2() {
    let root = workspace("mismatch");
    let served = root.join("served");
    serve_release(&served, Some(&"0".repeat(64)));
    let copy = installed_copy(&root);
    let original = fs::read(&copy).expect("the copy");

    let output = update(&copy, &served, &root, &["--version", VERSION]);

    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    assert!(
        text(&output.stderr).contains("nothing was installed"),
        "{}",
        text(&output.stderr)
    );
    assert_eq!(fs::read(&copy).expect("the copy"), original);
}

#[test]
fn a_dry_run_cannot_be_combined_with_a_check() {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["update", "--check", "--dry-run"])
        .output()
        .expect("rqx runs");
    assert!(!output.status.success());
}

//! The version lives in five manifests and the release notes live in a sixth file.
//! Nothing but this holds them together, and a release that ships a plugin claiming
//! last version's number is a release nobody can support.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

fn read(relative: &str) -> String {
    let path = root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The version on the first line starting with `prefix`: everything from its first
/// digit to the first character a version cannot hold. One reader for TOML, JSON and
/// a properties file, because all three write it as `<prefix><punctuation><version>`.
fn version_on(text: &str, prefix: &str) -> String {
    let line = text
        .lines()
        .map(str::trim_start)
        .find(|line| line.starts_with(prefix))
        .unwrap_or_else(|| panic!("no line starting `{prefix}`"));
    let rest = &line[prefix.len()..];
    let start = rest
        .find(|character: char| character.is_ascii_digit())
        .unwrap_or_else(|| panic!("no version after `{prefix}`"));
    rest[start..]
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric() || *character == '.' || *character == '-'
        })
        .collect()
}

#[test]
fn every_manifest_carries_the_same_version() {
    let expected = env!("CARGO_PKG_VERSION");

    // Twice in the workspace manifest: the package, and the version beside the path
    // on each internal dependency. Moving one and not the other leaves the workspace
    // unresolvable halfway through a release.
    let manifest = read("Cargo.toml");
    for prefix in [
        "version =",
        "rqx-core = { version =",
        "rqx-lang = { version =",
    ] {
        assert_eq!(
            version_on(&manifest, prefix),
            expected,
            "Cargo.toml line `{prefix}` is not at {expected}; `tools/release.sh` moves them together"
        );
    }
}

/// A reader copies the action and hook examples as written, so they have to name the
/// release the page ships with. `tools/release.sh` rewrites them.
#[test]
fn the_documented_pins_name_this_release() {
    let expected = env!("CARGO_PKG_VERSION");
    for (file, prefix) in [
        ("README.md", "jadhamwi21/rqx@v"),
        ("docs/ci.md", "jadhamwi21/rqx@v"),
        ("docs/ci.md", "rev: v"),
        ("docs/ci.md", "prefix: rqx-"),
    ] {
        let text = read(file);
        let start = text
            .find(prefix)
            .unwrap_or_else(|| panic!("{file} has no `{prefix}`"))
            + prefix.len();
        let pinned: String = text[start..]
            .chars()
            .take_while(|character| character.is_ascii_digit() || *character == '.')
            .collect();
        assert_eq!(
            pinned, expected,
            "{file} pins `{prefix}{pinned}`; `tools/release.sh` moves it with the version"
        );
    }
}

/// The release pipeline posts the changelog section for the tag it is building. A
/// version with no section ships a release whose notes are empty.
#[test]
fn the_changelog_has_a_section_for_this_version() {
    let changelog = read("CHANGELOG.md");
    let version = env!("CARGO_PKG_VERSION");
    let released = format!("## {version} ");

    assert!(
        changelog.contains(&released) || changelog.contains("## Unreleased"),
        "CHANGELOG.md holds neither `{released}…` nor an Unreleased section"
    );
}

/// Two `### Added` blocks under one heading is a merge nobody finished, and the
/// release notes then read as two half-lists.
#[test]
fn no_release_repeats_a_heading() {
    let mut seen: Vec<&str> = Vec::new();
    let changelog = read("CHANGELOG.md");
    for line in changelog.lines() {
        if let Some(release) = line.strip_prefix("## ") {
            assert!(
                !seen.contains(&release),
                "`## {release}` appears twice in CHANGELOG.md"
            );
            seen.push(release);
            continue;
        }
        if line.starts_with("### ") {
            let within = seen.last().copied().unwrap_or("the top of the file");
            let count = changelog
                .split(&format!("## {within}"))
                .nth(1)
                .map(|section| {
                    section
                        .split("\n## ")
                        .next()
                        .unwrap_or_default()
                        .matches(&format!("\n{line}\n"))
                        .count()
                })
                .unwrap_or(0);
            assert!(
                count <= 1,
                "`{line}` appears {count} times under `{within}`"
            );
        }
    }
}

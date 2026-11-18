//! The files a scan did not analyse, and why. A skip is always reported: a file that was
//! never read cannot be told apart from a clean one any other way.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Why a file was not analysed. The order is the order a summary names them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkipReason {
    /// A git submodule left out whole. One entry stands for every source file in it.
    Submodule,
    /// A line too long, or lines too long on average, for code a person writes.
    Minified,
    Generated,
    TooLarge,
    Unreadable,
    ParseTimeout,
    Unsupported,
    /// The walk itself failed on an entry, so there may be no file to name.
    Walk,
}

impl SkipReason {
    /// The key in the JSON summary's `skipped_by`.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Submodule => "submodule",
            Self::Minified => "minified",
            Self::Generated => "generated",
            Self::TooLarge => "too-large",
            Self::Unreadable => "unreadable",
            Self::ParseTimeout => "parse-timeout",
            Self::Unsupported => "unsupported",
            Self::Walk => "walk",
        }
    }

    /// What follows the count in the one-line summary: `14 minified`. A submodule is
    /// named on its own instead.
    const fn label(self) -> &'static str {
        match self {
            Self::Submodule => "in submodules",
            Self::Minified => "minified",
            Self::Generated => "generated",
            Self::TooLarge => "too large",
            Self::Unreadable => "unreadable",
            Self::ParseTimeout => "too slow to parse",
            Self::Unsupported => "unsupported",
            Self::Walk => "unreachable",
        }
    }
}

/// One entry of the skipped list. `detail` is the text the JSON `skipped` strings have
/// always carried after the path, so those strings keep their shape.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: SkipReason,
    pub detail: String,
    /// The source files the entry stands for.
    pub files: u32,
}

impl Skipped {
    pub fn file(path: &Path, reason: SkipReason, detail: String) -> Self {
        Self {
            path: path.to_owned(),
            reason,
            detail,
            files: 1,
        }
    }

    /// `path: detail`, the form of the JSON `skipped` strings.
    pub fn listed(&self) -> String {
        format!("{}: {}", self.path.display(), self.detail)
    }
}

/// The skipped files counted per reason, for the JSON summary's `skipped_by`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub files: u32,
    /// The submodules left out, relative to the scanned directory. Only a submodule
    /// entry has any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
}

pub fn tally(skipped: &[Skipped], root: &Path) -> BTreeMap<&'static str, Tally> {
    let mut tallies: BTreeMap<&'static str, Tally> = BTreeMap::new();
    for entry in skipped {
        let tally = tallies.entry(entry.reason.slug()).or_default();
        tally.files += entry.files;
        if entry.reason == SkipReason::Submodule {
            tally.paths.push(super::relative_to(root, &entry.path));
        }
    }
    tallies
}

/// The one line a terminal report gives the skipped files: `skipped 412 files: 398 in
/// submodule design/, 14 minified`. Every submodule is named, so a reader can see which
/// to bring back. `None` when nothing was skipped.
pub fn summary(skipped: &[Skipped], root: &Path) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }
    let mut by_reason: BTreeMap<SkipReason, u32> = BTreeMap::new();
    let mut submodules = Vec::new();
    for entry in skipped {
        if entry.reason == SkipReason::Submodule {
            submodules.push(format!(
                "{} in submodule {}/",
                entry.files,
                super::relative_to(root, &entry.path)
            ));
        } else {
            *by_reason.entry(entry.reason).or_default() += entry.files;
        }
    }
    let total: u32 = skipped.iter().map(|entry| entry.files).sum();
    let parts: Vec<String> = submodules
        .into_iter()
        .chain(
            by_reason
                .iter()
                .map(|(reason, files)| format!("{files} {}", reason.label())),
        )
        .collect();
    Some(format!(
        "skipped {}: {}",
        crate::report::plural(total, "file", "files"),
        parts.join(", ")
    ))
}

/// How far into a file a generator's header is looked for. Generators put it first;
/// the same words further down are a person writing about them.
const HEAD: usize = 4096;

/// A file a generator owns is one nobody refactors: the next run overwrites whatever
/// was changed, so every finding in it is noise. Only the conventional markers count;
/// a path called `generated` may hold code somebody actually maintains.
pub fn generated_by(source: &str) -> Option<&'static str> {
    // A cap counted in bytes can land inside a multi-byte character, which panics the
    // slice on any source file wide enough to have one.
    let mut cut = source.len().min(HEAD);
    while !source.is_char_boundary(cut) {
        cut -= 1;
    }
    let head = &source[..cut];
    // `@generated` is the convention shared by Bazel, Buck, protoc and gofmt tooling;
    // the other two are the exact headers of generators common enough to matter.
    let markers: [(&str, &str); 3] = [
        ("@generated", "generated code"),
        ("Code generated by", "generated code"),
        ("This is code generated by Prisma", "Prisma generated code"),
    ];
    markers
        .iter()
        .chain(&BUNDLER_RUNTIMES)
        .find(|(needle, _)| head.contains(needle))
        .map(|(_, label)| *label)
        .or_else(|| {
            comment_lines(head)
                .any(says_generated)
                .then_some("generated code")
        })
}

/// What a bundler's runtime defines at the top of its output. Definitions rather than
/// names, so a file that calls `__webpack_require__` is not taken for a bundle.
const BUNDLER_RUNTIMES: [(&str, &str); 6] = [
    ("// webpackBootstrap", "webpack bundle"),
    ("function __webpack_require__(", "webpack bundle"),
    ("var __commonJS = ", "esbuild bundle"),
    ("var __toESM = ", "esbuild bundle"),
    ("function getDefaultExportFromCjs", "rollup bundle"),
    ("var commonjsGlobal = typeof globalThis", "rollup bundle"),
];

/// Whether a comment's text is a generator's warning. Shouted `DO NOT EDIT` and the
/// `@auto-generated` tag are conventions; "automatically generated" counts only where
/// it is said of the file, so a comment about generated IDs is not one.
fn says_generated(comment: &str) -> bool {
    if comment.contains("DO NOT EDIT") || comment.contains("@auto-generated") {
        return true;
    }
    let lower = comment.to_ascii_lowercase();
    lower.starts_with("automatically generated")
        || [
            "file is automatically generated",
            "file was automatically generated",
            "file has been automatically generated",
            "code is automatically generated",
            "code was automatically generated",
            "code has been automatically generated",
        ]
        .iter()
        .any(|phrase| lower.contains(phrase))
}

/// The text of each comment line in `head`, without its comment marker. A line inside
/// a `/* */` or `<!-- -->` block counts, so a banner spread over a block is read whole.
fn comment_lines(head: &str) -> impl Iterator<Item = &str> {
    const OPENERS: [&str; 9] = ["//", "#", "/*", "*", "--", "<!--", ";", "\"\"\"", "'''"];
    let mut in_block = false;
    head.lines().filter_map(move |line| {
        let trimmed = line.trim_start();
        let continued = in_block;
        if trimmed.starts_with("/*") || trimmed.starts_with("<!--") {
            in_block = true;
        }
        if in_block && (trimmed.contains("*/") || trimmed.contains("-->")) {
            in_block = false;
        }
        (continued || OPENERS.iter().any(|opener| trimmed.starts_with(opener))).then(|| {
            trimmed
                .trim_start_matches(|c: char| "/*#-<!;\"'".contains(c))
                .trim()
        })
    })
}

/// A line longer than this is not one a person wrote. Set high, so a long string or
/// data table in hand-written code stays well under it.
const LONGEST_LINE: usize = 5_000;
/// Lines this long on average are a bundle broken into a few lines.
const AVERAGE_LINE: usize = 300;
/// An average over fewer lines says too little: three long imports are not a bundle.
const AVERAGE_OVER: usize = 5;

/// Why `source` reads as minified, or `None` when it reads as written by hand.
/// Bytes are counted first and characters only past a limit, so the common case costs
/// one pass with no decoding.
pub fn minified(source: &str) -> Option<String> {
    let (mut lines, mut bytes, mut longest) = (0usize, 0usize, "");
    for line in source.lines() {
        lines += 1;
        bytes += line.len();
        if line.len() > longest.len() {
            longest = line;
        }
    }
    if longest.len() > LONGEST_LINE {
        let characters = longest.chars().count();
        if characters > LONGEST_LINE {
            return Some(format!("a line of {characters} characters"));
        }
    }
    if lines >= AVERAGE_OVER && bytes / lines > AVERAGE_LINE {
        let characters = source
            .lines()
            .map(|line| line.chars().count())
            .sum::<usize>();
        let average = characters / lines;
        if average > AVERAGE_LINE {
            return Some(format!("its {lines} lines average {average} characters"));
        }
    }
    None
}

/// The submodules a walk from `root` would enter, as the paths the walker yields for
/// them. `.gitmodules` is read at `root` and at the repository root above it, whose
/// entries are relative to that root. A submodule that holds `root` is never one: a
/// scan started inside a submodule is a scan of that submodule.
pub fn submodules_under(root: &Path) -> Vec<PathBuf> {
    let Ok(start) = rqx_core::paths::canonicalize(root) else {
        return Vec::new();
    };
    let repository = start
        .ancestors()
        .find(|directory| directory.join(".git").exists());
    let mut found: Vec<PathBuf> = std::iter::once(start.as_path())
        .chain(repository.filter(|repository| *repository != start))
        .flat_map(|listed_at| {
            let listing =
                std::fs::read_to_string(listed_at.join(".gitmodules")).unwrap_or_default();
            submodule_paths(&listing)
                .filter_map(|path| {
                    let absolute = path
                        .split('/')
                        .fold(listed_at.to_path_buf(), |at, part| at.join(part));
                    let inside = absolute.strip_prefix(&start).ok()?;
                    (!inside.as_os_str().is_empty()).then(|| root.join(inside))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The `path = …` values of a `.gitmodules` file, which is in git's config format.
fn submodule_paths(listing: &str) -> impl Iterator<Item = &str> {
    listing.lines().filter_map(|line| {
        let (key, value) = line.split_once('=')?;
        let path = value.trim().trim_matches('"').trim_end_matches('/');
        (key.trim().eq_ignore_ascii_case("path") && !path.is_empty()).then_some(path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rqx-skip-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        rqx_core::paths::canonicalize(&root).unwrap()
    }

    #[test]
    fn a_marker_past_a_wide_character_is_still_found() {
        let source = format!("{}\n// @generated\n", "\u{4e16}".repeat(2000));
        assert_eq!(generated_by(&source), None);
        assert_eq!(
            generated_by("// @generated\nfn main() {}"),
            Some("generated code")
        );
    }

    #[test]
    fn a_handwritten_file_is_not_claimed() {
        assert_eq!(generated_by("fn main() {}"), None);
    }

    #[test]
    fn each_header_marker_in_a_comment_is_generated_code() {
        for header in [
            "// DO NOT EDIT: this file is rebuilt by make schema\n",
            "# Automatically generated by SWIG\n",
            "/* This file was automatically generated by protoc-gen-ts. */\n",
            "/**\n * @auto-generated by graphql-codegen\n */\n",
            "/*\n   THIS FILE IS AUTOMATICALLY GENERATED\n*/\n",
            "\"\"\"Automatically generated client; DO NOT EDIT.\"\"\"\n",
        ] {
            let source = format!("{header}export const a = 1;\n");
            assert_eq!(generated_by(&source), Some("generated code"), "{header}");
        }
    }

    #[test]
    fn each_bundler_runtime_is_named() {
        for (head, bundler) in [
            (
                "/******/ (() => { // webpackBootstrap\n/******/ \tvar __webpack_modules__ = ({});\n",
                "webpack bundle",
            ),
            (
                "/******/ \tfunction __webpack_require__(moduleId) {\n",
                "webpack bundle",
            ),
            (
                "var __commonJS = (cb, mod) => function __require() {\n",
                "esbuild bundle",
            ),
            (
                "var __toESM = (mod, isNodeMode, target) => (target = {});\n",
                "esbuild bundle",
            ),
            (
                "function getDefaultExportFromCjs (x) {\n\treturn x;\n}\n",
                "rollup bundle",
            ),
        ] {
            assert_eq!(generated_by(head), Some(bundler), "{head}");
        }
    }

    #[test]
    fn a_marker_past_the_header_is_prose_and_not_claimed() {
        let filler = "export const value = 1;\n".repeat(200);
        for marker in [
            "// DO NOT EDIT this list without telling the payments team\n",
            "// @auto-generated\n",
            "// Automatically generated ids are unique per tenant\n",
            "/******/ (() => { // webpackBootstrap\n",
        ] {
            let source = format!("{filler}{marker}");
            assert_eq!(generated_by(&source), None, "{marker}");
        }
    }

    #[test]
    fn a_marker_outside_a_comment_or_said_of_something_else_is_not_claimed() {
        for source in [
            "export const banner = \"DO NOT EDIT\";\n",
            "// do not edit the fixtures by hand\n",
            "// The ID is automatically generated by the database.\n",
            "# Order numbers are automatically generated on save.\n",
            "__webpack_require__.p = window.assetsRoot;\n",
            "const notes = \"var __toESM is esbuild's helper\";\n",
        ] {
            assert_eq!(generated_by(source), None, "{source}");
        }
    }

    #[test]
    fn one_line_over_five_thousand_characters_is_minified() {
        let source = format!("var a={};\n", "1,".repeat(3_000));
        assert_eq!(
            minified(&source).as_deref(),
            Some("a line of 6007 characters")
        );
    }

    #[test]
    fn ten_lines_averaging_four_hundred_characters_are_minified() {
        let source = format!("var a=\"{}\";\n", "x".repeat(391)).repeat(10);
        assert_eq!(
            minified(&source).as_deref(),
            Some("its 10 lines average 400 characters")
        );
    }

    #[test]
    fn a_long_string_in_hand_written_code_is_not_minified() {
        let mut source = String::from("export function load() {\n  return 1;\n}\n\n");
        source.push_str(&format!("export const blob = \"{}\";\n", "A".repeat(3_000)));
        source.push_str(&"export const a = 1;\n".repeat(20));
        assert_eq!(minified(&source), None);
    }

    #[test]
    fn a_few_long_lines_are_too_few_to_average() {
        let source = format!("import {{ {} }} from \"./names\";\n", "name, ".repeat(100)).repeat(4);
        assert_eq!(minified(&source), None);
    }

    #[test]
    fn wide_characters_are_counted_as_characters_not_bytes() {
        // 2,000 three-byte characters: 6,000 bytes, but a 2,000-character line.
        let source = format!("const s = \"{}\";\n", "\u{4e2d}".repeat(2_000));
        assert_eq!(minified(&source), None);
    }

    #[test]
    fn gitmodules_paths_are_read_with_or_without_quotes_and_slashes() {
        let listing = "[submodule \"design\"]\n\tpath = design\n\turl = ../design.git\n\
                       [submodule \"lib\"]\n\tpath = \"vendor/lib/\"\n";
        assert_eq!(
            submodule_paths(listing).collect::<Vec<_>>(),
            ["design", "vendor/lib"]
        );
    }

    #[test]
    fn submodules_are_found_from_the_root_and_from_a_subdirectory_but_not_from_inside() {
        let root = scratch("submodules");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("app/design/src")).unwrap();
        std::fs::write(
            root.join(".gitmodules"),
            "[submodule \"design\"]\n\tpath = app/design\n",
        )
        .unwrap();

        let from_root = submodules_under(&root);
        let from_app = submodules_under(&root.join("app"));
        let from_inside = submodules_under(&root.join("app/design/src"));
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(from_root, [root.join("app").join("design")]);
        assert_eq!(from_app, [root.join("app").join("design")]);
        assert!(from_inside.is_empty(), "{from_inside:?}");
    }

    fn entry(path: &str, reason: SkipReason) -> Skipped {
        Skipped::file(Path::new(path), reason, format!("{reason:?}"))
    }

    #[test]
    fn the_summary_names_each_submodule_then_counts_the_rest_in_a_fixed_order() {
        let root = Path::new("project");
        let skipped = [
            entry("project/big.ts", SkipReason::TooLarge),
            entry("project/a.pb.ts", SkipReason::Generated),
            entry("project/b.pb.ts", SkipReason::Generated),
            Skipped {
                files: 398,
                ..entry("project/design", SkipReason::Submodule)
            },
        ];
        assert_eq!(
            summary(&skipped, root).as_deref(),
            Some("skipped 401 files: 398 in submodule design/, 2 generated, 1 too large")
        );
        assert_eq!(summary(&[], root), None);
    }

    #[test]
    fn the_tally_is_keyed_by_slug_and_names_the_submodules() {
        let root = Path::new("project");
        let skipped = [
            entry("project/slow.ts", SkipReason::ParseTimeout),
            Skipped {
                files: 12,
                ..entry("project/vendor/lib", SkipReason::Submodule)
            },
        ];
        assert_eq!(
            serde_json::to_string(&tally(&skipped, root)).unwrap(),
            r#"{"parse-timeout":{"files":1},"submodule":{"files":12,"paths":["vendor/lib"]}}"#
        );
    }

    #[test]
    fn a_listed_entry_keeps_the_path_colon_reason_shape() {
        let skipped = Skipped::file(
            Path::new("src/big.ts"),
            SkipReason::TooLarge,
            "2048 KB exceeds the 1024 KB limit".to_owned(),
        );
        assert_eq!(
            skipped.listed(),
            "src/big.ts: 2048 KB exceeds the 1024 KB limit"
        );
    }
}

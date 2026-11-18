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
    /// Marked `linguist-generated` or `linguist-vendored` in `.gitattributes`.
    Gitattributes,
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
            Self::Gitattributes => "gitattributes",
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
            Self::Gitattributes => "marked in .gitattributes",
            Self::TooLarge => "too large",
            Self::Unreadable => "unreadable",
            Self::ParseTimeout => "too slow to parse",
            Self::Unsupported => "unsupported",
            Self::Walk => "unreachable",
        }
    }

    /// The setting that brings files skipped for this reason back, when one does.
    const fn setting(self) -> Option<&'static str> {
        match self {
            Self::Submodule => Some("scan.skip_submodules: none"),
            Self::Minified => Some("scan.skip_minified: false"),
            Self::Generated => Some("scan.skip_generated: false"),
            Self::Gitattributes => Some("scan.respect_gitattributes: false"),
            Self::TooLarge => Some("a larger scan.max_filesize"),
            Self::Unreadable | Self::ParseTimeout | Self::Unsupported | Self::Walk => None,
        }
    }
}

/// The settings that would bring the skipped files back, one per reason, in the order
/// the summary names the reasons.
pub fn settings_to_include(skipped: &[Skipped]) -> Vec<&'static str> {
    let reasons: std::collections::BTreeSet<SkipReason> =
        skipped.iter().map(|entry| entry.reason).collect();
    reasons
        .into_iter()
        .filter_map(SkipReason::setting)
        .collect()
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
    /// `host/organisation` of a submodule hosted elsewhere than the project.
    pub foreign: Option<String>,
}

impl Skipped {
    pub fn file(path: &Path, reason: SkipReason, detail: String) -> Self {
        Self {
            path: path.to_owned(),
            reason,
            detail,
            files: 1,
            foreign: None,
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
/// foreign submodule design/ (github.com/other-org), 14 minified`. Every submodule is named, so a reader can see which
/// to bring back. `None` when nothing was skipped.
pub fn summary(skipped: &[Skipped], root: &Path) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }
    let mut by_reason: BTreeMap<SkipReason, u32> = BTreeMap::new();
    let mut submodules = Vec::new();
    for entry in skipped {
        if entry.reason == SkipReason::Submodule {
            let directory = super::relative_to(root, &entry.path);
            submodules.push(match &entry.foreign {
                Some(hosted) => format!(
                    "{} in foreign submodule {directory}/ ({hosted})",
                    entry.files
                ),
                None => format!("{} in submodule {directory}/", entry.files),
            });
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

/// A line longer than this is wide. A file is judged minified only when at least half
/// of its lines are wide: a hand-written file that embeds a long SVG path or data URL
/// has one or two wide lines among many ordinary ones.
const WIDE_LINE: usize = 300;
/// A line at least this long counts towards the share of a file held in long lines.
const LONG_LINE: usize = 1_000;
/// A file whose longest line passes this, with long lines holding most of it, is not
/// one a person wrote.
const LONGEST_LINE: usize = 4_000;
/// The share of a file's characters that long lines must hold, in percent.
const LONG_SHARE: usize = 80;
/// Lines this long on average are a bundle broken into a few lines.
const AVERAGE_LINE: usize = 300;
/// An average over fewer lines says too little: three long imports are not a bundle.
const AVERAGE_OVER: usize = 5;
/// Fewer lines than `AVERAGE_OVER` read as a bundle only when they are this long on
/// average, as a chunk a bundler wrapped at a few thousand characters is.
const SHORT_FILE_AVERAGE: usize = 1_000;

/// Why `source` reads as minified, or `None` when it reads as written by hand. Blank
/// lines are not counted, so spacing cannot move a file either way.
pub fn minified(source: &str) -> Option<String> {
    let lengths: Vec<usize> = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().count())
        .collect();
    let lines = lengths.len();
    let wide = lengths.iter().filter(|length| **length > WIDE_LINE).count();
    if lines == 0 || wide * 2 < lines {
        return None;
    }
    let characters: usize = lengths.iter().sum();
    let longest = lengths.iter().copied().max().unwrap_or_default();
    let long: usize = lengths.iter().filter(|length| **length >= LONG_LINE).sum();
    if longest > LONGEST_LINE && long * 100 >= characters * LONG_SHARE {
        return Some(format!("a line of {longest} characters"));
    }
    let average = characters / lines;
    let wide_on_average = if lines >= AVERAGE_OVER {
        average > AVERAGE_LINE
    } else {
        average > SHORT_FILE_AVERAGE
    };
    wide_on_average.then(|| format!("its {lines} lines average {average} characters"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn one_line_over_four_thousand_characters_of_code_is_minified() {
        let source = format!("var a=[{}];\n", "1,".repeat(3_000));
        assert_eq!(
            minified(&source).as_deref(),
            Some("a line of 6009 characters")
        );
    }

    #[test]
    fn ten_lines_averaging_four_hundred_characters_are_minified() {
        let source = format!("var a=f({});\n", "1,".repeat(195)).repeat(10);
        assert_eq!(
            minified(&source).as_deref(),
            Some("its 10 lines average 400 characters")
        );
    }

    #[test]
    fn a_bundled_chunk_of_three_wrapped_lines_is_minified() {
        let line = format!(
            "(self.webpackChunk=self.webpackChunk||[]).push([[1],{{{}}}]);\n",
            "a:1,".repeat(700)
        );
        let source = line.repeat(3);
        assert!(
            minified(&source).is_some_and(|why| why.starts_with("its 3 lines average")),
            "{:?}",
            minified(&source)
        );
    }

    #[test]
    fn an_icon_component_with_one_huge_path_is_not_minified() {
        let path = format!("M0 0{}Z", " L12.5 7.25".repeat(1_500));
        let mut source = String::from("import Svg, { Path } from \"react-native-svg\";\n\n");
        source.push_str("export function Icon({ color }: { color: string }) {\n  return (\n");
        source.push_str("    <Svg width={24} height={24} viewBox=\"0 0 24 24\">\n");
        source.push_str(&format!("      <Path fill={{color}} d=\"{path}\" />\n"));
        source.push_str("    </Svg>\n  );\n}\n");
        source.push_str(&"// a line of ordinary code\n".repeat(21));
        assert_eq!(source.lines().count(), 30);
        assert!(path.len() > 15_000);
        assert_eq!(minified(&source), None);
    }

    #[test]
    fn a_long_string_in_hand_written_code_is_not_minified() {
        let mut source = String::from("export function load() {\n  return 1;\n}\n\n");
        source.push_str(&format!("export const blob = \"{}\";\n", "A".repeat(3_000)));
        source.push_str(&"export const a = 1;\n".repeat(20));
        assert_eq!(minified(&source), None);
    }

    #[test]
    fn one_long_line_of_code_in_a_large_ordinary_file_is_not_minified() {
        let mut source = format!("var a=[{}];\n", "1,".repeat(3_000));
        source.push_str(&"export const value = compute(a, 1);\n".repeat(200));
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
        let source = format!("const s = f({});\n", "\u{4e2d}".repeat(2_000))
            + &"export const a = 1;\n".repeat(10);
        assert_eq!(minified(&source), None);
    }

    #[test]
    fn an_icon_with_two_long_paths_among_a_few_lines_is_not_minified() {
        let path = format!("<Path d=\"M0 0{}Z\" />", " L1 2".repeat(1_500));
        let source = format!(
            "import Svg from \"react-native-svg\";\n\nexport const Icon = () => (\n  <Svg>\n\
             {path}\n{path}\n  </Svg>\n);\n\nexport default Icon;\n"
        );
        assert_eq!(minified(&source), None);
    }

    fn entry(path: &str, reason: SkipReason) -> Skipped {
        Skipped::file(Path::new(path), reason, format!("{reason:?}"))
    }

    #[test]
    fn a_minified_line_after_a_banner_is_still_minified() {
        let source = format!(
            "/*! lib v1.2.3 | MIT */\n!function(){{{}}}();\n",
            "a();".repeat(2_000)
        );
        assert_eq!(
            minified(&source).as_deref(),
            Some("a line of 8016 characters")
        );
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
                foreign: Some("github.com/other-org".to_owned()),
                ..entry("project/design", SkipReason::Submodule)
            },
            Skipped {
                files: 20,
                ..entry("project/shared", SkipReason::Submodule)
            },
        ];
        assert_eq!(
            summary(&skipped, root).as_deref(),
            Some(
                "skipped 421 files: 398 in foreign submodule design/ (github.com/other-org), \
                 20 in submodule shared/, 2 generated, 1 too large"
            )
        );
        assert_eq!(summary(&[], root), None);
    }

    #[test]
    fn each_reason_with_a_setting_names_it_once() {
        let skipped = [
            entry("project/a.min.js", SkipReason::Minified),
            entry("project/b.min.js", SkipReason::Minified),
            entry("project/design", SkipReason::Submodule),
            entry("project/broken.ts", SkipReason::Unreadable),
            entry("project/api.ts", SkipReason::Gitattributes),
        ];
        assert_eq!(
            settings_to_include(&skipped),
            [
                "scan.skip_submodules: none",
                "scan.skip_minified: false",
                "scan.respect_gitattributes: false"
            ]
        );
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

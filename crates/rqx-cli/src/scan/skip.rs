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

//! The files a scan did not analyse, and why. A skip is always reported: a file that was
//! never read cannot be told apart from a clean one any other way.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Why a file was not analysed. The order is the order a summary names them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkipReason {
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
            Self::Generated => "generated",
            Self::TooLarge => "too-large",
            Self::Unreadable => "unreadable",
            Self::ParseTimeout => "parse-timeout",
            Self::Unsupported => "unsupported",
            Self::Walk => "walk",
        }
    }

    /// What follows the count in the one-line summary: `14 minified`.
    const fn label(self) -> &'static str {
        match self {
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
}

pub fn tally(skipped: &[Skipped]) -> BTreeMap<&'static str, Tally> {
    let mut tallies: BTreeMap<&'static str, Tally> = BTreeMap::new();
    for entry in skipped {
        tallies.entry(entry.reason.slug()).or_default().files += entry.files;
    }
    tallies
}

/// The one line a terminal report gives the skipped files: `skipped 14 files: 12
/// minified, 2 too large`. `None` when nothing was skipped.
pub fn summary(skipped: &[Skipped]) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }
    let mut by_reason: BTreeMap<SkipReason, u32> = BTreeMap::new();
    for entry in skipped {
        *by_reason.entry(entry.reason).or_default() += entry.files;
    }
    let total: u32 = by_reason.values().sum();
    let parts: Vec<String> = by_reason
        .iter()
        .map(|(reason, files)| format!("{files} {}", reason.label()))
        .collect();
    Some(format!(
        "skipped {}: {}",
        crate::report::plural(total, "file", "files"),
        parts.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, reason: SkipReason) -> Skipped {
        Skipped::file(Path::new(path), reason, format!("{reason:?}"))
    }

    #[test]
    fn the_summary_counts_files_per_reason_in_a_fixed_order() {
        let skipped = [
            entry("big.ts", SkipReason::TooLarge),
            entry("a.pb.ts", SkipReason::Generated),
            entry("b.pb.ts", SkipReason::Generated),
        ];
        assert_eq!(
            summary(&skipped).as_deref(),
            Some("skipped 3 files: 2 generated, 1 too large")
        );
        assert_eq!(summary(&[]), None);
    }

    #[test]
    fn the_tally_is_keyed_by_slug() {
        let skipped = [
            entry("slow.ts", SkipReason::ParseTimeout),
            entry("big.ts", SkipReason::TooLarge),
        ];
        let tallies = tally(&skipped);
        assert_eq!(
            serde_json::to_string(&tallies).unwrap(),
            r#"{"parse-timeout":{"files":1},"too-large":{"files":1}}"#
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

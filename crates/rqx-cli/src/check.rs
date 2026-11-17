use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Result, bail};
use rqx_core::config::RqxConfig;

use crate::config::Resolved;
use crate::gate::{Scope, Verdict};
use crate::history;
use crate::scan::ScanOutcome;

pub struct Review {
    pub outcome: ScanOutcome,
    pub reviewed: BTreeSet<String>,
    pub base: String,
}

impl Review {
    pub fn is_reviewed(&self, file: &str) -> bool {
        self.reviewed.contains(file)
    }

    pub fn findings(&self) -> impl Iterator<Item = &rqx_core::finding::Finding> {
        self.outcome
            .findings
            .iter()
            .filter(|finding| self.is_reviewed(&finding.location.file))
    }

    pub fn judge(&self, config: &RqxConfig) -> Verdict {
        crate::gate::judge(&self.outcome, Scope::Files(&self.reviewed), config)
    }
}

/// The whole project is scanned even though only part of it is reported on: an
/// export looks dead until the file that imports it is parsed, so narrowing the scan
/// to the diff would invent findings that the full graph disproves.
pub fn review(root: &Path, settings: &Resolved, base: &str) -> Result<Review> {
    review_watched(root, settings, base, None)
}

/// A review is a whole-project scan, so it has the same progress to report as one.
pub fn review_watched(
    root: &Path,
    settings: &Resolved,
    base: &str,
    watch: Option<crate::scan::Watch>,
) -> Result<Review> {
    let Some(mut changed) = history::changed_since(root, base) else {
        // The commonest cause by far in CI, and the one nothing in the message used
        // to point at: `base...HEAD` needs the commit the two share, and a clone one
        // commit deep has none.
        let shallow = if history::is_shallow(root) {
            " This clone is shallow, so it may hold no commit in common with `{base}` \
             \u{2014} fetch more history (`fetch-depth: 0`, `GIT_DEPTH: 0`)."
        } else {
            ""
        };
        bail!(
            "cannot diff against `{base}`. A review needs a git repository and a revision \
             that exists; try `--base HEAD~1`, or `rqx scan` to look at every file instead.{}",
            shallow.replace("{base}", base)
        );
    };
    changed.extend(history::uncommitted(root));

    let prefix = history::repository_prefix(root).unwrap_or_default();
    let changed: BTreeSet<String> = changed
        .into_iter()
        .filter_map(|path| path.strip_prefix(&prefix).map(str::to_owned))
        .collect();

    let outcome = match watch {
        Some(watch) => crate::scan::scan_with_progress(root, settings, watch)?,
        None => crate::scan::scan(root, settings)?,
    };

    // Direct dependents are reviewed too, because deleting an export is only visible
    // as a problem in the file that was importing it.
    let mut reviewed = changed.clone();
    for (importer, imported) in &outcome.import_edges {
        if imported.iter().any(|target| changed.contains(target)) {
            reviewed.insert(importer.clone());
        }
    }

    // A lockfile or a stray asset is a changed file, but it is not a file this tool
    // has an opinion about; counting it would overstate what was reviewed.
    reviewed.retain(|file| outcome.analyzed.contains(file));

    Ok(Review {
        outcome,
        reviewed,
        base: base.to_owned(),
    })
}

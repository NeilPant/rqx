use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use rqx_core::config::RqxConfig;

use crate::config::Resolved;
use crate::gate::{Scope, Verdict};
use crate::history;
use crate::progress::Mode;
use crate::reimplemented::Reference;
use crate::scan::{self, ScanOutcome};

/// What a review is shown in place of a revision when the files were listed instead.
const LISTED: &str = "--files-from";

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

/// Where the files under review come from. Git is the usual answer, and the wrong one
/// for a pipeline that has no history to ask, or already knows which files it means.
#[derive(Clone, Copy)]
pub enum Reviewed<'a> {
    /// What differs from this revision, and what is not yet committed.
    Since(&'a str),
    /// Exactly these files, named relative to the scan root.
    Listed(&'a BTreeSet<String>),
}

impl Reviewed<'_> {
    fn label(&self) -> String {
        match self {
            Self::Since(base) => (*base).to_owned(),
            Self::Listed(_) => LISTED.to_owned(),
        }
    }

    fn files(&self, root: &Path) -> Result<BTreeSet<String>> {
        match self {
            Self::Since(base) => changed_since(root, base),
            Self::Listed(files) => Ok((*files).clone()),
        }
    }
}

/// The whole project is scanned even though only part of it is reported on: an
/// export looks dead until the file that imports it is parsed, so narrowing the scan
/// to the diff would invent findings that the full graph disproves.
pub fn review(root: &Path, settings: &Resolved, base: &str) -> Result<Review> {
    review_of(root, settings, Reviewed::Since(base), None)
}

/// A review whose scan is drawn on stderr as `mode` says. Against a revision, the
/// revision is scanned too, because what the change added can only be told from what
/// was already there by reading both. A review of listed files has no revision.
pub fn review_drawn(
    root: &Path,
    settings: &Resolved,
    source: Reviewed<'_>,
    mode: Mode,
) -> Result<Review> {
    let Reviewed::Since(base) = source else {
        return crate::progress::drawing(mode, |watch| review_of(root, settings, source, watch));
    };
    let changed = source.files(root)?;
    let reference = Arc::new(reference_at(root, settings, base, mode)?);
    let outcome = crate::progress::drawing(mode, |watch| {
        scan::scan_against(
            root,
            settings,
            watch,
            scan::CachePolicy::FromSettings,
            Some(Arc::clone(&reference)),
        )
    })?;
    Ok(assemble(changed, outcome, source.label()))
}

pub fn review_of(
    root: &Path,
    settings: &Resolved,
    source: Reviewed<'_>,
    watch: Option<crate::scan::Watch>,
) -> Result<Review> {
    let changed = source.files(root)?;

    let outcome = match watch {
        Some(watch) => crate::scan::scan_with_progress(root, settings, watch)?,
        None => crate::scan::scan(root, settings)?,
    };
    Ok(assemble(changed, outcome, source.label()))
}

/// What the base revision had, read from a checkout of the revision the change forked
/// from. The base is scanned with the same settings as the change, so the two sides
/// are judged by the same rules.
fn reference_at(root: &Path, settings: &Resolved, base: &str, mode: Mode) -> Result<Reference> {
    let forked_from = history::merge_base(root, base).unwrap_or_else(|| base.to_owned());
    let worktree = history::Worktree::add(root, &forked_from)
        .map_err(|error| anyhow::anyhow!("cannot read `{base}`: {error}"))?;
    let mut rebased = settings
        .rebased(root, worktree.path())
        .context("re-reading the configuration against the base checkout")?;
    // Other tools' results describe the working tree, so they are not read from the
    // base checkout, where the files they name are not.
    rebased.config.scan.sarif_in.clear();
    let before =
        scan::scan_drawn(worktree.path(), &rebased, mode).context("scanning the base revision")?;
    Ok(Reference::of(&before.shape_by_file))
}

fn assemble(changed: BTreeSet<String>, outcome: ScanOutcome, base: String) -> Review {
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

    Review {
        outcome,
        reviewed,
        base,
    }
}

/// Paths relative to the scan root, as findings name them.
fn changed_since(root: &Path, base: &str) -> Result<BTreeSet<String>> {
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
             that exists; try `--base HEAD~1`, `--files-from` to name the files, or \
             `rqx scan` to look at every file instead.{}",
            shallow.replace("{base}", base)
        );
    };
    changed.extend(history::uncommitted(root));

    let prefix = history::repository_prefix(root).unwrap_or_default();
    Ok(changed
        .into_iter()
        .filter_map(|path| path.strip_prefix(&prefix).map(str::to_owned))
        .collect())
}

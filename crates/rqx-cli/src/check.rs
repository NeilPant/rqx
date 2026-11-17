use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use rqx_core::config::{RegressionGate, RqxConfig};

use crate::config::Resolved;
use crate::credit::Credit;
use crate::gate::{Scope, Verdict};
use crate::history;
use crate::progress::Mode;
use crate::regressions::Regressions;
use crate::reimplemented::Reference;
use crate::scan::{self, ScanOutcome};

/// What a review is shown in place of a revision when the files were listed instead.
const LISTED: &str = "--files-from";
const STAGED: &str = "--staged";

pub struct Review {
    pub outcome: ScanOutcome,
    pub reviewed: BTreeSet<String>,
    pub base: String,
    /// What the change paid off, when the base revision was scanned.
    pub credit: Option<Credit>,
    /// The functions that cost more than they did at the base revision. `None` when
    /// the review was not asked to look.
    pub regressions: Option<Regressions>,
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

/// What a review is asked for beyond the settings.
#[derive(Clone, Copy, Default)]
pub struct Options {
    /// Report the functions that cost more than they did at the base revision, even
    /// when `gate.complexity_regression` is `off`.
    pub regressions: bool,
}

impl Options {
    fn compares_complexity(self, settings: &Resolved) -> bool {
        self.regressions || settings.config.gate.complexity_regression != RegressionGate::Off
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
    /// Exactly the staged files, named relative to the scan root. The root is the
    /// overlay `staged::Overlay` built, so the scan reads the index and not the
    /// working tree.
    Staged(&'a BTreeSet<String>),
}

impl Reviewed<'_> {
    fn label(&self) -> String {
        match self {
            Self::Since(base) => (*base).to_owned(),
            Self::Listed(_) => LISTED.to_owned(),
            Self::Staged(_) => STAGED.to_owned(),
        }
    }

    fn files(&self, root: &Path) -> Result<BTreeSet<String>> {
        match self {
            Self::Since(base) => changed_since(root, base),
            Self::Listed(files) | Self::Staged(files) => Ok((*files).clone()),
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
/// revision is scanned too when the change adds a function, because what it added can
/// only be told from what was already there by reading both. A change that adds none
/// has nothing to compare, so it is reviewed on one scan, and its credit is read from
/// the files it touched alone. A review of listed files has no revision.
///
/// The functions that cost more than at the base are looked for when the gate says to
/// or `options` does. They are worked out from the base's text for the files the change
/// touched, so asking does not add a scan of the base.
pub fn review_drawn(
    root: &Path,
    settings: &Resolved,
    source: Reviewed<'_>,
    mode: Mode,
    options: Options,
) -> Result<Review> {
    let compares = options.compares_complexity(settings);
    let gate = settings.config.gate.complexity_regression;
    let Reviewed::Since(base) = source else {
        let mut review = crate::progress::drawing(mode, |watch| {
            review_keeping(root, settings, source, watch, compares)
        })?;
        review.regressions = compares.then(|| Regressions::without_base(gate));
        return Ok(review);
    };
    let changed = source.files(root)?;
    let forked_from = history::merge_base(root, base).unwrap_or_else(|| base.to_owned());
    let held = held_at(root, &forked_from, &changed);
    let mut review = if adds_functions(root, settings, &held, &changed) {
        let before = scan_base(root, settings, &forked_from, mode)?;
        let reference = Arc::new(Reference::of(&before.outcome.shape_by_file));
        let outcome = crate::progress::drawing(mode, |watch| {
            scan::scan_against(
                root,
                settings,
                watch,
                scan::CachePolicy::FromSettings,
                scan::Comparison {
                    reference: Some(reference),
                    keep_costs: compares,
                },
                None,
            )
        })?;
        let credit = Credit::between(&before.outcome, &outcome);
        assemble(changed.clone(), outcome, source.label(), credit)
    } else {
        let mut review = crate::progress::drawing(mode, |watch| {
            review_keeping(root, settings, source, watch, compares)
        })?;
        review.credit = Credit::in_files(&held, root, settings, &review.outcome);
        review
    };
    review.regressions =
        compares.then(|| Regressions::between(&review.outcome, &held, &changed, root, settings));
    Ok(review)
}

pub fn review_of(
    root: &Path,
    settings: &Resolved,
    source: Reviewed<'_>,
    watch: Option<crate::scan::Watch>,
) -> Result<Review> {
    review_keeping(root, settings, source, watch, false)
}

/// One scan of the project, which keeps the cost facts when a comparison will read them.
fn review_keeping(
    root: &Path,
    settings: &Resolved,
    source: Reviewed<'_>,
    watch: Option<crate::scan::Watch>,
    keep_costs: bool,
) -> Result<Review> {
    let changed = source.files(root)?;
    let outcome = scan::scan_against(
        root,
        settings,
        watch,
        scan::CachePolicy::FromSettings,
        scan::Comparison {
            reference: None,
            keep_costs,
        },
        None,
    )?;
    Ok(assemble(changed, outcome, source.label(), None))
}

/// What the files the change touched held at the base revision, as text: the changed
/// files and the ones it deleted, that have source rqx reads and existed then. Read
/// with `git show`, so no checkout is needed.
fn held_at(root: &Path, revision: &str, changed: &BTreeSet<String>) -> BTreeMap<String, String> {
    let prefix = history::repository_prefix(root).unwrap_or_default();
    let deleted = history::deleted_since(root, revision)
        .into_iter()
        .filter_map(|path| path.strip_prefix(&prefix).map(str::to_owned));
    changed
        .iter()
        .cloned()
        .chain(deleted)
        .filter(|file| rqx_core::Language::of_path(Path::new(file)).is_some())
        .filter_map(|file| {
            let text = history::file_at(root, revision, &file)?;
            Some((file, text))
        })
        .collect()
}

/// Whether any changed file holds a named function that the file did not hold at the
/// base revision, given what the files held then. Only a new or changed file can add
/// one, so the rest are not read. Both sides of each file are analysed on their own,
/// which costs a few files rather than a checkout of the project. A file that cannot
/// be told apart counts as adding one, because the answer "no" skips the comparison.
fn adds_functions(
    root: &Path,
    settings: &Resolved,
    held: &BTreeMap<String, String>,
    changed: &BTreeSet<String>,
) -> bool {
    changed
        .iter()
        .any(|file| file_adds_functions(root, settings, held.get(file), file))
}

fn file_adds_functions(root: &Path, settings: &Resolved, was: Option<&String>, file: &str) -> bool {
    let path = root.join(file);
    let Some(now) = std::fs::read_to_string(&path).ok() else {
        return false;
    };
    let config = settings.for_file(&path);
    let names_in = |source: &str| -> Result<BTreeSet<String>, rqx_lang::AnalysisError> {
        let analysis = rqx_lang::analyze(&path, file, source, config)?;
        Ok(analysis
            .shape
            .functions
            .into_iter()
            .filter_map(|function| function.name)
            .collect())
    };
    let had = match was {
        Some(source) => names_in(source).unwrap_or_default(),
        None => BTreeSet::new(),
    };
    match names_in(&now) {
        Ok(has) => has.difference(&had).next().is_some(),
        Err(rqx_lang::AnalysisError::UnsupportedFile) => false,
        Err(_) => true,
    }
}

/// The revision the change forked from, scanned in a checkout that lasts as long as
/// this value: reading a finding's text later needs the files to still be there. The
/// base is scanned with the same settings as the change, so the two sides are judged
/// by the same rules, and with no display cap, so a finding is never absent from it
/// only because there were too many.
struct Base {
    outcome: ScanOutcome,
    _checkout: history::Worktree,
}

fn scan_base(root: &Path, settings: &Resolved, revision: &str, mode: Mode) -> Result<Base> {
    let checkout = history::Worktree::add(root, revision)
        .map_err(|error| anyhow::anyhow!("cannot read `{revision}`: {error}"))?;
    let mut rebased = settings
        .rebased(root, checkout.path())
        .context("re-reading the configuration against the base checkout")?;
    // Other tools' results describe the working tree, so they are not read from the
    // base checkout, where the files they name are not.
    rebased.config.scan.sarif_in.clear();
    rebased.config.scan.max_findings = 0;
    // The base reads the working tree's cache and never writes it: an unchanged file
    // has the same content at both revisions, so its entry is the base's answer too,
    // and the base's own results are never kept where the next run of the working
    // tree could find them.
    let outcome = crate::progress::drawing(mode, |watch| {
        scan::scan_against(
            checkout.path(),
            &rebased,
            watch,
            scan::CachePolicy::FromSettings,
            scan::Comparison::default(),
            Some(root),
        )
    })
    .context("scanning the base revision")?;
    Ok(Base {
        outcome,
        _checkout: checkout,
    })
}

fn assemble(
    changed: BTreeSet<String>,
    outcome: ScanOutcome,
    base: String,
    credit: Option<Credit>,
) -> Review {
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
        credit,
        regressions: None,
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

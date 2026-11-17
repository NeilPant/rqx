use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result, bail};
use rqx_core::Identity;
use rqx_core::finding::{Finding, SourceRange};
use rqx_core::rule::RuleId;

use crate::config::Resolved;
use crate::history;
use crate::progress::Mode;
use crate::scan::{self, ScanOutcome};

/// What happened to one finding between the base revision and now.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// Gone, and nothing like it appeared elsewhere.
    Resolved,
    /// Still there, measuring the same.
    Unchanged,
    /// Still there, measuring less. The work started and did not finish.
    Improved { from: u32, to: u32 },
    /// Still there, measuring more.
    Regressed { from: u32, to: u32 },
    /// Gone from where it was and reported somewhere else under the same name. The
    /// problem changed address rather than leaving.
    Relocated { to: SourceRange },
    /// Not there before.
    Introduced,
}

impl Outcome {
    pub const fn slug(&self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Unchanged => "unchanged",
            Self::Improved { .. } => "improved",
            Self::Regressed { .. } => "regressed",
            Self::Relocated { .. } => "relocated",
            Self::Introduced => "introduced",
        }
    }

    /// Whether this is the answer a reviewer wanted. Relocation is not: a helper that
    /// now carries the same finding is the same problem with a new address.
    pub const fn is_progress(&self) -> bool {
        matches!(self, Self::Resolved | Self::Improved { .. })
    }

    pub const fn is_setback(&self) -> bool {
        matches!(
            self,
            Self::Introduced | Self::Regressed { .. } | Self::Relocated { .. }
        )
    }
}

/// Whether the change is consistent with the refactoring rqx named. Only what syntax
/// can see: a function count that rose where the rule asked for an extraction is
/// evidence, and everything else says so rather than guessing.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Consistency {
    Consistent,
    Inconsistent,
    NotInferable,
}

impl Consistency {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Consistent => "consistent",
            Self::Inconsistent => "inconsistent",
            Self::NotInferable => "not inferable",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Change {
    pub rule: RuleId,
    /// What the rule is called. Held rather than derived, because a rule the project
    /// wrote has its name on the finding and the finding is gone by the time this is
    /// rendered.
    pub slug: String,
    pub subject: String,
    pub before: Option<SourceRange>,
    pub after: Option<SourceRange>,
    pub outcome: Outcome,
    pub technique: Consistency,
}

pub struct Verified {
    pub base: String,
    pub changes: Vec<Change>,
    pub health_before: u32,
    pub health_after: u32,
    /// Rules named with `--expect` that were not resolved.
    pub unmet: Vec<String>,
}

impl Verified {
    pub fn counted(&self, outcome: &Outcome) -> usize {
        self.changes
            .iter()
            .filter(|change| {
                std::mem::discriminant(&change.outcome) == std::mem::discriminant(outcome)
            })
            .count()
    }

    pub fn passed(&self) -> bool {
        self.unmet.is_empty()
            && !self
                .changes
                .iter()
                .any(|change| change.outcome.is_setback())
    }
}

/// One finding, keyed by what it is about rather than where it sits.
struct Keyed<'a> {
    identity: Identity,
    finding: &'a Finding,
    subject: String,
    magnitude: Option<u32>,
}

fn key<'a>(outcome: &'a ScanOutcome, sources: &mut HashMap<String, String>) -> Vec<Keyed<'a>> {
    outcome
        .findings
        .iter()
        .map(|finding| {
            let file = finding.location.file.to_string();
            let source = sources.entry(file.clone()).or_insert_with(|| {
                std::fs::read_to_string(outcome.root.join(&file)).unwrap_or_default()
            });
            let subject = finding.evidence.subject().map_or_else(
                || {
                    rqx_core::span::Span::new(finding.location.start, finding.location.end)
                        .text(source)
                        .trim()
                        .to_owned()
                },
                ToOwned::to_owned,
            );
            Keyed {
                identity: Identity::of(finding.rule, &file, &subject),
                finding,
                subject,
                magnitude: finding.evidence.magnitude(),
            }
        })
        .collect()
}

pub fn verify(root: &Path, settings: &Resolved, base: &str, expect: &[String]) -> Result<Verified> {
    verify_drawn(root, settings, base, expect, Mode::hidden())
}

/// `verify`, with both scans' progress drawn on stderr as `progress` says.
pub fn verify_drawn(
    root: &Path,
    settings: &Resolved,
    base: &str,
    expect: &[String],
    progress: Mode,
) -> Result<Verified> {
    let worktree = history::Worktree::add(root, base).map_err(|error| {
        anyhow::anyhow!(
            "cannot read `{base}`: {error}\nrqx compares against a revision, so it needs a \
             git repository and a ref that resolves."
        )
    })?;

    // The worktree is a different directory, so path-scoped overrides written against
    // the real root would silently match nothing there.
    let rebased = settings
        .rebased(root, worktree.path())
        .context("re-reading the configuration against the base checkout")?;
    let before = scan::scan_drawn(worktree.path(), &rebased, progress)
        .context("scanning the base revision")?;
    let after = scan::scan_drawn(root, settings, progress).context("scanning the working tree")?;

    let changes = changes_between(&before, &after);

    let unmet = expect
        .iter()
        .filter(|wanted| {
            !changes
                .iter()
                .any(|change| change.outcome.is_progress() && matches(change, wanted))
        })
        .cloned()
        .collect();

    Ok(Verified {
        base: base.to_owned(),
        health_before: crate::report::Report::everything(&before).health(),
        health_after: crate::report::Report::everything(&after).health(),
        changes,
        unmet,
    })
}

/// What happened to every finding between two scans, sorted by rule and subject.
pub fn changes_between(before: &ScanOutcome, after: &ScanOutcome) -> Vec<Change> {
    let mut base_sources = HashMap::new();
    let mut head_sources = HashMap::new();
    let was = key(before, &mut base_sources);
    let is = key(after, &mut head_sources);

    let shapes = shape_of(before, after);
    let mut changes = Vec::new();
    let mut matched: BTreeSet<usize> = BTreeSet::new();

    let head_by_identity: BTreeMap<Identity, usize> = is
        .iter()
        .enumerate()
        .map(|(index, keyed)| (keyed.identity, index))
        .collect();

    for old in &was {
        if let Some(&index) = head_by_identity.get(&old.identity) {
            matched.insert(index);
            let new = &is[index];
            let outcome = match (old.magnitude, new.magnitude) {
                (Some(from), Some(to)) if to < from => Outcome::Improved { from, to },
                (Some(from), Some(to)) if to > from => Outcome::Regressed { from, to },
                _ => Outcome::Unchanged,
            };
            changes.push(Change {
                rule: old.finding.rule,
                slug: old.finding.slug().to_owned(),
                subject: old.subject.clone(),
                before: Some(old.finding.location.clone()),
                after: Some(new.finding.location.clone()),
                technique: Consistency::NotInferable,
                outcome,
            });
            continue;
        }

        // Gone from where it was. Before calling that resolved, look for the same
        // problem under the same name somewhere else: a body moved into a helper
        // takes its complexity with it, and reporting that as fixed would be a lie
        // a reviewer has no way to catch.
        let moved = is.iter().enumerate().find(|(index, new)| {
            !matched.contains(index)
                && new.finding.rule == old.finding.rule
                && new.subject == old.subject
                && new.finding.location.file != old.finding.location.file
        });
        if let Some((index, new)) = moved {
            matched.insert(index);
            changes.push(Change {
                rule: old.finding.rule,
                slug: old.finding.slug().to_owned(),
                subject: old.subject.clone(),
                before: Some(old.finding.location.clone()),
                after: Some(new.finding.location.clone()),
                outcome: Outcome::Relocated {
                    to: new.finding.location.clone(),
                },
                technique: Consistency::NotInferable,
            });
            continue;
        }

        let technique = consistency(old.finding, &shapes);
        changes.push(Change {
            rule: old.finding.rule,
            slug: old.finding.slug().to_owned(),
            subject: old.subject.clone(),
            before: Some(old.finding.location.clone()),
            after: None,
            outcome: Outcome::Resolved,
            technique,
        });
    }

    for (index, new) in is.iter().enumerate() {
        if matched.contains(&index) {
            continue;
        }
        changes.push(Change {
            rule: new.finding.rule,
            slug: new.finding.slug().to_owned(),
            subject: new.subject.clone(),
            before: None,
            after: Some(new.finding.location.clone()),
            outcome: Outcome::Introduced,
            technique: Consistency::NotInferable,
        });
    }

    changes.sort_by(|left, right| {
        left.rule
            .slug()
            .cmp(&right.slug)
            .then(left.subject.cmp(&right.subject))
    });
    changes
}

/// `rule` or `rule@file`, so a reviewer can ask for a specific finding rather than
/// for any instance of a rule.
fn matches(change: &Change, wanted: &str) -> bool {
    match wanted.split_once('@') {
        Some((rule, file)) => {
            change.slug == rule
                && change
                    .before
                    .as_ref()
                    .is_some_and(|range| &*range.file == file)
        }
        None => change.slug == wanted,
    }
}

/// How many functions, classes and lines each file has on each side. A rule asking
/// for an extraction is answered by a file that gained a function, and contradicted
/// by one that did not.
struct Shape {
    functions_rose: BTreeSet<String>,
    classes_rose: BTreeSet<String>,
    shrank: BTreeSet<String>,
    files_gone: BTreeSet<String>,
}

fn shape_of(before: &ScanOutcome, after: &ScanOutcome) -> Shape {
    let mut shape = Shape {
        functions_rose: BTreeSet::new(),
        classes_rose: BTreeSet::new(),
        shrank: BTreeSet::new(),
        files_gone: BTreeSet::new(),
    };
    for (file, lines) in &before.lines_by_file {
        let name = file.to_string();
        match after.lines_by_file.get(file) {
            Some(now) if now < lines => {
                shape.shrank.insert(name);
            }
            None => {
                shape.files_gone.insert(name);
            }
            _ => {}
        }
    }
    // A new file that did not exist at the base is where an extraction lands.
    for file in after.lines_by_file.keys() {
        if !before.lines_by_file.contains_key(file) {
            shape.functions_rose.insert(file.to_string());
            shape.classes_rose.insert(file.to_string());
        }
    }
    // An extraction that stayed in the file it came from is the common case, and a
    // file-level count is the only thing that can see it.
    for (file, now) in &after.shape_by_file {
        let was = before.shape_by_file.get(file);
        let count = |shape: Option<&rqx_lang::FileShape>, of: fn(&rqx_lang::FileShape) -> usize| {
            shape.map_or(0, of)
        };
        if now.functions.len() > count(was, |shape| shape.functions.len()) {
            shape.functions_rose.insert(file.to_string());
        }
        if now.classes.len() > count(was, |shape| shape.classes.len()) {
            shape.classes_rose.insert(file.to_string());
        }
    }
    shape
}

fn consistency(finding: &Finding, shape: &Shape) -> Consistency {
    use rqx_core::technique::FixTechnique as Fix;
    let file = finding.location.file.to_string();
    let Some(technique) = finding.rule.spec().fixes.first() else {
        return Consistency::NotInferable;
    };
    match technique {
        Fix::RemoveDeadCode => {
            if shape.shrank.contains(&file) || shape.files_gone.contains(&file) {
                Consistency::Consistent
            } else {
                Consistency::Inconsistent
            }
        }
        Fix::DeleteFile => {
            if shape.files_gone.contains(&file) {
                Consistency::Consistent
            } else {
                Consistency::Inconsistent
            }
        }
        Fix::ExtractMethod | Fix::ExtractSharedModule | Fix::ExtractClass => {
            if shape.functions_rose.is_empty() && shape.classes_rose.is_empty() {
                Consistency::Inconsistent
            } else {
                Consistency::Consistent
            }
        }
        _ => Consistency::NotInferable,
    }
}

pub fn ensure_repository(root: &Path, base: &str) -> Result<()> {
    if history::changed_since(root, base).is_none() {
        bail!(
            "`{base}` is not a revision in this repository. rqx verify compares the working \
             tree against a base, so it needs a git repository and a ref that resolves."
        );
    }
    Ok(())
}

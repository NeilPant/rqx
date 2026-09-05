use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use rqx_core::edit::Edit;
use rqx_core::finding::Finding;
use rqx_core::score::FindingCounts;
use rqx_core::{Identity, Language};
use rqx_lang::rewrite::{Rewriter, Withheld};

use crate::config::Resolved;
use crate::report::Report;
use crate::scan::{self, ScanOutcome};

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub write: bool,
    pub max_rounds: u32,
    /// Files the run may touch. Empty means every file the scan reached; `check --fix`
    /// narrows it to the branch so a feature branch does not produce an unreviewable
    /// diff of the whole repository.
    pub only: BTreeSet<String>,
}

/// One file's proven edits, with the text before and after so a preview renders
/// straight from the bytes the fixer replaced rather than from a diff of its own.
#[derive(Clone, Debug)]
pub struct FileFix {
    pub file: String,
    pub before: String,
    pub after: String,
    pub applied: Vec<Edit>,
    pub dropped: Vec<Edit>,
    pub findings_after: Vec<Finding>,
}

#[derive(Clone, Debug)]
pub struct Withholding {
    pub file: String,
    pub edits: usize,
    pub reason: Withheld,
}

#[derive(Clone, Debug, Default)]
pub struct Round {
    pub fixes: Vec<FileFix>,
    pub withheld: Vec<Withholding>,
    /// Findings the round could not answer mechanically: the ones a person decides.
    pub undecided: u32,
    pub files_scanned: u32,
    /// Project-scoped findings the next scan reported that the previous one did not.
    /// One file's verification cannot see these, so they are named after the fact
    /// rather than silently left for the reader to discover.
    pub introduced: Vec<Finding>,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub rounds: Vec<Round>,
    pub health_before: u32,
    /// Exact after a write, because the final scan measured it; a projection from
    /// the verified per-file findings otherwise.
    pub health_after: u32,
    pub written: bool,
}

impl Outcome {
    pub fn fixes(&self) -> impl Iterator<Item = &FileFix> {
        self.rounds.iter().flat_map(|round| round.fixes.iter())
    }

    pub fn applied(&self) -> usize {
        self.fixes().map(|fix| fix.applied.len()).sum()
    }

    pub fn withheld(&self) -> impl Iterator<Item = &Withholding> {
        self.rounds.iter().flat_map(|round| round.withheld.iter())
    }

    pub fn introduced(&self) -> impl Iterator<Item = &Finding> {
        self.rounds.iter().flat_map(|round| round.introduced.iter())
    }

    /// Edits that lost an overlap. A fix the reader asked for and did not get is
    /// reported rather than dropped in silence.
    pub fn dropped(&self) -> impl Iterator<Item = &Edit> {
        self.fixes().flat_map(|fix| fix.dropped.iter())
    }
}

/// The edits one file's findings prove, and the text they produce.
///
/// The same plan and the same verification `run` performs, without the project scan
/// it starts with — so an editor or the interface can answer for one file in
/// milliseconds and still offer nothing the command line would withhold.
pub fn for_file(
    display: &str,
    source: &str,
    language: Language,
    findings: &[Finding],
    config: &rqx_core::RqxConfig,
) -> Option<(rqx_core::edit::Resolution, String)> {
    let allowed: Vec<Finding> = findings
        .iter()
        .filter(|finding| config.fix.allows(finding.rule))
        .cloned()
        .collect();
    if allowed.is_empty() {
        return None;
    }
    let rewriter = Rewriter::new(language, rqx_lang::holds_markup(display), source).ok()?;
    let plan = rewriter.plan(&allowed, &config.fix);
    if plan.is_empty() {
        return None;
    }
    rewriter.verify(display, config, &allowed, &plan).ok()?;
    let after = rqx_core::edit::apply(source, &plan)?;
    Some((plan, after))
}

pub fn run(root: &Path, settings: &Resolved, options: Options) -> Result<Outcome> {
    // The display cap keeps the highest-ranked findings, which are the least
    // mechanical; a fixer that only saw those would miss most of its own work.
    let mut settings = settings.clone();
    settings.config.scan.max_findings = 0;

    let mut scanned = scan::scan(root, &settings)?;
    let health_before = Report::everything(&scanned).health();
    let mut rounds: Vec<Round> = Vec::new();
    let mut projected;

    loop {
        let mut round = plan_round(root, &settings, &scanned, &options.only)?;
        let produced = !round.fixes.is_empty();

        if !options.write {
            projected = projected_health(&scanned, &round.fixes);
            rounds.push(round);
            break;
        }
        for fix in &round.fixes {
            write_atomically(&root.join(&fix.file), &fix.after)?;
        }
        let next = scan::scan(root, &settings)?;
        round.introduced = new_project_scoped(&scanned, &next);
        scanned = next;
        projected = Report::everything(&scanned).health();
        rounds.push(round);

        if !produced || rounds.len() as u32 >= options.max_rounds {
            break;
        }
    }

    Ok(Outcome {
        rounds,
        health_before,
        health_after: projected,
        written: options.write,
    })
}

fn plan_round(
    root: &Path,
    settings: &Resolved,
    scanned: &ScanOutcome,
    only: &BTreeSet<String>,
) -> Result<Round> {
    let mut by_file: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for finding in &scanned.findings {
        if !only.is_empty() && !only.contains(&*finding.location.file) {
            continue;
        }
        by_file
            .entry(&finding.location.file)
            .or_default()
            .push(finding);
    }

    let mut round = Round {
        files_scanned: scanned.files_scanned,
        undecided: scanned
            .findings
            .iter()
            .filter(|finding| !settings.config.fix.allows(finding.rule))
            .count() as u32,
        ..Round::default()
    };

    for (file, findings) in by_file {
        let path = root.join(file);
        if !findings
            .iter()
            .any(|finding| settings.for_file(&path).fix.allows(finding.rule))
        {
            continue;
        }
        let Some(language) = Language::of_path(&path) else {
            continue;
        };
        let before = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {} to fix it", path.display()))?;
        let owned: Vec<Finding> = findings.into_iter().cloned().collect();
        let rewriter = match Rewriter::new(language, rqx_lang::holds_markup(file), &before) {
            Ok(rewriter) => rewriter,
            Err(error) => {
                round.withheld.push(Withholding {
                    file: file.to_owned(),
                    edits: 0,
                    reason: Withheld::Unanalysable(error.to_string()),
                });
                continue;
            }
        };
        let plan = rewriter.plan(&owned, &settings.for_file(&path).fix);
        if plan.is_empty() {
            continue;
        }
        match rewriter.verify(file, settings.for_file(&path), &owned, &plan) {
            Ok(verified) => round.fixes.push(FileFix {
                file: file.to_owned(),
                before,
                after: verified.source,
                applied: plan.kept,
                dropped: plan.dropped,
                findings_after: verified.findings,
            }),
            Err(reason) => round.withheld.push(Withholding {
                file: file.to_owned(),
                edits: plan.kept.len(),
                reason,
            }),
        }
    }
    Ok(round)
}

/// The health the project would have with the verified files' findings replacing
/// the scanned ones. Project-scoped findings are carried over unchanged, since one
/// file's re-analysis says nothing about them.
fn projected_health(scanned: &ScanOutcome, fixes: &[FileFix]) -> u32 {
    let fixed: BTreeSet<&str> = fixes.iter().map(|fix| fix.file.as_str()).collect();
    let mut counts = FindingCounts::default();
    for finding in &scanned.findings {
        if !fixed.contains(&*finding.location.file) || finding.rule.is_project_scoped() {
            counts.record(finding);
        }
    }
    for fix in fixes {
        for finding in &fix.findings_after {
            counts.record(finding);
        }
    }
    counts.weighted_health_score(scanned.code_lines, &scanned.impact)
}

fn new_project_scoped(before: &ScanOutcome, after: &ScanOutcome) -> Vec<Finding> {
    let known: BTreeSet<Identity> = before
        .findings
        .iter()
        .filter(|finding| finding.rule.is_project_scoped())
        .map(Identity::exact)
        .collect();
    after
        .findings
        .iter()
        .filter(|finding| finding.rule.is_project_scoped())
        .filter(|finding| !known.contains(&Identity::exact(finding)))
        .cloned()
        .collect()
}

/// A crash between the two steps leaves the original in place, and the rename is
/// atomic on every filesystem that matters.
pub(crate) fn write_atomically(path: &Path, contents: &str) -> Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temporary = directory.join(format!(".{name}.rqx-tmp"));
    std::fs::write(&temporary, contents)
        .with_context(|| format!("writing {}", temporary.display()))?;
    if let Ok(metadata) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&temporary, metadata.permissions());
    }
    std::fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))
}

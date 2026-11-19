use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use rqx_core::edit::{Edit, Resolution};
use rqx_core::finding::Finding;
use rqx_core::rule::RuleId;
use rqx_core::score::FindingCounts;
use rqx_core::{Identity, Language};
use rqx_lang::rewrite::{Rewriter, Withheld};

use crate::config::Resolved;
use crate::progress::Mode;
use crate::report::Report;
use crate::scan::{self, ScanOutcome};

mod build;
mod package;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub write: bool,
    pub max_rounds: u32,
    /// Files the run may touch. Empty means every file the scan reached; `check --fix`
    /// narrows it to the branch so a feature branch does not produce an unreviewable
    /// diff of the whole repository.
    pub only: BTreeSet<String>,
    /// Plan edits for this rule alone. The other findings still take part in
    /// verification, so a narrowed fix is held to the same standard as a full one.
    pub rule: Option<RuleId>,
    /// Also run `go vet` on a copy of each Go module with the round's edits applied,
    /// and withhold the module's edits if it fails.
    pub verify_build: bool,
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
    /// The findings the applied edits answer, which verification found gone.
    pub resolved: Vec<Finding>,
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
    /// Findings with a mechanical fix that `fix.min_confidence` leaves to a person.
    pub below_floor: u32,
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

    /// Fixes `fix.min_confidence` withheld in the last round, which is what is still
    /// left once the earlier rounds were written.
    pub fn below_floor(&self) -> u32 {
        self.rounds.last().map_or(0, |round| round.below_floor)
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

pub fn run(root: &Path, settings: &Resolved, options: Options) -> Result<Outcome> {
    run_drawn(root, settings, options, Mode::hidden())
}

/// `run`, with the scans' progress drawn on stderr as `progress` says.
pub fn run_drawn(
    root: &Path,
    settings: &Resolved,
    options: Options,
    progress: Mode,
) -> Result<Outcome> {
    // The display cap keeps the highest-ranked findings, which are the least
    // mechanical; a fixer that only saw those would miss most of its own work.
    let mut settings = settings.clone();
    settings.config.scan.max_findings = 0;

    let mut scanned = scan::scan_drawn(root, &settings, progress)?;
    let health_before = Report::everything(&scanned).health();
    let mut rounds: Vec<Round> = Vec::new();
    let mut projected;

    loop {
        let mut round = plan_round(root, &settings, &scanned, &options)?;
        let produced = !round.fixes.is_empty();

        if !options.write {
            projected = projected_health(&scanned, &round.fixes);
            rounds.push(round);
            break;
        }
        for fix in &round.fixes {
            write_atomically(&root.join(&fix.file), &fix.after)?;
        }
        let next = scan::scan_drawn(root, &settings, progress)?;
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
    options: &Options,
) -> Result<Round> {
    let only = &options.only;
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
            .filter(|finding| {
                !settings
                    .for_file(&root.join(&*finding.location.file))
                    .fix
                    .admits(finding)
            })
            .count() as u32,
        ..Round::default()
    };

    for (file, findings) in by_file {
        let path = root.join(file);
        let fix = &settings.for_file(&path).fix;
        let admitted = findings
            .iter()
            .any(|finding| wanted(options, finding) && fix.admits(finding));
        let floored: Vec<&Finding> = findings
            .iter()
            .copied()
            .filter(|finding| {
                wanted(options, finding) && fix.allows(finding.rule) && fix.below_floor(finding)
            })
            .collect();
        if !admitted && floored.is_empty() {
            continue;
        }
        let Some(language) = Language::of_path(&path) else {
            continue;
        };
        if !admitted {
            round.below_floor += floor_fixes(&path, language, file, &floored);
            continue;
        }
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
        round.below_floor += answerable(&rewriter, &floored);
        let asked: Vec<Finding> = owned
            .iter()
            .filter(|finding| wanted(options, finding))
            .cloned()
            .collect();
        let plan = rewriter.plan(&asked, fix);
        if plan.is_empty() {
            continue;
        }
        match rewriter.verify(file, settings.for_file(&path), &owned, &plan) {
            Ok(verified) => round.fixes.push(FileFix {
                file: file.to_owned(),
                resolved: answered(&owned, &plan, &before),
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
    let fixes = std::mem::take(&mut round.fixes);
    let fixes = package::guard(root, &scanned.analyzed, fixes, &mut round.withheld);
    round.fixes = if options.verify_build {
        build::guard(root, fixes, &mut round.withheld)
    } else {
        fixes
    };
    Ok(round)
}

/// How many of a file's below-floor findings the fixer would have answered, for a
/// file with nothing else to fix. A file that cannot be read or parsed counts none.
fn floor_fixes(path: &Path, language: Language, file: &str, floored: &[&Finding]) -> u32 {
    let Ok(source) = std::fs::read_to_string(path) else {
        return 0;
    };
    let Ok(rewriter) = Rewriter::new(language, rqx_lang::holds_markup(file), &source) else {
        return 0;
    };
    answerable(&rewriter, floored)
}

fn answerable(rewriter: &Rewriter<'_>, findings: &[&Finding]) -> u32 {
    findings
        .iter()
        .filter(|finding| !rewriter.edit(finding).is_empty())
        .count() as u32
}

fn wanted(options: &Options, finding: &Finding) -> bool {
    options.rule.is_none_or(|rule| finding.rule == rule)
}

fn answered(findings: &[Finding], plan: &Resolution, source: &str) -> Vec<Finding> {
    let answers: BTreeSet<Identity> = plan.kept.iter().map(|edit| edit.answers).collect();
    findings
        .iter()
        .filter(|finding| answers.contains(&Identity::resolved(finding, source)))
        .cloned()
        .collect()
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

pub(crate) fn write_atomically(path: &Path, contents: &str) -> Result<()> {
    write_bytes_atomically(path, contents.as_bytes())
}

/// A crash between the two steps leaves the original in place, and the rename is
/// atomic on every filesystem that matters.
///
/// A symlink is written through, to the file it points at: renaming over the link
/// would turn a shared `rqx.yaml` or source file into a private copy. The temporary
/// file is new and named for this process and this write, so two runs saving into
/// one directory never share one, and it is removed when the write fails.
pub(crate) fn write_bytes_atomically(path: &Path, contents: &[u8]) -> Result<()> {
    static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temporary = directory.join(format!(
        ".{name}.{}-{}.rqx-tmp",
        std::process::id(),
        WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let written = write_new(&temporary, contents, &target).and_then(|()| {
        std::fs::rename(&temporary, &target)
            .with_context(|| format!("replacing {}", target.display()))
    });
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// The contents in a file that did not exist before, synced, with the permissions of
/// the file it is about to replace.
fn write_new(temporary: &Path, contents: &[u8], replacing: &Path) -> Result<()> {
    use std::io::Write as _;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .with_context(|| format!("writing {}", temporary.display()))?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .with_context(|| format!("writing {}", temporary.display()))?;
    if let Ok(metadata) = std::fs::metadata(replacing) {
        let _ = std::fs::set_permissions(temporary, metadata.permissions());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("rqx-atomic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory");
        directory
    }

    fn leftovers(directory: &Path) -> Vec<String> {
        std::fs::read_dir(directory)
            .expect("listed")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".rqx-tmp"))
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_written_through_to_the_file_it_points_at() {
        let directory = scratch("symlink");
        let shared = directory.join("shared.ts");
        let linked = directory.join("linked.ts");
        std::fs::write(&shared, "before\n").expect("a file");
        std::os::unix::fs::symlink(&shared, &linked).expect("a link");

        write_atomically(&linked, "after\n").expect("written");

        assert!(
            std::fs::symlink_metadata(&linked)
                .expect("still there")
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read_to_string(&shared).expect("read"), "after\n");
        assert!(leftovers(&directory).is_empty());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let directory = scratch("failed");
        // A directory cannot be replaced by a file, so the rename fails.
        let target = directory.join("occupied");
        std::fs::create_dir_all(target.join("inside")).expect("a directory");

        assert!(write_atomically(&target, "text").is_err());
        assert!(
            leftovers(&directory).is_empty(),
            "{:?}",
            leftovers(&directory)
        );
        let _ = std::fs::remove_dir_all(&directory);
    }
}

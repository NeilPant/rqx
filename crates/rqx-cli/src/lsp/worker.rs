use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver as Ticks, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Sender, bounded};
use lsp_server::{Message, RequestId, Response};
use rqx_core::finding::Finding;
use serde_json::json;

use crate::check;
use crate::config::Resolved;
use crate::report::{Report, projection};
use crate::scan::{self, ScanOutcome, ScanProgress};

use super::diagnostics::{FileFacts, Publisher};

#[derive(Clone)]
pub struct Job {
    pub config: Resolved,
    /// Set when a client asked for the scan and is waiting on a reply; unset for the
    /// scans the server runs on its own after startup and after a save.
    pub reply_to: Option<RequestId>,
    pub patterns: bool,
    pub task: Task,
    /// Files the editor holds unsaved. Their diagnostics belong to the buffer the user
    /// can see, so a scan reading them back off disk would republish findings at the
    /// wrong lines.
    pub dirty: Vec<String>,
}

/// What the worker is asked to do. A review is a scan that narrows what it reports
/// on — narrowing the scan itself would invent dead-export findings the full graph
/// disproves — and the rest are the CLI's other commands, run off the message loop
/// because each one scans.
#[derive(Clone)]
pub enum Task {
    Scan,
    Review { base: String },
    Fix { write: bool, only: BTreeSet<String> },
    Verify { base: String, expect: Vec<String> },
    Baseline { update: bool },
}

impl Task {
    /// Which waiting slot this task competes for: two scans answer one question, two
    /// reviews another, and the actions a person asked for by name a third.
    const fn slot(&self) -> Slot {
        match self {
            Self::Scan => Slot::Scan,
            Self::Review { .. } => Slot::Review,
            Self::Fix { .. } | Self::Verify { .. } | Self::Baseline { .. } => Slot::Action,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Scan,
    Review,
    Action,
}

/// Scanning happens off the message loop because a full scan of a large repository
/// takes seconds, and a loop that is scanning is a loop that answers no hover, no save
/// and no shutdown.
/// What a request that could not be queued should be told.
pub enum Refused {
    /// A newer request of the same kind replaced this one and will answer the same
    /// question, so this id is owed a reply now rather than a second identical scan.
    Coalesced(RequestId),
    /// The worker is gone, so no answer is ever coming from it.
    Dead(RequestId),
}

/// One waiting job of each kind. A single queue slot shared between them meant a review
/// arriving while a scan was queued had nowhere to go, and the caller was told the
/// server was busy — for two jobs that never actually competed.
#[derive(Default)]
struct Pending {
    scan: Option<Job>,
    review: Option<Job>,
    action: Option<Job>,
}

impl Pending {
    fn slot(&mut self, slot: Slot) -> &mut Option<Job> {
        match slot {
            Slot::Scan => &mut self.scan,
            Slot::Review => &mut self.review,
            Slot::Action => &mut self.action,
        }
    }
}

pub struct Worker {
    pending: Arc<Mutex<Pending>>,
    wake: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn spawn(publisher: Arc<Publisher>) -> Self {
        // The channel only wakes the worker; the jobs themselves live in `pending`, so
        // a queued job can be replaced by a newer one of its kind rather than piling up.
        let (wake, woken) = bounded::<()>(1);
        let pending = Arc::new(Mutex::new(Pending::default()));
        let thread = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || {
                for () in woken {
                    // Drained rather than one-per-wake: a wake can be dropped when the
                    // channel is already full, and a job left behind would wait for a
                    // wake that never comes.
                    while let Some(job) = take(&pending) {
                        match job.task.clone() {
                            Task::Scan => run(&publisher, &job),
                            Task::Review { base } => run_check(&publisher, &job, &base),
                            Task::Fix { write, only } => run_fix(&publisher, &job, write, only),
                            Task::Verify { base, expect } => {
                                run_verify(&publisher, &job, &base, &expect);
                            }
                            Task::Baseline { update } => run_baseline(&publisher, &job, update),
                        }
                    }
                }
            })
        };
        Self {
            pending,
            wake: Some(wake),
            thread: Some(thread),
        }
    }

    /// A job already queued makes a second one of the same kind redundant, so it
    /// coalesces rather than making the editor wait twice. A review never folds into a
    /// scan, because the two answer different questions and a client asking for one
    /// cannot read the other's reply.
    pub fn request(&self, job: Job) -> Option<Refused> {
        let Some(wake) = self.wake.as_ref() else {
            return job.reply_to.map(Refused::Dead);
        };
        let displaced = {
            let mut pending = self
                .pending
                .lock()
                .expect("the pending slots are only swapped");
            // A waiting job of this kind would answer the same question against older
            // state, so the newer request takes its place.
            pending.slot(job.task.slot()).replace(job)
        };
        // A full wake channel means the worker has one queued already and will look at
        // `pending` again before sleeping, so a dropped wake loses nothing.
        if wake.try_send(()).is_err() && wake.is_empty() {
            return displaced.and_then(|job| job.reply_to).map(Refused::Dead);
        }
        displaced
            .and_then(|job| job.reply_to)
            .map(Refused::Coalesced)
    }
}

/// Actions and reviews before scans: those are always something a person just asked
/// for, while a scan is usually the one that follows a save.
fn take(pending: &Mutex<Pending>) -> Option<Job> {
    let mut pending = pending.lock().expect("the pending slots are only swapped");
    pending
        .action
        .take()
        .or_else(|| pending.review.take())
        .or_else(|| pending.scan.take())
}

fn reply(publisher: &Publisher, job: &Job, value: serde_json::Value) {
    if let Some(id) = job.reply_to.clone() {
        publisher.send(Message::Response(Response::new_ok(id, value)));
    }
}

/// A preview answers with the edits and the text either side; a write puts them on
/// disk and then runs the ordinary scan, so the diagnostics and the report catch up
/// the same way they would after a save.
fn run_fix(publisher: &Arc<Publisher>, job: &Job, write: bool, only: BTreeSet<String>) {
    let options = crate::fix::Options {
        write,
        max_rounds: if write { 3 } else { 1 },
        only,
    };
    match crate::fix::run(publisher.root(), &job.config, options) {
        Ok(outcome) => {
            reply(publisher, job, projection::fixes(&outcome, !write));
            if write {
                run(
                    publisher,
                    &Job {
                        reply_to: None,
                        ..job.clone()
                    },
                );
            }
        }
        Err(error) => reply(publisher, job, json!({ "error": format!("{error:#}") })),
    }
}

fn run_verify(publisher: &Arc<Publisher>, job: &Job, base: &str, expect: &[String]) {
    match crate::verify::verify(publisher.root(), &job.config, base, expect) {
        Ok(verified) => reply(publisher, job, projection::verified(&verified)),
        Err(error) => reply(publisher, job, json!({ "error": format!("{error:#}") })),
    }
}

/// Followed by a scan, because every finding just became `known` and the squiggles
/// and the panel should say so.
fn run_baseline(publisher: &Arc<Publisher>, job: &Job, update: bool) {
    match crate::baseline::take(publisher.root(), &job.config, update, false) {
        Ok(taken) => {
            reply(
                publisher,
                job,
                json!({
                    "path": taken.path,
                    "recorded": taken.recorded,
                    "findings": taken.findings,
                    "replaced": taken.replaced,
                    "configured": taken.configured,
                }),
            );
            run(
                publisher,
                &Job {
                    reply_to: None,
                    ..job.clone()
                },
            );
        }
        Err(error) => reply(publisher, job, json!({ "error": format!("{error:#}") })),
    }
}

impl Drop for Worker {
    /// The writer thread stops only once every sender is dropped, so the job channel
    /// has to close and the worker has to finish before the connection goes away — or
    /// a clean editor shutdown hangs the process forever.
    fn drop(&mut self) {
        self.wake = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(publisher: &Arc<Publisher>, job: &Job) {
    let token = json!("rqx/scan");
    publisher.request("window/workDoneProgress/create", json!({ "token": token }));
    publisher.notify(
        "$/progress",
        json!({
            "token": token,
            "value": { "kind": "begin", "title": "rqx", "message": "scanning the workspace" }
        }),
    );

    let outcome = scan_reporting(publisher, &token, job);

    publisher.notify(
        "$/progress",
        json!({ "token": token, "value": { "kind": "end" } }),
    );

    let Some(outcome) = outcome else {
        reply(publisher, job, json!({ "error": "scan failed" }));
        return;
    };

    republish(
        publisher,
        outcome.findings.iter(),
        &outcome.advice,
        job,
        Clear::Stale,
    );

    let outcome = Arc::new(outcome);
    let report = Report::everything(&outcome);
    publisher.set_facts(facts_of(&report, &outcome));
    let health = projection::health(&report, &outcome);
    publisher.notify("rqx/health", health.clone());
    publisher.notify("rqx/report", projection::scan(&report, &outcome));
    publisher.remember(outcome);
    publisher.refresh_lenses();
    reply(publisher, job, health);
}

/// A review scans the whole project and narrows only what it reports on, because an
/// export looks dead until the file that imports it is parsed.
fn run_check(publisher: &Arc<Publisher>, job: &Job, base: &str) {
    let Ok(review) = check::review(publisher.root(), &job.config, base) else {
        reply(
            publisher,
            job,
            json!({ "error": format!("could not review against `{base}`") }),
        );
        return;
    };
    let verdict = check::judge(&review, &job.config.config);

    // Only the reviewed files are republished, and nothing else is cleared: a review is
    // a narrower question about the same project, not a statement that the rest of it
    // became clean.
    let advice: Vec<rqx_core::Advice> = review
        .outcome
        .advice
        .iter()
        .filter(|advice| review.is_reviewed(&advice.location.file))
        .cloned()
        .collect();
    republish(publisher, review.findings(), &advice, job, Clear::Nothing);

    let report = Report::reviewed(&review, &verdict);
    publisher.merge_facts(facts_of(&report, &review.outcome));
    publisher.refresh_lenses();
    // A review still moved the status line, and a client left spinning on the scan it
    // asked for has no other way to learn the work finished.
    publisher.notify(
        "rqx/health",
        json!({
            "health": report.health(),
            "findings": review.findings().count(),
            "files": review.reviewed.len(),
            "elapsedMs": review.outcome.elapsed_ms,
        }),
    );
    let judgement = projection::review(&review, &verdict);
    // The review scanned the whole project to answer its narrower question, so the
    // panel gets a fresh report rather than going stale until the next save.
    let everything = Report::everything(&review.outcome);
    let mut fresh = projection::scan(&everything, &review.outcome);
    fresh["review"] = judgement.clone();
    publisher.notify("rqx/report", fresh);
    publisher.remember(Arc::new(review.outcome));
    reply(publisher, job, judgement);
}

fn scan_reporting(
    publisher: &Arc<Publisher>,
    token: &serde_json::Value,
    job: &Job,
) -> Option<ScanOutcome> {
    let (watcher, ticks): (SyncSender<ScanProgress>, Ticks<ScanProgress>) = mpsc::sync_channel(64);
    let reporter = {
        let publisher = Arc::clone(publisher);
        let token = token.clone();
        thread::spawn(move || forward(&publisher, &token, &ticks))
    };
    let outcome =
        scan::scan_with_progress(publisher.root(), &job.config, scan::Watch::new(watcher)).ok();
    let _ = reporter.join();
    outcome
}

/// The scan ticks once per file, which on a large repository is thousands of messages
/// nobody can read — so progress is coalesced to what a person can actually follow.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

fn forward(publisher: &Publisher, token: &serde_json::Value, ticks: &Ticks<ScanProgress>) {
    let mut last = Instant::now() - PROGRESS_INTERVAL;
    for tick in ticks {
        if last.elapsed() < PROGRESS_INTERVAL {
            continue;
        }
        last = Instant::now();
        publisher.notify(
            "$/progress",
            json!({
                "token": token,
                "value": {
                    "kind": "report",
                    "message": format!("{} files · {} findings", tick.files_scanned, tick.findings),
                }
            }),
        );
    }
}

/// The whole project is republished at once, because a cross-file rule's answer for one
/// file changes when a different file does — an export stops being dead the moment
/// something imports it.
/// Whether files this publish does not mention should lose their squiggles. A whole
/// project scan speaks for every file; a review speaks only for the ones it looked at.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Clear {
    Stale,
    Nothing,
}

fn republish<'a>(
    publisher: &Arc<Publisher>,
    findings: impl Iterator<Item = &'a Finding>,
    advice: &'a [rqx_core::Advice],
    job: &Job,
    clear: Clear,
) {
    let patterns = job.patterns;
    let mut by_file: BTreeMap<&str, Vec<Finding>> = BTreeMap::new();
    for finding in findings {
        by_file
            .entry(&finding.location.file)
            .or_default()
            .push(finding.clone());
    }
    let mut advice_by_file: BTreeMap<&str, Vec<rqx_core::Advice>> = BTreeMap::new();
    if patterns {
        for advice in advice {
            advice_by_file
                .entry(&advice.location.file)
                .or_default()
                .push(advice.clone());
        }
    }

    let touched: HashSet<&str> = by_file
        .keys()
        .chain(advice_by_file.keys())
        .copied()
        .collect();
    for file in &touched {
        // A file the editor holds unsaved has already been published from the buffer,
        // and reading it back off disk would move every squiggle to the line it was on
        // before the edit.
        if job.dirty.iter().any(|open| open == file) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(publisher.root().join(file)) else {
            continue;
        };
        publisher.publish(
            file,
            &source,
            by_file.remove(*file).unwrap_or_default(),
            &advice_by_file.remove(*file).unwrap_or_default(),
            &job.config,
        );
    }

    // Swept after the publishes rather than before, so a file keeps its old squiggles
    // until its new ones are ready instead of flickering empty in between.
    if clear == Clear::Stale {
        publisher.retain(
            touched
                .iter()
                .map(|file| (*file).to_owned())
                .chain(job.dirty.iter().cloned())
                .collect(),
        );
    }
}

fn facts_of(report: &Report<'_>, outcome: &ScanOutcome) -> HashMap<String, FileFacts> {
    report
        .files
        .iter()
        .map(|file| {
            let impact = outcome.impact.get(file);
            (
                file.clone(),
                FileFacts {
                    grade: report.grade_of(file),
                    findings: outcome
                        .findings_by_file
                        .get(file.as_str())
                        .copied()
                        .unwrap_or(0),
                    commits: impact.commits,
                    importers: impact.importers,
                },
            )
        })
        .collect()
}

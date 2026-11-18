use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, unbounded};
use lsp_server::{Message, Notification};
use lsp_types::notification::{Notification as _, PublishDiagnostics};
use lsp_types::{
    CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag,
    Location, NumberOrString, Position, PublishDiagnosticsParams, Range,
};
use rqx_core::advice::Advice;
use rqx_core::finding::{Confidence, Finding, Severity};
use rqx_core::rule::RuleId;
use rqx_core::{Cleanliness, LineIndex};

use super::protocol::{Encoding, range_at, uri_for};
use crate::config::Resolved;
use crate::scan::ScanOutcome;

pub const SOURCE: &str = "rqx";
pub const PATTERN_SOURCE: &str = "rqx-patterns";

const REFERENCE: &str = "https://github.com/jadhamwi21/rqx/blob/main/docs/rules";

/// A finding kept beside the range it was published at, so hover and code actions
/// answer from the coordinates the editor actually drew rather than recomputing them
/// against a file that may have changed since.
pub struct Published {
    pub range: Range,
    pub finding: Finding,
    /// Decided by the project's `[fix]` config at publish time, so the answer matches
    /// what `rqx fix` would do rather than what the rule could do in principle.
    pub autofixable: bool,
}

/// What a scan learned about a file that is not a finding: its grade, and how much the
/// project leans on it. A code lens and the start-here ranking both read this, and
/// neither has anywhere else to get it.
#[derive(Clone, Copy)]
pub struct FileFacts {
    pub grade: Cleanliness,
    pub findings: u32,
    pub commits: u32,
    pub importers: u32,
}

fn severity_of(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    }
}

fn message_for(finding: &Finding) -> String {
    let confidence = match finding.confidence {
        Confidence::Verified => "",
        Confidence::Heuristic => " (measured threshold)",
        Confidence::Low => " (low confidence)",
    };
    // A rule the project wrote says everything it has to say in its message; the
    // catalogue's rationale is not its rationale.
    let why = match &finding.evidence {
        rqx_core::finding::Evidence::Custom { .. } => String::new(),
        _ => format!("\n\n{}", finding.rule.spec().why),
    };
    let fix = if finding.fix_hint.is_empty() {
        String::new()
    } else {
        format!("\n\nFix: {}", finding.fix_hint)
    };
    format!("{}{confidence}{why}{fix}", finding.message())
}

/// The generated reference is one page per family with a heading per rule, so the
/// anchor a client follows is the rule's own slug.
fn reference_for(finding: &Finding) -> Option<CodeDescription> {
    // A rule the project wrote is not in the reference, so there is nothing to link.
    if matches!(finding.evidence, rqx_core::finding::Evidence::Custom { .. }) {
        return None;
    }
    let spec = finding.rule.spec();
    let href = format!("{REFERENCE}/{}.md#{}", spec.category.slug(), spec.slug);
    Some(CodeDescription {
        href: href.parse().ok()?,
    })
}

fn of_finding(
    root: &Path,
    index: &LineIndex<'_>,
    encoding: Encoding,
    finding: &Finding,
    autofixable: bool,
) -> Diagnostic {
    let spec = finding.rule.spec();
    let related: Vec<DiagnosticRelatedInformation> = finding
        .related
        .iter()
        .filter_map(|site| {
            let line = site.line.saturating_sub(1);
            Some(DiagnosticRelatedInformation {
                location: Location {
                    uri: uri_for(root, &site.file)?,
                    range: Range {
                        start: Position {
                            line,
                            character: site.column.saturating_sub(1),
                        },
                        end: Position {
                            line,
                            character: site.column,
                        },
                    },
                },
                message: "same shape here".to_owned(),
            })
        })
        .collect();

    // An unused parameter greys out like dead code but is rewritten rather than
    // deleted, so the autofix kind alone would drop it.
    let unnecessary = spec.autofix == rqx_core::rule::Autofix::Deletion
        || finding.rule == RuleId::UnusedParameter;

    Diagnostic {
        range: range_at(index, encoding, &finding.location),
        severity: Some(severity_of(finding.severity)),
        code: Some(NumberOrString::String(finding.slug().to_owned())),
        code_description: reference_for(finding),
        source: Some(SOURCE.to_owned()),
        message: message_for(finding),
        related_information: (!related.is_empty()).then_some(related),
        tags: unnecessary.then(|| vec![DiagnosticTag::UNNECESSARY]),
        // The declared severity travels here as well as on the diagnostic, because a
        // client that caps how loudly it renders still needs to know what rqx said.
        data: Some(serde_json::json!({
            "rule": finding.slug(),
            "category": finding.category().slug(),
            "severity": finding.severity.label(),
            "evidence": finding.evidence.to_string(),
            "confidence": finding.confidence.label(),
            "autofixable": autofixable,
            "known": finding.known,
            "technique": spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>(),
        })),
    }
}

/// Advice is not a finding and must never reach the counts, so it publishes under its
/// own source: a client can filter it out, and nothing downstream can mistake it for
/// something that moves the health score.
fn of_advice(index: &LineIndex<'_>, encoding: Encoding, advice: &Advice) -> Diagnostic {
    let spec = advice.advisory.spec();
    Diagnostic {
        range: range_at(index, encoding, &advice.location),
        severity: Some(DiagnosticSeverity::HINT),
        code: Some(NumberOrString::String(spec.slug.to_owned())),
        code_description: None,
        source: Some(PATTERN_SOURCE.to_owned()),
        message: format!("{}\n\n{}", advice.note, spec.why),
        related_information: None,
        tags: None,
        data: None,
    }
}

pub fn hover_markdown(finding: &Finding) -> String {
    let spec = finding.rule.spec();
    format!(
        "**{}**\n\n{}\n\n{}\n\n**Fix — {}**\n\n{}\n\n---\n\n*{}*",
        spec.slug,
        finding.message(),
        spec.why,
        spec.fixes
            .iter()
            .map(|fix| fix.name())
            .collect::<Vec<_>>()
            .join(" / "),
        finding.fix_hint,
        spec.category.title()
    )
}

/// Diagnostics leave the server from one place and one thread.
///
/// The message loop republishes a file on save while the scan worker republishes the
/// workspace, and both have to send the notification *and* record what they sent. Doing
/// those two steps from two threads lets one thread's send land after the other's while
/// its record lands before — so the editor draws one set of squiggles and hover answers
/// from another. Holding a lock across the send would order them, but the writer channel
/// is a rendezvous: a client that stops reading would then block the message loop too,
/// and with it the shutdown that ends the session.
///
/// So the send and the record happen on a single owner, in queue order, and nothing that
/// leaves the server can overtake anything else — `rqx/report` cannot arrive before the
/// diagnostics it describes.
enum Outgoing {
    Publish {
        relative: String,
        source: String,
        findings: Vec<(Finding, bool)>,
        advice: Vec<Advice>,
    },
    /// Clear every file published but not named here. Evaluated against the map at the
    /// moment it is processed, so a file the message loop published while a scan was
    /// running is never wiped by that scan's stale sweep.
    Retain(HashSet<String>),
    Send(Message),
}

pub struct Publisher {
    outbox: Option<Sender<Outgoing>>,
    root: PathBuf,
    published: Arc<Mutex<HashMap<String, Vec<Published>>>>,
    facts: Mutex<HashMap<String, FileFacts>>,
    /// The last whole-project scan, kept so a plan or a filtered listing can be answered
    /// from it without scanning again.
    last: Mutex<Option<Arc<ScanOutcome>>>,
    next_request: AtomicU64,
    thread: Option<JoinHandle<()>>,
}

fn expect_lock<T>(result: Result<T, std::sync::PoisonError<T>>) -> T {
    result.expect("the diagnostic maps are only ever held to read or swap one entry")
}

impl Publisher {
    pub fn new(sender: Sender<Message>, root: PathBuf, encoding: Encoding) -> Self {
        // Unbounded so no producer ever waits: the message loop must stay free to answer
        // a shutdown even while a scan is pushing a thousand files at it.
        let (outbox, queue) = unbounded::<Outgoing>();
        let published = Arc::new(Mutex::new(HashMap::new()));
        let thread = {
            let published = Arc::clone(&published);
            let root = root.clone();
            thread::spawn(move || deliver(&sender, &root, encoding, &published, &queue))
        };
        Self {
            outbox: Some(outbox),
            root,
            published,
            facts: Mutex::new(HashMap::new()),
            last: Mutex::new(None),
            next_request: AtomicU64::new(0),
            thread: Some(thread),
        }
    }

    pub const fn root(&self) -> &PathBuf {
        &self.root
    }

    fn queue(&self, outgoing: Outgoing) {
        if let Some(outbox) = self.outbox.as_ref() {
            let _ = outbox.send(outgoing);
        }
    }

    /// A client caches lenses until it is told they went stale, so a scan that changed
    /// every file's grade shows nothing without this.
    pub fn refresh_lenses(&self) {
        self.request("workspace/codeLens/refresh", serde_json::Value::Null);
    }

    /// Server-to-client request ids must be unique for the life of the session, or a
    /// client keying its pending requests by id pairs the wrong response with the wrong
    /// request.
    pub fn request(&self, method: &str, params: serde_json::Value) {
        let id = self.next_request.fetch_add(1, Ordering::Relaxed);
        self.queue(Outgoing::Send(Message::Request(lsp_server::Request {
            id: lsp_server::RequestId::from(format!("rqx/{id}")),
            method: method.to_owned(),
            params,
        })));
    }

    pub fn notify(&self, method: &str, params: serde_json::Value) {
        self.queue(Outgoing::Send(Message::Notification(Notification {
            method: method.to_owned(),
            params,
        })));
    }

    pub fn send(&self, message: Message) {
        self.queue(Outgoing::Send(message));
    }

    pub fn publish(
        &self,
        relative: &str,
        source: &str,
        findings: Vec<Finding>,
        advice: &[Advice],
        settings: &Resolved,
    ) {
        let fix = &settings.for_file(&self.root.join(relative)).fix;
        let findings = findings
            .into_iter()
            .map(|finding| {
                let autofixable = fix.allows(finding.rule);
                (finding, autofixable)
            })
            .collect();
        self.queue(Outgoing::Publish {
            relative: relative.to_owned(),
            source: source.to_owned(),
            findings,
            advice: advice.to_vec(),
        });
    }

    pub fn remember(&self, outcome: Arc<ScanOutcome>) {
        *expect_lock(self.last.lock()) = Some(outcome);
    }

    pub fn last_outcome(&self) -> Option<Arc<ScanOutcome>> {
        expect_lock(self.last.lock()).clone()
    }

    /// A file that had findings and now has none still needs an empty publish, or the
    /// editor keeps showing the squiggles it was last told about — so a sweep names what
    /// survives and everything else is cleared.
    pub fn retain(&self, keep: HashSet<String>) {
        self.queue(Outgoing::Retain(keep));
    }

    pub fn set_facts(&self, facts: HashMap<String, FileFacts>) {
        *expect_lock(self.facts.lock()) = facts;
    }

    /// A review only learned about the files it reviewed, so replacing the map would
    /// leave the start-here ranking and every code lens knowing about the diff alone.
    pub fn merge_facts(&self, facts: HashMap<String, FileFacts>) {
        expect_lock(self.facts.lock()).extend(facts);
    }

    pub fn facts(&self, relative: &str) -> Option<FileFacts> {
        expect_lock(self.facts.lock()).get(relative).copied()
    }

    /// What to open first: worst grade, then how much depends on it. The same ordering
    /// the terminal's `Start here` uses, so an editor and the CLI agree about where to
    /// send someone.
    pub fn priority(&self, limit: usize) -> Vec<(String, FileFacts)> {
        let facts = expect_lock(self.facts.lock());
        let mut ranked: Vec<(String, FileFacts)> = facts
            .iter()
            .filter(|(_, facts)| facts.findings > 0)
            .map(|(file, facts)| (file.clone(), *facts))
            .collect();
        ranked.sort_by(|left, right| {
            left.1
                .grade
                .cmp(&right.1.grade)
                .then_with(|| right.1.importers.cmp(&left.1.importers))
                .then_with(|| right.1.commits.cmp(&left.1.commits))
                .then_with(|| left.0.cmp(&right.0))
        });
        ranked.truncate(limit);
        ranked
    }

    /// The narrowest finding covering a position. Narrowest rather than first, because
    /// a long-function finding spans every finding inside it and answering with the
    /// outer one hides the specific thing under the cursor.
    pub fn at(&self, relative: &str, position: Position) -> Option<(Range, Finding)> {
        let published = expect_lock(self.published.lock());
        published
            .get(relative)?
            .iter()
            .filter(|held| covers(held.range, position))
            .min_by_key(|held| span_of(held.range))
            .map(|held| (held.range, held.finding.clone()))
    }
}

impl Drop for Publisher {
    /// The delivery thread owns a clone of the connection's sender and the writer stops
    /// only once every sender is gone, so the queue has to close and the thread has to
    /// finish before the connection does.
    fn drop(&mut self) {
        self.outbox = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn deliver(
    sender: &Sender<Message>,
    root: &Path,
    encoding: Encoding,
    published: &Mutex<HashMap<String, Vec<Published>>>,
    queue: &Receiver<Outgoing>,
) {
    for outgoing in queue {
        match outgoing {
            Outgoing::Send(message) => {
                if sender.send(message).is_err() {
                    return;
                }
            }
            Outgoing::Publish {
                relative,
                source,
                findings,
                advice,
            } => {
                let index = LineIndex::new(&source);
                let held: Vec<Published> = findings
                    .into_iter()
                    .map(|(finding, autofixable)| Published {
                        range: range_at(&index, encoding, &finding.location),
                        finding,
                        autofixable,
                    })
                    .collect();
                let mut diagnostics: Vec<Diagnostic> = held
                    .iter()
                    .map(|held| of_finding(root, &index, encoding, &held.finding, held.autofixable))
                    .collect();
                diagnostics.extend(
                    advice
                        .iter()
                        .map(|advice| of_advice(&index, encoding, advice)),
                );
                if !notify_diagnostics(sender, root, &relative, diagnostics) {
                    return;
                }
                expect_lock(published.lock()).insert(relative, held);
            }
            Outgoing::Retain(keep) => {
                let stale: Vec<String> = expect_lock(published.lock())
                    .keys()
                    .filter(|file| !keep.contains(*file))
                    .cloned()
                    .collect();
                for relative in stale {
                    if !notify_diagnostics(sender, root, &relative, Vec::new()) {
                        return;
                    }
                    expect_lock(published.lock()).remove(&relative);
                }
            }
        }
    }
}

fn notify_diagnostics(
    sender: &Sender<Message>,
    root: &Path,
    relative: &str,
    diagnostics: Vec<Diagnostic>,
) -> bool {
    let Some(uri) = uri_for(root, relative) else {
        return true;
    };
    sender
        .send(Message::Notification(Notification {
            method: PublishDiagnostics::METHOD.to_owned(),
            params: serde_json::to_value(PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: None,
            })
            .unwrap_or_default(),
        }))
        .is_ok()
}

fn covers(range: Range, position: Position) -> bool {
    let after_start =
        (range.start.line, range.start.character) <= (position.line, position.character);
    let before_end = (position.line, position.character) <= (range.end.line, range.end.character);
    after_start && before_end
}

fn span_of(range: Range) -> (u32, u32) {
    (
        range.end.line.saturating_sub(range.start.line),
        range.end.character.saturating_sub(range.start.character),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rqx_core::finding::{Evidence, SourceRange};
    use rqx_core::rule::RuleId;

    fn finding(file: &str) -> Finding {
        Finding {
            rule: RuleId::UnusedExport,
            location: SourceRange {
                file: file.into(),
                start: 0,
                end: 4,
                line: 1,
                column: 1,
                end_line: 1,
            },
            severity: Severity::Warning,
            confidence: Confidence::Verified,
            evidence: Evidence::UnusedExport {
                symbol: "thing".to_owned(),
                kind: "const".to_owned(),
                ran: None,
            },
            fix_hint: "delete it".to_owned(),
            related: Vec::new(),
            known: false,
            focus: None,
        }
    }

    /// The delivery thread is the only writer, so the map is only settled once the queue
    /// is empty and the thread is between messages.
    fn drained(publisher: &Publisher) -> Vec<String> {
        let outbox = publisher.outbox.as_ref().expect("an open queue");
        while !outbox.is_empty() {
            std::thread::yield_now();
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut files: Vec<String> = expect_lock(publisher.published.lock())
            .keys()
            .cloned()
            .collect();
        files.sort();
        files
    }

    /// The stale sweep used to read the published map in the worker and clear from
    /// there, so a file the message loop published in between was wiped by a scan that
    /// never knew about it. Evaluating the sweep on the delivery thread, in queue order,
    /// is what makes that impossible.
    #[test]
    fn a_file_published_after_a_sweep_was_decided_survives_it() {
        let (sender, _drain) = crossbeam_channel::unbounded();
        let publisher = Publisher::new(sender, PathBuf::from("/tmp/rqx"), Encoding::Utf16);

        let settings = Resolved::default();
        publisher.publish(
            "old.ts",
            "const a = 1;\n",
            vec![finding("old.ts")],
            &[],
            &settings,
        );
        publisher.publish(
            "gone.ts",
            "const c = 3;\n",
            vec![finding("gone.ts")],
            &[],
            &settings,
        );
        // The sweep is queued naming only `old.ts`, exactly as a scan would, and a save
        // lands behind it — the case that used to wipe the file the user just saved.
        publisher.retain(HashSet::from(["old.ts".to_owned()]));
        publisher.publish(
            "saved.ts",
            "const b = 2;\n",
            vec![finding("saved.ts")],
            &[],
            &settings,
        );

        let files = drained(&publisher);
        assert_eq!(
            files,
            vec!["old.ts", "saved.ts"],
            "the sweep took the wrong files"
        );
    }
}

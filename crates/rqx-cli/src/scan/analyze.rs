//! Analyzes one file, or records why it was skipped.

use std::path::Path;
use std::sync::mpsc::SyncSender;

use rqx_core::Language;
use rqx_core::finding::Finding;
use rqx_core::rule::RuleId;

use super::skip;
use super::skip::{SkipReason, Skipped};
use crate::cache::{Cached, Stamp};
use crate::config::Resolved;
use crate::stats::FileReads;

use super::{Analyzed, ScanMessage};

pub(crate) fn relative_to(root: &Path, path: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(root) {
        return rqx_core::paths::display_path(relative);
    }
    // The graph works in canonical paths while the walker yields them as given, so
    // the two only line up after both are resolved.
    if let (Ok(canonical_root), Ok(canonical)) = (
        rqx_core::paths::canonicalize(root),
        rqx_core::paths::canonicalize(path),
    ) && let Ok(relative) = canonical.strip_prefix(&canonical_root)
    {
        return rqx_core::paths::display_path(relative);
    }
    rqx_core::paths::display_path(path)
}

/// Oversized files are rejected here rather than by the walker's own
/// `max_filesize`, which drops them before they are ever visible: a file that was
/// never analyzed has to be reported as such, or a clean score is a false one.
pub(super) fn analyze_entry(
    entry: &ignore::DirEntry,
    root: &Path,
    settings: &Resolved,
    cache: Option<&crate::cache::Cache>,
    reads: &FileReads,
    sender: &SyncSender<ScanMessage>,
) {
    let metadata = entry.metadata().ok();
    let size = metadata.as_ref().map(std::fs::Metadata::len);
    let stamp = metadata.as_ref().and_then(Stamp::of);
    let _ = sender.send(analyze_file(
        entry.path(),
        root,
        size,
        stamp,
        settings,
        cache,
        reads,
    ));
}

/// One file, analysed. Split from the send so a caller holding its own map of results
/// — the server behind `rqx serve` — can re-analyse one file without a channel and a
/// walk around it.
pub(crate) fn analyze_file(
    path: &Path,
    root: &Path,
    size: Option<u64>,
    stamp: Option<Stamp>,
    settings: &Resolved,
    cache: Option<&crate::cache::Cache>,
    reads: &FileReads,
) -> ScanMessage {
    let (config, facts) = settings.for_file_with_facts(path);
    if size.is_some_and(|bytes| bytes > config.scan.max_filesize) {
        return skipped(
            path,
            SkipReason::TooLarge,
            format!(
                "{} KB exceeds the {} KB limit (raise `scan.max_filesize`)",
                size.unwrap_or_default() / 1024,
                config.scan.max_filesize / 1024
            ),
        );
    }
    let display = relative_to(root, path);
    if let Some(cached) = cache
        .zip(stamp)
        .and_then(|(cache, stamp)| cache.unchanged(&display, stamp))
    {
        return analyzed(path, display, cached, settings);
    }
    reads.note();
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => return skipped(path, SkipReason::Unreadable, error.to_string()),
    };
    if config.scan.skip_generated
        && let Some(tool) = skip::generated_by(&source)
    {
        return skipped(
            path,
            SkipReason::Generated,
            format!("{tool} (set `scan.skip_generated: false` to include it)"),
        );
    }
    if config.scan.skip_minified
        && let Some(why) = skip::minified(&source)
    {
        return skipped(
            path,
            SkipReason::Minified,
            format!("minified code: {why} (set `scan.skip_minified: false` to include it)"),
        );
    }
    let cached = match cache.and_then(|cache| cache.get(&display, &source, stamp)) {
        Some(cached) => cached,
        None => {
            match rqx_lang::analyze_planned(
                path,
                &display,
                &source,
                config,
                &settings.cost_tables,
                facts,
            ) {
                Ok(analysis) => {
                    let identities = identities_of(&analysis.findings, &source);
                    if let Some(cache) = cache {
                        cache.put(&display, &source, stamp, &analysis, &identities);
                    }
                    Cached {
                        analysis,
                        identities,
                    }
                }
                Err(error) => {
                    let reason = if error.timed_out() {
                        SkipReason::ParseTimeout
                    } else if error.too_deep() {
                        SkipReason::TooDeep
                    } else {
                        SkipReason::Unsupported
                    };
                    return skipped(path, reason, error.to_string());
                }
            }
        }
    };
    analyzed(path, display, cached, settings)
}

pub(super) fn skipped(path: &Path, reason: SkipReason, detail: String) -> ScanMessage {
    ScanMessage::Skipped(Skipped::file(path, reason, detail))
}

/// Resolved here, where the file's text is already in hand, and kept with the cached
/// analysis so that a file that is not read again can still be matched against a
/// baseline.
fn identities_of(findings: &[Finding], source: &str) -> Vec<rqx_core::Identity> {
    findings
        .iter()
        .map(|finding| rqx_core::Identity::resolved(finding, source))
        .collect()
}

fn analyzed(path: &Path, display: String, cached: Cached, settings: &Resolved) -> ScanMessage {
    let Cached {
        mut analysis,
        mut identities,
    } = cached;
    // Which Go a module is written for is in its `go.mod`, not in the file, so it is
    // applied here rather than kept in the file's cached analysis.
    if analysis.language == Language::Go
        && analysis
            .findings
            .iter()
            .any(|finding| finding.rule == RuleId::ClosureOverLoopVariable)
        && rqx_lang::go::loop_variables_per_iteration(path)
    {
        let kept: Vec<bool> = analysis
            .findings
            .iter()
            .map(|finding| finding.rule != RuleId::ClosureOverLoopVariable)
            .collect();
        let mut index = 0;
        identities.retain(|_| {
            index += 1;
            kept.get(index - 1).copied().unwrap_or(true)
        });
        analysis
            .findings
            .retain(|finding| finding.rule != RuleId::ClosureOverLoopVariable);
    }
    // The cost analysis follows calls across imports, which it can only do for a file it
    // can name the way the import resolver does.
    let takes_part_in_cost =
        !analysis.cost.is_empty() || rqx_lang::cost::compose::is_barrel(&analysis.module);
    ScanMessage::File(Box::new(Analyzed {
        canonical: (settings.needs_modules() || takes_part_in_cost)
            .then(|| rqx_core::paths::canonicalize(path).unwrap_or_else(|_| path.to_owned())),
        display,
        language: analysis.language,
        findings: analysis.findings,
        identities,
        advice: analysis.advice,
        clones: analysis.clones,
        module: analysis.module,
        code_lines: analysis.code_lines,
        shape: analysis.shape,
        suppressions: analysis.suppressions,
        todos: analysis.todos,
        cost: analysis.cost,
    }))
}

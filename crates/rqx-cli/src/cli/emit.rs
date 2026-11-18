//! Where reports go: the sinks, the pager and the rendered output.

use std::io::Write;

use anyhow::{Context, Result};

use crate::brand::{ColorDepth, Palette};
use crate::report::{Layout, Report};
use crate::sink::Sink;
use crate::{brand, pager, report, scan, sink, update};

use super::args::{DetailArg, View};

/// What a scan has to keep for the reports it will write: the HTML report explains the
/// cost of the slowest functions, and plain reports need nothing beyond the findings.
pub(super) fn comparison_for(sinks: &[Sink]) -> scan::Comparison {
    if sinks
        .iter()
        .any(|sink| sink.reporter == report::Reporter::Html)
    {
        scan::Comparison::costed()
    } else {
        scan::Comparison::default()
    }
}

/// The HTML report goes in 0.6.0. Said on stderr, which no reporter writes a document
/// to, so a pipeline that still asks for it sees the notice in its log.
pub(super) fn warn_of_deprecated(sinks: &[Sink]) {
    if sinks
        .iter()
        .any(|sink| sink.reporter == report::Reporter::Html)
    {
        anstream::eprintln!(
            "rqx: the html reporter is deprecated and will be removed in 0.6.0; \
             use json or sarif for a report kept as an artifact"
        );
    }
}

pub(crate) fn sinks(view: &View) -> Result<Vec<Sink>> {
    let mut sinks = view.reporters.clone();
    if let Some(reporter) = view.format {
        sinks.push(Sink {
            reporter,
            path: None,
        });
    }
    sink::settle(sinks)
}

/// Where the lines that accompany a report go. A document on stdout has to parse
/// whole, so anything else the run wants to say goes to stderr instead.
pub(super) fn companion_stream(sinks: &[Sink]) -> Box<dyn std::io::Write> {
    let document_on_stdout = sinks
        .iter()
        .any(|sink| sink.path.is_none() && sink.reporter.is_document());
    if document_on_stdout {
        Box::new(anstream::stderr().lock())
    } else {
        Box::new(anstream::stdout().lock())
    }
}

pub(super) fn emit(
    report: &Report<'_>,
    sinks: &[Sink],
    view: &View,
    palette: Palette,
) -> Result<()> {
    if view.verbose > 1 {
        write_diagnostics(report.outcome, view, palette)?;
    }
    if let Some(pager) = &view.pager
        && only_the_terminal_report_on_stdout(sinks)
    {
        let mut held = Vec::new();
        if view.timings {
            emit_timed(report, sinks, view, palette, &mut held)?;
        } else {
            for sink in sinks {
                write_sink(report, sink, view, palette, &mut held)?;
            }
        }
        return show_held(pager, &held);
    }
    if !view.timings {
        for sink in sinks {
            write_sink(report, sink, view, palette, &mut stdout_for(sink))?;
        }
        return Ok(());
    }
    let (held, render) = render_held(report, sinks, view, palette)?;
    report::timings(
        report.outcome,
        &[("render", render)],
        palette,
        &mut companion_stream(sinks),
    )?;
    anstream::stdout().lock().write_all(&held)?;
    Ok(())
}

/// A document on stdout reaches its reader byte for byte, so only the terminal report
/// is ever paged.
fn only_the_terminal_report_on_stdout(sinks: &[Sink]) -> bool {
    sinks
        .iter()
        .filter(|sink| sink.path.is_none())
        .all(|sink| sink.reporter == report::Reporter::Terminal)
}

/// A report rendered into memory, through the pager when it is taller than the window
/// and the pager starts, and printed as it always was otherwise.
fn show_held(pager: &pager::Pager, held: &[u8]) -> Result<()> {
    let window = crossterm::terminal::size().ok();
    if window.is_some_and(|window| pager::taller_than(held, window)) && pager::page(pager, held) {
        return Ok(());
    }
    anstream::stdout().lock().write_all(held)?;
    Ok(())
}

/// What `-vv` adds on stderr, where it cannot corrupt a document on stdout. The stage
/// timings are left out when `--timings` is already printing them.
fn write_diagnostics(outcome: &scan::ScanOutcome, view: &View, palette: Palette) -> Result<()> {
    let mut stderr = anstream::stderr().lock();
    if !view.timings {
        report::timings(outcome, &[], palette, &mut stderr)?;
    }
    report::git_commands(outcome, palette, &mut stderr)
}

/// The timed report into one writer, stages first, for a caller whose output is
/// never a document, such as the session's scrollback.
pub(crate) fn emit_timed(
    report: &Report<'_>,
    sinks: &[Sink],
    view: &View,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let (held, render) = render_held(report, sinks, view, palette)?;
    report::timings(report.outcome, &[("render", render)], palette, out)?;
    out.write_all(&held)?;
    Ok(())
}

/// Every sink rendered into memory and how long that took. Held back so rendering
/// can be timed and still printed beneath the stages it is one of.
fn render_held(
    report: &Report<'_>,
    sinks: &[Sink],
    view: &View,
    palette: Palette,
) -> Result<(Vec<u8>, u128)> {
    let mut held = Vec::new();
    let rendering = std::time::Instant::now();
    for sink in sinks {
        write_sink(report, sink, view, palette, &mut held)?;
    }
    Ok((held, rendering.elapsed().as_millis()))
}

fn stdout_for(sink: &Sink) -> Box<dyn Write> {
    if sink.reporter.is_coloured() {
        Box::new(anstream::stdout().lock())
    } else {
        Box::new(std::io::stdout().lock())
    }
}

/// One sink's report, to its file or to `stdout`, whichever the sink names.
pub(crate) fn write_sink(
    report: &Report<'_>,
    sink: &Sink,
    view: &View,
    palette: Palette,
    stdout: &mut dyn Write,
) -> Result<()> {
    let plain = Layout {
        detail: view.detail.into(),
        grouping: view.grouping.into(),
        show_suppressed: view.show_suppressed,
        order: view.order.into(),
        top: view.top,
        palette: Palette::with_depth(ColorDepth::Monochrome),
        show_passed: view.show_passed,
        stats: view.stats,
        quiet: view.quiet,
        verbose: view.verbose > 0,
        width: None,
        links: None,
        // Full is the level that shows everything, so it is the one that pays
        // for surrounding lines; at 285 findings they are most of the scroll.
        context: view.context.unwrap_or(match view.detail {
            DetailArg::Full => 2,
            DetailArg::Terse | DetailArg::Normal => 0,
        }),
    };
    match &sink.path {
        Some(path) => {
            let mut file = std::fs::File::create(path)
                .with_context(|| format!("writing {}", path.display()))?;
            report::emit(report, sink.reporter, &plain, &mut file)?;
            file.flush()?;
        }
        None if sink.reporter.is_coloured() => {
            let layout = Layout {
                palette,
                width: view.width,
                links: view.links.clone().filter(|_| !palette.is_monochrome()),
                ..plain
            };
            report::emit(report, sink.reporter, &layout, stdout)?;
        }
        None => report::emit(report, sink.reporter, &plain, stdout)?,
    }
    Ok(())
}

/// One dim line under a report when a newer release is already known about, and a
/// check on a thread nobody waits for so the next run knows. Never on a report going
/// somewhere other than a terminal: a line about updates in a JSON document or a
/// JUnit file is corruption, not news.
pub(super) fn update_notice(view: &View, sinks: &[Sink], palette: Palette, enabled: bool) {
    // `is_coloured` is about the reporter's kind, not about where it lands, so it is
    // true for the default reporter piped into a build log. `update::wanted` is what
    // actually tests the terminal.
    let readable = sinks
        .iter()
        .any(|sink| sink.path.is_none() && sink.reporter.is_coloured());
    if view.quiet || !readable || !update::wanted(enabled) {
        return;
    }
    // A quarter of a second at the very end, for a check that started before the
    // scan did. Long enough to catch an answer that is all but arrived, short enough
    // that nobody attributes it to the scan.
    if let Some(newer) = update::settle(std::time::Duration::from_millis(250)) {
        // Through anstream so a redirected stderr gets the words without the escape
        // sequences, which is the whole reason `paint` exists.
        anstream::eprintln!(
            "\n  {}",
            brand::paint(palette.style(brand::Ink::Muted), newer.line())
        );
    }
}

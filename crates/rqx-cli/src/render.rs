//! What each command prints once it holds a scan.
//!
//! The command line scans and then calls these; the interactive session already has
//! its scan and calls the same ones. One function per command is what keeps
//! `/plan --budget 20` and `rqx plan --budget 20` from ever printing different things.

use std::io::Write;
use std::path::Path;

use anyhow::Result;

use crate::brand::Palette;
use crate::config::Resolved;
use crate::scan::ScanOutcome;
use crate::{complexity, health, perf, plan, recommend, report, slop};

pub struct Plan {
    pub budget: Option<u32>,
    pub limit: usize,
    pub simulate: bool,
    pub json: bool,
}

pub fn plan(
    outcome: &ScanOutcome,
    request: &Plan,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let ranked = plan::rank(outcome, request.budget, request.limit);
    if request.json {
        return report::plan_json(outcome, &ranked, out);
    }
    report::plan(
        outcome,
        &ranked,
        request.budget,
        request.simulate,
        palette,
        out,
    )
}

pub fn health(
    outcome: &ScanOutcome,
    by: health::By,
    json: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    if json {
        return health::json(outcome, by, out);
    }
    health::render(outcome, by, palette, out)
}

pub fn complexity(
    outcome: &ScanOutcome,
    view: &complexity::View,
    json: bool,
    settings: &Resolved,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    if json {
        return complexity::json(outcome, view, out);
    }
    let threshold = settings.config.thresholds.cognitive_complexity;
    complexity::render(outcome, view, threshold, palette, out)
}

pub fn perf(
    outcome: &ScanOutcome,
    view: &perf::View,
    json: bool,
    settings: &Resolved,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    if json {
        return perf::json(outcome, view, out);
    }
    let threshold = settings.config.thresholds.loop_nesting;
    perf::render(outcome, view, threshold, palette, out)
}

pub fn recommend(
    outcome: &ScanOutcome,
    settings: &Resolved,
    only: Option<recommend::Only>,
    json: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    if json {
        return recommend::json(outcome, settings, only, out);
    }
    recommend::render(outcome, settings, only, palette, out)
}

pub struct Slop {
    pub top: usize,
    pub min: u32,
    pub json: bool,
}

pub fn slop(
    outcome: &ScanOutcome,
    request: &Slop,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    if request.json {
        return slop::json(outcome, request.top, request.min, out);
    }
    slop::render(outcome, request.top, request.min, palette, out)
}

/// A scan with the signals `slop` reads switched on. They are Info rules and several
/// ship off, so the command turns its own family on rather than reporting nothing.
pub fn slop_settings(settings: &mut Resolved) {
    for (rule, _) in rqx_core::slop::SIGNALS {
        settings.config.rules.enabled.insert(rule.slug().to_owned());
    }
}

pub fn patterns(
    outcome: &ScanOutcome,
    json: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    if json {
        return report::advice_json(outcome, out);
    }
    report::advice(outcome, palette, out)
}

pub fn suppressions(
    root: &Path,
    outcome: &ScanOutcome,
    expiring: bool,
    json: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    if json {
        return report::waivers_json(root, outcome, out);
    }
    report::waivers(root, outcome, expiring, palette, out)
}

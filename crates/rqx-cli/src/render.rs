//! What each command prints once it holds a scan.
//!
//! The command line scans and then calls these; the interactive session already has
//! its scan and calls the same ones. One function per command is what keeps
//! `/plan --budget 20` and `rqx plan --budget 20` from ever printing different things.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;

use rqx_core::Language;
use rqx_core::rule::{Category, RuleId};

use crate::brand::Palette;
use crate::config::Resolved;
use crate::scan::ScanOutcome;
use crate::{complexity, health, hotspots, ownership, perf, plan, recommend, report, slop, trend};

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

pub struct Trend {
    pub path: PathBuf,
    pub last: Option<usize>,
    pub by: trend::By,
    pub json: bool,
}

pub fn trend(request: &Trend, palette: Palette, out: &mut dyn Write) -> Result<()> {
    let snapshots = trend::read(&request.path)?;
    let shown = trend::last(&snapshots, request.last);
    if request.json {
        return report::trend_json(shown, request.by, out);
    }
    report::trend(shown, request.by, palette, out)
}

pub fn owners(
    outcome: &ScanOutcome,
    json: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let rows = ownership::table(outcome, &outcome.analyzed);
    if json {
        return report::owners_json(&rows, out);
    }
    if outcome.owners.is_none() {
        return report::owners_missing(palette, out);
    }
    report::owners(&rows, palette, out)
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

pub fn perf_explain(
    outcome: &ScanOutcome,
    target: &str,
    json: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    perf::explain(outcome, target, json, palette, out)
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
/// What `hotspots` was asked for.
#[derive(Clone, Copy)]
pub struct Hotspots {
    pub top: usize,
    pub days: u32,
    pub json: bool,
}

impl Hotspots {
    /// The request, with the churn window written into the settings so the scan reads
    /// the history the request names. Refused when the policy locks a different window.
    pub fn over(settings: &mut Resolved, top: usize, since: Option<u32>) -> Result<Self> {
        if let Some(days) = since {
            settings.set_by_flag("scan.churn_days", "--since", |config| {
                config.scan.churn_days = days;
            })?;
        }
        Ok(Self {
            top,
            days: settings.config.scan.churn_days,
            json: false,
        })
    }

    pub const fn printed_as(self, json: bool) -> Self {
        Self { json, ..self }
    }
}

pub fn hotspots(
    outcome: &ScanOutcome,
    request: &Hotspots,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    if request.json {
        return hotspots::json(outcome, request.top, request.days, out);
    }
    hotspots::render(outcome, request.top, request.days, palette, out)
}

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

/// A rule's entry, or a pattern's, whichever `name` is.
pub fn explain(name: &str, palette: Palette, out: &mut dyn Write) -> Result<()> {
    if let Some(rule) = RuleId::parse(name) {
        return report::explain(rule, palette, out);
    }
    // An advisory slug resolves to its pattern: `strategy-fits` and `strategy`
    // are the same question asked two ways.
    let pattern = rqx_core::Pattern::parse(name)
        .or_else(|| rqx_core::AdviceId::parse(name).map(rqx_core::AdviceId::pattern));
    let Some(pattern) = pattern else {
        let mut names: Vec<&str> = RuleId::ALL.iter().map(|rule| rule.slug()).collect();
        names.extend(rqx_core::Pattern::ALL.iter().map(|p| p.slug()));
        names.extend(rqx_core::AdviceId::ALL.iter().map(|a| a.slug()));
        return Err(report::unknown(
            "rule or pattern",
            name,
            &names,
            "Run `rqx rules` for the rules and `rqx patterns --coverage` for the patterns.",
        ));
    };
    report::explain_pattern(pattern, palette, out)
}

pub fn rules(
    category: Option<&str>,
    language: Option<&str>,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    report::rules(category, language, palette, out)
}

/// One slug per line and nothing else, for a script or a shell completion. The same
/// narrowing the printed listing does: without it a script asking for one family's
/// slugs got all of them and no sign it had been ignored.
pub fn rule_slugs(
    category: Option<&str>,
    language: Option<&str>,
    out: &mut dyn Write,
) -> Result<()> {
    let only = category
        .map(|name| {
            Category::parse(name).ok_or_else(|| {
                report::unknown(
                    "family",
                    name,
                    &Category::ALL.map(Category::slug),
                    &format!(
                        "The families are: {}.",
                        Category::ALL.map(Category::slug).join(", ")
                    ),
                )
            })
        })
        .transpose()?;
    let spoken = language
        .map(|name| {
            Language::parse(name).ok_or_else(|| {
                report::unknown(
                    "language",
                    name,
                    &Language::ALL.map(Language::slug),
                    &format!(
                        "rqx reads: {}.",
                        Language::ALL.map(Language::slug).join(", ")
                    ),
                )
            })
        })
        .transpose()?;
    // `writeln!` rather than `println!`, which panics when the reader closes the
    // pipe. `rqx rules --slugs | head` is the shape a shell completion uses, and
    // `reader_went_away` can only forgive an error that is returned to it.
    for rule in RuleId::ALL {
        let spec = rule.spec();
        if only.is_some_and(|family| spec.category != family) {
            continue;
        }
        if spoken.is_some_and(|language| !rule.applies_to(language)) {
            continue;
        }
        writeln!(out, "{}", spec.slug)?;
    }
    out.flush()?;
    Ok(())
}

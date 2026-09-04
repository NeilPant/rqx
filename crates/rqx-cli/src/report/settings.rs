use std::io::Write;

use anyhow::Result;
use rqx_core::config::{GateGrade, GateSeverity, RqxConfig, Thresholds};
use rqx_core::rule::RuleId;

use crate::brand::{Ink, Palette, paint};
use crate::config::Resolved;

/// One line of the effective configuration, and enough to say where it came from.
/// The value alone is not the answer a reader wants: a threshold read from a file
/// behaves differently from the same number left at its default.
pub struct Row {
    pub name: &'static str,
    pub value: String,
    pub table: &'static str,
    pub is_default: bool,
}

impl Row {
    pub fn source(&self, settings: &Resolved) -> String {
        if self.is_default {
            return "default".to_owned();
        }
        settings
            .source_of(self.table)
            .and_then(|path| path.file_name())
            .map_or_else(
                || "flag".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            )
    }
}

fn row(name: &'static str, table: &'static str, value: String, is_default: bool) -> Row {
    Row {
        name,
        value,
        table,
        is_default,
    }
}

pub fn rows(settings: &Resolved) -> Vec<Row> {
    let config = &settings.config;
    let default = RqxConfig::default();
    let thresholds = &config.thresholds;
    let defaults = Thresholds::default();

    let languages = if config.languages.enabled.is_empty() {
        "all five".to_owned()
    } else {
        config
            .languages
            .enabled()
            .iter()
            .map(|language| language.slug())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let untouched = config.rules.disabled.is_empty()
        && config.rules.enabled.is_empty()
        && config.rules.categories.is_empty();
    let advice_untouched = config.patterns.disabled.is_empty()
        && config.patterns.enabled.is_empty()
        && config.patterns.families.is_empty()
        && config.patterns.stances.is_empty();
    let advisories = rqx_core::AdviceId::ALL
        .iter()
        .filter(|advisory| config.patterns.enabled(**advisory))
        .count();

    let mut rows = vec![
        row(
            "languages",
            "languages",
            languages,
            config.languages.enabled.is_empty(),
        ),
        row(
            "rules enabled",
            "rules",
            format!("{} of {}", settings.rules_enabled(), RuleId::ALL.len()),
            untouched,
        ),
        row(
            "pattern advice",
            "patterns",
            format!("{advisories} of {}", rqx_core::AdviceId::ALL.len()),
            advice_untouched,
        ),
        row("threads", "scan", threads(config), config.scan.threads == 0),
        row(
            "display cap",
            "scan",
            config.scan.max_findings.to_string(),
            config.scan.max_findings == default.scan.max_findings,
        ),
        row(
            "max file size",
            "scan",
            format!("{} KB", config.scan.max_filesize / 1024),
            config.scan.max_filesize == default.scan.max_filesize,
        ),
        row(
            "churn window",
            "scan",
            format!("{} days", config.scan.churn_days),
            config.scan.churn_days == default.scan.churn_days,
        ),
        row(
            "cache",
            "scan",
            config.scan.cache.to_string(),
            !config.scan.cache,
        ),
        row(
            "pruned directories",
            "scan",
            config.scan.prune_dirs.len().to_string(),
            config.scan.prune_dirs == default.scan.prune_dirs,
        ),
        row(
            "ignored globs",
            "ignore",
            config.ignore.patterns.len().to_string(),
            config.ignore.patterns == default.ignore.patterns,
        ),
        row(
            "entry points",
            "entrypoints",
            if config.entrypoints.paths.is_empty() {
                "none declared".to_owned()
            } else {
                config.entrypoints.paths.join(", ")
            },
            config.entrypoints.paths.is_empty(),
        ),
        row(
            "gate: fail on",
            "gate",
            fail_on(config.gate.fail_on).to_owned(),
            matches!(config.gate.fail_on, GateSeverity::Error),
        ),
        row(
            "gate: allowed",
            "gate",
            config.gate.max_findings.to_string(),
            config.gate.max_findings == 0,
        ),
        row(
            "gate: min health",
            "gate",
            config.gate.min_health.to_string(),
            config.gate.min_health == 0,
        ),
        row(
            "gate: min grade",
            "gate",
            min_grade(config.gate.min_grade).to_owned(),
            matches!(config.gate.min_grade, GateGrade::Never),
        ),
    ];

    for (name, value, fallback) in threshold_rows(thresholds, &defaults) {
        rows.push(row(
            name,
            "thresholds",
            value.to_string(),
            value == fallback,
        ));
    }
    rows
}

fn threads(config: &RqxConfig) -> String {
    match config.scan.threads {
        0 => "auto".to_owned(),
        count => count.to_string(),
    }
}

const fn fail_on(severity: GateSeverity) -> &'static str {
    match severity {
        GateSeverity::Error => "error",
        GateSeverity::Warning => "warning",
        GateSeverity::Info => "info",
        GateSeverity::Never => "never",
    }
}

const fn min_grade(grade: GateGrade) -> &'static str {
    match grade {
        GateGrade::Pristine => "pristine",
        GateGrade::Clean => "clean",
        GateGrade::Fair => "fair",
        GateGrade::Poor => "poor",
        GateGrade::Critical => "critical",
        GateGrade::Never => "never",
    }
}

/// All twenty-two, because a table that shows half of them cannot answer "why did
/// this fire" for the other half.
fn threshold_rows(values: &Thresholds, defaults: &Thresholds) -> Vec<(&'static str, u32, u32)> {
    vec![
        (
            "long function",
            values.long_function_statements,
            defaults.long_function_statements,
        ),
        (
            "cognitive complexity",
            values.cognitive_complexity,
            defaults.cognitive_complexity,
        ),
        (
            "nesting depth",
            values.nesting_depth,
            defaults.nesting_depth,
        ),
        ("max parameters", values.max_params, defaults.max_params),
        (
            "class members",
            values.class_members,
            defaults.class_members,
        ),
        (
            "class statements",
            values.class_statements,
            defaults.class_statements,
        ),
        (
            "clone statements",
            values.clone_min_statements,
            defaults.clone_min_statements,
        ),
        (
            "clone nodes",
            values.clone_min_nodes,
            defaults.clone_min_nodes,
        ),
        (
            "condition operands",
            values.condition_operands,
            defaults.condition_operands,
        ),
        (
            "message chain depth",
            values.message_chain_depth,
            defaults.message_chain_depth,
        ),
        (
            "commented code lines",
            values.commented_code_lines,
            defaults.commented_code_lines,
        ),
        (
            "magic number repeats",
            values.magic_number_repeats,
            defaults.magic_number_repeats,
        ),
        (
            "class public members",
            values.class_public_members,
            defaults.class_public_members,
        ),
        (
            "feature envy ratio",
            values.feature_envy_ratio,
            defaults.feature_envy_ratio,
        ),
        (
            "data clump size",
            values.data_clump_size,
            defaults.data_clump_size,
        ),
        (
            "data clump sites",
            values.data_clump_sites,
            defaults.data_clump_sites,
        ),
        (
            "short name scope",
            values.short_name_scope_lines,
            defaults.short_name_scope_lines,
        ),
        ("slow test", values.slow_test_ms, defaults.slow_test_ms),
        (
            "boundary repeats",
            values.boundary_repeats,
            defaults.boundary_repeats,
        ),
        (
            "declaration distance",
            values.declaration_distance_lines,
            defaults.declaration_distance_lines,
        ),
        (
            "expression operations",
            values.expression_operations,
            defaults.expression_operations,
        ),
        (
            "configuration depth",
            values.configuration_depth,
            defaults.configuration_depth,
        ),
    ]
}

/// `rqx config`. The list of files comes first because that is the question a
/// surprising value raises, and answering it by inspection means guessing.
pub fn render(settings: &Resolved, palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let dim = palette.style(Ink::Muted);

    writeln!(out)?;
    if settings.sources.is_empty() {
        writeln!(
            out,
            "  {}",
            paint(dim, "no RQX.toml found; every value below is a default")
        )?;
        writeln!(
            out,
            "  {}",
            paint(dim, "`rqx init` writes one you can edit")
        )?;
    } else {
        writeln!(
            out,
            "  {}",
            paint(palette.style(Ink::Accent).bold(), "read, in order")
        )?;
        for path in &settings.sources {
            writeln!(out, "    {}", path.display())?;
        }
    }
    writeln!(out)?;

    let rows = rows(settings);
    let width = rows.iter().map(|row| row.name.len()).max().unwrap_or(0);
    writeln!(
        out,
        "  {} {} {}",
        paint(dim, format!("{:width$}", "setting")),
        paint(dim, format!("{:>12}", "value")),
        paint(dim, "from")
    )?;
    for entry in &rows {
        writeln!(
            out,
            "  {:width$} {} {}",
            entry.name,
            paint(palette.style(Ink::Accent), format!("{:>12}", entry.value)),
            paint(dim, entry.source(settings))
        )?;
    }
    writeln!(out)?;
    Ok(())
}

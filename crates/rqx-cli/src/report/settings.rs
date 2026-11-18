use std::io::Write;

use anyhow::Result;
use rqx_core::config::{GateGrade, GateSeverity, Level, RqxConfig, Thresholds};
use rqx_core::finding::Confidence;
use rqx_core::rule::RuleId;
use serde::Serialize;
use serde_json::Value;

use crate::brand::{Ink, Palette, paint};
use crate::config::Resolved;

/// One line of the effective configuration, and enough to say where it came from.
/// The value alone is not the answer a reader wants: a threshold read from a file
/// behaves differently from the same number left at its default.
pub struct Row {
    pub name: &'static str,
    pub value: String,
    pub table: &'static str,
    /// The setting as it is written in a config file, dotted: `gate.fail_on`.
    pub setting: String,
    /// The key inside the table, where the row is one. Two files can both write
    /// `[thresholds]` and only this says which of them wrote this line.
    pub key: Option<&'static str>,
    pub is_default: bool,
    /// Moved by the level rather than written down, so the `from` column names the
    /// level instead of sending a reader to a line that is not in the file.
    pub from_level: Option<Level>,
}

impl Row {
    pub fn source(&self, settings: &Resolved) -> String {
        if self.is_default {
            return "default".to_owned();
        }
        if let Some(level) = self.from_level {
            return format!("level:{level}");
        }
        let named = settings
            .origin_of(self.table, self.key)
            .map_or_else(|| "flag".to_owned(), |origin| origin.label());
        if self
            .key
            .is_some_and(|key| settings.is_locked(self.table, key))
        {
            return format!("{named} \u{b7} locked");
        }
        named
    }
}

fn row(
    name: &'static str,
    table: &'static str,
    setting: &str,
    value: String,
    is_default: bool,
) -> Row {
    Row {
        name,
        value,
        table,
        setting: setting.to_owned(),
        key: None,
        is_default,
        from_level: None,
    }
}

pub fn rows(settings: &Resolved) -> Vec<Row> {
    let config = &settings.config;
    let level = settings.level();
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
            "languages.enabled",
            languages,
            config.languages.enabled.is_empty(),
        ),
        row(
            "rules enabled",
            "rules",
            "rules",
            format!("{} of {}", settings.rules_enabled(), RuleId::ALL.len()),
            untouched,
        ),
        row(
            "pattern advice",
            "patterns",
            "patterns",
            format!("{advisories} of {}", rqx_core::AdviceId::ALL.len()),
            advice_untouched,
        ),
        row(
            "threads",
            "scan",
            "scan.threads",
            threads(config),
            config.scan.threads == 0,
        ),
        row(
            "display cap",
            "scan",
            "scan.max_findings",
            config.scan.max_findings.to_string(),
            config.scan.max_findings == default.scan.max_findings,
        ),
        row(
            "max file size",
            "scan",
            "scan.max_filesize",
            format!("{} KB", config.scan.max_filesize / 1024),
            config.scan.max_filesize == default.scan.max_filesize,
        ),
        row(
            "churn window",
            "scan",
            "scan.churn_days",
            format!("{} days", config.scan.churn_days),
            config.scan.churn_days == default.scan.churn_days,
        ),
        row(
            "cache",
            "scan",
            "scan.cache",
            config.scan.cache.to_string(),
            !config.scan.cache,
        ),
        row(
            "pruned directories",
            "scan",
            "scan.prune_dirs",
            config.scan.prune_dirs.len().to_string(),
            config.scan.prune_dirs == default.scan.prune_dirs,
        ),
        row(
            "skip submodules",
            "scan",
            "scan.skip_submodules",
            config.scan.skip_submodules.to_string(),
            config.scan.skip_submodules == default.scan.skip_submodules,
        ),
        row(
            "skip minified",
            "scan",
            "scan.skip_minified",
            config.scan.skip_minified.to_string(),
            config.scan.skip_minified == default.scan.skip_minified,
        ),
        row(
            "ignored globs",
            "ignore",
            "ignore.patterns",
            config.ignore.patterns.len().to_string(),
            config.ignore.patterns == default.ignore.patterns,
        ),
        row(
            "entry points",
            "entrypoints",
            "entrypoints.paths",
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
            "gate.fail_on",
            config.gate.fail_on.slug().to_owned(),
            matches!(config.gate.fail_on, GateSeverity::Error),
        ),
        row(
            "gate: allowed",
            "gate",
            "gate.max_findings",
            config.gate.max_findings.to_string(),
            config.gate.max_findings == 0,
        ),
        row(
            "gate: min health",
            "gate",
            "gate.min_health",
            config.gate.min_health.to_string(),
            config.gate.min_health == 0,
        ),
        row(
            "gate: min grade",
            "gate",
            "gate.min_grade",
            min_grade(config.gate.min_grade).to_owned(),
            matches!(config.gate.min_grade, GateGrade::Never),
        ),
        row(
            "level",
            "level",
            "level",
            format!("{} \u{2014} {}", level, level.summary()),
            level == Level::Standard,
        ),
        row(
            "fix: apply",
            "fix",
            "fix.apply",
            if config.fix.apply.is_empty() {
                "every kind".to_owned()
            } else {
                config
                    .fix
                    .apply
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            config.fix.apply.is_empty(),
        ),
        row(
            "fix: disabled",
            "fix",
            "fix.disabled",
            if config.fix.disabled.is_empty() {
                "none".to_owned()
            } else {
                config
                    .fix
                    .disabled
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            config.fix.disabled.is_empty(),
        ),
        row(
            "fix: max rounds",
            "fix",
            "fix.max_rounds",
            config.fix.max_rounds.to_string(),
            config.fix.max_rounds == 3,
        ),
        row(
            "fix: min confidence",
            "fix",
            "fix.min_confidence",
            format!("{:?}", config.fix.min_confidence).to_lowercase(),
            config.fix.min_confidence == Confidence::Verified,
        ),
    ];

    for (name, key, value, fallback) in threshold_rows(thresholds, &defaults) {
        let mut entry = row(
            name,
            "thresholds",
            &format!("thresholds.{key}"),
            value.to_string(),
            value == fallback,
        );
        entry.key = Some(key);
        // A threshold the level moved is not one this project chose, and naming a file
        // would send a reader to a line that is not in it.
        if value != fallback && !settings.was_written("thresholds", key) {
            entry.from_level = Some(level);
        }
        rows.push(entry);
    }
    rows
}

fn threads(config: &RqxConfig) -> String {
    match config.scan.threads {
        0 => "auto".to_owned(),
        count => count.to_string(),
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
fn threshold_rows(
    values: &Thresholds,
    defaults: &Thresholds,
) -> Vec<(&'static str, &'static str, u32, u32)> {
    vec![
        (
            "long function",
            "long_function_statements",
            values.long_function_statements,
            defaults.long_function_statements,
        ),
        (
            "cognitive complexity",
            "cognitive_complexity",
            values.cognitive_complexity,
            defaults.cognitive_complexity,
        ),
        (
            "nesting depth",
            "nesting_depth",
            values.nesting_depth,
            defaults.nesting_depth,
        ),
        (
            "max parameters",
            "max_params",
            values.max_params,
            defaults.max_params,
        ),
        (
            "class members",
            "class_members",
            values.class_members,
            defaults.class_members,
        ),
        (
            "class statements",
            "class_statements",
            values.class_statements,
            defaults.class_statements,
        ),
        (
            "clone statements",
            "clone_min_statements",
            values.clone_min_statements,
            defaults.clone_min_statements,
        ),
        (
            "clone nodes",
            "clone_min_nodes",
            values.clone_min_nodes,
            defaults.clone_min_nodes,
        ),
        (
            "condition operands",
            "condition_operands",
            values.condition_operands,
            defaults.condition_operands,
        ),
        (
            "message chain depth",
            "message_chain_depth",
            values.message_chain_depth,
            defaults.message_chain_depth,
        ),
        (
            "commented code lines",
            "commented_code_lines",
            values.commented_code_lines,
            defaults.commented_code_lines,
        ),
        (
            "magic number repeats",
            "magic_number_repeats",
            values.magic_number_repeats,
            defaults.magic_number_repeats,
        ),
        (
            "class public members",
            "class_public_members",
            values.class_public_members,
            defaults.class_public_members,
        ),
        (
            "feature envy ratio",
            "feature_envy_ratio",
            values.feature_envy_ratio,
            defaults.feature_envy_ratio,
        ),
        (
            "data clump size",
            "data_clump_size",
            values.data_clump_size,
            defaults.data_clump_size,
        ),
        (
            "data clump sites",
            "data_clump_sites",
            values.data_clump_sites,
            defaults.data_clump_sites,
        ),
        (
            "short name scope",
            "short_name_scope_lines",
            values.short_name_scope_lines,
            defaults.short_name_scope_lines,
        ),
        (
            "slow test",
            "slow_test_ms",
            values.slow_test_ms,
            defaults.slow_test_ms,
        ),
        (
            "test decisions",
            "test_decisions",
            values.test_decisions,
            defaults.test_decisions,
        ),
        (
            "boundary repeats",
            "boundary_repeats",
            values.boundary_repeats,
            defaults.boundary_repeats,
        ),
        (
            "declaration distance",
            "declaration_distance_lines",
            values.declaration_distance_lines,
            defaults.declaration_distance_lines,
        ),
        (
            "expression operations",
            "expression_operations",
            values.expression_operations,
            defaults.expression_operations,
        ),
        (
            "configuration depth",
            "configuration_depth",
            values.configuration_depth,
            defaults.configuration_depth,
        ),
        (
            "loop nesting",
            "loop_nesting",
            values.loop_nesting,
            defaults.loop_nesting,
        ),
    ]
}

/// `rqx config`. The list of files comes first because that is the question a
/// surprising value raises, and answering it by inspection means guessing.
pub fn render(settings: &Resolved, palette: Palette, out: &mut dyn Write) -> Result<()> {
    let dim = palette.style(Ink::Muted);

    writeln!(out)?;
    if settings.sources.is_empty() {
        writeln!(
            out,
            "  {}",
            paint(dim, "no rqx.yaml found; every value below is a default")
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
            paint(palette.bold(Ink::Accent), "read, in order")
        )?;
        for path in &settings.sources {
            writeln!(out, "    {}", path.display())?;
        }
        if !settings.presets.is_empty() {
            writeln!(
                out,
                "    {} {}",
                paint(dim, "built on"),
                settings.presets.join(", ")
            )?;
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

#[derive(Serialize)]
struct JsonSetting<'a> {
    kind: &'static str,
    key: &'a str,
    value: Value,
    from: String,
    default: bool,
}

#[derive(Serialize)]
struct JsonSummary {
    kind: &'static str,
    schema_version: u32,
    files: Vec<String>,
}

/// The value as the config holds it, so a script compares a number with a number. A
/// row that summarises a whole table, such as how many rules run, has no one value to
/// give and carries the words the table report uses.
fn typed_value(entry: &Row, config: &Value) -> Value {
    entry
        .setting
        .split('.')
        .try_fold(config, |at, part| at.get(part))
        .filter(|found| !found.is_object())
        .cloned()
        .unwrap_or_else(|| Value::String(entry.value.clone()))
}

/// `rqx config --json`: one line per setting, then one for the files they came from.
pub fn render_json(settings: &Resolved, out: &mut dyn Write) -> Result<()> {
    let config = serde_json::to_value(&settings.config)?;
    for entry in rows(settings) {
        let line = JsonSetting {
            kind: "setting",
            key: &entry.setting,
            value: typed_value(&entry, &config),
            from: entry.source(settings),
            default: entry.is_default,
        };
        writeln!(out, "{}", serde_json::to_string(&line)?)?;
    }
    let summary = JsonSummary {
        kind: "summary",
        schema_version: super::SCHEMA_VERSION,
        files: settings
            .sources
            .iter()
            .map(|file| super::validation::shown_path(file))
            .collect(),
    };
    writeln!(out, "{}", serde_json::to_string(&summary)?)?;
    Ok(())
}

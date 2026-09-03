use std::collections::BTreeSet;

use serde::Deserialize;

use crate::rule::{Category, RuleId};

pub const CONFIG_FILE_NAME: &str = "rqx.toml";

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RqxConfig {
    pub thresholds: Thresholds,
    pub rules: Rules,
    pub ignore: Ignore,
    pub scan: Scan,
    pub entrypoints: Entrypoints,
    pub gate: Gate,
}

/// What makes `rqx check` fail. Defaults are deliberately permissive: a gate that
/// fails on day one gets disabled on day one, and a disabled gate checks nothing.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Gate {
    /// Findings allowed in the reviewed set before the check fails.
    pub max_findings: u32,
    /// Lowest severity that counts against the gate.
    pub fail_on: GateSeverity,
    /// Health score below which the check fails; 0 disables the score gate.
    pub min_health: u32,
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GateSeverity {
    Error,
    Warning,
    Info,
    Never,
}

impl Default for Gate {
    fn default() -> Self {
        Self { max_findings: 0, fail_on: GateSeverity::Error, min_health: 0 }
    }
}

/// Every threshold is the highest acceptable value: a finding fires strictly
/// above it. Mixing `>` and `>=` across rules makes a configured number mean
/// different things depending on which rule reads it.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Thresholds {
    pub long_function_statements: u32,
    pub cognitive_complexity: u32,
    pub nesting_depth: u32,
    pub max_params: u32,
    pub class_members: u32,
    pub class_statements: u32,
    pub clone_min_statements: u32,
    /// A floor on AST nodes rather than statements, to reject a block of three
    /// bare identifiers. Statement count is the primary filter: candidates are
    /// only taken at extractable block boundaries, where a real three-statement
    /// block runs to roughly twenty nodes.
    pub clone_min_nodes: u32,
    pub condition_operands: u32,
    pub message_chain_depth: u32,
    pub commented_code_lines: u32,
    pub magic_number_repeats: u32,
    pub class_public_members: u32,
    pub feature_envy_ratio: u32,
    pub data_clump_size: u32,
    pub data_clump_sites: u32,
    pub short_name_scope_lines: u32,
}

/// Modules the project is entered through. Anything reachable from one is live by
/// definition, so these are read before an export is called unused.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Entrypoints {
    pub paths: Vec<String>,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            long_function_statements: 40,
            cognitive_complexity: 15,
            nesting_depth: 3,
            max_params: 4,
            class_members: 20,
            class_statements: 200,
            clone_min_statements: 3,
            clone_min_nodes: 12,
            condition_operands: 3,
            message_chain_depth: 3,
            commented_code_lines: 2,
            magic_number_repeats: 3,
            class_public_members: 20,
            feature_envy_ratio: 3,
            data_clump_size: 3,
            data_clump_sites: 3,
            short_name_scope_lines: 25,
        }
    }
}

/// Rules and whole families can be switched on or off by name. A rule marked
/// `off_by_default` in the catalogue stays off unless it is named in `enabled`,
/// which keeps the noisy-but-real rules available without inflicting them.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Rules {
    pub disabled: BTreeSet<String>,
    pub enabled: BTreeSet<String>,
    /// When non-empty, only these families run.
    pub categories: BTreeSet<String>,
}

impl Rules {
    pub fn enabled(&self, rule: RuleId) -> bool {
        let spec = rule.spec();
        if self.disabled.contains(spec.slug) || self.disabled.contains(spec.category.slug()) {
            return false;
        }
        if !self.categories.is_empty() && !self.categories.contains(spec.category.slug()) {
            return false;
        }
        if spec.off_by_default {
            return self.enabled.contains(spec.slug) || self.enabled.contains(spec.category.slug());
        }
        true
    }

    pub fn needs_semantics(&self) -> bool {
        RuleId::ALL
            .iter()
            .copied()
            .filter(|rule| rule.spec().category == Category::Dispensables
                || rule.spec().category == Category::Couplers
                || rule.spec().category == Category::OoAbusers)
            .any(|rule| self.enabled(rule))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ignore {
    pub patterns: Vec<String>,
}

/// Tests are deliberately absent from the defaults. An ignored file is never
/// parsed, so its imports are invisible to the module graph, and a helper used
/// only by its own test would be reported as an unused export. Ignoring tests
/// trades a few findings for a class of false ones.
impl Default for Ignore {
    fn default() -> Self {
        Self {
            patterns: ["**/*.d.ts", "**/generated/**", "**/*.generated.*", "**/*.gen.ts"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

/// `prune_dirs` exists separately from `Ignore::patterns` because a glob still
/// requires descending into a directory to produce the paths it then rejects,
/// which on a populated `node_modules` is the difference between skipping a
/// directory and walking every file in it.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scan {
    pub threads: usize,
    pub max_filesize: u64,
    pub max_findings: usize,
    pub prune_dirs: Vec<String>,
    /// Days of git history to read for churn; 0 turns the signal off.
    pub churn_days: u32,
    /// Reuse `.rqx/cache.json` for files whose contents have not changed. Off by
    /// default because it does not currently pay: on a 12k-file repository a cold
    /// run costs 1.54s with the cache against 0.72s without, and a fully warm run
    /// still costs 0.73s — oxc parses a file faster than serde deserializes its
    /// cached findings. Worth revisiting if a slower analysis stage lands.
    pub cache: bool,
}

impl Default for Scan {
    fn default() -> Self {
        Self {
            threads: 0,
            max_filesize: 1 << 20,
            max_findings: 500,
            churn_days: 90,
            cache: false,
            prune_dirs: ["node_modules", "dist", "build", "out", "coverage", ".git", "vendor"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

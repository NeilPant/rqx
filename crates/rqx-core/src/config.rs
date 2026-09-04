use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;

use crate::advice::{AdviceId, Stance};
use crate::language::Language;
use crate::pattern::{Pattern, PatternFamily};
use crate::rule::{Analysis, Autofix, Category, RuleId};
use crate::score::Cleanliness;

/// Uppercase and unmistakable, the way `README`, `LICENSE` and `Dockerfile` are: a
/// file that decides how a whole repository is judged should be visible at the top of
/// a directory listing rather than sorted in among the source.
pub const CONFIG_FILE_NAME: &str = "RQX.toml";

/// The name Rqx used before the convention settled. Still read where it is the only
/// one present, so an existing checkout keeps working.
pub const LEGACY_CONFIG_FILE_NAME: &str = "rqx.toml";

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RqxConfig {
    /// Stops the search for further config files above this one, for a repository
    /// checked out inside another one.
    pub root: bool,
    pub languages: Languages,
    pub thresholds: Thresholds,
    pub rules: Rules,
    pub patterns: Patterns,
    pub ignore: Ignore,
    pub scan: Scan,
    pub entrypoints: Entrypoints,
    pub gate: Gate,
    pub fix: Fix,
}

impl RqxConfig {
    /// Rules and advisories read the same collectors, so a stage runs when either wants it.
    /// Asking `rules` alone is what would leave the pattern advisories looking at facts
    /// nothing had collected.
    pub fn needs_semantics(&self) -> bool {
        self.rules.needs_semantics() || self.patterns.needs(Analysis::Semantics)
    }

    pub fn needs_modules(&self) -> bool {
        self.rules.needs_modules() || self.patterns.needs(Analysis::Modules)
    }

    /// A name that resolves to nothing would otherwise be a silent no-op, or worse: a
    /// `categories` allowlist naming only unknown families turns every rule off.
    pub fn unknown_names(&self) -> Vec<UnknownName> {
        let mut unknown = self.rules.unknown_names();
        unknown.extend(self.patterns.unknown_names());
        unknown.extend(self.fix.unknown_names());
        unknown
    }
}

/// Which mechanical fixes `rqx fix` may apply. A project that keeps commented-out
/// code as documentation should not have it deleted, and naming the rule here beats
/// remembering a flag per run.
#[derive(Clone, Debug, Deserialize, Hash)]
#[serde(default, deny_unknown_fields)]
pub struct Fix {
    /// Autofix kinds, or individual rule slugs. Empty applies every kind.
    pub apply: BTreeSet<String>,
    pub disabled: BTreeSet<String>,
    /// One fix can make the next possible, so fixing repeats. A cap rather than a
    /// fixed point, because a rule pair that undid each other would never settle.
    pub max_rounds: u32,
}

impl Default for Fix {
    fn default() -> Self {
        Self {
            apply: BTreeSet::new(),
            disabled: BTreeSet::new(),
            max_rounds: 3,
        }
    }
}

impl Fix {
    pub fn allows(&self, rule: RuleId) -> bool {
        let spec = rule.spec();
        if !spec.autofix.is_automatic() {
            return false;
        }
        if self.disabled.contains(spec.slug)
            || self.disabled.contains(spec.category.slug())
            || self.disabled.contains(spec.autofix.slug())
        {
            return false;
        }
        self.apply.is_empty()
            || self.apply.contains(spec.slug)
            || self.apply.contains(spec.category.slug())
            || self.apply.contains(spec.autofix.slug())
    }

    pub fn unknown_names(&self) -> Vec<UnknownName> {
        let known = |name: &str| {
            RuleId::parse(name).is_some()
                || Category::parse(name).is_some()
                || Autofix::parse(name).is_some()
        };
        unknown_in("fix", "apply", self.apply.iter(), known)
            .chain(unknown_in("fix", "disabled", self.disabled.iter(), known))
            .collect()
    }
}

/// The family slugs the catalogue carried before it took its own names, so a config
/// written against the old ones says what to write instead.
const RENAMED_FAMILIES: [(&str, Category); 5] = [
    ("bloaters", Category::Size),
    ("oo-abusers", Category::Abstraction),
    ("change-preventers", Category::Change),
    ("dispensables", Category::Waste),
    ("couplers", Category::Coupling),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownName {
    pub table: &'static str,
    pub key: &'static str,
    pub name: String,
    pub renamed_to: Option<&'static str>,
}

impl fmt::Display for UnknownName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` in [{}] {} names nothing",
            self.name, self.table, self.key
        )?;
        match self.renamed_to {
            Some(new) => write!(f, "; that family is now `{new}`"),
            None => write!(
                f,
                "; `rqx rules` and `rqx patterns --coverage` list every name"
            ),
        }
    }
}

fn unknown_in<'a>(
    table: &'static str,
    key: &'static str,
    names: impl Iterator<Item = &'a String>,
    known: impl Fn(&str) -> bool,
) -> impl Iterator<Item = UnknownName> {
    names
        .filter(move |name| !known(name))
        .map(move |name| UnknownName {
            table,
            key,
            name: name.clone(),
            renamed_to: RENAMED_FAMILIES
                .iter()
                .find(|(old, _)| old == name)
                .map(|(_, category)| category.slug()),
        })
}

/// A starting `RQX.toml`, every table present and every value at the default the
/// code already uses. Written out rather than generated because a generator would
/// have to carry the prose anyway; the test below is what keeps it honest.
pub const TEMPLATE: &str = include_str!("rqx.template.toml");

pub const TABLES: [&str; 9] = [
    "languages",
    "thresholds",
    "rules",
    "patterns",
    "ignore",
    "scan",
    "entrypoints",
    "gate",
    "fix",
];

/// Only these three, because every other table describes the run as a whole: the walker
/// is built once and the gate reaches one verdict, so honouring a scoped `[scan]` would
/// mean walking the tree twice.
pub const SCOPED_TABLES: [&str; 3] = ["thresholds", "rules", "patterns"];

/// Which languages a scan reads. Empty means all of them: a project that only holds
/// one language pays nothing for the others, because the file walker never yields a
/// path no language claims.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Languages {
    pub enabled: Vec<Language>,
}

impl Languages {
    pub fn enabled(&self) -> Vec<Language> {
        if self.enabled.is_empty() {
            return Language::ALL.to_vec();
        }
        let mut chosen = self.enabled.clone();
        chosen.sort();
        chosen.dedup();
        chosen
    }

    pub fn includes(&self, language: Language) -> bool {
        self.enabled.is_empty() || self.enabled.contains(&language)
    }
}

/// What makes `rqx check` fail. Defaults are deliberately permissive: a gate that
/// fails on day one gets disabled on day one, and a disabled gate checks nothing.
#[derive(Clone, Debug, Deserialize, Hash)]
#[serde(default, deny_unknown_fields)]
pub struct Gate {
    /// Findings allowed in the reviewed set before the check fails.
    pub max_findings: u32,
    /// Lowest severity that counts against the gate.
    pub fail_on: GateSeverity,
    /// Health score below which the check fails; 0 disables the score gate.
    pub min_health: u32,
    /// Lowest grade any single file may carry. Catches a file rotting on its own,
    /// which a project-wide score averages away.
    pub min_grade: GateGrade,
    /// Fail on a design pattern that is half applied. Off by default and the only way
    /// advice reaches an exit code: a pattern that would merely fit is not a defect,
    /// and turning this on for everyone would change what every existing pipeline
    /// returns.
    pub fail_on_incomplete: bool,
}

#[derive(Copy, Clone, Debug, Deserialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GateGrade {
    Pristine,
    Clean,
    Fair,
    Poor,
    Critical,
    Never,
}

impl GateGrade {
    pub const fn floor(self) -> Option<Cleanliness> {
        match self {
            Self::Never => None,
            Self::Pristine => Some(Cleanliness::Pristine),
            Self::Clean => Some(Cleanliness::Clean),
            Self::Fair => Some(Cleanliness::Fair),
            Self::Poor => Some(Cleanliness::Poor),
            Self::Critical => Some(Cleanliness::Critical),
        }
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GateSeverity {
    Error,
    Warning,
    Info,
    Never,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            max_findings: 0,
            fail_on: GateSeverity::Error,
            min_health: 0,
            min_grade: GateGrade::Never,
            fail_on_incomplete: false,
        }
    }
}

/// Every threshold is the highest acceptable value: a finding fires strictly
/// above it. Mixing `>` and `>=` across rules makes a configured number mean
/// different things depending on which rule reads it.
#[derive(Clone, Debug, Deserialize, Hash)]
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
    /// A pause in a test longer than this is wall clock on every run.
    pub slow_test_ms: u32,
    pub boundary_repeats: u32,
    /// Lines a reader must hold a binding in mind before its first use is too far.
    pub declaration_distance_lines: u32,
    /// Operations packed into one unnamed expression before it stops being readable.
    pub expression_operations: u32,
    /// Block nesting at which a configuration literal counts as buried.
    pub configuration_depth: u32,
}

impl Thresholds {
    /// Which rules each threshold moves. A number in a config file is not an answer on
    /// its own: "what will change if I raise this" is the question, and nothing else in
    /// the tool could answer it.
    pub const TUNES: [(&'static str, &'static [RuleId]); 22] = [
        ("long_function_statements", &[RuleId::LongFunction]),
        ("cognitive_complexity", &[RuleId::CognitiveComplexity]),
        ("nesting_depth", &[RuleId::DeepNesting]),
        ("max_params", &[RuleId::LongParameterList]),
        ("class_members", &[RuleId::LargeClass]),
        ("class_statements", &[RuleId::LargeClass]),
        ("clone_min_statements", &[RuleId::DuplicateCode]),
        ("clone_min_nodes", &[RuleId::DuplicateCode]),
        ("condition_operands", &[RuleId::ComplexCondition]),
        ("message_chain_depth", &[RuleId::MessageChain]),
        ("commented_code_lines", &[RuleId::CommentedOutCode]),
        ("magic_number_repeats", &[RuleId::MagicNumber]),
        // Nothing reads this one yet: the count it would compare against is collected
        // but no rule reports on it. Kept so an existing config still parses.
        ("class_public_members", &[]),
        ("feature_envy_ratio", &[RuleId::FeatureEnvy]),
        ("data_clump_size", &[RuleId::DataClump]),
        ("data_clump_sites", &[RuleId::DataClump]),
        ("short_name_scope_lines", &[RuleId::ShortNameWideScope]),
        ("slow_test_ms", &[RuleId::SlowTest]),
        ("boundary_repeats", &[RuleId::BoundaryArithmetic]),
        ("declaration_distance_lines", &[RuleId::DistantDeclaration]),
        ("expression_operations", &[RuleId::ObscuredExpression]),
        ("configuration_depth", &[RuleId::BuriedConfiguration]),
    ];

    /// The key that tunes a rule, for the rule's own documentation.
    pub fn tuned_by(rule: RuleId) -> Vec<&'static str> {
        Self::TUNES
            .iter()
            .filter(|(_, rules)| rules.contains(&rule))
            .map(|(key, _)| *key)
            .collect()
    }
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
            slow_test_ms: 100,
            boundary_repeats: 3,
            declaration_distance_lines: 20,
            expression_operations: 6,
            configuration_depth: 3,
        }
    }
}

/// Rules and whole families can be switched on or off by name. A rule marked
/// `off_by_default` in the catalogue stays off unless it is named in `enabled`,
/// which keeps the noisy-but-real rules available without inflicting them.
#[derive(Clone, Debug, Default, Deserialize, Hash)]
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
        self.needs(Analysis::Semantics)
    }

    /// Resolving the import graph costs a walk of every module's specifiers, so it
    /// runs only when a rule actually reads it.
    pub fn needs_modules(&self) -> bool {
        self.needs(Analysis::Modules)
    }

    fn needs(&self, level: Analysis) -> bool {
        RuleId::ALL
            .iter()
            .copied()
            .filter(|rule| rule.spec().analysis >= level)
            .any(|rule| self.enabled(rule))
    }

    pub fn unknown_names(&self) -> Vec<UnknownName> {
        let rule_or_family =
            |name: &str| RuleId::parse(name).is_some() || Category::parse(name).is_some();
        let family = |name: &str| Category::parse(name).is_some();
        unknown_in("rules", "disabled", self.disabled.iter(), rule_or_family)
            .chain(unknown_in(
                "rules",
                "enabled",
                self.enabled.iter(),
                rule_or_family,
            ))
            .chain(unknown_in(
                "rules",
                "categories",
                self.categories.iter(),
                family,
            ))
            .collect()
    }
}

/// Addressable four ways because the useful question differs: by advisory when one
/// claim is wrong for this project, by pattern when the whole pattern is, by family,
/// and by stance when a project wants the half-applied patterns and not the
/// suggestions.
#[derive(Clone, Debug, Default, Deserialize, Hash)]
#[serde(default, deny_unknown_fields)]
pub struct Patterns {
    pub disabled: BTreeSet<String>,
    pub enabled: BTreeSet<String>,
    /// When non-empty, only these families run.
    pub families: BTreeSet<String>,
    /// When non-empty, only these stances are reported.
    pub stances: BTreeSet<String>,
}

impl Patterns {
    pub fn enabled(&self, advisory: AdviceId) -> bool {
        let spec = advisory.spec();
        let pattern = spec.pattern;
        let named = |set: &BTreeSet<String>| {
            set.contains(spec.slug)
                || set.contains(pattern.slug())
                || set.contains(pattern.family().slug())
        };
        if named(&self.disabled) {
            return false;
        }
        if !self.families.is_empty() && !self.families.contains(pattern.family().slug()) {
            return false;
        }
        if !self.stances.is_empty() && !self.stances.contains(spec.stance.slug()) {
            return false;
        }
        if spec.off_by_default {
            return named(&self.enabled);
        }
        true
    }

    pub fn any_enabled(&self) -> bool {
        AdviceId::ALL
            .iter()
            .copied()
            .any(|advisory| self.enabled(advisory))
    }

    pub fn needs(&self, level: Analysis) -> bool {
        AdviceId::ALL
            .iter()
            .copied()
            .filter(|advisory| advisory.spec().analysis >= level)
            .any(|advisory| self.enabled(advisory))
    }

    pub fn unknown_names(&self) -> Vec<UnknownName> {
        let addressable = |name: &str| {
            AdviceId::parse(name).is_some()
                || Pattern::parse(name).is_some()
                || PatternFamily::parse(name).is_some()
        };
        let family = |name: &str| PatternFamily::parse(name).is_some();
        let stance = |name: &str| Stance::parse(name).is_some();
        unknown_in("patterns", "disabled", self.disabled.iter(), addressable)
            .chain(unknown_in(
                "patterns",
                "enabled",
                self.enabled.iter(),
                addressable,
            ))
            .chain(unknown_in(
                "patterns",
                "families",
                self.families.iter(),
                family,
            ))
            .chain(unknown_in(
                "patterns",
                "stances",
                self.stances.iter(),
                stance,
            ))
            .collect()
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
            patterns: [
                "**/*.d.ts",
                "**/generated/**",
                "**/*.generated.*",
                "**/*.gen.ts",
                // Generated code is nobody's to fix: a finding in it is a finding
                // about the generator, reported against the wrong file.
                "**/*_pb2.py",
                "**/*_pb2.pyi",
                "**/*.pb.go",
                "**/*_gen.go",
                "**/*.pb.h",
                "**/*.pb.cc",
            ]
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
    pub skip_generated: bool,
    pub max_filesize: u64,
    pub max_findings: usize,
    pub prune_dirs: Vec<String>,
    /// Days of git history to read for churn; 0 turns the signal off.
    pub churn_days: u32,
    /// Reuse `.rqx/cache.json` for files whose contents have not changed. Off by
    /// default because it does not currently pay: on a 12k-file repository a cold
    /// run costs 1.54s with the cache against 0.72s without, and a fully warm run
    /// still costs 0.73s — parsing a file is faster than serde deserializing its
    /// cached findings. Worth revisiting if a slower analysis stage lands.
    pub cache: bool,
}

impl Default for Scan {
    fn default() -> Self {
        Self {
            threads: 0,
            skip_generated: true,
            max_filesize: 1 << 20,
            max_findings: 500,
            churn_days: 90,
            cache: false,
            // A glob still has to descend into a directory to produce the paths it
            // then rejects, which on a populated `node_modules` or `.venv` is the
            // difference between skipping a directory and walking every file in it.
            prune_dirs: [
                "node_modules",
                "dist",
                "build",
                "out",
                "coverage",
                ".git",
                "vendor",
                "__pycache__",
                ".venv",
                "venv",
                ".tox",
                ".mypy_cache",
                ".pytest_cache",
                "target",
                ".gradle",
                ".mvn",
                "bin",
                "obj",
                "cmake-build-debug",
                "cmake-build-release",
                "third_party",
            ]
            .map(str::to_owned)
            .to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key that names no field, or a field no row names, makes the table a lie the
    /// moment a threshold is added or renamed.
    #[test]
    fn every_threshold_appears_in_the_table_that_says_what_it_tunes() {
        let template: toml::Table = toml::from_str(TEMPLATE).expect("the template parses");
        let declared: Vec<&String> = template["thresholds"]
            .as_table()
            .expect("[thresholds] is a table")
            .keys()
            .collect();
        for (key, _) in Thresholds::TUNES {
            assert!(
                declared.iter().any(|name| *name == key),
                "{key} is not a threshold"
            );
        }
        for name in declared {
            assert!(
                Thresholds::TUNES.iter().any(|(key, _)| key == name),
                "{name} is not in Thresholds::TUNES"
            );
        }
    }

    /// The template is prose around a copy of the defaults, so the one thing that can
    /// go wrong is the copy drifting from what the code actually uses.
    #[test]
    fn a_renamed_family_says_its_new_name() {
        for (old, category) in RENAMED_FAMILIES {
            let mut rules = Rules::default();
            rules.categories.insert(old.to_owned());
            let unknown = rules.unknown_names();
            assert_eq!(unknown.len(), 1, "{old}");
            assert_eq!(unknown[0].renamed_to, Some(category.slug()), "{old}");
            assert!(
                unknown[0].to_string().contains(category.slug()),
                "{}",
                unknown[0]
            );
        }
    }

    #[test]
    fn every_real_name_is_known_and_a_made_up_one_is_not() {
        let mut rules = Rules::default();
        rules.disabled.insert(RuleId::ALL[0].slug().to_owned());
        rules.enabled.insert(Category::ALL[0].slug().to_owned());
        rules.disabled.insert("no-such-rule".to_owned());
        let unknown = rules.unknown_names();
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0].name, "no-such-rule");
        assert_eq!(unknown[0].renamed_to, None);

        let mut patterns = Patterns::default();
        patterns.stances.insert("upside-down".to_owned());
        patterns
            .families
            .insert(PatternFamily::ALL[0].slug().to_owned());
        assert_eq!(patterns.unknown_names().len(), 1);
    }

    #[test]
    fn the_template_states_the_defaults_it_claims_to() {
        let written: RqxConfig = toml::from_str(TEMPLATE).expect("the template parses");
        let defaults = RqxConfig::default();
        assert_eq!(
            toml::to_string(&Comparable::of(&written)).unwrap(),
            toml::to_string(&Comparable::of(&defaults)).unwrap()
        );
    }

    /// `RqxConfig` only derives `Deserialize`, so the two are compared through a
    /// serialisable stand-in rather than by growing the schema a `Serialize` it has no
    /// other use for.
    #[derive(serde::Serialize)]
    struct Comparable {
        root: bool,
        languages: Vec<String>,
        disabled: Vec<String>,
        enabled: Vec<String>,
        categories: Vec<String>,
        pattern_disabled: Vec<String>,
        pattern_enabled: Vec<String>,
        pattern_families: Vec<String>,
        pattern_stances: Vec<String>,
        thresholds: Vec<u32>,
        ignore: Vec<String>,
        entrypoints: Vec<String>,
        scan: Vec<String>,
        gate_advice: bool,
        gate: Vec<String>,
        fix: Vec<String>,
    }

    impl Comparable {
        fn of(config: &RqxConfig) -> Self {
            let t = &config.thresholds;
            Self {
                root: config.root,
                languages: config
                    .languages
                    .enabled
                    .iter()
                    .map(|l| format!("{l:?}"))
                    .collect(),
                disabled: config.rules.disabled.iter().cloned().collect(),
                enabled: config.rules.enabled.iter().cloned().collect(),
                categories: config.rules.categories.iter().cloned().collect(),
                pattern_disabled: config.patterns.disabled.iter().cloned().collect(),
                pattern_enabled: config.patterns.enabled.iter().cloned().collect(),
                pattern_families: config.patterns.families.iter().cloned().collect(),
                pattern_stances: config.patterns.stances.iter().cloned().collect(),
                thresholds: vec![
                    t.long_function_statements,
                    t.cognitive_complexity,
                    t.nesting_depth,
                    t.max_params,
                    t.class_members,
                    t.class_statements,
                    t.clone_min_statements,
                    t.clone_min_nodes,
                    t.condition_operands,
                    t.message_chain_depth,
                    t.commented_code_lines,
                    t.magic_number_repeats,
                    t.class_public_members,
                    t.feature_envy_ratio,
                    t.data_clump_size,
                    t.data_clump_sites,
                    t.short_name_scope_lines,
                    t.slow_test_ms,
                    t.boundary_repeats,
                    t.declaration_distance_lines,
                    t.expression_operations,
                    t.configuration_depth,
                ],
                ignore: config.ignore.patterns.clone(),
                entrypoints: config.entrypoints.paths.clone(),
                gate_advice: config.gate.fail_on_incomplete,
                scan: vec![
                    config.scan.threads.to_string(),
                    config.scan.skip_generated.to_string(),
                    config.scan.max_filesize.to_string(),
                    config.scan.max_findings.to_string(),
                    config.scan.churn_days.to_string(),
                    config.scan.cache.to_string(),
                    config.scan.prune_dirs.join(","),
                ],
                gate: vec![
                    format!("{:?}", config.gate.fail_on),
                    config.gate.max_findings.to_string(),
                    config.gate.min_health.to_string(),
                    format!("{:?}", config.gate.min_grade),
                ],
                fix: vec![
                    config
                        .fix
                        .apply
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(","),
                    config
                        .fix
                        .disabled
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(","),
                    config.fix.max_rounds.to_string(),
                ],
            }
        }
    }
}

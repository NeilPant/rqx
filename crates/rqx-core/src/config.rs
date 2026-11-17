use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::advice::{AdviceId, Stance};
use crate::finding::{Confidence, Severity};
use crate::language::Language;
use crate::pattern::{Pattern, PatternFamily};
use crate::rule::{Analysis, Autofix, Category, RuleId};
use crate::rule_options::RuleOptions;
use crate::score::Cleanliness;

/// The name `rqx init` writes. Lowercase, because YAML tooling and editors recognise
/// the extension rather than the capitalisation.
pub const CONFIG_FILE_NAME: &str = "rqx.yaml";

/// Every spelling rqx reads, in the order it looks. A directory holding two of them is
/// an error rather than a choice: which one wins would depend on this list.
pub const CONFIG_FILE_NAMES: [&str; 4] = ["rqx.yaml", ".rqx.yml", "rqx.yml", ".rqx.yaml"];

/// The TOML names rqx used to read. Finding one is a refusal, never a fallback: a
/// config that is silently skipped can loosen a CI gate without anyone choosing to.
pub const RETIRED_CONFIG_FILE_NAMES: [&str; 2] = ["RQX.toml", "rqx.toml"];

/// The first line of a config file `rqx init` or `rqx config migrate` writes, which
/// editors with a YAML language server read to offer completion and validation.
pub const SCHEMA_DIRECTIVE: &str = "# yaml-language-server: $schema=https://raw.githubusercontent.com/jadhamwi21/rqx/main/docs/rqx.schema.json";

/// The JSON Schema of a config file, derived from the same types the loader reads, so
/// the schema an editor validates against cannot disagree with what rqx accepts. A
/// field's doc comment is its description.
pub fn json_schema() -> serde_json::Value {
    let mut schema =
        serde_json::to_value(schemars::schema_for!(RqxConfig)).expect("a schema is always JSON");
    if let Some(root) = schema.as_object_mut() {
        root.insert("title".to_owned(), "rqx configuration".into());
    }
    describe_rules(&mut schema);
    unwrap_descriptions(&mut schema);
    schema
}

/// `rules` takes a key per rule, so the derived schema can only say "any key holding
/// rule options". Naming each rule lets an editor complete the slugs, refuse a
/// misspelt one, and offer exactly the thresholds that rule reads.
fn describe_rules(schema: &mut serde_json::Value) {
    let defs = &mut schema["$defs"];
    let options = defs["RuleOptions"].clone();
    let thresholds = defs["Thresholds"]["properties"].clone();
    let Some(rules) = defs["Rules"].as_object_mut() else {
        return;
    };
    rules.insert("additionalProperties".to_owned(), false.into());
    let properties = rules
        .entry("properties")
        .or_insert_with(|| serde_json::json!({}));
    for rule in RuleId::ALL.iter().copied() {
        let spec = rule.spec();
        let mut one = options.clone();
        one["description"] = format!("Settings for `{}`. {}", spec.slug, spec.why).into();
        one["additionalProperties"] = false.into();
        for key in Thresholds::tuned_by(rule) {
            one["properties"][key] = thresholds[key].clone();
        }
        properties[spec.slug] = one;
    }
}

/// Doc comments are wrapped at the width of the source, which a hover tooltip is not.
/// A single newline is a wrap; a blank line is a paragraph and stays.
fn unwrap_descriptions(node: &mut serde_json::Value) {
    match node {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                match value {
                    serde_json::Value::String(text) if key == "description" => {
                        *text = text
                            .split("\n\n")
                            .map(|paragraph| paragraph.replace('\n', " "))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                    }
                    other => unwrap_descriptions(other),
                }
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(unwrap_descriptions),
        _ => {}
    }
}

/// How much a project asks of itself. A level is a layer of settings merged *under*
/// everything written by hand, so naming one changes the defaults and never overrides
/// a value the project stated.
#[derive(
    Copy, Clone, Debug, Default, Deserialize, Hash, PartialEq, Eq, Serialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Relaxed,
    #[default]
    Standard,
    Strict,
    Pristine,
}

impl Level {
    pub const ALL: [Self; 4] = [Self::Relaxed, Self::Standard, Self::Strict, Self::Pristine];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Relaxed => "relaxed",
            Self::Standard => "standard",
            Self::Strict => "strict",
            Self::Pristine => "pristine",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::Relaxed => "legacy code that has to stay shippable while it is cleaned up",
            Self::Standard => "the catalogue as it stands",
            Self::Strict => "held to what a reader can follow without effort",
            Self::Pristine => "every rule, including the ones that are off by default",
        }
    }

    /// The whole of what a level changes, as the YAML it would take to write by hand.
    /// Standard is empty because it is the catalogue's own defaults.
    pub const fn layer(self) -> &'static str {
        match self {
            Self::Relaxed => include_str!("levels/relaxed.yaml"),
            Self::Standard => "",
            Self::Strict => include_str!("levels/strict.yaml"),
            Self::Pristine => include_str!("levels/pristine.yaml"),
        }
    }

    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.slug() == slug)
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RqxConfig {
    /// Stops the search for further config files above this one, for a repository
    /// checked out inside another one.
    pub root: bool,
    /// How much this project asks of itself: relaxed, standard, strict or pristine. A level is merged under everything written by hand, so it moves the defaults and never overrides a stated value.
    pub level: Level,
    /// Settings this file builds on, merged under its own keys, later entries over
    /// earlier ones. Each is a built-in preset (`recommended`, `strict`,
    /// `legacy-adoption`, `ci-gate`, `perf`, or a level name) or a path to another YAML
    /// file, relative to this one. Read by the loader; it never reaches a resolved
    /// config.
    pub extends: Vec<String>,
    pub languages: Languages,
    pub thresholds: Thresholds,
    pub rules: Rules,
    pub patterns: Patterns,
    pub ignore: Ignore,
    pub scan: Scan,
    pub entrypoints: Entrypoints,
    pub gate: Gate,
    pub fix: Fix,
    pub update: Update,
    pub architecture: Architecture,
    pub performance: Performance,
    /// Rules this project wrote, as `[[rule]]` blocks. The grammars are already in
    /// the binary; this is what lets a team ask a question rqx will never ship.
    #[serde(rename = "rule")]
    pub custom: Vec<CustomRule>,
    /// A shared policy file, relative to this one, that every repository pointing at
    /// it inherits. Read by the loader; it never reaches a resolved config.
    pub policy: Option<String>,
    /// Only meaningful in a policy: the keys a project may not change.
    pub locked: Option<Locked>,
    /// Settings for a subtree, matched by glob against this file's directory. Read by
    /// the loader, which turns each entry into its own resolved config.
    pub overrides: Vec<SubtreeOverride>,
}

/// Keys a policy forbids a project to change, as `table.key`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Locked {
    /// Keys, as `table.key`, that a project may not set to a different value.
    pub keys: Vec<String>,
}

/// What a subtree may say differently. Only thresholds, rules and patterns, because
/// the walker is built once and the gate reaches one verdict: honouring a scoped
/// `scan` would mean walking the tree twice.
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SubtreeOverride {
    /// Globs, relative to the directory of the file that wrote them.
    pub paths: Vec<String>,
    /// A subtree can be held to a different level: strict in the domain, relaxed in
    /// the scripts. The gate is not moved by it.
    pub level: Option<Level>,
    /// Thresholds for this subtree, merged over the project's.
    pub thresholds: Option<Thresholds>,
    /// Rule switches for this subtree, merged over the project's.
    pub rules: Option<Rules>,
    /// Pattern advice switches for this subtree, merged over the project's.
    pub patterns: Option<Patterns>,
}

/// A rule written as a tree-sitter query. The query is not compiled here — core has
/// no grammars — but every other field is checked the moment the config parses.
#[derive(Clone, Debug, Deserialize, Hash, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomRule {
    /// The rule's slug, used to disable it and to waive it.
    pub id: String,
    /// Languages the query runs against.
    pub languages: Vec<Language>,
    /// A tree-sitter query. Name the capture to report `@site`; without one the first capture is used.
    pub query: String,
    /// What the finding says.
    pub message: String,
    /// Defaults to `warning`.
    #[serde(default = "warning")]
    pub severity: Severity,
    /// The family the rule is filed under.
    pub family: Category,
    /// Defaults to `verified`: a query either matched or it did not.
    #[serde(default = "verified")]
    pub confidence: Confidence,
    /// What to do about it, shown beside the finding.
    #[serde(default)]
    pub hint: String,
}

const fn warning() -> Severity {
    Severity::Warning
}

/// A query either matched or it did not — there is no judgement in between, which is
/// what every other `Verified` rule means by the word.
const fn verified() -> Confidence {
    Confidence::Verified
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
        let mut unknown = self.rules.unknown_names(&self.custom);
        unknown.extend(self.patterns.unknown_names());
        unknown.extend(self.fix.unknown_names());
        unknown
    }
}

/// The direction dependencies are allowed to point. `layers` is ordered inner to
/// outer, and a file may import its own layer and any layer before it — so the domain
/// cannot reach for an adapter, whatever the import graph would otherwise allow.
///
/// Silent until a project declares it: an architecture nobody stated is not one rqx
/// should invent.
#[derive(Clone, Debug, Default, Deserialize, Hash, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Architecture {
    /// Layer names, ordered inner to outer. A file may import its own layer and any layer before it.
    pub layers: Vec<String>,
    /// Glob to what it belongs to. A sub-table because `flatten` and
    /// `deny_unknown_fields` cannot both apply to one struct, and losing the typo
    /// check would cost more than the extra line of YAML.
    pub paths: BTreeMap<String, Assignment>,
}

#[derive(Clone, Debug, Default, Deserialize, Hash, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Assignment {
    /// The layer this subtree belongs to; it must be named in `architecture.layers`.
    pub layer: Option<String>,
    /// Globs this subtree may not import from.
    pub may_not_import: Vec<String>,
    /// The only globs this subtree may import from.
    pub may_only_import: Vec<String>,
}

impl Architecture {
    pub fn is_declared(&self) -> bool {
        !self.paths.is_empty()
    }

    /// How far out a layer sits. `None` for a path in no declared layer, which is
    /// neither a violation nor a licence: an unassigned file is simply not governed.
    pub fn rank(&self, layer: &str) -> Option<usize> {
        self.layers.iter().position(|name| name == layer)
    }

    /// A layer named in `paths` that `layers` never declares would silently govern
    /// nothing, which is the failure mode the whole table exists to prevent.
    pub fn undeclared_layers(&self) -> Vec<String> {
        let mut unknown: Vec<String> = self
            .paths
            .values()
            .filter_map(|assignment| assignment.layer.as_ref())
            .filter(|layer| self.rank(layer).is_none())
            .cloned()
            .collect();
        unknown.sort();
        unknown.dedup();
        unknown
    }

    pub fn duplicate_layers(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.layers
            .iter()
            .filter(|layer| !seen.insert((*layer).clone()))
            .cloned()
            .collect()
    }
}

/// Whether rqx may ask GitHub if a newer release exists. The check runs on a thread
/// nobody waits for, caches its answer for a day, and never runs in CI — but a tool
/// that reaches the network at all should be answerable to a line in a config file.
#[derive(Clone, Debug, Deserialize, Hash, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Update {
    /// Ask GitHub whether a newer release exists.
    pub check: bool,
}

impl Default for Update {
    fn default() -> Self {
        Self { check: true }
    }
}

/// Which mechanical fixes `rqx fix` may apply. A project that keeps commented-out
/// code as documentation should not have it deleted, and naming the rule here beats
/// remembering a flag per run.
#[derive(Clone, Debug, Deserialize, Hash, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Fix {
    /// Autofix kinds, or individual rule slugs. Empty applies every kind.
    pub apply: BTreeSet<String>,
    /// Fixes to leave alone even when their kind is applied: kinds or rule slugs.
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
            "`{}` in `{}.{}` names nothing",
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

/// A starting `rqx.yaml`, every table present and every value at the default the
/// code already uses. Written out rather than generated because a generator would
/// have to carry the prose anyway; the test below is what keeps it honest.
pub const TEMPLATE: &str = include_str!("rqx.template.yaml");

pub const TABLES: [&str; 11] = [
    "languages",
    "thresholds",
    "rules",
    "patterns",
    "ignore",
    "scan",
    "entrypoints",
    "gate",
    "fix",
    "architecture",
    "performance",
];

/// Only these three, because every other table describes the run as a whole: the walker
/// is built once and the gate reaches one verdict, so honouring a scoped `scan` would
/// mean walking the tree twice.
pub const SCOPED_TABLES: [&str; 3] = ["thresholds", "rules", "patterns"];

/// Which languages a scan reads. Empty means all of them: a project that only holds
/// one language pays nothing for the others, because the file walker never yields a
/// path no language claims.
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Languages {
    /// Language names. Empty means all five.
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
#[derive(Clone, Debug, Deserialize, Hash, Serialize, schemars::JsonSchema)]
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
    /// Where the recorded debt lives. When it is set, only findings the baseline
    /// does not already hold count against `fail_on` and `max_findings` — which is
    /// what lets a codebase with ten years of history turn the gate on at all.
    /// `min_health` and `min_grade` still see everything.
    pub baseline: Option<String>,
    /// Fail when the total number of findings is above what the baseline recorded.
    /// A baseline says which debt is tolerated; this says it may not grow.
    pub ratchet: bool,
}

#[derive(Copy, Clone, Debug, Deserialize, Hash, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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

#[derive(Copy, Clone, Debug, Deserialize, Hash, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GateSeverity {
    Error,
    Warning,
    Info,
    Never,
}

impl GateSeverity {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Never => "never",
        }
    }
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            max_findings: 0,
            fail_on: GateSeverity::Error,
            min_health: 0,
            min_grade: GateGrade::Never,
            fail_on_incomplete: false,
            baseline: None,
            ratchet: false,
        }
    }
}

/// Every threshold is the highest acceptable value: a finding fires strictly
/// above it. Mixing `>` and `>=` across rules makes a configured number mean
/// different things depending on which rule reads it.
#[derive(Clone, Debug, Deserialize, Hash, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Thresholds {
    /// Statements in one function. Tunes `long-function`.
    pub long_function_statements: u32,
    /// Cognitive complexity of one function, which counts nesting as well as branches. Tunes `cognitive-complexity`.
    pub cognitive_complexity: u32,
    /// Levels of nested control flow in one function. Tunes `deep-nesting`.
    pub nesting_depth: u32,
    /// Parameters one function may take. Tunes `long-parameter-list`.
    pub max_params: u32,
    /// Members one class may declare. Tunes `large-class`.
    pub class_members: u32,
    /// Statements across all the methods of one class. Tunes `large-class`.
    pub class_statements: u32,
    /// Smallest repeated block, in statements, that is reported as duplicate code. Tunes `duplicate-code`.
    pub clone_min_statements: u32,
    /// Tunes `duplicate-code`. A floor on AST nodes rather than statements, to reject a block of three
    /// bare identifiers. Statement count is the primary filter: candidates are
    /// only taken at extractable block boundaries, where a real three-statement
    /// block runs to roughly twenty nodes.
    pub clone_min_nodes: u32,
    /// Operands in one boolean condition. Tunes `complex-condition`.
    pub condition_operands: u32,
    /// Calls chained on one object before the chain reaches too far. Tunes `message-chain`.
    pub message_chain_depth: u32,
    /// Consecutive lines of commented-out code. Tunes `commented-out-code`.
    pub commented_code_lines: u32,
    /// Times one unnamed number may repeat before it wants a name. Tunes `magic-number`.
    pub magic_number_repeats: u32,
    /// Public members one class may expose. Tunes `wide-surface`.
    pub class_public_members: u32,
    /// Modules one file may import. Tunes `wide-fan-out`.
    pub fan_out: u32,
    /// Modules that may import one file. Tunes `wide-fan-in`.
    pub fan_in: u32,
    /// Levels a relative import may climb. Tunes `deep-relative-import`.
    pub relative_climb: u32,
    /// Statements in one file across all its functions. Tunes `oversized-module`.
    pub module_statements: u32,
    /// Local variables one function may declare. Tunes `many-locals`.
    pub function_locals: u32,
    /// Statements inside one try block. Tunes `wide-try`.
    pub try_statements: u32,
    /// Consecutive lines each narrated by its own comment. Tunes `comment-per-line`.
    pub narrated_run: u32,
    /// How many times longer than the code it describes a comment may be. Tunes `explanation-longer-than-code`.
    pub explanation_ratio: u32,
    /// Guard clauses stacked at the top of one function. Tunes `guard-cascade`.
    pub guard_cascade: u32,
    /// Commits by one author, and no one else, before a file counts as single-author. Tunes `single-author-file`.
    pub sole_author_commits: u32,
    /// Commits to one file within `scan.churn_days` before it is a hotspot. Tunes `hotspot`.
    pub hotspot_commits: u32,
    /// Independent paths through one function. Tunes `cyclomatic-complexity`.
    pub cyclomatic_complexity: u32,
    /// Results one function may return. Tunes `many-results`.
    pub max_results: u32,
    /// Other classes one class may depend on. Tunes `many-collaborators`.
    pub class_collaborators: u32,
    /// Percentage of a function's statements that are logging. Tunes `over-logging`.
    pub log_percent: u32,
    /// Levels of inheritance above one class. Tunes `deep-inheritance`.
    pub inheritance_depth: u32,
    /// Days a TODO may stand before it is stale. Tunes `stale-todo`.
    pub todo_age_days: u32,
    /// Ways out of one function body, past the guard clauses at the top. Tunes `many-exits`.
    pub max_exits: u32,
    /// Arms in one switch before it stops being a table a reader scans. Tunes `wide-switch`.
    pub max_cases: u32,
    /// Arms below which a switch is an if spelled in more lines. Tunes `tiny-switch`, which is off by default.
    pub min_cases: u32,
    /// Function literals nested inside other function literals. Tunes `callback-nesting`.
    pub callback_depth: u32,
    /// How many times more a method uses another class's members than its own. Tunes `feature-envy`.
    pub feature_envy_ratio: u32,
    /// Parameters that travel together before they are a data clump. Tunes `data-clump`.
    pub data_clump_size: u32,
    /// Functions that must share the same group of parameters before it is a data clump. Tunes `data-clump`.
    pub data_clump_sites: u32,
    /// Lines a short name may stay in scope. Tunes `short-name-wide-scope`.
    pub short_name_scope_lines: u32,
    /// Milliseconds a test may pause: a longer pause is wall clock on every run. Tunes `slow-test`.
    pub slow_test_ms: u32,
    /// Times the same off-by-one boundary arithmetic may repeat. Tunes `boundary-arithmetic`.
    pub boundary_repeats: u32,
    /// Lines a reader must hold a binding in mind before its first use is too far. Tunes `distant-declaration`.
    pub declaration_distance_lines: u32,
    /// Operations packed into one unnamed expression before it stops being readable. Tunes `obscured-expression`.
    pub expression_operations: u32,
    /// Block nesting at which a configuration literal counts as buried. Tunes `buried-configuration`.
    pub configuration_depth: u32,
    /// Loops inside loops before the body's cost is a power of the input. Tunes `deep-loop-nesting`.
    pub loop_nesting: u32,
}

impl Thresholds {
    /// Which rules each threshold moves. A number in a config file is not an answer on
    /// its own: "what will change if I raise this" is the question, and nothing else in
    /// the tool could answer it.
    pub const TUNES: [(&'static str, &'static [RuleId]); 44] = [
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
        ("class_public_members", &[RuleId::WideSurface]),
        ("fan_out", &[RuleId::WideFanOut]),
        ("fan_in", &[RuleId::WideFanIn]),
        ("relative_climb", &[RuleId::DeepRelativeImport]),
        ("module_statements", &[RuleId::OversizedModule]),
        ("function_locals", &[RuleId::ManyLocals]),
        ("try_statements", &[RuleId::WideTry]),
        ("narrated_run", &[RuleId::CommentPerLine]),
        ("explanation_ratio", &[RuleId::ExplanationLongerThanCode]),
        ("guard_cascade", &[RuleId::GuardCascade]),
        ("sole_author_commits", &[RuleId::SingleAuthorFile]),
        ("hotspot_commits", &[RuleId::Hotspot]),
        ("cyclomatic_complexity", &[RuleId::CyclomaticComplexity]),
        ("max_results", &[RuleId::ManyResults]),
        ("class_collaborators", &[RuleId::ManyCollaborators]),
        ("log_percent", &[RuleId::OverLogging]),
        ("inheritance_depth", &[RuleId::DeepInheritance]),
        ("todo_age_days", &[RuleId::StaleTodo]),
        ("max_exits", &[RuleId::ManyExits]),
        ("max_cases", &[RuleId::WideSwitch]),
        ("min_cases", &[RuleId::TinySwitch]),
        ("callback_depth", &[RuleId::CallbackNesting]),
        ("feature_envy_ratio", &[RuleId::FeatureEnvy]),
        ("data_clump_size", &[RuleId::DataClump]),
        ("data_clump_sites", &[RuleId::DataClump]),
        ("short_name_scope_lines", &[RuleId::ShortNameWideScope]),
        ("slow_test_ms", &[RuleId::SlowTest]),
        ("boundary_repeats", &[RuleId::BoundaryArithmetic]),
        ("declaration_distance_lines", &[RuleId::DistantDeclaration]),
        ("expression_operations", &[RuleId::ObscuredExpression]),
        ("configuration_depth", &[RuleId::BuriedConfiguration]),
        ("loop_nesting", &[RuleId::DeepLoopNesting]),
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
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Entrypoints {
    /// Globs of the modules the project is entered through.
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
            fan_out: 20,
            fan_in: 20,
            relative_climb: 2,
            module_statements: 400,
            function_locals: 15,
            try_statements: 10,
            narrated_run: 4,
            explanation_ratio: 2,
            guard_cascade: 3,
            sole_author_commits: 6,
            hotspot_commits: 12,
            cyclomatic_complexity: 12,
            max_results: 3,
            class_collaborators: 8,
            log_percent: 40,
            inheritance_depth: 3,
            todo_age_days: 180,
            max_exits: 4,
            max_cases: 12,
            min_cases: 3,
            callback_depth: 3,
            feature_envy_ratio: 3,
            data_clump_size: 3,
            data_clump_sites: 3,
            short_name_scope_lines: 25,
            slow_test_ms: 100,
            boundary_repeats: 3,
            declaration_distance_lines: 20,
            expression_operations: 6,
            configuration_depth: 3,
            loop_nesting: 2,
        }
    }
}

/// Rules and whole families can be switched on or off by name. A rule marked
/// `off_by_default` in the catalogue stays off unless it is named in `enabled`,
/// which keeps the noisy-but-real rules available without inflicting them. A single
/// rule can also be given its own settings by writing its slug as a key, which is
/// why `Rules` is read by hand: see `rule_options`.
#[derive(Clone, Debug, Default, Hash, schemars::JsonSchema, Serialize)]
#[serde(default)]
pub struct Rules {
    /// Rule slugs or family slugs to switch off. `rqx rules` lists both.
    pub disabled: BTreeSet<String>,
    /// Rules that ship off, switched on by slug or family.
    pub enabled: BTreeSet<String>,
    /// When non-empty, only these families run.
    pub categories: BTreeSet<String>,
    /// Settings for one rule, keyed by its slug: its severity, whether it runs, the
    /// paths it applies to, its confidence, and the thresholds it reads.
    #[serde(flatten)]
    pub options: BTreeMap<String, RuleOptions>,
}

impl Rules {
    pub fn enabled(&self, rule: RuleId) -> bool {
        if let Some(switched) = self.switched(rule) {
            return switched;
        }
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

    /// A project's own rule, gated by its id or its family. Never off by default —
    /// writing one is the act of asking for it.
    pub fn enabled_custom(&self, rule: &CustomRule) -> bool {
        let family = rule.family.slug();
        if self.disabled.contains(&rule.id) || self.disabled.contains(family) {
            return false;
        }
        self.categories.is_empty() || self.categories.contains(family)
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

    pub fn unknown_names(&self, custom: &[CustomRule]) -> Vec<UnknownName> {
        let known_custom = |name: &str| custom.iter().any(|rule| rule.id == name);
        let rule_or_family = |name: &str| {
            RuleId::parse(name).is_some() || Category::parse(name).is_some() || known_custom(name)
        };
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
#[derive(Clone, Debug, Default, Deserialize, Hash, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Patterns {
    /// Advisory slugs, pattern slugs or families to switch off.
    pub disabled: BTreeSet<String>,
    /// Advisories that ship off, switched on by advisory, pattern or family.
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

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Ignore {
    /// Globs of files rqx does not read, matched against paths relative to the scanned directory.
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

/// Client names and call names the I/O rules should recognise on top of the ones
/// rqx ships. Added to the built-in vocabulary rather than replacing it, because a
/// project that names its own gateway still has a `fetch` somewhere.
#[derive(Clone, Debug, Default, Deserialize, Hash, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Performance {
    /// Client names the I/O rules should recognise, one segment each.
    pub receivers: Vec<String>,
    /// Call names the I/O rules should recognise, one segment each.
    pub methods: Vec<String>,
}

impl Performance {
    /// A name with a dot in it can never match: the rules compare one segment of a
    /// dotted callee at a time.
    pub fn unusable(&self) -> Option<&str> {
        self.receivers
            .iter()
            .chain(&self.methods)
            .find(|name| {
                name.trim().is_empty()
                    || name.contains('.')
                    || name.chars().any(char::is_whitespace)
            })
            .map(String::as_str)
    }
}

/// `prune_dirs` exists separately from `Ignore::patterns` because a glob still
/// requires descending into a directory to produce the paths it then rejects,
/// which on a populated `node_modules` is the difference between skipping a
/// directory and walking every file in it.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Scan {
    /// Worker threads. 0 lets rqx pick from the machine.
    pub threads: usize,
    /// Skip files carrying a generator's marker, and list them as skipped.
    pub skip_generated: bool,
    /// Bytes. A larger file is reported as skipped.
    pub max_filesize: u64,
    /// How many findings the report shows. 0 shows every one. Never moves the score.
    pub max_findings: usize,
    /// Directory names never descended into.
    pub prune_dirs: Vec<String>,
    /// Coverage files to read, so a test run can contradict a dead-code finding.
    pub coverage: Vec<String>,
    /// CPU profiles to read, so a run can say which performance findings are on a
    /// path that executes.
    pub profile: Vec<String>,
    /// Days of git history to read for churn; 0 turns the signal off.
    pub churn_days: u32,
    /// Reuse `.rqx/cache.json` for files whose contents have not changed. It was
    /// off because a warm run was no faster than a cold one, but that was measured
    /// while the collector joined the git thread before draining the channel: the
    /// wait, not the parsing, was what a cache could not beat. With that gone a
    /// warm scan of 3,440 files costs 298ms against 2,023ms cold.
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
            cache: true,
            coverage: Vec::new(),
            profile: Vec::new(),
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

    fn property<'a>(schema: &'a serde_json::Value, path: &[&str]) -> &'a serde_json::Value {
        let mut at = schema;
        for part in path {
            let next = &at["properties"][*part];
            at = match next["$ref"].as_str() {
                Some(reference) => &schema["$defs"][reference.trim_start_matches("#/$defs/")],
                None => next,
            };
        }
        at
    }

    /// The schema is what an editor offers on hover, so a key with no description is
    /// a key a reader has to leave the editor to understand.
    #[test]
    fn every_threshold_is_described_and_names_the_rule_it_tunes() {
        let schema = json_schema();
        let thresholds = property(&schema, &["thresholds"]);
        for (key, rules) in Thresholds::TUNES {
            let description = thresholds["properties"][key]["description"]
                .as_str()
                .unwrap_or_else(|| panic!("`{key}` has no description"));
            for rule in rules {
                assert!(
                    description.contains(&format!("`{}`", rule.slug())),
                    "`{key}` is described as {description:?} but tunes `{}`",
                    rule.slug()
                );
            }
        }
    }

    #[test]
    fn the_schema_names_every_rule_and_the_thresholds_it_reads() {
        let schema = json_schema();
        let rules = property(&schema, &["rules"]);
        assert_eq!(rules["additionalProperties"], false);
        for rule in RuleId::ALL.iter().copied() {
            let one = &rules["properties"][rule.spec().slug];
            assert!(one["properties"]["severity"].is_object(), "{rule}");
            for key in Thresholds::tuned_by(rule) {
                assert!(one["properties"][key].is_object(), "{rule} lacks {key}");
            }
        }
        assert!(rules["properties"]["disabled"].is_object());
    }

    #[test]
    fn the_schema_carries_defaults_and_refuses_unknown_keys() {
        let schema = json_schema();
        let thresholds = property(&schema, &["thresholds"]);
        assert_eq!(thresholds["additionalProperties"], false);
        assert_eq!(thresholds["properties"]["max_params"]["default"], 4);
        assert_eq!(
            property(&schema, &["gate"])["properties"]["fail_on"]["default"],
            "error"
        );
    }

    #[test]
    fn the_template_validates_against_the_schema_it_names() {
        assert_eq!(TEMPLATE.lines().next(), Some(SCHEMA_DIRECTIVE));
        let template: serde_json::Value = serde_saphyr::from_str(TEMPLATE).expect("the template");
        let schema = json_schema();
        let allowed = schema["properties"].as_object().expect("properties");
        for key in template.as_object().expect("a mapping").keys() {
            assert!(
                allowed.contains_key(key),
                "the template sets `{key}`, which the schema lacks"
            );
        }
    }

    /// A key that names no field, or a field no row names, makes the table a lie the
    /// moment a threshold is added or renamed.
    #[test]
    fn every_threshold_appears_in_the_table_that_says_what_it_tunes() {
        let template: serde_json::Value =
            serde_saphyr::from_str(TEMPLATE).expect("the template parses");
        let declared: Vec<&String> = template["thresholds"]
            .as_object()
            .expect("`thresholds` is a mapping")
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
    /// A typo in a preset is a setting that silently does nothing, which is the
    /// failure the whole table exists to prevent elsewhere.
    #[test]
    fn every_level_parses_and_names_only_things_that_exist() {
        for level in Level::ALL {
            let parsed: RqxConfig = serde_saphyr::from_str(level.layer())
                .unwrap_or_else(|error| panic!("{level}: {error}"));
            assert!(
                parsed.unknown_names().is_empty(),
                "{level}: {:?}",
                parsed.unknown_names()
            );
        }
    }

    /// A level moves the defaults. Letting one set `scan` or `ignore` would let
    /// it change what is read rather than how strictly it is judged.
    #[test]
    fn a_level_only_moves_thresholds_rules_patterns_and_the_gate() {
        for level in Level::ALL {
            let layer: serde_json::Value = serde_saphyr::from_str(level.layer()).expect("a level");
            for key in layer.as_object().into_iter().flat_map(|table| table.keys()) {
                assert!(
                    ["thresholds", "rules", "patterns", "gate"].contains(&key.as_str()),
                    "{level} sets `{key}`"
                );
            }
        }
    }

    #[test]
    fn pristine_turns_on_exactly_the_rules_that_ship_off() {
        let pristine: RqxConfig =
            serde_saphyr::from_str(Level::Pristine.layer()).expect("pristine");
        let off: BTreeSet<&str> = RuleId::ALL
            .iter()
            .filter(|rule| rule.spec().off_by_default)
            .map(|rule| rule.spec().slug)
            .collect();
        let named: BTreeSet<&str> = pristine.rules.enabled.iter().map(String::as_str).collect();
        assert_eq!(named, off);
        for rule in RuleId::ALL.iter().copied() {
            assert!(pristine.rules.enabled(rule), "{rule} is off under pristine");
        }
    }

    #[test]
    fn standard_is_the_catalogue_as_it_stands() {
        assert!(Level::Standard.layer().is_empty());
        assert_eq!(Level::default(), Level::Standard);
    }

    #[test]
    fn a_stricter_level_never_raises_a_threshold() {
        let of = |level: Level| -> Thresholds {
            serde_saphyr::from_str::<RqxConfig>(level.layer())
                .expect("a level")
                .thresholds
        };
        let (relaxed, strict, pristine) =
            (of(Level::Relaxed), of(Level::Strict), of(Level::Pristine));
        assert!(strict.cognitive_complexity < relaxed.cognitive_complexity);
        assert!(pristine.cognitive_complexity <= strict.cognitive_complexity);
        assert!(strict.max_params < relaxed.max_params);
        assert!(pristine.long_function_statements <= strict.long_function_statements);
    }

    #[test]
    fn a_renamed_family_says_its_new_name() {
        for (old, category) in RENAMED_FAMILIES {
            let mut rules = Rules::default();
            rules.categories.insert(old.to_owned());
            let unknown = rules.unknown_names(&[]);
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
        let unknown = rules.unknown_names(&[]);
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
        let written: RqxConfig = serde_saphyr::from_str(TEMPLATE).expect("the template parses");
        let defaults = RqxConfig::default();
        assert_eq!(
            serde_json::to_string(&Comparable::of(&written)).unwrap(),
            serde_json::to_string(&Comparable::of(&defaults)).unwrap()
        );
    }

    /// `RqxConfig` only derives `Deserialize`, so the two are compared through a
    /// serialisable stand-in rather than by growing the schema a `Serialize` it has no
    /// other use for.
    #[derive(serde::Serialize)]
    struct Comparable {
        root: bool,
        level: String,
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
        architecture: Vec<String>,
        performance: Vec<String>,
    }

    impl Comparable {
        fn of(config: &RqxConfig) -> Self {
            let t = &config.thresholds;
            Self {
                root: config.root,
                level: config.level.slug().to_owned(),
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
                    t.fan_out,
                    t.fan_in,
                    t.relative_climb,
                    t.module_statements,
                    t.function_locals,
                    t.try_statements,
                    t.narrated_run,
                    t.explanation_ratio,
                    t.guard_cascade,
                    t.sole_author_commits,
                    t.hotspot_commits,
                    t.cyclomatic_complexity,
                    t.max_results,
                    t.class_collaborators,
                    t.log_percent,
                    t.inheritance_depth,
                    t.todo_age_days,
                    t.max_exits,
                    t.max_cases,
                    t.min_cases,
                    t.callback_depth,
                    t.feature_envy_ratio,
                    t.data_clump_size,
                    t.data_clump_sites,
                    t.short_name_scope_lines,
                    t.slow_test_ms,
                    t.boundary_repeats,
                    t.declaration_distance_lines,
                    t.expression_operations,
                    t.configuration_depth,
                    t.loop_nesting,
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
                    config.scan.coverage.join(","),
                    config.scan.profile.join(","),
                ],
                gate: vec![
                    format!("{:?}", config.gate.fail_on),
                    config.gate.max_findings.to_string(),
                    config.gate.min_health.to_string(),
                    format!("{:?}", config.gate.min_grade),
                    config.gate.baseline.clone().unwrap_or_default(),
                    config.gate.ratchet.to_string(),
                ],
                architecture: config.architecture.layers.clone(),
                performance: config
                    .performance
                    .receivers
                    .iter()
                    .chain(&config.performance.methods)
                    .cloned()
                    .collect(),
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

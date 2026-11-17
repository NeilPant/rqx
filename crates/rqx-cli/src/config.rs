use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use ignore::overrides::{Override, OverrideBuilder};
use rqx_core::config::{
    CONFIG_FILE_NAME, Level, RqxConfig, SCOPED_TABLES, SubtreeOverride, TABLES,
};
use serde_json::{Map, Value};

mod extends;
mod files;
mod hoist;
mod origin;
mod quickfix;

pub use files::{check_text, config_path_in, user_config_directories};
pub use origin::Origin;
pub use quickfix::disable_entry;

pub const CONFIG_ENV: &str = "RQX_CONFIG";
pub const POLICY_ENV: &str = "RQX_POLICY";

struct Scope {
    matcher: Override,
    config: RqxConfig,
    /// The directory the globs were built against. An `Override` matches paths under
    /// its own root only, so comparing two checkouts needs the globs rebuilt against
    /// the other one rather than reused.
    directory: PathBuf,
    globs: Vec<String>,
}

/// Every layer of configuration that applies to one run, already merged, plus the
/// directory-scoped overrides that could not be merged because they apply to some
/// files and not others.
pub struct Resolved {
    pub config: RqxConfig,
    pub sources: Vec<PathBuf>,
    pub provenance: BTreeMap<&'static str, Origin>,
    /// Which file wrote each individual `table.key`. The per-table map cannot answer
    /// that once two layers both touch `[thresholds]`, and a locked key has to be
    /// able to name the file that set it.
    key_sources: BTreeMap<String, Origin>,
    /// The keys the policy will not let a project change.
    locked: BTreeSet<String>,
    pub policy: Option<PathBuf>,
    /// The presets the run built on, in the order they were laid down.
    pub presets: Vec<String>,
    /// What the project actually wrote, before the level's layer went under it, so
    /// the `from` column can tell a value a file set from one a level moved.
    written: Value,
    scopes: Vec<Scope>,
    modules: bool,
    /// The debt already recorded, when the gate names a baseline. Shared rather than
    /// copied because every worker reads it and none of them writes it.
    pub baseline: Option<Arc<HashSet<rqx_core::identity::Identity>>>,
    /// How many findings the baseline was taken over, for the ratchet.
    pub baseline_findings: Option<u32>,
}

impl Clone for Resolved {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            sources: self.sources.clone(),
            provenance: self.provenance.clone(),
            key_sources: self.key_sources.clone(),
            locked: self.locked.clone(),
            policy: self.policy.clone(),
            presets: self.presets.clone(),
            written: self.written.clone(),
            scopes: self
                .scopes
                .iter()
                .map(|scope| Scope {
                    matcher: scope.matcher.clone(),
                    config: scope.config.clone(),
                    directory: scope.directory.clone(),
                    globs: scope.globs.clone(),
                })
                .collect(),
            modules: self.modules,
            baseline: self.baseline.clone(),
            baseline_findings: self.baseline_findings,
        }
    }
}

impl Default for Resolved {
    fn default() -> Self {
        Self::of(RqxConfig::default())
    }
}

impl Resolved {
    /// One config, no files, no scopes. What the language server and the tests use
    /// when they already hold the settings they mean.
    pub fn of(config: RqxConfig) -> Self {
        let modules = config.needs_modules();
        Self {
            config,
            sources: Vec::new(),
            provenance: BTreeMap::new(),
            key_sources: BTreeMap::new(),
            locked: BTreeSet::new(),
            policy: None,
            presets: Vec::new(),
            written: files::empty(),
            scopes: Vec::new(),
            baseline: None,
            baseline_findings: None,
            modules,
        }
    }

    /// The settings that govern one file. Overrides are tried last-declared first, so
    /// the most specific rule a project wrote is the one that answers.
    pub fn for_file(&self, path: &Path) -> &RqxConfig {
        self.scopes
            .iter()
            .rev()
            .find(|scope| scope.matcher.matched(path, false).is_whitelist())
            .map_or(&self.config, |scope| &scope.config)
    }

    /// The import graph is built once for every file the walk saw, so it has to be
    /// collected if a single subtree asks for a rule that reads it. Per-file stages
    /// need no such union: each file is analysed under its own settings.
    pub const fn needs_modules(&self) -> bool {
        self.modules
    }

    /// Whether any layer runs this rule. Collection that happens once for the whole
    /// walk has to switch on if a single subtree wants it.
    pub fn enabled_anywhere(&self, rule: rqx_core::rule::RuleId) -> bool {
        self.config.rules.enabled(rule)
            || self
                .scopes
                .iter()
                .any(|scope| scope.config.rules.enabled(rule))
    }

    pub fn rules_enabled(&self) -> usize {
        rqx_core::rule::RuleId::ALL
            .iter()
            .filter(|rule| self.enabled_anywhere(**rule))
            .count()
    }

    /// Everything that changes what a finding is, for the cache key.
    pub fn signature(&self) -> impl std::hash::Hash + '_ {
        (
            &self.config.thresholds,
            &self.config.rules,
            &self.config.patterns,
            self.scopes
                .iter()
                .map(|scope| {
                    (
                        &scope.config.thresholds,
                        &scope.config.rules,
                        &scope.config.patterns,
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    /// The project file that wrote `table.key`, addressed as one string, which is not
    /// always the last file to touch its table. `set` defaults to it. A preset or an
    /// extended file never answers: neither is the project's to edit.
    pub fn file_that_set(&self, key: &str) -> Option<&Path> {
        self.key_sources.get(key).and_then(Origin::file)
    }

    /// Every locked key, so a writer can refuse before it touches the file.
    pub fn locked_keys(&self) -> impl Iterator<Item = &str> {
        self.locked.iter().map(String::as_str)
    }

    /// What wrote the setting at a dotted path such as `rules.long-function.severity`.
    /// `None` when nothing did, which is to say it is a default.
    pub fn origin_of_path(&self, path: &str) -> Option<Origin> {
        self.key_sources.get(path).cloned()
    }

    /// What wrote a setting, whether a file, a preset or a flag. The key is the more
    /// precise answer; the table is what is left when two layers both touched it.
    pub fn origin_of(&self, table: &str, key: Option<&str>) -> Option<Origin> {
        key.and_then(|key| self.key_sources.get(&format!("{table}.{key}")))
            .or_else(|| self.provenance.get(table))
            .cloned()
            .or_else(|| self.sources.last().cloned().map(Origin::File))
    }

    /// Whether a policy forbids this project changing one setting.
    pub fn is_locked(&self, table: &str, key: &str) -> bool {
        self.locked.contains(&format!("{table}.{key}"))
    }

    /// Whether a key was written down rather than moved by the level.
    pub fn was_written(&self, table: &str, key: &str) -> bool {
        self.written
            .get(table)
            .and_then(|table| table.get(key))
            .is_some()
    }

    pub const fn level(&self) -> Level {
        self.config.level
    }

    /// The same settings, with every path-scoped override rebuilt against another
    /// checkout of the same project. Without this a comparison against a base
    /// revision would judge the two sides by different rules: the globs would match
    /// in the working tree and silently match nothing in the temporary one.
    pub fn rebased(&self, from: &Path, to: &Path) -> Result<Self> {
        let mut scopes = Vec::new();
        for scope in &self.scopes {
            let directory = match scope.directory.strip_prefix(from) {
                Ok(rest) => to.join(rest),
                Err(_) => scope.directory.clone(),
            };
            let mut builder = OverrideBuilder::new(&directory);
            for glob in &scope.globs {
                builder
                    .add(glob)
                    .with_context(|| format!("`{glob}` against {}", directory.display()))?;
            }
            scopes.push(Scope {
                matcher: builder.build().with_context(|| {
                    format!(
                        "rebuilding the override globs against {}",
                        directory.display()
                    )
                })?,
                config: scope.config.clone(),
                directory,
                globs: scope.globs.clone(),
            });
        }
        Ok(Self {
            config: self.config.clone(),
            sources: self.sources.clone(),
            provenance: self.provenance.clone(),
            key_sources: self.key_sources.clone(),
            locked: self.locked.clone(),
            policy: self.policy.clone(),
            presets: self.presets.clone(),
            written: self.written.clone(),
            scopes,
            baseline: self.baseline.clone(),
            baseline_findings: self.baseline_findings,
            modules: self.modules,
        })
    }
}

/// The files that make up a run, lowest precedence first. An explicit path is the
/// user saying which file they mean, so it is the only one whose absence is an error.
pub fn resolve(root: &Path, explicit: Option<&Path>, level: Option<Level>) -> Result<Resolved> {
    let anchor = anchor_of(root);
    let project = project_configs(&anchor)?;
    let mut layers: Vec<PathBuf> = Vec::new();
    layers.extend(files::user_config()?);
    // Under the project rather than over it: a repository may tighten anything the
    // policy did not lock, and a policy that sat on top would make every project
    // identical rather than every project compliant.
    let policy = policy_of(&project)?;
    layers.extend(policy.clone());
    layers.extend(project);
    if let Some(path) = std::env::var_os(CONFIG_ENV).map(PathBuf::from) {
        files::refuse_retired(&path)?;
        if !path.exists() {
            bail!(
                "{CONFIG_ENV} points at `{}`, which does not exist. Unset it, or point it \
                 at a config file.",
                path.display()
            );
        }
        layers.push(path);
    }
    if let Some(path) = explicit {
        files::refuse_retired(path)?;
        if !path.exists() {
            bail!(
                "`{}` does not exist. Drop `--config` to let rqx find `{CONFIG_FILE_NAME}` \
                 on its own, or run `rqx init` to write one.",
                path.display()
            );
        }
        layers.push(path.to_path_buf());
    }

    let loaded: Vec<(PathBuf, Value)> = layers
        .into_iter()
        .map(|path| files::read(&path).map(|value| (path, value)))
        .collect::<Result<_>>()?;
    let mut stack = Vec::new();
    for (path, value) in &loaded {
        stack.extend(extends::inherited(path, value)?);
    }
    stack.extend(loaded.into_iter().map(|(path, value)| extends::Layer {
        origin: Origin::File(path),
        value,
    }));

    let mut merged = files::empty();
    let mut provenance = BTreeMap::new();
    let mut raw_overrides: Vec<(PathBuf, Value)> = Vec::new();
    let mut sources = Vec::new();
    let mut presets: Vec<String> = Vec::new();
    let mut locked: Vec<String> = Vec::new();
    let mut key_sources: BTreeMap<String, Origin> = BTreeMap::new();
    let mut written: Vec<(Origin, Vec<(String, Value)>)> = Vec::new();

    for extends::Layer { origin, mut value } in stack {
        let path = match &origin {
            Origin::File(path) | Origin::Extended(path) => path.clone(),
            Origin::Preset(name) => {
                presets.push(name.clone());
                PathBuf::from(format!("preset:{name}"))
            }
            Origin::Flag(_) => continue,
        };
        extends::take_extends(&mut value);
        for mut entry in take_overrides(&mut value, &path)? {
            hoist::thresholds(&mut entry, &format!("an entry of `overrides` in {origin}"))?;
            raw_overrides.push((path.clone(), entry));
        }
        hoist::thresholds(&mut value, &origin.to_string())?;
        let is_policy =
            matches!(&origin, Origin::File(file) if policy.as_deref() == Some(file.as_path()));
        locked.extend(take_locked(&mut value, &path, is_policy)?);
        take_policy(&mut value);
        if let Some(table) = value.as_object() {
            for key in table.keys() {
                if let Some(known) = TABLES.iter().find(|table| *table == key) {
                    provenance.insert(*known, origin.clone());
                }
            }
        }
        let mut leaves = Vec::new();
        flatten("", &value, &mut leaves);
        for (key, _) in &leaves {
            key_sources.insert(key.clone(), origin.clone());
        }
        written.push((origin.clone(), leaves));
        merge(&mut merged, value);
        if !matches!(origin, Origin::Preset(_)) {
            sources.push(path);
        }
    }
    presets.dedup();

    if let Some(policy) = &policy {
        enforce_locks(&locked, policy, &written, &raw_overrides, level)?;
    }

    // The level's settings go under everything written by hand, so naming a level
    // changes the defaults and never overrides a value the project stated.
    let chosen = match level {
        Some(level) => level,
        None => merged
            .get("level")
            .and_then(Value::as_str)
            .and_then(Level::parse)
            .unwrap_or_default(),
    };
    let effective = under(chosen, &merged)?;

    let config: RqxConfig =
        serde_json::from_value(effective).with_context(|| describe(&sources))?;
    reject_unknown_names(&config, &describe(&sources))?;
    if level.is_some() {
        provenance.insert("level", Origin::Flag("--level"));
    }

    let mut scopes = Vec::new();
    for (path, entry) in raw_overrides {
        scopes.push(scope(&path, entry, &merged, chosen)?);
    }

    let modules = config.needs_modules() || scopes.iter().any(|scope| scope.config.needs_modules());

    // Compiled once, before anything is scanned, so a typo in a query is a config
    // error naming the rule rather than a rule that silently never fires.
    rqx_lang::custom::validate(&config.custom).map_err(|error| anyhow::anyhow!("{error}"))?;

    // Read once here rather than at each of the commands that scan, so no command can
    // forget it and quietly gate on the whole backlog.
    let recorded = match &config.gate.baseline {
        Some(named) => {
            let file = crate::baseline::path_of(&anchor, Some(named));
            if !file.exists() {
                bail!(
                    "`gate.baseline` names {}, which does not exist. \
                     Run `rqx baseline` to record what this project already has.",
                    file.display()
                );
            }
            Some(crate::baseline::Baseline::read(&file)?)
        }
        None => None,
    };

    Ok(Resolved {
        config,
        sources,
        provenance,
        key_sources,
        locked: locked.into_iter().collect(),
        policy,
        presets,
        written: merged,
        scopes,
        modules,
        baseline_findings: recorded.as_ref().map(crate::baseline::Baseline::findings),
        baseline: recorded.map(|recorded| Arc::new(recorded.recorded())),
    })
}

/// Names the files a parse error could have come from. serde reports the offending
/// key but not which of four possible files holds it.
fn describe(sources: &[PathBuf]) -> String {
    let names: Vec<String> = sources
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    match names.len() {
        0 => "reading the built-in defaults".to_owned(),
        1 => format!("reading {}", names[0]),
        _ => format!("reading {} together", names.join(" and ")),
    }
}

/// Writes the commented starting config. Returns where it went, so each front end can
/// say so in its own words.
pub fn init(directory: &Path, force: bool) -> Result<PathBuf> {
    let existing = files::in_directory(directory)?;
    if let Some(path) = existing.as_ref().filter(|_| !force) {
        bail!(
            "`{}` is already there. Pass --force to replace it, or `rqx config` to see what \
             it resolves to.",
            path.display()
        );
    }
    let path = existing.unwrap_or_else(|| directory.join(CONFIG_FILE_NAME));
    std::fs::write(&path, rqx_core::config::TEMPLATE)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// A scanned file has no ancestors of its own worth searching; the directory holding
/// it does.
fn anchor_of(root: &Path) -> PathBuf {
    let absolute = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if absolute.is_file() {
        return absolute
            .parent()
            .map_or(absolute.clone(), Path::to_path_buf);
    }
    absolute
}

/// Walks up from the scanned directory, outermost file first so the nearest one wins
/// the merge. Stops at a repository boundary: a config above a checkout describes a
/// different project, and reading it would make a scan depend on where the repository
/// happens to sit.
fn project_configs(anchor: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for directory in anchor.ancestors() {
        let candidate = files::in_directory(directory)?;
        let declares_root = candidate
            .as_ref()
            .and_then(|path| files::read(path).ok())
            .and_then(|value| value.get("root").and_then(Value::as_bool))
            .unwrap_or(false);
        found.extend(candidate);
        if declares_root || directory.join(".git").exists() {
            break;
        }
    }
    found.reverse();
    Ok(found)
}

/// The policy a project points at, if any. Read from the nearest config that names
/// one so a monorepo can put the pointer at its root, and overridden by the
/// environment for a CI job that has to impose one.
fn policy_of(project: &[PathBuf]) -> Result<Option<PathBuf>> {
    if let Some(named) = std::env::var_os(POLICY_ENV).map(PathBuf::from) {
        files::refuse_retired(&named)?;
        if !named.exists() {
            bail!(
                "{POLICY_ENV} points at `{}`, which does not exist.",
                named.display()
            );
        }
        return Ok(Some(named));
    }
    // Nearest wins, the way every other setting does, so the innermost config that
    // names a policy is the one that chose it.
    for config in project.iter().rev() {
        let Some(named) = files::read(config)?
            .get("policy")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let resolved = config
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&named);
        files::refuse_retired(&resolved)?;
        if !resolved.exists() {
            bail!(
                "{} names the policy `{named}`, which does not exist at {}.",
                config.display(),
                resolved.display()
            );
        }
        return Ok(Some(resolved));
    }
    Ok(None)
}

/// `policy` is a pointer, not a setting, so it never reaches the resolved config.
fn take_policy(value: &mut Value) {
    if let Some(table) = value.as_object_mut() {
        table.remove("policy");
    }
}

fn take_locked(value: &mut Value, path: &Path, is_policy: bool) -> Result<Vec<String>> {
    let Some(table) = value.as_object_mut() else {
        return Ok(Vec::new());
    };
    let Some(entry) = table.remove("locked") else {
        return Ok(Vec::new());
    };
    if !is_policy {
        bail!(
            "`locked` in {} does nothing: only the policy a project points at can \
             lock a key.",
            path.display()
        );
    }
    let keys = entry
        .get("keys")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("`locked.keys` in {} must be a list", path.display()))?;
    keys.iter()
        .map(|key| {
            key.as_str().map(str::to_owned).ok_or_else(|| {
                anyhow::anyhow!(
                    "`locked.keys` in {} holds something that is not a name",
                    path.display()
                )
            })
        })
        .collect()
}

/// Every leaf of a config document as `table.key`, so "which file wrote this one
/// setting" is answerable. Arrays are leaves: a list is one value, not several.
fn flatten(prefix: &str, value: &Value, out: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(table) => {
            for (key, value) in table {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&path, value, out);
            }
        }
        other => out.push((prefix.to_owned(), other.clone())),
    }
}

/// A locked key may not be given a different value by anything above the policy.
/// Not "may not be loosened": `rules.disabled` has no direction, so the only rule
/// that holds for every key is that the policy's value is the value.
fn enforce_locks(
    locked: &[String],
    policy: &Path,
    written: &[(Origin, Vec<(String, Value)>)],
    overrides: &[(PathBuf, Value)],
    level: Option<Level>,
) -> Result<()> {
    let is_policy = |origin: &Origin| origin.file() == Some(policy);
    let from_policy: &[(String, Value)] = written
        .iter()
        .find(|(origin, _)| is_policy(origin))
        .map_or(&[], |(_, leaves)| leaves.as_slice());
    let above = written
        .iter()
        .skip_while(|(origin, _)| !is_policy(origin))
        .skip(1);

    for key in locked {
        let want = from_policy.iter().find(|(named, _)| named == key);
        let Some((_, want)) = want else {
            bail!(
                "{} locks `{key}`, which it does not set. Give it a value or drop it \
                 from `[locked] keys`.",
                policy.display()
            );
        };
        for (origin, leaves) in above.clone() {
            if let Some((_, got)) = leaves.iter().find(|(named, _)| named == key)
                && got != want
            {
                bail!(
                    "`{key}` is locked to {want} by {}; {origin} sets {got}.",
                    policy.display()
                );
            }
        }
        // An override is merged after every layer, so a lock it contradicts would win.
        if let Some((table, field)) = key.split_once('.') {
            for (path, entry) in overrides {
                if let Some(got) = entry.get(table).and_then(|values| values.get(field))
                    && got != want
                {
                    bail!(
                        "`{key}` is locked to {want} by {}; an entry of `overrides` in {} sets {got}.",
                        policy.display(),
                        path.display()
                    );
                }
            }
        }
        if key == "level"
            && let Some(level) = level
            && Some(level) != want.as_str().and_then(Level::parse)
        {
            bail!(
                "`level` is locked to {want} by {}; --level asks for `{}`.",
                policy.display(),
                level.slug()
            );
        }
    }
    Ok(())
}

fn take_overrides(value: &mut Value, path: &Path) -> Result<Vec<Value>> {
    let Some(table) = value.as_object_mut() else {
        bail!("{} must be a mapping of settings", path.display());
    };
    match table.remove("overrides") {
        None => Ok(Vec::new()),
        Some(Value::Array(entries)) => Ok(entries),
        Some(_) => bail!(
            "`overrides` in {} must be a list, one entry per subtree",
            path.display()
        ),
    }
}

/// An entry's keys were checked when its file was read, so what is left to refuse is
/// what only the whole run can know: that it names somewhere to apply, and that the
/// globs compile. The raw entry is merged rather than the typed one, because a typed
/// `thresholds` carries every default and would overwrite what the project set.
fn scope(path: &Path, entry: Value, base: &Value, level: Level) -> Result<Scope> {
    let typed: SubtreeOverride = serde_json::from_value(entry.clone())
        .with_context(|| format!("reading an entry of `overrides` in {}", path.display()))?;
    if typed.paths.is_empty() {
        bail!(
            "an entry of `overrides` in {} names no `paths`, so nothing would use it",
            path.display()
        );
    }
    let globs = typed.paths;

    let directory = path.parent().unwrap_or(Path::new("."));
    let mut builder = OverrideBuilder::new(directory);
    for glob in &globs {
        builder
            .add(glob)
            .with_context(|| format!("`{glob}` in {} is not a valid glob", path.display()))?;
    }
    let matcher = builder
        .build()
        .with_context(|| format!("building the override globs in {}", path.display()))?;

    // The gate is dropped from a scoped level because a gate reaches one verdict for
    // the whole run.
    let mut preset = files::parse(typed.level.unwrap_or(level).layer())?;
    if let Some(table) = preset.as_object_mut() {
        table.remove("gate");
    }
    let mut merged = preset;
    merge(&mut merged, base.clone());
    merge(&mut merged, scoped_settings(&entry));
    let config: RqxConfig = serde_json::from_value(merged)
        .with_context(|| format!("reading an entry of `overrides` in {}", path.display()))?;
    reject_unknown_names(
        &config,
        &format!("an entry of `overrides` in {}", path.display()),
    )?;

    Ok(Scope {
        matcher,
        config,
        directory: directory.to_path_buf(),
        globs,
    })
}

fn scoped_settings(entry: &Value) -> Value {
    let kept: Map<String, Value> = SCOPED_TABLES
        .iter()
        .filter_map(|name| Some(((*name).to_owned(), entry.get(*name)?.clone())))
        .collect();
    Value::Object(kept)
}

/// A level's settings with the explicit ones on top. Written as a merge rather than
/// a set of defaults per field so a preset stays one readable YAML file.
fn under(level: Level, explicit: &Value) -> Result<Value> {
    let mut base = files::parse(level.layer()).with_context(|| format!("the `{level}` level"))?;
    merge(&mut base, explicit.clone());
    Ok(base)
}

fn reject_unknown_names(config: &RqxConfig, source: &str) -> Result<()> {
    let plan = &config.architecture;
    let undeclared = plan.undeclared_layers();
    if !undeclared.is_empty() {
        bail!(
            "{source}: `architecture.paths` puts a subtree in {}, which `layers` does \
             not declare. Layers are {:?}, ordered inner to outer.",
            undeclared
                .iter()
                .map(|layer| format!("`{layer}`"))
                .collect::<Vec<_>>()
                .join(", "),
            plan.layers
        );
    }
    let duplicates = plan.duplicate_layers();
    if !duplicates.is_empty() {
        bail!(
            "{source}: `architecture.layers` names {:?} twice, so which way a \
             dependency may point between them has two answers.",
            duplicates
        );
    }

    let unknown = config.unknown_names();
    if unknown.is_empty() {
        return Ok(());
    }
    let lines: Vec<String> = unknown.iter().map(|name| format!("  {name}")).collect();
    bail!("{source}:\n{}", lines.join("\n"));
}

/// Tables merge key by key so a file can set one threshold without restating the
/// other twenty-one. Arrays replace, because appending would make a narrower layer
/// unable to shorten a list the layer beneath it set.
fn merge(base: &mut Value, layer: Value) {
    match (base, layer) {
        (Value::Object(base), Value::Object(layer)) => {
            for (key, value) in layer {
                match base.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, layer) => *base = layer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of its own per test, so two running at once cannot read each
    /// other's policy.
    fn workspace(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rqx-policy-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn write(at: &Path, body: &str) {
        std::fs::create_dir_all(at.parent().expect("a directory")).expect("mkdir");
        std::fs::write(at, body).expect("write");
    }

    fn refusal(root: &Path) -> String {
        match resolve(root, None, None) {
            Ok(_) => panic!("expected the configuration to be refused"),
            Err(error) => format!("{error:#}"),
        }
    }

    /// A policy that cannot be enforced is a suggestion, and the whole point of the
    /// table is that a repository cannot quietly opt out of it.
    #[test]
    fn a_project_that_moves_a_locked_key_is_refused_naming_the_key_and_both_files() {
        let root = workspace("locked-moved");
        write(
            &root.join("platform/policy.yaml"),
            "thresholds:\n  cognitive_complexity: 12\n\nlocked:\n  keys: [thresholds.cognitive_complexity]\n",
        );
        write(
            &root.join("service/rqx.yaml"),
            "policy: ../platform/policy.yaml\n\nthresholds:\n  cognitive_complexity: 25\n",
        );

        let error = refusal(&root.join("service"));
        assert!(error.contains("thresholds.cognitive_complexity"), "{error}");
        assert!(error.contains("policy.yaml"), "{error}");
        assert!(error.contains("rqx.yaml"), "{error}");
        assert!(error.contains("25"), "{error}");
    }

    #[test]
    fn a_policy_sets_what_the_project_leaves_alone_and_yields_where_it_is_not_locked() {
        let root = workspace("policy-yields");
        write(
            &root.join("platform/policy.yaml"),
            "thresholds:\n  cognitive_complexity: 12\n  max_params: 6\n\nlocked:\n  keys: [thresholds.cognitive_complexity]\n",
        );
        write(
            &root.join("service/rqx.yaml"),
            "policy: ../platform/policy.yaml\n\nthresholds:\n  max_params: 2\n",
        );

        let settings = resolve(&root.join("service"), None, None).expect("resolves");
        assert_eq!(settings.config.thresholds.cognitive_complexity, 12);
        assert_eq!(
            settings.config.thresholds.max_params, 2,
            "unlocked, so the project wins"
        );
        assert!(settings.is_locked("thresholds", "cognitive_complexity"));
        assert!(!settings.is_locked("thresholds", "max_params"));
        // The per-table map cannot tell these apart; the per-key one has to.
        assert!(
            settings
                .file_that_set("thresholds.cognitive_complexity")
                .is_some_and(|path| path.ends_with("policy.yaml"))
        );
        assert!(
            settings
                .file_that_set("thresholds.max_params")
                .is_some_and(|path| path.ends_with("rqx.yaml"))
        );
    }

    /// Repeating the policy's own value is not a change, and refusing it would make a
    /// project unable to state the standard it is holding itself to.
    #[test]
    fn a_project_may_restate_a_locked_value() {
        let root = workspace("restate");
        write(
            &root.join("platform/policy.yaml"),
            "gate:\n  fail_on: warning\n\nlocked:\n  keys: [gate.fail_on]\n",
        );
        write(
            &root.join("service/rqx.yaml"),
            "policy: ../platform/policy.yaml\n\ngate:\n  fail_on: warning\n",
        );
        assert!(resolve(&root.join("service"), None, None).is_ok());
    }

    /// An override is merged after every layer, so a lock it contradicts would win.
    #[test]
    fn an_override_cannot_move_a_locked_key_either() {
        let root = workspace("override-lock");
        write(
            &root.join("platform/policy.yaml"),
            "thresholds:\n  max_params: 3\n\nlocked:\n  keys: [thresholds.max_params]\n",
        );
        write(
            &root.join("service/rqx.yaml"),
            "policy: ../platform/policy.yaml\n\noverrides:\n  - paths: [legacy/**]\n    thresholds:\n      max_params: 9\n",
        );
        let error = refusal(&root.join("service"));
        assert!(error.contains("overrides"), "{error}");
    }

    /// `locked` outside the policy would let a project lock itself against the
    /// platform, which is the wrong way round.
    #[test]
    fn only_the_policy_may_lock_anything() {
        let root = workspace("project-lock");
        write(
            &root.join("service/rqx.yaml"),
            "thresholds:\n  max_params: 3\n\nlocked:\n  keys: [thresholds.max_params]\n",
        );
        let error = refusal(&root.join("service"));
        assert!(error.contains("only the policy"), "{error}");
    }

    /// A TOML file that is skipped is a gate nobody knows has changed, so finding one
    /// stops the run and says how to convert it.
    #[test]
    fn a_toml_config_is_refused_with_the_command_that_converts_it() {
        let root = workspace("retired");
        write(
            &root.join("service/RQX.toml"),
            "[gate]\nfail_on = \"info\"\n",
        );
        let error = refusal(&root.join("service"));
        assert!(error.contains("RQX.toml"), "{error}");
        assert!(error.contains("rqx config migrate"), "{error}");
    }

    #[test]
    fn a_toml_config_is_refused_even_beside_a_yaml_one() {
        let root = workspace("retired-beside");
        write(&root.join("RQX.toml"), "[gate]\n");
        write(&root.join("rqx.yaml"), "gate: {}\n");
        assert!(refusal(&root).contains("rqx config migrate"));
    }

    #[test]
    fn a_toml_file_named_by_a_flag_or_a_policy_pointer_is_refused() {
        let root = workspace("retired-named");
        write(&root.join("custom.toml"), "[gate]\n");
        let Err(flag) = resolve(&root, Some(&root.join("custom.toml")), None) else {
            panic!("--config accepted TOML");
        };
        assert!(flag.to_string().contains("rqx config migrate"), "{flag}");

        write(&root.join("platform/policy.toml"), "[gate]\n");
        write(&root.join("rqx.yaml"), "policy: platform/policy.toml\n");
        assert!(refusal(&root).contains("rqx config migrate"));
    }

    #[test]
    fn every_accepted_file_name_is_read() {
        for name in rqx_core::config::CONFIG_FILE_NAMES {
            let root = workspace("names");
            write(&root.join(name), "thresholds:\n  max_params: 2\n");
            let settings = resolve(&root, None, None).expect(name);
            assert_eq!(settings.config.thresholds.max_params, 2, "{name}");
        }
    }

    #[test]
    fn two_config_files_in_one_directory_are_refused_naming_both() {
        let root = workspace("two");
        write(&root.join("rqx.yaml"), "gate: {}\n");
        write(&root.join(".rqx.yml"), "gate: {}\n");
        let error = refusal(&root);
        assert!(
            error.contains("rqx.yaml") && error.contains(".rqx.yml"),
            "{error}"
        );
    }

    #[test]
    fn a_mistake_in_a_file_names_the_file_and_the_line() {
        let root = workspace("located");
        write(
            &root.join("rqx.yaml"),
            "gate:\n  fail_on: error\nthresholds:\n  max_params: many\n",
        );
        let error = refusal(&root);
        assert!(error.contains("rqx.yaml:4"), "{error}");
    }

    #[test]
    fn a_misspelt_key_is_refused_with_the_keys_it_could_have_been() {
        let root = workspace("typo");
        write(&root.join("rqx.yaml"), "thresholds:\n  max_param: 2\n");
        let error = refusal(&root);
        assert!(
            error.contains("max_param") && error.contains("max_params"),
            "{error}"
        );
    }

    #[test]
    fn an_empty_file_and_one_of_only_comments_set_nothing() {
        let root = workspace("empty");
        write(&root.join("rqx.yaml"), "# nothing yet\n");
        let settings = resolve(&root, None, None).expect("resolves");
        assert_eq!(settings.config.thresholds.max_params, 4);
    }

    #[test]
    fn a_preset_sets_what_the_file_leaves_alone_and_names_itself_as_the_source() {
        let root = workspace("preset");
        write(
            &root.join("rqx.yaml"),
            "extends: [ci-gate]\n\ngate:\n  max_findings: 5\n",
        );
        let settings = resolve(&root, None, None).expect("resolves");
        assert_eq!(settings.config.gate.min_health, 70);
        assert_eq!(
            settings.config.gate.max_findings, 5,
            "the file's own key wins"
        );
        assert_eq!(settings.presets, ["ci-gate"]);
        let from = |key| {
            settings
                .origin_of("gate", Some(key))
                .expect("origin")
                .label()
        };
        assert_eq!(from("min_health"), "preset:ci-gate");
        assert_eq!(from("max_findings"), "rqx.yaml");
    }

    #[test]
    fn a_later_entry_of_extends_wins_over_an_earlier_one() {
        let root = workspace("extends-order");
        write(&root.join("rqx.yaml"), "extends: [perf, legacy-adoption]\n");
        let settings = resolve(&root, None, None).expect("resolves");
        // perf says 2, and legacy-adoption comes through the relaxed level, which says 3.
        assert_eq!(settings.config.thresholds.callback_depth, 2);
        assert_eq!(settings.config.thresholds.max_params, 6);
        assert_eq!(settings.config.thresholds.loop_nesting, 1);
        assert_eq!(settings.presets, ["perf", "relaxed", "legacy-adoption"]);
    }

    /// presets and extends < policy < per-directory files, so a preset can never
    /// override what a policy set, whichever file named it.
    #[test]
    fn a_policy_overrides_a_preset_and_a_project_file_overrides_both() {
        let root = workspace("precedence");
        write(
            &root.join("platform/policy.yaml"),
            "thresholds:\n  loop_nesting: 5\n  callback_depth: 4\n",
        );
        write(
            &root.join("service/rqx.yaml"),
            "policy: ../platform/policy.yaml\nextends: [perf]\nthresholds:\n  callback_depth: 6\n",
        );
        let settings = resolve(&root.join("service"), None, None).expect("resolves");
        assert_eq!(
            settings.config.thresholds.loop_nesting, 5,
            "policy over preset"
        );
        assert_eq!(
            settings.config.thresholds.callback_depth, 6,
            "file over policy"
        );
    }

    #[test]
    fn a_preset_cannot_move_a_key_the_policy_locks() {
        let root = workspace("preset-lock");
        write(
            &root.join("platform/policy.yaml"),
            "gate:\n  min_health: 90\n\nlocked:\n  keys: [gate.min_health]\n",
        );
        write(
            &root.join("service/rqx.yaml"),
            "policy: ../platform/policy.yaml\nextends: [ci-gate]\n",
        );
        let settings = resolve(&root.join("service"), None, None).expect("resolves");
        assert_eq!(settings.config.gate.min_health, 90);
    }

    #[test]
    fn a_file_can_extend_a_file_beside_it_and_that_file_is_not_the_one_set_edits() {
        let root = workspace("extends-file");
        write(
            &root.join("shared/base.yaml"),
            "thresholds:\n  max_params: 2\n",
        );
        write(
            &root.join("rqx.yaml"),
            "extends: [shared/base.yaml]\nthresholds:\n  nesting_depth: 2\n",
        );
        let settings = resolve(&root, None, None).expect("resolves");
        assert_eq!(settings.config.thresholds.max_params, 2);
        assert_eq!(
            settings
                .origin_of("thresholds", Some("max_params"))
                .expect("origin")
                .label(),
            "base.yaml (extends)"
        );
        assert!(settings.file_that_set("thresholds.max_params").is_none());
        assert!(settings.file_that_set("thresholds.nesting_depth").is_some());
    }

    #[test]
    fn an_unknown_preset_is_refused_with_the_one_it_was_close_to() {
        let root = workspace("unknown-preset");
        write(&root.join("rqx.yaml"), "extends: [stirct]\n");
        let error = refusal(&root);
        assert!(error.contains("`stirct`"), "{error}");
        assert!(error.contains("Did you mean `strict`?"), "{error}");
        assert!(error.contains("legacy-adoption"), "{error}");
    }

    #[test]
    fn files_that_extend_each_other_are_refused_rather_than_followed_forever() {
        let root = workspace("extends-loop");
        write(&root.join("a.yaml"), "extends: [b.yaml]\n");
        write(&root.join("b.yaml"), "extends: [a.yaml]\n");
        write(&root.join("rqx.yaml"), "extends: [a.yaml]\n");
        assert!(refusal(&root).contains("loops"));
    }

    #[test]
    fn extending_a_file_that_is_missing_or_toml_is_refused() {
        let root = workspace("extends-missing");
        write(&root.join("rqx.yaml"), "extends: [nowhere.yaml]\n");
        assert!(refusal(&root).contains("nowhere.yaml"));
        write(&root.join("rqx.yaml"), "extends: [old.toml]\n");
        assert!(refusal(&root).contains("rqx config migrate"));
    }

    fn failure(outcome: Result<Scope>) -> String {
        match outcome {
            Ok(_) => panic!("expected the override to be refused"),
            Err(error) => format!("{error:#}"),
        }
    }

    fn tree(text: &str) -> Value {
        files::parse(text).expect("yaml")
    }

    fn merged(layers: &[&str]) -> RqxConfig {
        let mut base = files::empty();
        for layer in layers {
            merge(&mut base, tree(layer));
        }
        serde_json::from_value(base).unwrap()
    }

    #[test]
    fn a_later_layer_sets_one_key_without_resetting_its_neighbours() {
        let config = merged(&[
            "thresholds:\n  max_params: 9\n  nesting_depth: 7\n",
            "thresholds:\n  max_params: 2\n",
        ]);
        assert_eq!(config.thresholds.max_params, 2);
        assert_eq!(config.thresholds.nesting_depth, 7);
    }

    #[test]
    fn a_later_layer_replaces_a_list_rather_than_growing_it() {
        let config = merged(&[
            "ignore:\n  patterns: [a, b]\n",
            "ignore:\n  patterns: [c]\n",
        ]);
        assert_eq!(config.ignore.patterns, vec!["c".to_owned()]);
    }

    /// The keys an override may carry are checked when its file is read, so the answer
    /// to "can a subtree move the scan" is the same however the run reaches it.
    #[test]
    fn an_override_that_reaches_beyond_its_subtree_is_refused() {
        let root = workspace("override-scan");
        write(
            &root.join("rqx.yaml"),
            "overrides:\n  - paths: [src/**]\n    scan:\n      threads: 2\n",
        );
        let error = refusal(&root);
        assert!(error.contains("scan"), "{error}");
        assert!(error.contains("thresholds"), "{error}");
    }

    #[test]
    fn an_override_without_paths_is_refused() {
        let base = files::empty();
        let entry = tree("thresholds:\n  max_params: 9\n");
        let error = failure(scope(Path::new("rqx.yaml"), entry, &base, Level::Standard));
        assert!(error.contains("names no `paths`"), "{error}");
    }

    /// Strict in the domain and relaxed in the scripts is the reason a level can be
    /// scoped at all; the gate is not, because a run reaches one verdict.
    #[test]
    fn an_override_may_hold_its_subtree_to_a_different_level() {
        let base = files::empty();
        let entry = tree("paths: [src/domain/**]\nlevel: strict\n");
        let scoped = scope(Path::new("rqx.yaml"), entry, &base, Level::Relaxed).unwrap();
        assert_eq!(scoped.config.thresholds.cognitive_complexity, 10);
        assert_eq!(
            scoped.config.gate.fail_on.slug(),
            "error",
            "a scoped level must not move the gate"
        );
    }

    #[test]
    fn a_level_never_overrides_what_the_project_wrote() {
        let base = files::empty();
        let entry = tree("paths: [src/**]\nlevel: strict\nthresholds:\n  max_params: 9\n");
        let scoped = scope(Path::new("rqx.yaml"), entry, &base, Level::Standard).unwrap();
        assert_eq!(scoped.config.thresholds.max_params, 9);
        assert_eq!(scoped.config.thresholds.nesting_depth, 2);
    }

    #[test]
    fn an_override_naming_a_renamed_family_is_refused_with_the_new_name() {
        let base = files::empty();
        let entry = tree("paths: [src/**]\nrules:\n  categories: [bloaters]\n");
        let error = failure(scope(Path::new("rqx.yaml"), entry, &base, Level::Standard));
        assert!(
            error.contains("`bloaters`") && error.contains("`size`"),
            "{error}"
        );
    }

    #[test]
    fn an_override_keeps_the_thresholds_it_does_not_mention() {
        let base = tree("thresholds:\n  max_params: 9\n  nesting_depth: 7\n");
        let entry = tree("paths: [src/**]\nthresholds:\n  max_params: 2\n");
        let scope = scope(Path::new("rqx.yaml"), entry, &base, Level::Standard).unwrap();
        assert_eq!(scope.config.thresholds.max_params, 2);
        assert_eq!(scope.config.thresholds.nesting_depth, 7);
    }
}

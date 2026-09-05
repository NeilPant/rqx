use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use ignore::overrides::{Override, OverrideBuilder};
use rqx_core::config::{
    CONFIG_FILE_NAME, LEGACY_CONFIG_FILE_NAME, Level, RqxConfig, SCOPED_TABLES, TABLES,
};
use toml::Value;

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
    pub provenance: BTreeMap<&'static str, PathBuf>,
    /// Which file wrote each individual `table.key`. The per-table map cannot answer
    /// that once two layers both touch `[thresholds]`, and a locked key has to be
    /// able to name the file that set it.
    key_sources: BTreeMap<String, PathBuf>,
    /// The keys the policy will not let a project change.
    locked: BTreeSet<String>,
    pub policy: Option<PathBuf>,
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
            written: Value::Table(toml::Table::new()),
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

    /// The name of the file the settings came from, for the header. The last one wins
    /// because that is the one whose values survived the merge.
    pub fn display_source(&self) -> Option<String> {
        let path = self.sources.last()?;
        Some(path.file_name()?.to_string_lossy().into_owned())
    }

    /// The file that set a table, for the `from` column. Falls back to the last file
    /// read, so a value that came from a layer rqx merged still names a real file.
    /// The file that wrote one setting, which is not always the last file to touch
    /// its table.
    pub fn source_of_key(&self, table: &str, key: &str) -> Option<&Path> {
        self.key_sources
            .get(&format!("{table}.{key}"))
            .map(PathBuf::as_path)
    }

    /// Whether a policy forbids this project changing one setting.
    pub fn is_locked(&self, table: &str, key: &str) -> bool {
        self.locked.contains(&format!("{table}.{key}"))
    }

    pub fn source_of(&self, table: &str) -> Option<&Path> {
        self.provenance
            .get(table)
            .or_else(|| self.sources.last())
            .map(PathBuf::as_path)
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
    let project = project_configs(&anchor);
    let mut layers: Vec<PathBuf> = Vec::new();
    layers.extend(user_config());
    // Under the project rather than over it: a repository may tighten anything the
    // policy did not lock, and a policy that sat on top would make every project
    // identical rather than every project compliant.
    let policy = policy_of(&project)?;
    layers.extend(policy.clone());
    layers.extend(project);
    if let Some(path) = std::env::var_os(CONFIG_ENV).map(PathBuf::from) {
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
        if !path.exists() {
            bail!(
                "`{}` does not exist. Drop `--config` to let rqx find `{CONFIG_FILE_NAME}` \
                 on its own, or run `rqx init` to write one.",
                path.display()
            );
        }
        layers.push(path.to_path_buf());
    }

    let mut merged = Value::Table(toml::Table::new());
    let mut provenance = BTreeMap::new();
    let mut raw_overrides: Vec<(PathBuf, Value)> = Vec::new();
    let mut sources = Vec::new();
    let mut locked: Vec<String> = Vec::new();
    let mut key_sources: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut written: Vec<(PathBuf, Vec<(String, Value)>)> = Vec::new();

    for path in layers {
        let mut value = read(&path)?;
        for entry in take_overrides(&mut value, &path)? {
            raw_overrides.push((path.clone(), entry));
        }
        let is_policy = policy.as_deref() == Some(path.as_path());
        locked.extend(take_locked(&mut value, &path, is_policy)?);
        take_policy(&mut value);
        if let Some(table) = value.as_table() {
            for key in table.keys() {
                if let Some(known) = TABLES.iter().find(|table| *table == key) {
                    provenance.insert(*known, path.clone());
                }
            }
        }
        let mut leaves = Vec::new();
        flatten("", &value, &mut leaves);
        for (key, _) in &leaves {
            key_sources.insert(key.clone(), path.clone());
        }
        written.push((path.clone(), leaves));
        merge(&mut merged, value);
        sources.push(path);
    }

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

    let config: RqxConfig = effective.try_into().with_context(|| describe(&sources))?;
    reject_unknown_names(&config, &describe(&sources))?;
    if level.is_some() {
        provenance.insert("level", PathBuf::from("--level"));
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
                    "`[gate] baseline` names {}, which does not exist. \
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

fn read(path: &Path) -> Result<Value> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    document(&text).with_context(|| format!("{} is not valid TOML", path.display()))
}

/// A config file is a document, not a bare value: `"a = 1".parse::<Value>()` reads
/// the `a` and then rejects the rest of the line.
fn document(text: &str) -> Result<Value> {
    Ok(Value::Table(text.parse::<toml::Table>()?))
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

/// Personal defaults, applied under anything the project says. The first location
/// that exists wins, so a machine with both an XDG directory and a macOS one does not
/// silently read two.
fn user_config() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        candidates.push(PathBuf::from(xdg).join("rqx").join(CONFIG_FILE_NAME));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".config").join("rqx").join(CONFIG_FILE_NAME));
        if cfg!(target_os = "macos") {
            candidates.push(
                home.join("Library")
                    .join("Application Support")
                    .join("rqx")
                    .join(CONFIG_FILE_NAME),
            );
        }
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(PathBuf::from(appdata).join("rqx").join(CONFIG_FILE_NAME));
    }
    candidates
        .into_iter()
        .flat_map(|path| {
            let legacy = path.with_file_name(LEGACY_CONFIG_FILE_NAME);
            [path, legacy]
        })
        .find(|path| path.exists())
}

/// The config a directory holds, under the name Rqx uses now or the one it used
/// before. Checking both here is what keeps one existing checkout working without
/// every caller knowing there were ever two spellings.
fn named_config(directory: &Path) -> PathBuf {
    let canonical = directory.join(CONFIG_FILE_NAME);
    if canonical.exists() {
        return canonical;
    }
    let legacy = directory.join(LEGACY_CONFIG_FILE_NAME);
    if legacy.exists() {
        return legacy;
    }
    canonical
}

/// Walks up from the scanned directory, outermost file first so the nearest one wins
/// the merge. Stops at a repository boundary: a config above a checkout describes a
/// different project, and reading it would make a scan depend on where the repository
/// happens to sit.
fn project_configs(anchor: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for directory in anchor.ancestors() {
        let candidate = named_config(directory);
        let declares_root = candidate
            .exists()
            .then(|| read(&candidate).ok())
            .flatten()
            .and_then(|value| value.get("root").and_then(Value::as_bool))
            .unwrap_or(false);
        if candidate.exists() {
            found.push(candidate);
        }
        if declares_root || directory.join(".git").exists() {
            break;
        }
    }
    found.reverse();
    found
}

/// Removes `[[override]]` before the rest is deserialized, because `RqxConfig` denies
/// unknown fields and an override is not one of its tables.
/// The policy a project points at, if any. Read from the nearest config that names
/// one so a monorepo can put the pointer at its root, and overridden by the
/// environment for a CI job that has to impose one.
fn policy_of(project: &[PathBuf]) -> Result<Option<PathBuf>> {
    if let Some(named) = std::env::var_os(POLICY_ENV).map(PathBuf::from) {
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
        let Some(named) = read(config)?
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

/// `policy` is a pointer, not a setting, so it never reaches `RqxConfig` — which
/// denies unknown fields and would refuse it.
fn take_policy(value: &mut Value) {
    if let Some(table) = value.as_table_mut() {
        table.remove("policy");
    }
}

fn take_locked(value: &mut Value, path: &Path, is_policy: bool) -> Result<Vec<String>> {
    let Some(table) = value.as_table_mut() else {
        return Ok(Vec::new());
    };
    let Some(entry) = table.remove("locked") else {
        return Ok(Vec::new());
    };
    if !is_policy {
        bail!(
            "`[locked]` in {} does nothing: only the policy a project points at can \
             lock a key.",
            path.display()
        );
    }
    let keys = entry
        .get("keys")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("`[locked] keys` in {} must be a list", path.display()))?;
    keys.iter()
        .map(|key| {
            key.as_str().map(str::to_owned).ok_or_else(|| {
                anyhow::anyhow!(
                    "`[locked] keys` in {} holds something that is not a name",
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
        Value::Table(table) => {
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
    written: &[(PathBuf, Vec<(String, Value)>)],
    overrides: &[(PathBuf, Value)],
    level: Option<Level>,
) -> Result<()> {
    let from_policy: &[(String, Value)] = written
        .iter()
        .find(|(path, _)| path == policy)
        .map_or(&[], |(_, leaves)| leaves.as_slice());
    let above = written
        .iter()
        .skip_while(|(path, _)| path != policy)
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
        for (path, leaves) in above.clone() {
            if let Some((_, got)) = leaves.iter().find(|(named, _)| named == key)
                && got != want
            {
                bail!(
                    "`{key}` is locked to {want} by {}; {} sets {got}.",
                    policy.display(),
                    path.display()
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
                        "`{key}` is locked to {want} by {}; an `[[override]]` in {} sets {got}.",
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
    let Some(table) = value.as_table_mut() else {
        bail!("{} must be a table of settings", path.display());
    };
    let Some(entries) = table.remove("override") else {
        return Ok(Vec::new());
    };
    let Value::Array(entries) = entries else {
        bail!(
            "`override` in {} must be written as `[[override]]` sections, one per subtree",
            path.display()
        );
    };
    Ok(entries)
}

fn scope(path: &Path, entry: Value, base: &Value, level: Level) -> Result<Scope> {
    let Some(table) = entry.as_table() else {
        bail!("an `[[override]]` in {} is not a table", path.display());
    };
    for key in table.keys() {
        // `level` joins `paths` rather than the scoped tables: it names a set of
        // defaults for this subtree, which is exactly what a subtree may narrow.
        if key != "paths" && key != "level" && !SCOPED_TABLES.contains(&key.as_str()) {
            bail!(
                "`[[override]]` in {} sets `{key}`, which applies to the whole run and \
                 cannot be scoped to a subtree. Move it to the top level; an override \
                 may set `level`, {}.",
                path.display(),
                SCOPED_TABLES.join(" or ")
            );
        }
    }
    if let Some(name) = table.get("level").and_then(Value::as_str)
        && Level::parse(name).is_none()
    {
        bail!(
            "`[[override]]` in {} sets `level = \"{name}\"`, which is not a level. \
             Levels are {}.",
            path.display(),
            Level::ALL
                .iter()
                .map(|level| level.slug())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let globs: Vec<String> = table
        .get("paths")
        .and_then(Value::as_array)
        .map(|paths| {
            paths
                .iter()
                .filter_map(|glob| glob.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if globs.is_empty() {
        bail!(
            "an `[[override]]` in {} names no `paths`, so nothing would use it",
            path.display()
        );
    }

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

    let merged = base.clone();
    // A subtree can be held to a different level: strict in the domain, relaxed in
    // the scripts. The gate is dropped from a scoped preset because a gate reaches
    // one verdict for the whole run.
    let scoped_level = table
        .get("level")
        .and_then(Value::as_str)
        .and_then(Level::parse)
        .unwrap_or(level);
    let mut preset = document(scoped_level.layer())?;
    if let Some(table) = preset.as_table_mut() {
        table.remove("gate");
    }
    let mut merged = {
        let mut base = preset;
        merge(&mut base, merged);
        base
    };

    let mut scoped = toml::Table::new();
    for name in SCOPED_TABLES {
        if let Some(value) = table.get(name) {
            scoped.insert(name.to_owned(), value.clone());
        }
    }
    merge(&mut merged, Value::Table(scoped));
    let config: RqxConfig = merged
        .try_into()
        .with_context(|| format!("reading an `[[override]]` in {}", path.display()))?;
    reject_unknown_names(&config, &format!("an `[[override]]` in {}", path.display()))?;

    Ok(Scope {
        matcher,
        config,
        directory: directory.to_path_buf(),
        globs,
    })
}

/// A level's settings with the explicit ones on top. Written as a merge rather than
/// a set of defaults per field so a preset stays one readable TOML file.
fn under(level: Level, explicit: &Value) -> Result<Value> {
    let mut base = document(level.layer()).with_context(|| format!("the `{level}` level"))?;
    merge(&mut base, explicit.clone());
    Ok(base)
}

fn reject_unknown_names(config: &RqxConfig, source: &str) -> Result<()> {
    let plan = &config.architecture;
    let undeclared = plan.undeclared_layers();
    if !undeclared.is_empty() {
        bail!(
            "{source}: `[architecture.paths]` puts a subtree in {}, which `layers` does \
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
            "{source}: `[architecture] layers` names {:?} twice, so which way a \
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
        (Value::Table(base), Value::Table(layer)) => {
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

    /// A policy that cannot be enforced is a suggestion, and the whole point of the
    /// table is that a repository cannot quietly opt out of it.
    #[test]
    fn a_project_that_moves_a_locked_key_is_refused_naming_the_key_and_both_files() {
        let root = workspace("locked-moved");
        write(
            &root.join("platform/policy.toml"),
            "[thresholds]\ncognitive_complexity = 12\n\n[locked]\nkeys = [\"thresholds.cognitive_complexity\"]\n",
        );
        write(
            &root.join("service/RQX.toml"),
            "policy = \"../platform/policy.toml\"\n\n[thresholds]\ncognitive_complexity = 25\n",
        );

        let Err(error) = resolve(&root.join("service"), None, None) else {
            panic!("a locked key was moved");
        };
        let error = error.to_string();
        assert!(error.contains("thresholds.cognitive_complexity"), "{error}");
        assert!(error.contains("policy.toml"), "{error}");
        assert!(error.contains("RQX.toml"), "{error}");
        assert!(error.contains("25"), "{error}");
    }

    #[test]
    fn a_policy_sets_what_the_project_leaves_alone_and_yields_where_it_is_not_locked() {
        let root = workspace("policy-yields");
        write(
            &root.join("platform/policy.toml"),
            "[thresholds]\ncognitive_complexity = 12\nmax_params = 6\n\n[locked]\nkeys = [\"thresholds.cognitive_complexity\"]\n",
        );
        write(
            &root.join("service/RQX.toml"),
            "policy = \"../platform/policy.toml\"\n\n[thresholds]\nmax_params = 2\n",
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
                .source_of_key("thresholds", "cognitive_complexity")
                .is_some_and(|path| path.ends_with("policy.toml"))
        );
        assert!(
            settings
                .source_of_key("thresholds", "max_params")
                .is_some_and(|path| path.ends_with("RQX.toml"))
        );
    }

    /// Repeating the policy's own value is not a change, and refusing it would make a
    /// project unable to state the standard it is holding itself to.
    #[test]
    fn a_project_may_restate_a_locked_value() {
        let root = workspace("restate");
        write(
            &root.join("platform/policy.toml"),
            "[gate]\nfail_on = \"warning\"\n\n[locked]\nkeys = [\"gate.fail_on\"]\n",
        );
        write(
            &root.join("service/RQX.toml"),
            "policy = \"../platform/policy.toml\"\n\n[gate]\nfail_on = \"warning\"\n",
        );
        assert!(resolve(&root.join("service"), None, None).is_ok());
    }

    /// An override is merged after every layer, so a lock it contradicts would win.
    #[test]
    fn an_override_cannot_move_a_locked_key_either() {
        let root = workspace("override-lock");
        write(
            &root.join("platform/policy.toml"),
            "[thresholds]\nmax_params = 3\n\n[locked]\nkeys = [\"thresholds.max_params\"]\n",
        );
        write(
            &root.join("service/RQX.toml"),
            "policy = \"../platform/policy.toml\"\n\n[[override]]\npaths = [\"legacy/**\"]\n\n[override.thresholds]\nmax_params = 9\n",
        );
        let Err(error) = resolve(&root.join("service"), None, None) else {
            panic!("an override moved a locked key");
        };
        let error = error.to_string();
        assert!(error.contains("override"), "{error}");
    }

    /// `[locked]` outside the policy would let a project lock itself against the
    /// platform, which is the wrong way round.
    #[test]
    fn only_the_policy_may_lock_anything() {
        let root = workspace("project-lock");
        write(
            &root.join("service/RQX.toml"),
            "[thresholds]\nmax_params = 3\n\n[locked]\nkeys = [\"thresholds.max_params\"]\n",
        );
        let Err(error) = resolve(&root.join("service"), None, None) else {
            panic!("a project tried to lock a key");
        };
        let error = error.to_string();
        assert!(error.contains("only the policy"), "{error}");
    }

    fn failure(outcome: Result<Scope>) -> String {
        match outcome {
            Ok(_) => panic!("expected the override to be refused"),
            Err(error) => error.to_string(),
        }
    }

    fn merged(layers: &[&str]) -> RqxConfig {
        let mut base = Value::Table(toml::Table::new());
        for layer in layers {
            merge(&mut base, document(layer).unwrap());
        }
        base.try_into().unwrap()
    }

    #[test]
    fn a_later_layer_sets_one_key_without_resetting_its_neighbours() {
        let config = merged(&[
            "[thresholds]\nmax_params = 9\nnesting_depth = 7\n",
            "[thresholds]\nmax_params = 2\n",
        ]);
        assert_eq!(config.thresholds.max_params, 2);
        assert_eq!(config.thresholds.nesting_depth, 7);
    }

    #[test]
    fn a_later_layer_replaces_a_list_rather_than_growing_it() {
        let config = merged(&[
            "[ignore]\npatterns = [\"a\", \"b\"]\n",
            "[ignore]\npatterns = [\"c\"]\n",
        ]);
        assert_eq!(config.ignore.patterns, vec!["c".to_owned()]);
    }

    #[test]
    fn an_override_that_reaches_beyond_its_subtree_is_refused() {
        let base = Value::Table(toml::Table::new());
        let entry = document("paths = [\"src/**\"]\n[scan]\nthreads = 2\n").unwrap();
        let error = failure(scope(Path::new("RQX.toml"), entry, &base, Level::Standard));
        assert!(error.contains("cannot be scoped"), "{error}");
    }

    #[test]
    fn an_override_without_paths_is_refused() {
        let base = Value::Table(toml::Table::new());
        let entry = document("[thresholds]\nmax_params = 9\n").unwrap();
        let error = failure(scope(Path::new("RQX.toml"), entry, &base, Level::Standard));
        assert!(error.contains("names no `paths`"), "{error}");
    }

    /// Strict in the domain and relaxed in the scripts is the reason a level can be
    /// scoped at all; the gate is not, because a run reaches one verdict.
    #[test]
    fn an_override_may_hold_its_subtree_to_a_different_level() {
        let base = Value::Table(toml::Table::new());
        let entry = document("paths = [\"src/domain/**\"]\nlevel = \"strict\"\n").unwrap();
        let scoped = scope(Path::new("RQX.toml"), entry, &base, Level::Relaxed).unwrap();
        assert_eq!(scoped.config.thresholds.cognitive_complexity, 10);
        assert_eq!(
            scoped.config.gate.fail_on.slug(),
            "error",
            "a scoped level must not move the gate"
        );
    }

    #[test]
    fn a_level_never_overrides_what_the_project_wrote() {
        let base = Value::Table(toml::Table::new());
        let entry =
            document("paths = [\"src/**\"]\nlevel = \"strict\"\n[thresholds]\nmax_params = 9\n")
                .unwrap();
        let scoped = scope(Path::new("RQX.toml"), entry, &base, Level::Standard).unwrap();
        assert_eq!(scoped.config.thresholds.max_params, 9);
        assert_eq!(scoped.config.thresholds.nesting_depth, 2);
    }

    #[test]
    fn an_override_naming_a_renamed_family_is_refused_with_the_new_name() {
        let base = Value::Table(toml::Table::new());
        let entry =
            document("paths = [\"src/**\"]\n[rules]\ncategories = [\"bloaters\"]\n").unwrap();
        let error = failure(scope(Path::new("RQX.toml"), entry, &base, Level::Standard));
        assert!(
            error.contains("`bloaters`") && error.contains("`size`"),
            "{error}"
        );
    }

    #[test]
    fn an_override_keeps_the_thresholds_it_does_not_mention() {
        let base = document("[thresholds]\nmax_params = 9\nnesting_depth = 7\n").unwrap();
        let entry = document("paths = [\"src/**\"]\n[thresholds]\nmax_params = 2\n").unwrap();
        let scope = scope(Path::new("RQX.toml"), entry, &base, Level::Standard).unwrap();
        assert_eq!(scope.config.thresholds.max_params, 2);
        assert_eq!(scope.config.thresholds.nesting_depth, 7);
    }
}

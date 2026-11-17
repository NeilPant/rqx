//! `extends`: building a config on presets and on other files.
//!
//! Everything a file extends is laid *under* every file the run reads, not just under
//! the file that named it. That is what makes the precedence one sentence: presets and
//! extended files, then the user config, then a policy, then project files from the
//! outside in, then `RQX_CONFIG`, then `--config`, then flags. A preset that sat just
//! under the file naming it would sit above a policy and quietly override it.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use rqx_core::{presets, suggest};
use serde_json::Value;

use super::files;
use super::origin::Origin;

/// A fragment of settings and where it came from.
pub struct Layer {
    pub origin: Origin,
    pub value: Value,
}

/// Everything `file` extends, lowest precedence first: for each name in order, what
/// that name itself extends and then the name's own settings.
pub fn inherited(file: &Path, value: &Value) -> Result<Vec<Layer>> {
    let owner = Origin::File(file.to_path_buf());
    let mut layers = Vec::new();
    expand(
        &names_in(value),
        &owner,
        &mut vec![owner.clone()],
        &mut layers,
    )?;
    Ok(layers)
}

/// `extends` is a pointer, not a setting, so it never reaches the resolved config.
pub fn take_extends(value: &mut Value) {
    if let Some(table) = value.as_object_mut() {
        table.remove("extends");
    }
}

fn names_in(value: &Value) -> Vec<String> {
    value
        .get("extends")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn expand(
    names: &[String],
    owner: &Origin,
    trail: &mut Vec<Origin>,
    out: &mut Vec<Layer>,
) -> Result<()> {
    for name in names {
        let (origin, value) = resolve_name(name, owner)?;
        if trail.contains(&origin) {
            bail!(
                "`extends` loops: {} extends {origin}, which leads back to it",
                trail.last().map_or_else(String::new, ToString::to_string)
            );
        }
        trail.push(origin.clone());
        expand(&names_in(&value), &origin, trail, out)?;
        trail.pop();
        out.push(Layer { origin, value });
    }
    Ok(())
}

fn resolve_name(name: &str, owner: &Origin) -> Result<(Origin, Value)> {
    if is_path(name) {
        return read_file(name, owner);
    }
    match presets::lookup(name) {
        Some(yaml) => Ok((Origin::Preset(name.to_owned()), files::parse(yaml)?)),
        None => Err(unknown(name, owner)),
    }
}

/// A name with a separator or a file extension is a file; a `.toml` one is let through
/// to be refused with the way to migrate it, rather than reported as an unknown preset. Presets never have either, so the two
/// cannot be confused.
fn is_path(name: &str) -> bool {
    name.contains(['/', '\\'])
        || [".yaml", ".yml", ".toml"]
            .iter()
            .any(|extension| name.ends_with(extension))
}

fn read_file(name: &str, owner: &Origin) -> Result<(Origin, Value)> {
    let (Origin::File(from) | Origin::Extended(from)) = owner else {
        bail!("the `{owner}` cannot extend a file, only a preset");
    };
    let path: PathBuf = from.parent().unwrap_or(Path::new(".")).join(name);
    files::refuse_retired(&path)?;
    let path = rqx_core::paths::canonicalize(&path).map_err(|_| {
        anyhow!(
            "{} extends `{name}`, which does not exist at {}",
            from.display(),
            from.parent().unwrap_or(Path::new(".")).join(name).display()
        )
    })?;
    let value = files::read(&path)?;
    Ok((Origin::Extended(path), value))
}

fn unknown(name: &str, owner: &Origin) -> anyhow::Error {
    let available = presets::names();
    let hint = suggest::closest(name, available.iter().copied())
        .map_or_else(String::new, |close| format!(" Did you mean `{close}`?"));
    anyhow!(
        "{owner} extends `{name}`, which is not a preset or a path to a YAML file.{hint} \
         Presets are {}.",
        available.join(", ")
    )
}

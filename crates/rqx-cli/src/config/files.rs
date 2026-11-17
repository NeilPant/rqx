//! Finding a config file and reading it.
//!
//! Everything here answers one of two questions: which file does a directory (or the
//! user, or a flag) mean, and what does it say. A TOML file is never an answer to
//! either. It is refused, loudly, because skipping it would let a project that moved
//! its settings to YAML last week keep gating on the old file's absence.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rqx_core::config::{CONFIG_FILE_NAME, CONFIG_FILE_NAMES, RETIRED_CONFIG_FILE_NAMES, RqxConfig};
use serde_json::{Map, Value};

pub fn empty() -> Value {
    Value::Object(Map::new())
}

/// The config a directory holds, if it holds one. Two is an error because which wins
/// would depend on an order nobody wrote down, and a retired TOML file is an error
/// whether or not a YAML one sits beside it.
pub fn in_directory(directory: &Path) -> Result<Option<PathBuf>> {
    if let Some(retired) = RETIRED_CONFIG_FILE_NAMES
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.exists())
    {
        return Err(retired_error(&retired));
    }
    let present: Vec<PathBuf> = CONFIG_FILE_NAMES
        .iter()
        .map(|name| directory.join(name))
        .filter(|path| path.exists())
        .collect();
    match present.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(only.clone())),
        [first, second, ..] => bail!(
            "{} and {} both configure {}. Keep one of them.",
            first.display(),
            second.display(),
            directory.display()
        ),
    }
}

/// The config a directory has, or the file `rqx init` would write there. For callers
/// that mean to read or edit the project's file and must not create a second one
/// beside a `.rqx.yml` that is already in use.
pub fn config_path_in(directory: &Path) -> PathBuf {
    in_directory(directory)
        .ok()
        .flatten()
        .unwrap_or_else(|| directory.join(CONFIG_FILE_NAME))
}

/// Where personal defaults may live, most specific first. Several are listed because
/// platforms disagree about where a user's config belongs.
pub fn user_config_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        directories.push(PathBuf::from(xdg).join("rqx"));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        directories.push(home.join(".config").join("rqx"));
        if cfg!(target_os = "macos") {
            directories.push(home.join("Library").join("Application Support").join("rqx"));
        }
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        directories.push(PathBuf::from(appdata).join("rqx"));
    }
    directories
}

/// Personal defaults, applied under anything the project says. The first location
/// that holds a file wins, so a machine with both an XDG directory and a macOS one
/// does not silently read two.
pub fn user_config() -> Result<Option<PathBuf>> {
    for directory in user_config_directories() {
        if let Some(found) = in_directory(&directory)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// A path someone named directly — `--config`, `RQX_CONFIG`, a policy pointer — that
/// is a TOML file. Named by extension rather than by the retired names, because a
/// policy can be called anything.
pub fn refuse_retired(path: &Path) -> Result<()> {
    if path
        .extension()
        .is_some_and(|extension| extension == "toml")
    {
        return Err(retired_error(path));
    }
    Ok(())
}

fn retired_error(path: &Path) -> anyhow::Error {
    Located::refusal(
        path,
        None,
        format!(
            "{} is a TOML file. rqx no longer reads TOML configuration, and skipping it \
             could change what a pipeline gates on, so it stops here. Run \
             `rqx config migrate` to convert it to YAML; the original is kept as a `.bak`.",
            path.display()
        ),
    )
}

/// A line and a column in a file, both counted from one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: u64,
    pub column: u64,
}

/// A refusal that knows which file it is about, and where in it when the file can say.
/// It prints exactly what the same refusal always printed; the extra fields are for
/// `rqx config validate`, which has to report a place rather than a paragraph.
#[derive(Debug)]
pub struct Located {
    pub file: PathBuf,
    pub position: Option<Position>,
    pub message: String,
    shown: String,
}

impl Located {
    /// A refusal whose printed form is its message.
    pub fn refusal(file: &Path, position: Option<Position>, message: String) -> anyhow::Error {
        anyhow::Error::new(Self {
            file: file.to_path_buf(),
            position,
            shown: message.clone(),
            message,
        })
    }
}

impl fmt::Display for Located {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.shown)
    }
}

impl std::error::Error for Located {}

/// A config file as a tree of values, after checking it against the shape the engine
/// expects. The check runs on the file alone so an error names a line in it; once
/// layers are merged that information is gone.
pub fn read(path: &Path) -> Result<Value> {
    refuse_retired(path)?;
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    deserialize::<RqxConfig>(&text, path)?;
    parse(&text).with_context(|| format!("reading {}", path.display()))
}

/// Whether text would load as a config file, without reading it from disk. For a writer
/// that must not leave a project with a file rqx refuses.
pub fn check_text(text: &str, path: &Path) -> Result<()> {
    deserialize::<RqxConfig>(text, path).map(drop)
}

/// YAML text as a tree. An empty document, or one that is only comments, is a config
/// that sets nothing.
pub fn parse(text: &str) -> Result<Value> {
    match serde_saphyr::from_str::<Value>(text)? {
        Value::Null => Ok(empty()),
        tree @ Value::Object(_) => Ok(tree),
        _ => bail!("a config file is a mapping of settings, not a list or a single value"),
    }
}

/// Deserialises with the file's name in the error, so a type mismatch reads
/// `rqx.yaml:12:3` rather than `<input>:12:3`.
pub fn deserialize<T: serde::de::DeserializeOwned>(text: &str, path: &Path) -> Result<T> {
    serde_saphyr::from_str(text).map_err(|error| {
        let name = path.display().to_string();
        let shown = error.render_with_options(serde_saphyr::render_options! {
            source_name: Some(name.as_str()),
        });
        let position = error.location().map(|at| Position {
            line: at.line(),
            column: at.column(),
        });
        anyhow::Error::new(Located {
            file: path.to_path_buf(),
            position,
            message: plain_message(&error, position),
            shown,
        })
    })
}

/// The parser's words without the position it prefixes them with, which the report
/// prints in its own column.
fn plain_message(error: &serde_saphyr::Error, position: Option<Position>) -> String {
    let plain = error.without_snippet().to_string();
    let prefix = position.map(|at| format!("error: line {} column {}: ", at.line, at.column));
    match prefix
        .as_deref()
        .and_then(|prefix| plain.strip_prefix(prefix))
    {
        Some(rest) => rest.to_owned(),
        None => plain,
    }
}

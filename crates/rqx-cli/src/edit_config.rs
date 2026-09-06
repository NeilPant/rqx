//! Changing a setting by editing the file that holds it.
//!
//! Config composes from several files — a user config, a policy, one `RQX.toml` per
//! directory up to the repository root — so "set this threshold" has to answer *in
//! which file* before it can answer anything else.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::config::Resolved;

/// A file a setting could be written to, and what it already says about the key
/// being changed.
pub struct Target {
    pub path: PathBuf,
    /// How the file names itself to a reader: the directory it governs, relative to
    /// the project, or an absolute path when it sits outside.
    pub label: String,
    pub holds: Option<String>,
    /// A policy is read-only from here: changing it changes every repository that
    /// points at it, which is not a thing to do by accident from one of them.
    pub writable: bool,
}

/// Every file in the chain, nearest last, so a reader picking one sees the same
/// order `rqx config` lists them in.
pub fn targets(settings: &Resolved, root: &Path, key: &str) -> Vec<Target> {
    settings
        .sources
        .iter()
        .map(|path| {
            let policy = settings.policy.as_deref() == Some(path.as_path());
            Target {
                label: label_of(path, root),
                holds: read(path).ok().and_then(|document| show(&document, key)),
                writable: !policy,
                path: path.clone(),
            }
        })
        .collect()
}

fn label_of(path: &Path, root: &Path) -> String {
    let directory = path.parent().unwrap_or(path);
    match directory.strip_prefix(root) {
        Ok(inside) if inside.as_os_str().is_empty() => "this project".to_owned(),
        Ok(inside) => inside.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

/// The file a `set` with no `--file` writes to: the one that already holds the key,
/// else the nearest project config. Anything else surprises someone.
pub fn default_target(settings: &Resolved, key: &str) -> Option<PathBuf> {
    settings
        .source_of_key_public(key)
        .map(Path::to_path_buf)
        .or_else(|| settings.sources.last().cloned())
}

pub fn read(path: &Path) -> Result<DocumentMut> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.parse::<DocumentMut>()
        .with_context(|| format!("{} is not valid TOML", path.display()))
}

fn show(document: &DocumentMut, key: &str) -> Option<String> {
    let mut item = document.as_item();
    for part in key.split('.') {
        item = item.get(part)?;
    }
    Some(item.to_string().trim().to_owned())
}

/// Written as the type the key already has where there is one, so `max_params = "3"`
/// cannot happen by typing a number at a prompt.
fn typed(raw: &str) -> Item {
    if let Ok(number) = raw.parse::<i64>() {
        return value(number);
    }
    if let Ok(number) = raw.parse::<f64>() {
        return value(number);
    }
    match raw {
        "true" => value(true),
        "false" => value(false),
        _ if raw.starts_with('[') => raw
            .parse::<Item>()
            .unwrap_or_else(|_| value(raw.trim_matches('"'))),
        _ => value(raw.trim_matches('"')),
    }
}

/// Writes one `table.key`, creating the table if the file does not have it yet.
/// Formatting, comments and key order elsewhere in the file are preserved, which is
/// the whole reason this does not go through `toml::Value`.
pub fn set(path: &Path, key: &str, raw: &str) -> Result<String> {
    let mut document = if path.exists() {
        read(path)?
    } else {
        DocumentMut::new()
    };
    let before = show(&document, key);
    let (table, field) = split(key)?;

    let entry = document
        .as_table_mut()
        .entry(table)
        .or_insert_with(|| Item::Table(Table::new()));
    let Some(entry) = entry.as_table_mut() else {
        bail!("`{table}` in {} is not a table", path.display());
    };
    entry.set_implicit(false);
    entry[field] = typed(raw);

    write(path, &document)?;
    Ok(match before {
        Some(was) => format!("{was} \u{2192} {raw}"),
        None => raw.to_owned(),
    })
}

pub fn unset(path: &Path, key: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let mut document = read(path)?;
    let (table, field) = split(key)?;
    let Some(entry) = document.get_mut(table).and_then(Item::as_table_mut) else {
        return Ok(false);
    };
    if entry.remove(field).is_none() {
        return Ok(false);
    }
    // A table left empty by the removal is noise, and the next `set` recreates it.
    if entry.is_empty() {
        document.as_table_mut().remove(table);
    }
    write(path, &document)?;
    Ok(true)
}

fn write(path: &Path, document: &DocumentMut) -> Result<()> {
    if let Some(directory) = path.parent()
        && !directory.as_os_str().is_empty()
    {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("creating {}", directory.display()))?;
    }
    crate::fix::write_atomically(path, &document.to_string())
}

fn split(key: &str) -> Result<(&str, &str)> {
    key.split_once('.').ok_or_else(|| {
        anyhow::anyhow!(
            "`{key}` names no table. Settings are written `table.key`, as in \
             `thresholds.max_params`."
        )
    })
}

/// What the `config` subcommands ask for, kept out of the clap types so the module
/// does not depend on the shape of the command line.
pub enum Change {
    Set {
        key: String,
        value: String,
        file: Option<PathBuf>,
        pick: bool,
    },
    Unset {
        key: String,
        file: Option<PathBuf>,
        pick: bool,
    },
    Files,
}

pub fn run(
    root: &Path,
    settings: &Resolved,
    change: Change,
    palette: crate::brand::Palette,
    out: &mut dyn std::io::Write,
) -> Result<()> {
    use crate::brand::{Ink, paint};
    let dim = palette.style(Ink::Muted);

    let (key, file, pick) = match &change {
        Change::Files => {
            writeln!(
                out,
                "\n  {}  {}",
                paint(palette.bold(Ink::Text), "Settings come from"),
                paint(dim, "nearest last")
            )?;
            for path in &settings.sources {
                let policy = settings.policy.as_deref() == Some(path.as_path());
                writeln!(
                    out,
                    "    {}{}",
                    path.display(),
                    paint(dim, if policy { "  policy, read-only" } else { "" })
                )?;
            }
            if settings.sources.is_empty() {
                writeln!(
                    out,
                    "    {}",
                    paint(dim, "nothing yet — `rqx init` writes one")
                )?;
            }
            return Ok(());
        }
        Change::Set {
            key, file, pick, ..
        }
        | Change::Unset { key, file, pick } => (key, file.clone(), *pick),
    };

    refuse_if_locked(settings, key)?;

    let target = match (file, pick) {
        (Some(named), _) => root.join(named),
        (None, true) => choose(settings, root, key)?,
        (None, false) => default_target(settings, key)
            .unwrap_or_else(|| root.join(rqx_core::config::CONFIG_FILE_NAME)),
    };
    if settings.policy.as_deref() == Some(target.as_path()) {
        bail!(
            "{} is the policy this project points at, and changing it changes every \
             project that does. Edit it where it lives.",
            target.display()
        );
    }

    match change {
        Change::Set { key, value, .. } => {
            let moved = set(&target, &key, &value)?;
            writeln!(
                out,
                "  {key} {}  {}",
                paint(palette.style(Ink::Good), moved),
                paint(dim, target.display().to_string())
            )?;
        }
        Change::Unset { key, .. } => {
            if unset(&target, &key)? {
                writeln!(
                    out,
                    "  {key} removed  {}",
                    paint(dim, target.display().to_string())
                )?;
            } else {
                writeln!(out, "  {key} was not set in {}", target.display())?;
            }
        }
        Change::Files => unreachable!("answered above"),
    }
    Ok(())
}

/// A policy exists so a project cannot quietly move a key; writing it into the
/// project's own file would produce a config that refuses to load.
fn refuse_if_locked(settings: &Resolved, key: &str) -> Result<()> {
    if settings.locked_keys().any(|locked| locked == key) {
        let policy = settings.policy.as_ref().map_or_else(
            || "the policy".to_owned(),
            |path| path.display().to_string(),
        );
        bail!("`{key}` is locked by {policy} and cannot be changed here.");
    }
    Ok(())
}

/// Reads the choice from the terminal rather than guessing, because which file a
/// setting belongs in is a question about how the project is organised.
fn choose(settings: &Resolved, root: &Path, key: &str) -> Result<PathBuf> {
    use std::io::Write;
    let targets = targets(settings, root, key);
    if targets.is_empty() {
        return Ok(root.join(rqx_core::config::CONFIG_FILE_NAME));
    }

    let mut out = anstream::stdout().lock();
    writeln!(out, "\n  Which file should hold `{key}`?\n")?;
    for (at, target) in targets.iter().enumerate() {
        writeln!(
            out,
            "    {}  {}{}{}",
            at + 1,
            target.label,
            target
                .holds
                .as_ref()
                .map_or_else(String::new, |held| format!("  (now {held})")),
            if target.writable {
                ""
            } else {
                "  policy, read-only"
            }
        )?;
    }
    write!(out, "\n  1-{}: ", targets.len())?;
    out.flush().ok();

    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("reading the choice")?;
    let chosen: usize = answer
        .trim()
        .parse()
        .ok()
        .filter(|at| (1..=targets.len()).contains(at))
        .ok_or_else(|| anyhow::anyhow!("that is not one of the files listed"))?;
    Ok(targets[chosen - 1].path.clone())
}

/// Removing a key when the config will not load. A project that wrote a value its
/// policy forbids cannot resolve, so the ordinary path is closed exactly when it is
/// most needed — but only a removal, and only one that names its own file, because
/// nothing else can be decided without the resolved chain.
pub fn force(
    root: &Path,
    change: Change,
    palette: crate::brand::Palette,
    out: &mut dyn std::io::Write,
) -> Result<()> {
    use crate::brand::{Ink, paint};
    let Change::Unset {
        key,
        file: Some(file),
        ..
    } = change
    else {
        bail!("the configuration does not load, so only `unset --file <path>` can run");
    };
    let target = root.join(file);
    if unset(&target, &key)? {
        writeln!(
            out,
            "  {key} removed  {}",
            paint(palette.style(Ink::Muted), target.display().to_string())
        )?;
    } else {
        writeln!(out, "  {key} was not set in {}", target.display())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rqx-edit-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        root
    }

    /// The template rqx writes is mostly comments explaining what each key is for.
    /// Reserialising through a plain TOML value would throw all of it away the first
    /// time anybody changed a threshold.
    #[test]
    fn writing_one_key_leaves_every_comment_and_neighbour_alone() {
        let root = workspace("comments");
        let file = root.join("RQX.toml");
        std::fs::write(
            &file,
            "# what this project holds itself to\n[thresholds]\n# how many is too many\nmax_params = 4\nnesting_depth = 3\n",
        )
        .expect("write");

        assert_eq!(
            set(&file, "thresholds.max_params", "2").expect("set"),
            "4 → 2"
        );
        assert_eq!(
            std::fs::read_to_string(&file).expect("read"),
            "# what this project holds itself to\n[thresholds]\n# how many is too many\nmax_params = 2\nnesting_depth = 3\n"
        );
    }

    /// A number typed at a prompt is a number, not the string "2" — which would parse
    /// as a config error rather than a threshold.
    #[test]
    fn a_value_is_written_as_the_type_it_reads_as() {
        let root = workspace("types");
        let file = root.join("RQX.toml");
        set(&file, "thresholds.max_params", "2").expect("number");
        set(&file, "gate.ratchet", "true").expect("boolean");
        set(&file, "gate.fail_on", "warning").expect("string");
        let written = std::fs::read_to_string(&file).expect("read");
        assert!(written.contains("max_params = 2"), "{written}");
        assert!(written.contains("ratchet = true"), "{written}");
        assert!(written.contains("fail_on = \"warning\""), "{written}");
        // It has to still parse as the config it claims to be.
        written.parse::<DocumentMut>().expect("valid toml");
    }

    #[test]
    fn a_key_written_into_a_file_with_no_such_table_creates_it() {
        let root = workspace("newtable");
        let file = root.join("RQX.toml");
        std::fs::write(&file, "[thresholds]\nmax_params = 4\n").expect("write");
        set(&file, "gate.fail_on", "warning").expect("set");
        let written = std::fs::read_to_string(&file).expect("read");
        assert!(written.contains("[gate]"), "{written}");
        assert!(written.contains("max_params = 4"), "the old table survived");
    }

    /// A table left empty reads as a project that meant something by it, so the
    /// removal takes the heading with the last key under it.
    #[test]
    fn removing_the_last_key_removes_the_table_it_was_under() {
        let root = workspace("unset");
        let file = root.join("RQX.toml");
        std::fs::write(
            &file,
            "[thresholds]\nmax_params = 4\n\n[gate]\nratchet = true\n",
        )
        .expect("write");

        assert!(unset(&file, "thresholds.max_params").expect("unset"));
        let written = std::fs::read_to_string(&file).expect("read");
        assert!(!written.contains("[thresholds]"), "{written}");
        assert!(
            written.contains("[gate]"),
            "the other table survived: {written}"
        );

        assert!(
            !unset(&file, "thresholds.max_params").expect("second unset"),
            "removing what is not there is not a change"
        );
    }

    #[test]
    fn a_key_that_names_no_table_is_refused_with_the_shape_it_wanted() {
        let root = workspace("bare");
        let error = set(&root.join("RQX.toml"), "max_params", "2")
            .expect_err("a bare key")
            .to_string();
        assert!(error.contains("thresholds.max_params"), "{error}");
    }
}

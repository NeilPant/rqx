//! `rqx config migrate`: the way off TOML.
//!
//! rqx no longer reads `RQX.toml`, and refuses to when it finds one, because a file
//! that is silently ignored can loosen a CI gate. This turns the project's TOML files
//! into YAML next to where they were, and keeps the originals as `.bak` so nothing is
//! lost if the result is not what a reader wanted.

mod yaml;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rqx_core::config::{CONFIG_FILE_NAME, RETIRED_CONFIG_FILE_NAMES, SCHEMA_DIRECTIVE};
use toml_edit::{DocumentMut, Value};

use crate::brand::{Ink, Palette, paint};

#[cfg(test)]
use crate::brand::ColorDepth;

/// One TOML file and what it becomes.
struct Conversion {
    from: PathBuf,
    to: PathBuf,
    backup: PathBuf,
    yaml: String,
}

pub fn run(
    path: &Path,
    dry_run: bool,
    palette: Palette,
    out: &mut dyn std::io::Write,
) -> Result<()> {
    run_with(
        path,
        &crate::config::user_config_directories(),
        dry_run,
        palette,
        out,
    )
}

/// The user's directories are a parameter so a test never converts the config of
/// whoever runs it.
fn run_with(
    path: &Path,
    user_directories: &[PathBuf],
    dry_run: bool,
    palette: Palette,
    out: &mut dyn std::io::Write,
) -> Result<()> {
    let conversions = plan(path, user_directories)?;
    let dim = palette.style(Ink::Muted);
    if conversions.is_empty() {
        writeln!(
            out,
            "\n  {}\n",
            paint(dim, "no TOML configuration found, nothing to migrate")
        )?;
        return Ok(());
    }
    writeln!(out)?;
    for conversion in &conversions {
        if !dry_run {
            apply(conversion)?;
        }
        writeln!(
            out,
            "  {} {}  {}  {}",
            if dry_run {
                "would convert"
            } else {
                "converted"
            },
            conversion.from.display(),
            paint(
                palette.style(Ink::Good),
                format!("\u{2192} {}", conversion.to.display())
            ),
            paint(
                dim,
                format!("original kept as {}", file_name(&conversion.backup))
            )
        )?;
    }
    let noun = if conversions.len() == 1 {
        "file"
    } else {
        "files"
    };
    writeln!(
        out,
        "\n  {} {noun}{}\n",
        conversions.len(),
        if dry_run {
            " would change; run again without --dry-run to write them"
        } else {
            " migrated. Review the YAML, then delete the .bak files once you are happy."
        }
    )?;
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn apply(conversion: &Conversion) -> Result<()> {
    crate::fix::write_atomically(&conversion.to, &conversion.yaml)?;
    std::fs::rename(&conversion.from, &conversion.backup).with_context(|| {
        format!(
            "keeping {} as {}",
            conversion.from.display(),
            conversion.backup.display()
        )
    })
}

/// Everything that would change, worked out before anything is written, so a refusal
/// halfway through cannot leave a project with some files converted and some not.
fn plan(path: &Path, user_directories: &[PathBuf]) -> Result<Vec<Conversion>> {
    let mut found = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        configs_under(path)
    };
    found.extend(user_configs(user_directories));
    let mut referenced = Vec::new();
    for config in &found {
        referenced.extend(policy_beside(config)?);
    }
    found.extend(referenced);
    found.sort();
    found.dedup();

    found
        .iter()
        .map(|from| conversion_of(from, &found))
        .collect()
}

fn configs_under(root: &Path) -> Vec<PathBuf> {
    ignore::WalkBuilder::new(root)
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter(|entry| {
            RETIRED_CONFIG_FILE_NAMES
                .iter()
                .any(|name| entry.file_name() == *name)
        })
        .map(ignore::DirEntry::into_path)
        .collect()
}

fn user_configs(directories: &[PathBuf]) -> Vec<PathBuf> {
    directories
        .iter()
        .flat_map(|directory| {
            RETIRED_CONFIG_FILE_NAMES
                .iter()
                .map(move |name| directory.join(name))
        })
        .filter(|path| path.is_file())
        .collect()
}

/// The TOML policy a config points at, resolved against the config's own directory.
fn policy_beside(config: &Path) -> Result<Option<PathBuf>> {
    let text = read(config)?;
    let document: DocumentMut = text
        .parse()
        .with_context(|| format!("{} is not valid TOML", config.display()))?;
    let Some(named) = document.get("policy").and_then(|item| item.as_str()) else {
        return Ok(None);
    };
    let resolved = config.parent().unwrap_or(Path::new(".")).join(named);
    let is_toml = resolved.extension().is_some_and(|ext| ext == "toml");
    Ok((is_toml && resolved.is_file()).then_some(resolved))
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

/// Where a TOML file's YAML goes. A project config is `rqx.yaml` in its directory; a
/// policy keeps its own name with the extension swapped, because it is not called
/// anything rqx looks for.
fn destination(from: &Path) -> PathBuf {
    let is_config = RETIRED_CONFIG_FILE_NAMES
        .iter()
        .any(|name| from.file_name().is_some_and(|found| found == *name));
    if is_config {
        from.with_file_name(CONFIG_FILE_NAME)
    } else {
        from.with_extension("yaml")
    }
}

fn conversion_of(from: &Path, everything: &[PathBuf]) -> Result<Conversion> {
    let to = destination(from);
    if to.exists() {
        bail!(
            "{} would become {}, which already exists. Move one of them aside and run \
             `rqx config migrate` again.",
            from.display(),
            to.display()
        );
    }
    let backup = PathBuf::from(format!("{}.bak", from.display()));
    if backup.exists() {
        bail!(
            "{} already exists, so {} cannot be kept beside it. Move it aside and run \
             `rqx config migrate` again.",
            backup.display(),
            from.display()
        );
    }
    let text = repointed(from, &read(from)?, everything)?;
    let body = yaml::convert(&text).with_context(|| format!("converting {}", from.display()))?;
    Ok(Conversion {
        from: from.to_path_buf(),
        to,
        backup,
        yaml: format!("{SCHEMA_DIRECTIVE}\n{body}"),
    })
}

/// A config that names a policy has to name the converted file, or the migrated
/// project would point at a TOML file rqx no longer reads.
fn repointed(from: &Path, text: &str, everything: &[PathBuf]) -> Result<String> {
    let mut document: DocumentMut = text
        .parse()
        .with_context(|| format!("{} is not valid TOML", from.display()))?;
    let Some(item) = document.get_mut("policy") else {
        return Ok(text.to_owned());
    };
    let Some(named) = item.as_str().map(str::to_owned) else {
        return Ok(text.to_owned());
    };
    let target = from.parent().unwrap_or(Path::new(".")).join(&named);
    if !everything.contains(&target) {
        return Ok(text.to_owned());
    }
    let renamed = named.rsplit_once('/').map_or_else(
        || file_name(&destination(&target)),
        |(directory, _)| format!("{directory}/{}", file_name(&destination(&target))),
    );
    let decor = item.as_value().map(|value| value.decor().clone());
    let mut value = Value::from(renamed);
    if let Some(decor) = decor {
        *value.decor_mut() = decor;
    }
    *item = toml_edit::Item::Value(value);
    Ok(document.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str) -> crate::scratch::Scratch {
        crate::scratch::Scratch::new(&format!("migrate-{name}"))
    }

    fn migrate(root: &Path, dry_run: bool) -> Result<String> {
        let mut out = Vec::new();
        run(
            root,
            dry_run,
            Palette::with_depth(ColorDepth::Monochrome),
            &mut out,
        )?;
        Ok(String::from_utf8(out).expect("utf-8"))
    }

    #[test]
    fn a_project_config_becomes_rqx_yaml_and_the_original_is_kept() {
        let root = workspace("project");
        std::fs::write(
            root.join("RQX.toml"),
            "# mine\n[gate]\nfail_on = \"warning\"\n",
        )
        .expect("write");

        let said = migrate(&root, false).expect("migrates");
        assert!(said.contains("converted"), "{said}");
        let written = std::fs::read_to_string(root.join("rqx.yaml")).expect("rqx.yaml");
        assert!(written.starts_with(SCHEMA_DIRECTIVE), "{written}");
        assert!(
            written.contains("# mine\ngate:\n  fail_on: warning\n"),
            "{written}"
        );
        assert!(root.join("RQX.toml.bak").exists());
        assert!(!root.join("RQX.toml").exists());
    }

    #[test]
    fn a_dry_run_writes_nothing() {
        let root = workspace("dry");
        std::fs::write(root.join("RQX.toml"), "[gate]\n").expect("write");
        let said = migrate(&root, true).expect("plans");
        assert!(said.contains("would convert"), "{said}");
        assert!(root.join("RQX.toml").exists());
        assert!(!root.join("rqx.yaml").exists());
    }

    #[test]
    fn a_policy_is_converted_and_the_config_points_at_the_new_file() {
        let root = workspace("policy");
        std::fs::create_dir_all(root.join("platform")).expect("mkdir");
        std::fs::write(
            root.join("platform/policy.toml"),
            "[thresholds]\nmax_params = 3\n\n[locked]\nkeys = [\"thresholds.max_params\"]\n",
        )
        .expect("policy");
        std::fs::create_dir_all(root.join("service")).expect("mkdir");
        std::fs::write(
            root.join("service/RQX.toml"),
            "policy = \"../platform/policy.toml\"\n",
        )
        .expect("config");

        migrate(&root, false).expect("migrates");
        let config = std::fs::read_to_string(root.join("service/rqx.yaml")).expect("config");
        assert!(
            config.contains("policy: ../platform/policy.yaml"),
            "{config}"
        );
        assert!(root.join("platform/policy.yaml").exists());
        assert!(root.join("platform/policy.toml.bak").exists());
    }

    #[test]
    fn an_existing_rqx_yaml_is_never_overwritten() {
        let root = workspace("exists");
        std::fs::write(root.join("RQX.toml"), "[gate]\n").expect("write");
        std::fs::write(root.join("rqx.yaml"), "gate: {}\n").expect("write");
        let error = migrate(&root, false).expect_err("refused").to_string();
        assert!(error.contains("already exists"), "{error}");
        assert!(root.join("RQX.toml").exists(), "nothing was touched");
    }

    #[test]
    fn a_directory_with_no_toml_says_so() {
        let said = migrate(&workspace("none"), false).expect("runs");
        assert!(said.contains("nothing to migrate"), "{said}");
    }
}

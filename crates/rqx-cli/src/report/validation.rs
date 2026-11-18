//! `rqx config validate`: the problems a config has, as a reader's list or as NDJSON.

use std::io::Write;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::brand::{Ink, Palette, paint};
use crate::config::{Problem, Severity, Validation};

/// Relative to where the command ran when it can be, which is the form a CI log turns
/// into a link and a person can read at a glance.
///
/// The working directory is tried as the system names it and in its canonical form,
/// because Windows can give the same directory two spellings (`RUNNER~1` and
/// `runneradmin`), and the config file's path is the canonical one. The parts are joined
/// with `/` on every platform, like every other path rqx prints.
pub fn shown_path(path: &Path) -> String {
    let relative = std::env::current_dir().ok().and_then(|here| {
        let canonical = rqx_core::paths::canonical_or_same(&here);
        [here, canonical]
            .iter()
            .find_map(|base| path.strip_prefix(base).ok().map(Path::to_path_buf))
    });
    rqx_core::paths::display_path(&relative.unwrap_or_else(|| path.to_path_buf()))
}

fn place(problem: &Problem) -> Option<String> {
    let file = shown_path(problem.file.as_deref()?);
    Some(match problem.position {
        Some(at) => format!("{file}:{}:{}", at.line, at.column),
        None => file,
    })
}

fn count(number: usize, noun: &str) -> String {
    format!("{number} {noun}{}", if number == 1 { "" } else { "s" })
}

pub fn render(
    validation: &Validation,
    strict: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    for problem in &validation.problems {
        let ink = match problem.severity {
            Severity::Error => Ink::Danger,
            Severity::Warning => Ink::Warn,
        };
        let label = paint(palette.bold(ink), problem.severity.slug());
        match place(problem) {
            Some(place) => writeln!(out, "{place}: {label}: {}", problem.message)?,
            None => writeln!(out, "{label}: {}", problem.message)?,
        }
    }

    let errors = validation.count(Severity::Error);
    let warnings = validation.count(Severity::Warning);
    let (ink, mark, verdict) = if errors > 0 {
        (Ink::Danger, '\u{2717}', "config is invalid".to_owned())
    } else if strict && warnings > 0 {
        (
            Ink::Danger,
            '\u{2717}',
            format!(
                "config has {}, and --strict fails on them",
                count(warnings, "warning")
            ),
        )
    } else {
        let files = count(validation.files.len(), "file");
        let detail = if warnings > 0 {
            format!("{files}, {}", count(warnings, "warning"))
        } else {
            files
        };
        (Ink::Good, '\u{2713}', format!("config is valid ({detail})"))
    };
    writeln!(
        out,
        "{}",
        paint(palette.style(ink), format!("{mark} {verdict}"))
    )?;
    Ok(())
}

#[derive(Serialize)]
struct JsonProblem<'a> {
    kind: &'static str,
    severity: &'static str,
    file: Option<String>,
    line: Option<u64>,
    column: Option<u64>,
    message: &'a str,
}

#[derive(Serialize)]
struct JsonSummary {
    kind: &'static str,
    schema_version: u32,
    valid: bool,
    errors: usize,
    warnings: usize,
    files: Vec<String>,
}

/// One line per problem, then a summary whose `valid` says whether the command exits
/// zero, so a consumer need not repeat the `--strict` rule.
pub fn json(validation: &Validation, valid: bool, out: &mut dyn Write) -> Result<()> {
    for problem in &validation.problems {
        let line = JsonProblem {
            kind: "problem",
            severity: problem.severity.slug(),
            file: problem.file.as_deref().map(shown_path),
            line: problem.position.map(|at| at.line),
            column: problem.position.map(|at| at.column),
            message: &problem.message,
        };
        writeln!(out, "{}", serde_json::to_string(&line)?)?;
    }
    let summary = JsonSummary {
        kind: "summary",
        schema_version: super::SCHEMA_VERSION,
        valid,
        errors: validation.count(Severity::Error),
        warnings: validation.count(Severity::Warning),
        files: validation
            .files
            .iter()
            .map(|file| shown_path(file))
            .collect(),
    };
    writeln!(out, "{}", serde_json::to_string(&summary)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_under_the_working_directory_is_shown_relative_with_forward_slashes() {
        let here = std::env::current_dir().unwrap();
        assert_eq!(
            shown_path(&here.join("sub").join("rqx.yaml")),
            "sub/rqx.yaml"
        );
        let canonical = rqx_core::paths::canonical_or_same(&here);
        assert_eq!(shown_path(&canonical.join("rqx.yaml")), "rqx.yaml");
    }

    #[test]
    fn a_file_elsewhere_keeps_its_whole_path() {
        let elsewhere = std::env::temp_dir().join("elsewhere-rqx.yaml");
        assert!(shown_path(&elsewhere).ends_with("elsewhere-rqx.yaml"));
    }
}

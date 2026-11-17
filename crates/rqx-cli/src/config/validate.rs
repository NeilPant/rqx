//! `rqx config validate`: whether the settings a run would read are sound, and where
//! they are not.
//!
//! The config is resolved by the same code a scan resolves it with, so a file that
//! validates is a file that loads. What differs is what happens to a mistake that does
//! not stop it loading: a run refuses it, and here it is reported with the rest.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rqx_core::config::{Level, Thresholds};

use super::files::{Located, Position};
use super::{Lapse, Origin, Resolved, locate, resolve_noting_lapses};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The config does not load, so no run would start.
    Error,
    /// The config loads, and something in it does not do what it appears to.
    Warning,
}

impl Severity {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

pub struct Problem {
    pub severity: Severity,
    pub file: Option<PathBuf>,
    pub position: Option<Position>,
    pub message: String,
}

impl Problem {
    fn warning(file: Option<PathBuf>, position: Option<Position>, message: String) -> Self {
        Self {
            severity: Severity::Warning,
            file,
            position,
            message,
        }
    }

    /// A refusal, placed in its file when the refusal knew where it came from.
    fn refusal(error: &anyhow::Error) -> Self {
        let located = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<Located>());
        Self {
            severity: Severity::Error,
            file: located.map(|located| located.file.clone()),
            position: located.and_then(|located| located.position),
            message: located
                .map_or_else(|| format!("{error:#}"), |located| located.message.clone()),
        }
    }
}

pub struct Validation {
    pub problems: Vec<Problem>,
    /// The files that were read. Empty when reading them failed, because which of
    /// them were reached is not something a refusal says.
    pub files: Vec<PathBuf>,
}

impl Validation {
    pub fn count(&self, severity: Severity) -> usize {
        self.problems
            .iter()
            .filter(|problem| problem.severity == severity)
            .count()
    }

    /// Whether a pipeline should go on: no errors, and under `--strict` no warnings
    /// either.
    pub fn passes(&self, strict: bool) -> bool {
        self.count(Severity::Error) == 0 && !(strict && self.count(Severity::Warning) > 0)
    }
}

pub fn validate(root: &Path, explicit: Option<&Path>, level: Option<Level>) -> Validation {
    match resolve_noting_lapses(root, explicit, level) {
        Err(error) => Validation {
            problems: vec![Problem::refusal(&error)],
            files: Vec::new(),
        },
        Ok((resolved, lapses)) => {
            let mut problems: Vec<Problem> = lapses.iter().flat_map(problems_of).collect();
            problems.extend(unread_thresholds(&resolved));
            // An override starts from the whole config, so a name wrong at the top is
            // wrong again inside every entry, at the same line.
            let mut seen = HashSet::new();
            problems.retain(|problem| {
                seen.insert((
                    problem.file.clone(),
                    problem.position,
                    problem.message.clone(),
                ))
            });
            Validation {
                problems,
                files: resolved.sources,
            }
        }
    }
}

fn read(file: &Path) -> String {
    std::fs::read_to_string(file).unwrap_or_default()
}

fn problems_of(lapse: &Lapse) -> Vec<Problem> {
    match lapse {
        Lapse::UnknownNames { files, names, .. } => names
            .iter()
            .map(|unknown| {
                let found = files.iter().rev().find_map(|file| {
                    locate::name(&read(file), &unknown.name).map(|at| (file.clone(), at))
                });
                let (file, position) = match found {
                    Some((file, at)) => (Some(file), Some(at)),
                    None => (files.last().cloned(), None),
                };
                Problem::warning(file, position, unknown.to_string())
            })
            .collect(),
        Lapse::EmptyOverride { file } => vec![Problem::warning(
            Some(file.clone()),
            locate::key(&read(file), "overrides"),
            "an entry of `overrides` names no `paths`, so nothing would use it".to_owned(),
        )],
    }
}

/// A threshold a project wrote that moves nothing, because every rule that reads it is
/// off. Left alone it reads as a decision the tool is honouring.
fn unread_thresholds(resolved: &Resolved) -> Vec<Problem> {
    resolved
        .key_sources
        .iter()
        .filter_map(|(key, origin)| {
            let name = key.strip_prefix("thresholds.")?;
            let (Origin::File(file) | Origin::Extended(file)) = origin else {
                return None;
            };
            let (_, tuned) = Thresholds::TUNES.iter().find(|(known, _)| *known == name)?;
            if tuned.iter().any(|rule| resolved.enabled_anywhere(*rule)) {
                return None;
            }
            let rules: Vec<String> = tuned
                .iter()
                .map(|rule| format!("`{}`", rule.spec().slug))
                .collect();
            Some(Problem::warning(
                Some(file.clone()),
                locate::key(&read(file), name),
                format!(
                    "`{key}` is set, but {} is off, so no enabled rule reads it",
                    rules.join(" and ")
                ),
            ))
        })
        .collect()
}

//! What a change paid off: the findings the base revision had that the change no
//! longer has, and how the health score moved.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rqx_core::rule::RuleId;
use serde::Serialize;

use crate::config::Resolved as Settings;
use crate::scan::{self, ScanOutcome};
use crate::verify::{self, Outcome};

#[derive(Clone, Debug, Serialize)]
pub struct Resolved {
    pub rule: String,
    pub subject: String,
    pub file: String,
    pub line: u32,
}

/// What the credit was counted over, which decides what a consumer can conclude from
/// its absence: a finding that is not listed was not resolved, or was outside the scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// Every finding of a scan of the base revision, including those that read the
    /// whole project.
    Project,
    /// The findings that one file's analysis produces, in the files the change touched.
    ChangedFiles,
}

#[derive(Clone, Copy, Debug)]
pub struct Health {
    pub before: u32,
    pub after: u32,
}

#[derive(Clone, Debug)]
pub struct Credit {
    pub resolved: Vec<Resolved>,
    pub scope: Scope,
    /// Only when the whole base was scanned: a score needs every finding of both sides.
    pub health: Option<Health>,
}

impl Credit {
    /// `None` when either scan dropped findings to stay within its display cap: a
    /// finding the cap dropped from the working tree's list would read as fixed.
    pub fn between(before: &ScanOutcome, after: &ScanOutcome) -> Option<Self> {
        if before.omitted > 0 || after.omitted > 0 {
            return None;
        }
        Some(Self {
            resolved: resolved_between(before, after, |_| true),
            scope: Scope::Project,
            health: Some(Health {
                before: crate::report::Report::everything(before).health(),
                after: crate::report::Report::everything(after).health(),
            }),
        })
    }

    /// The credit for a change whose base was not scanned: what the files it touched
    /// held at the base, analysed on their own, against what the working tree's scan
    /// found in them. A finding that needs more than its own file, such as an unused
    /// export or a duplicate, cannot be read from one file and is left out.
    pub fn in_files(
        files: &BTreeMap<String, String>,
        root: &Path,
        settings: &Settings,
        after: &ScanOutcome,
    ) -> Option<Self> {
        if after.omitted > 0 {
            return None;
        }
        let mirror = Mirror::of(files)?;
        let mut rebased = settings.rebased(root, mirror.path()).ok()?;
        rebased.config.scan.sarif_in.clear();
        rebased.config.scan.max_findings = 0;
        let before = scan::scan_with(mirror.path(), &rebased, None, scan::CachePolicy::Off).ok()?;
        Some(Self {
            resolved: resolved_between(&before, after, |rule| !rule.is_project_scoped()),
            scope: Scope::ChangedFiles,
            health: None,
        })
    }

    pub fn health_delta(&self) -> Option<i32> {
        self.health
            .map(|health| health.after as i32 - health.before as i32)
    }
}

fn resolved_between(
    before: &ScanOutcome,
    after: &ScanOutcome,
    counts: impl Fn(RuleId) -> bool,
) -> Vec<Resolved> {
    verify::changes_between(before, after)
        .into_iter()
        .filter(|change| matches!(change.outcome, Outcome::Resolved) && counts(change.rule))
        .filter_map(|change| {
            let was = change.before?;
            Some(Resolved {
                rule: change.slug,
                subject: change.subject,
                file: was.file.to_string(),
                line: was.line,
            })
        })
        .collect()
}

/// Files written under a temporary root, so a scan can read them as a project.
struct Mirror {
    path: PathBuf,
}

impl Mirror {
    fn of(files: &BTreeMap<String, String>) -> Option<Self> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "rqx-base-files-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mirror = Self { path };
        for (file, text) in files {
            let target = mirror.path.join(file);
            std::fs::create_dir_all(target.parent()?).ok()?;
            std::fs::write(target, text).ok()?;
        }
        std::fs::create_dir_all(&mirror.path).ok()?;
        Some(mirror)
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Mirror {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_delta_is_signed_and_absent_without_a_full_base_scan() {
        let mut credit = Credit {
            resolved: Vec::new(),
            scope: Scope::Project,
            health: Some(Health {
                before: 80,
                after: 74,
            }),
        };
        assert_eq!(credit.health_delta(), Some(-6));
        credit.health = None;
        assert_eq!(credit.health_delta(), None);
    }

    #[test]
    fn the_scopes_are_named_as_the_documentation_names_them() {
        assert_eq!(
            serde_json::to_string(&Scope::Project).unwrap(),
            "\"project\""
        );
        assert_eq!(
            serde_json::to_string(&Scope::ChangedFiles).unwrap(),
            "\"changed-files\""
        );
    }
}

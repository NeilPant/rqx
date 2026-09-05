//! The debt a project already has, recorded once so a gate can ask about the debt a
//! change adds.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rqx_core::finding::Finding;
use rqx_core::identity::Identity;
use serde::{Deserialize, Serialize};

use crate::scan::ScanOutcome;

pub const DEFAULT_PATH: &str = ".rqx/baseline.json";

/// Bumped when an entry stops meaning what it meant. A file from an older version is
/// refused rather than half-read, because a baseline that silently matches nothing
/// turns a gate off without saying so.
const VERSION: u32 = 1;

#[derive(Debug, Deserialize, Serialize)]
pub struct Baseline {
    version: u32,
    /// The commit it was taken at and the version that took it. Neither is read back;
    /// they are here for whoever opens the file asking where it came from.
    created: u64,
    rqx: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    /// How many findings there were when it was taken. Two findings can resolve to
    /// one identity, so the entry count is not the finding count, and the ratchet
    /// has to compare like with like.
    findings: u32,
    entries: Vec<Entry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Entry {
    identity: String,
    rule: String,
    file: String,
}

impl Baseline {
    pub fn of(outcome: &ScanOutcome) -> Self {
        let mut entries: Vec<Entry> = identities(outcome)
            .map(|(identity, finding)| Entry {
                identity: identity.to_string(),
                rule: finding.rule.slug().to_owned(),
                file: finding.location.file.to_string(),
            })
            .collect();
        entries.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.rule.cmp(&b.rule))
                .then(a.identity.cmp(&b.identity))
        });
        entries.dedup_by(|a, b| a.identity == b.identity);
        Self {
            version: VERSION,
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            rqx: env!("CARGO_PKG_VERSION").to_owned(),
            commit: crate::history::head(&outcome.root),
            findings: outcome.counts.total,
            entries,
        }
    }

    pub fn read(path: &Path) -> Result<Self> {
        let body = std::fs::read_to_string(path)
            .with_context(|| format!("reading the baseline at {}", path.display()))?;
        let baseline: Self = serde_json::from_str(&body)
            .with_context(|| format!("reading the baseline at {}", path.display()))?;
        anyhow::ensure!(
            baseline.version == VERSION,
            "{} was written by a different version of rqx (format {} against {VERSION}). \
             Run `rqx baseline --update` to take it again.",
            path.display(),
            baseline.version
        );
        Ok(baseline)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)
                .with_context(|| format!("creating {}", directory.display()))?;
        }
        crate::fix::write_atomically(path, &format!("{}\n", serde_json::to_string_pretty(self)?))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// What the ratchet compares against.
    pub const fn findings(&self) -> u32 {
        self.findings
    }

    pub fn recorded(&self) -> HashSet<Identity> {
        self.entries
            .iter()
            .filter_map(|entry| u64::from_str_radix(&entry.identity, 16).ok())
            .map(Identity::from_raw)
            .collect()
    }
}

/// Where the baseline lives: what the gate names, or the conventional path.
pub fn path_of(root: &Path, configured: Option<&str>) -> PathBuf {
    root.join(configured.unwrap_or(DEFAULT_PATH))
}

/// Which of a scan's findings the baseline already knew about. Reads each file once,
/// because resolving an identity needs the text a finding sits on.
pub fn identities(outcome: &ScanOutcome) -> impl Iterator<Item = (Identity, &Finding)> {
    let mut sources: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    outcome.findings.iter().map(move |finding| {
        let file = finding.location.file.to_string();
        let source = sources.entry(file).or_insert_with(|| {
            std::fs::read_to_string(outcome.root.join(&*finding.location.file)).unwrap_or_default()
        });
        (Identity::resolved(finding, source), finding)
    })
}

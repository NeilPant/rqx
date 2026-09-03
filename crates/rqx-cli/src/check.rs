use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Result, bail};
use rqx_core::config::{GateSeverity, RqxConfig};
use rqx_core::finding::Severity;

use crate::history;
use crate::scan::ScanOutcome;

pub struct Review {
    pub outcome: ScanOutcome,
    pub reviewed: BTreeSet<String>,
    pub base: String,
}

impl Review {
    pub fn is_reviewed(&self, file: &str) -> bool {
        self.reviewed.contains(file)
    }

    pub fn findings(&self) -> impl Iterator<Item = &rqx_core::finding::Finding> {
        self.outcome.findings.iter().filter(|finding| self.is_reviewed(&finding.location.file))
    }
}

/// The whole project is scanned even though only part of it is reported on: an
/// export looks dead until the file that imports it is parsed, so narrowing the scan
/// to the diff would invent findings that the full graph disproves.
pub fn review(root: &Path, config: &RqxConfig, base: &str) -> Result<Review> {
    let Some(mut changed) = history::changed_since(root, base) else {
        bail!(
            "could not diff against `{base}` — is this a git repository, and does that \
             revision exist?"
        );
    };
    changed.extend(history::uncommitted(root));

    let prefix = history::repository_prefix(root).unwrap_or_default();
    let changed: BTreeSet<String> = changed
        .into_iter()
        .filter_map(|path| path.strip_prefix(&prefix).map(str::to_owned))
        .collect();

    let outcome = crate::scan::scan(root, config)?;

    // Direct dependents are reviewed too, because deleting an export is only visible
    // as a problem in the file that was importing it.
    let mut reviewed = changed.clone();
    for (importer, imported) in &outcome.import_edges {
        if imported.iter().any(|target| changed.contains(target)) {
            reviewed.insert(importer.clone());
        }
    }

    // A lockfile or a stray asset is a changed file, but it is not a file this tool
    // has an opinion about; counting it would overstate what was reviewed.
    reviewed.retain(|file| outcome.analyzed.contains(file));

    Ok(Review { outcome, reviewed, base: base.to_owned() })
}

pub struct Verdict {
    pub failures: Vec<String>,
    pub counted: u32,
}

impl Verdict {
    pub const fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

pub fn judge(review: &Review, config: &RqxConfig) -> Verdict {
    let gate = &config.gate;
    let threshold = match gate.fail_on {
        GateSeverity::Never => {
            return Verdict { failures: Vec::new(), counted: 0 };
        }
        GateSeverity::Error => Severity::Error,
        GateSeverity::Warning => Severity::Warning,
        GateSeverity::Info => Severity::Info,
    };

    let counted =
        review.findings().filter(|finding| finding.severity >= threshold).count() as u32;

    let mut failures = Vec::new();
    if counted > gate.max_findings {
        failures.push(format!(
            "{counted} findings at or above `{}` in the reviewed files, limit is {}",
            match gate.fail_on {
                GateSeverity::Error => "error",
                GateSeverity::Warning => "warning",
                GateSeverity::Info => "info",
                GateSeverity::Never => "never",
            },
            gate.max_findings
        ));
    }

    if gate.min_health > 0 {
        let score = review
            .outcome
            .counts
            .weighted_health_score(review.outcome.code_lines, &review.outcome.impact);
        if score < gate.min_health {
            failures.push(format!(
                "health score {score} is below the required {}",
                gate.min_health
            ));
        }
    }

    Verdict { failures, counted }
}

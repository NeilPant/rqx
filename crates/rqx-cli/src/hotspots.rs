//! Files ranked by how often they change and how much is wrong with them.
//!
//! A problem in a file nobody edits costs little; the same problem in a file that
//! changes every week is paid for each time. The score is the commits in the churn
//! window multiplied by the file's weight: the severity weight of its findings that
//! count toward the health score, plus one point per ten points of cognitive
//! complexity across its functions.

use std::io::Write;

use anyhow::Result;

use crate::brand::{Ink, Palette, paint};
use crate::scan::ScanOutcome;

const COMPLEXITY_PER_POINT: f64 = 10.0;

pub struct Hotspot {
    pub file: String,
    pub score: f64,
    pub commits: u32,
    pub findings: u32,
    pub finding_weight: f64,
    pub complexity: u32,
    /// The rule behind most of the file's findings.
    pub main_rule: Option<String>,
}

impl Hotspot {
    pub fn explanation(&self, days: u32) -> String {
        let mut parts = vec![
            format!("{} in {days} days", count(self.commits, "commit")),
            format!(
                "{} (weight {:.0})",
                count(self.findings, "finding"),
                self.finding_weight
            ),
        ];
        if self.complexity > 0 {
            parts.push(format!("cognitive complexity {}", self.complexity));
        }
        if let Some(rule) = &self.main_rule {
            parts.push(format!("mostly {rule}"));
        }
        parts.join(", ")
    }
}

fn count(number: u32, noun: &str) -> String {
    format!("{number} {noun}{}", if number == 1 { "" } else { "s" })
}

/// Files with commits in the window and something wrong in them, highest score first.
/// The findings must be uncapped for the main rule to be exact.
pub fn rank(outcome: &ScanOutcome) -> Vec<Hotspot> {
    let mut ranked: Vec<Hotspot> = outcome
        .analyzed
        .iter()
        .filter_map(|file| hotspot_of(outcome, file))
        .collect();
    ranked.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.file.cmp(&b.file))
    });
    ranked
}

fn hotspot_of(outcome: &ScanOutcome, file: &str) -> Option<Hotspot> {
    let commits = outcome.impact.get(file).commits;
    let complexity: u32 = outcome.shape_by_file.get(file).map_or(0, |shape| {
        shape.functions.iter().map(|f| f.cognitive_complexity).sum()
    });
    let finding_weight = outcome.counts.weight_of(file);
    let weight = finding_weight + f64::from(complexity) / COMPLEXITY_PER_POINT;
    if commits == 0 || weight <= 0.0 {
        return None;
    }
    Some(Hotspot {
        file: file.to_owned(),
        score: f64::from(commits) * weight,
        commits,
        findings: outcome.findings_by_file.get(file).copied().unwrap_or(0),
        finding_weight,
        complexity,
        main_rule: main_rule(outcome, file),
    })
}

fn main_rule(outcome: &ScanOutcome, file: &str) -> Option<String> {
    let mut by_rule: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
    for finding in outcome
        .findings
        .iter()
        .filter(|finding| &*finding.location.file == file)
    {
        *by_rule.entry(finding.slug()).or_default() += 1;
    }
    by_rule
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(rule, _)| rule.to_owned())
}

pub fn render(
    outcome: &ScanOutcome,
    top: usize,
    days: u32,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    let ranked = rank(outcome);
    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.bold(Ink::Text), "Hotspots"),
        paint(
            dim,
            format!("commits in the last {days} days multiplied by the weight of what is wrong")
        )
    )?;
    if days == 0 || !outcome.versioned {
        writeln!(
            out,
            "\n  {}\n",
            paint(
                dim,
                "no git history was read, so there is no churn to rank by"
            )
        )?;
        return Ok(());
    }
    if ranked.is_empty() {
        writeln!(
            out,
            "\n  {}\n",
            paint(dim, "no file with findings changed in this window")
        )?;
        return Ok(());
    }
    for (position, hotspot) in ranked.iter().take(top).enumerate() {
        writeln!(
            out,
            "\n  {:>2}. {}  {}",
            position + 1,
            hotspot.file,
            paint(palette.style(Ink::Warn), format!("{:.0}", hotspot.score))
        )?;
        writeln!(out, "      {}", paint(dim, hotspot.explanation(days)))?;
    }
    writeln!(out)?;
    Ok(())
}

pub fn json(outcome: &ScanOutcome, top: usize, days: u32, out: &mut dyn Write) -> Result<()> {
    for hotspot in rank(outcome).into_iter().take(top) {
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "file": hotspot.file,
                "score": hotspot.score,
                "commits": hotspot.commits,
                "findings": hotspot.findings,
                "finding_weight": hotspot.finding_weight,
                "cognitive_complexity": hotspot.complexity,
                "main_rule": hotspot.main_rule,
                "explanation": hotspot.explanation(days),
            })
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_explanation_states_each_factor_of_the_score() {
        let hotspot = Hotspot {
            file: "src/a.ts".to_owned(),
            score: 84.0,
            commits: 12,
            findings: 3,
            finding_weight: 7.0,
            complexity: 40,
            main_rule: Some("deep-nesting".to_owned()),
        };
        assert_eq!(
            hotspot.explanation(90),
            "12 commits in 90 days, 3 findings (weight 7), cognitive complexity 40, mostly deep-nesting"
        );
    }

    #[test]
    fn a_single_commit_is_singular() {
        assert_eq!(count(1, "commit"), "1 commit");
    }
}

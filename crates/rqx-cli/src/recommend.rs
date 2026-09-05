//! What to do about this project, as distinct from what is wrong with it.
//!
//! `plan` ranks findings. This answers the questions a lead asks before there is a
//! plan: is the configuration describing this codebase or fighting it, which
//! refactoring would answer the most at once, and which files should a new person
//! read first.

use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;
use clap::ValueEnum;

use crate::brand::{Ink, Palette, paint};
use crate::config::Resolved;
use crate::scan::ScanOutcome;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Only {
    Config,
    Refactor,
    Reading,
}

/// A section of the report: which flag selects it, what it is called, and how it is
/// gathered.
type Section = (Only, &'static str, fn(&ScanOutcome, &Resolved) -> Vec<Note>);

/// One suggestion, with the reason attached. A recommendation a reader cannot check
/// is a recommendation they have to trust, and this tool does not ask for trust.
struct Note {
    headline: String,
    because: String,
}

pub fn render(
    outcome: &ScanOutcome,
    settings: &Resolved,
    only: Option<Only>,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let sections: [Section; 3] = [
        (Only::Config, "Configuration", configuration),
        (Only::Refactor, "Refactoring", refactoring),
        (Only::Reading, "Read these first", reading),
    ];

    for (which, title, gather) in sections {
        if only.is_some_and(|wanted| wanted != which) {
            continue;
        }
        let notes = gather(outcome, settings);
        writeln!(out, "\n  {}", paint(palette.style(Ink::Text).bold(), title))?;
        if notes.is_empty() {
            writeln!(
                out,
                "    {}",
                paint(palette.style(Ink::Muted), "nothing worth changing")
            )?;
            continue;
        }
        for note in notes {
            writeln!(out, "\n    {}", note.headline)?;
            writeln!(
                out,
                "      {}",
                paint(palette.style(Ink::Muted), note.because)
            )?;
        }
    }
    writeln!(out)?;
    Ok(())
}

/// Thresholds are judgement calls, and the only evidence for one is what the codebase
/// actually looks like. A threshold the codebase sits above is not a standard, it is
/// a rule that fires on normal code here.
fn configuration(outcome: &ScanOutcome, settings: &Resolved) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut scores: Vec<u32> = outcome
        .shape_by_file
        .values()
        .flat_map(|shape| shape.functions.iter().map(|f| f.cognitive_complexity))
        .collect();

    if !scores.is_empty() {
        scores.sort_unstable();
        let at = |percent: usize| scores[((scores.len() - 1) * percent) / 100];
        let (p90, p99) = (at(90), at(99));
        let threshold = settings.config.thresholds.cognitive_complexity;
        let over = scores.iter().filter(|score| **score >= threshold).count();
        let share = (over * 100) / scores.len().max(1);

        if p90 >= threshold {
            notes.push(Note {
                headline: format!(
                    "raise `thresholds.cognitive_complexity` from {threshold} to about {}",
                    p90.max(threshold + 1)
                ),
                because: format!(
                    "{share}% of functions are at or above it, so it is describing normal \
                     code here rather than the exceptions \u{2014} p90 is {p90}, p99 is {p99}"
                ),
            });
        } else if over == 0 {
            // Counted, not shared: a hundred and thirty-seven functions out of thirty
            // thousand rounds to nought per cent and is not nothing.
            notes.push(Note {
                headline: format!(
                    "lower `thresholds.cognitive_complexity` from {threshold} to about {}",
                    p99.max(4)
                ),
                because: format!(
                    "no function reaches it, so the rule cannot fire \u{2014} the worst here \
                     scores {}",
                    scores.last().copied().unwrap_or(0)
                ),
            });
        }
    }

    // A rule that fires everywhere is either a real house problem or the wrong rule
    // for this house, and either way it is a decision somebody should make on purpose.
    let lines = (outcome.code_lines / 1000).max(1);
    for (rule, count) in &outcome.counts.by_rule {
        let per = u64::from(*count) / lines;
        if per >= 20 && rule.spec().confidence == rqx_core::finding::Confidence::Heuristic {
            notes.push(Note {
                headline: format!("decide about `{}`", rule.slug()),
                because: format!(
                    "{count} findings, {per} per thousand lines \u{2014} at that rate it is \
                     either the project's biggest problem or the wrong rule for it"
                ),
            });
        }
    }
    notes
}

/// One technique answering forty findings is a week's work with a shape. Forty
/// findings answered by forty different techniques is just a list.
fn refactoring(outcome: &ScanOutcome, _settings: &Resolved) -> Vec<Note> {
    let mut by_technique: BTreeMap<&'static str, (u32, BTreeMap<String, u32>)> = BTreeMap::new();
    for finding in &outcome.findings {
        let Some(technique) = finding.rule.spec().fixes.first() else {
            continue;
        };
        let entry = by_technique.entry(technique.name()).or_default();
        entry.0 += 1;
        let directory = finding
            .location
            .file
            .rsplit_once('/')
            .map_or(".", |(at, _)| at)
            .to_owned();
        *entry.1.entry(directory).or_default() += 1;
    }

    let mut ranked: Vec<(&str, u32, String, u32)> = by_technique
        .into_iter()
        .map(|(technique, (total, places))| {
            let (where_most, most) = places
                .into_iter()
                .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
                .unwrap_or_default();
            (technique, total, where_most, most)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    ranked
        .into_iter()
        .take(3)
        .filter(|(_, total, _, _)| *total > 1)
        .map(|(technique, total, where_most, most)| Note {
            headline: format!("{technique} answers {total} findings"),
            because: format!("{most} of them are under `{where_most}`"),
        })
        .collect()
}

/// Reach times churn times trouble. A file nobody imports and nobody changes is not
/// where a new person should start however bad it is.
fn reading(outcome: &ScanOutcome, _settings: &Resolved) -> Vec<Note> {
    let report = crate::report::Report::everything(outcome);
    let mut ranked: Vec<(f64, &str, u32, u32, u32)> = outcome
        .lines_by_file
        .keys()
        .map(|file| {
            let impact = outcome.impact.get(file);
            let findings = outcome.findings_by_file.get(file).copied().unwrap_or(0);
            let weight = f64::from(impact.importers + 1)
                * f64::from(impact.commits + 1)
                * (1.0 + f64::from(findings));
            (
                weight,
                file.as_ref(),
                impact.importers,
                impact.commits,
                findings,
            )
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(b.1)));

    ranked
        .into_iter()
        .take(5)
        .filter(|(weight, ..)| *weight > 1.0)
        .map(|(_, file, importers, commits, findings)| Note {
            headline: file.to_owned(),
            because: format!(
                "{} importers \u{b7} {} commits \u{b7} {} \u{b7} grade {}",
                importers,
                commits,
                crate::report::plural(findings, "finding", "findings"),
                report.grade_of(file).label()
            ),
        })
        .collect()
}

pub fn json(
    outcome: &ScanOutcome,
    settings: &Resolved,
    only: Option<Only>,
    out: &mut dyn Write,
) -> Result<()> {
    let sections: [Section; 3] = [
        (Only::Config, "config", configuration),
        (Only::Refactor, "refactor", refactoring),
        (Only::Reading, "reading", reading),
    ];
    for (which, name, gather) in sections {
        if only.is_some_and(|wanted| wanted != which) {
            continue;
        }
        for note in gather(outcome, settings) {
            writeln!(
                out,
                "{}",
                serde_json::json!({
                    "section": name,
                    "recommendation": note.headline,
                    "because": note.because,
                })
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scored(complexities: &[u32]) -> ScanOutcome {
        let mut outcome = crate::report::tests::outcome_with(&[], 0);
        let shape = outcome
            .shape_by_file
            .entry(std::sync::Arc::from("a.ts"))
            .or_default();
        for (at, complexity) in complexities.iter().enumerate() {
            shape.functions.push(rqx_lang::FunctionShape {
                name: Some(format!("f{at}")),
                range: rqx_core::finding::SourceRange {
                    file: std::sync::Arc::from("a.ts"),
                    start: 0,
                    end: 1,
                    line: 1,
                    column: 1,
                    end_line: 1,
                },
                end: 1,
                last_line: 1,
                loop_depth: 0,
                self_calls: 0,
                parameters: 0,
                statements: 1,
                cognitive_complexity: *complexity,
                max_nesting: 0,
            });
        }
        outcome
    }

    fn with_threshold(complexity: u32) -> Resolved {
        let mut config = rqx_core::config::RqxConfig::default();
        config.thresholds.cognitive_complexity = complexity;
        Resolved::of(config)
    }

    /// A threshold most of the codebase sits above is not a standard; it is a rule
    /// that fires on normal code here, and every finding it produces is noise.
    #[test]
    fn a_threshold_the_codebase_lives_above_is_told_to_move() {
        let outcome = scored(&[20, 22, 25, 30, 40]);
        let notes = configuration(&outcome, &with_threshold(15));
        assert!(
            notes.iter().any(|note| note.headline.contains("raise")),
            "{:?}",
            notes.iter().map(|note| &note.headline).collect::<Vec<_>>()
        );
    }

    /// And one nothing reaches cannot fire at all, which is the same failure the
    /// other way round.
    #[test]
    fn a_threshold_nothing_reaches_is_told_to_move_too() {
        let outcome = scored(&[1, 2, 3, 2, 1]);
        let notes = configuration(&outcome, &with_threshold(40));
        assert!(notes.iter().any(|note| note.headline.contains("lower")));
    }

    /// The case in between is the one that must stay quiet, and it is the common one:
    /// a long tail under the threshold with a handful of real offenders over it.
    #[test]
    fn a_threshold_doing_its_job_is_left_alone() {
        let mut complexities = vec![1_u32; 200];
        complexities.extend([20, 25, 40]);
        let notes = configuration(&scored(&complexities), &with_threshold(15));
        assert!(
            notes.is_empty(),
            "{:?}",
            notes.iter().map(|note| &note.headline).collect::<Vec<_>>()
        );
    }

    /// Percentages hide small absolute numbers, and a hundred functions over the line
    /// in thirty thousand rounds to nought per cent.
    #[test]
    fn a_handful_over_the_line_is_not_nothing() {
        let mut complexities = vec![1_u32; 10_000];
        complexities.extend(std::iter::repeat_n(30, 20));
        let notes = configuration(&scored(&complexities), &with_threshold(15));
        assert!(
            notes.is_empty(),
            "twenty in ten thousand is the rule working: {:?}",
            notes.iter().map(|note| &note.headline).collect::<Vec<_>>()
        );
    }
}

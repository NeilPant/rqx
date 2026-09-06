//! Where the health score comes from.
//!
//! The score is one number, and one number cannot be argued with. This is the same
//! arithmetic broken into buckets, each with what the score would be without it — so
//! "fix the duplication" and "fix the dead code" can be compared before either is
//! attempted rather than after.

use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;
use clap::ValueEnum;
use rqx_core::finding::Finding;

use crate::brand::{Ink, Palette, paint, score_ink};
use crate::report::{Align, column};
use crate::scan::ScanOutcome;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum By {
    Family,
    Rule,
    Dir,
    File,
}

/// Shared with the interface's Health screen: the ledger is the same ledger.
pub struct Bucket {
    pub name: String,
    pub findings: u32,
    /// The score the project would have if every finding in this bucket were gone.
    /// Not a share of the total: the curve is exponential, so shares do not add up
    /// and the only honest answer is to recompute.
    pub without: u32,
    /// This bucket's weight in the score's own arithmetic, as a share of all of it.
    /// The score saturates — a project deep enough in the red stays at zero whichever
    /// bucket you remove — and then this is the only column that still discriminates.
    pub share: u32,
    pub worst: Vec<String>,
}

pub fn render(outcome: &ScanOutcome, by: By, palette: Palette, out: &mut dyn Write) -> Result<()> {
    let now = crate::report::Report::everything(outcome).health();
    let buckets = buckets(outcome, by, now);
    let dim = palette.style(Ink::Muted);

    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.bold(Ink::Text), "Health"),
        paint(palette.style(score_ink(now)), format!("{now}/100"))
    )?;

    if buckets.is_empty() {
        writeln!(out, "\n  {}\n", paint(dim, "nothing is costing anything"))?;
        return Ok(());
    }

    let width = buckets
        .iter()
        .map(|bucket| bucket.name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 40);

    // The curve bottoms out: past a certain density every bucket leaves the score at
    // zero, and a column reading `0 (+0)` on every row is worse than no column.
    let moves = buckets.iter().any(|bucket| bucket.without > now);
    let without = |text: &str, paint: &dyn Fn(String) -> String| {
        if moves {
            format!(" {}", paint(column(text, 9, Align::Right)))
        } else {
            String::new()
        }
    };

    writeln!(
        out,
        "\n    {:<width$} {} {}{}  {}",
        "",
        paint(dim, column("found", 7, Align::Right)),
        paint(dim, column("debt", 7, Align::Right)),
        without("without", &|text| paint(dim, text)),
        paint(dim, "worst of it")
    )?;
    for bucket in &buckets {
        let gain = bucket.without.saturating_sub(now);
        writeln!(
            out,
            "    {:<width$} {:>7} {:>6}%{}  {}",
            bucket.name,
            bucket.findings,
            bucket.share,
            without(&format!("{} (+{gain})", bucket.without), &|text| {
                paint(
                    palette.style(if gain > 0 { Ink::Good } else { Ink::Muted }),
                    text,
                )
            }),
            paint(dim, bucket.worst.join(", "))
        )?;
    }

    writeln!(
        out,
        "\n    {}\n",
        paint(
            dim,
            if moves {
                "`debt` is the bucket's share of the weighted total; `without` is the score \
                 with it gone, recomputed rather than apportioned, so those do not add up"
                    .to_owned()
            } else {
                format!(
                    "the score stays at {now} whichever single bucket goes, so `debt` is what \
                     separates them: the weight each carries in the same arithmetic"
                )
            }
        )
    )?;
    Ok(())
}

fn label(finding: &Finding, by: By, root: &std::path::Path) -> String {
    let _ = root;
    match by {
        By::Family => finding.category().slug().to_owned(),
        By::Rule => finding.slug().to_owned(),
        By::File => finding.location.file.to_string(),
        By::Dir => finding
            .location
            .file
            .rsplit_once('/')
            .map_or(".", |(at, _)| at)
            .to_owned(),
    }
}

pub fn buckets(outcome: &ScanOutcome, by: By, now: u32) -> Vec<Bucket> {
    let total = outcome.counts.weighted_total(&outcome.impact);
    let mut grouped: BTreeMap<String, Vec<&Finding>> = BTreeMap::new();
    for finding in &outcome.findings {
        grouped
            .entry(label(finding, by, &outcome.root))
            .or_default()
            .push(finding);
    }

    let mut buckets: Vec<Bucket> = grouped
        .into_iter()
        .map(|(name, findings)| {
            let counts = outcome.counts.without(findings.iter().copied());
            let share = if total > 0.0 {
                (((total - counts.weighted_total(&outcome.impact)) / total) * 100.0).round() as u32
            } else {
                0
            };
            let mut worst: Vec<&&Finding> = findings.iter().collect();
            worst.sort_by(|a, b| {
                b.rank()
                    .cmp(&a.rank())
                    .then(a.location.file.cmp(&b.location.file))
            });
            Bucket {
                share,
                findings: findings.len() as u32,
                without: counts.weighted_health_score(outcome.code_lines, &outcome.impact),
                worst: worst
                    .iter()
                    .take(2)
                    .map(|finding| match by {
                        By::Rule => format!(
                            "{}:{}",
                            short(&finding.location.file),
                            finding.location.line
                        ),
                        _ => finding.slug().to_owned(),
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                name,
            }
        })
        .collect();

    // Biggest gain first: the question this answers is what to do next.
    buckets.sort_by(|a, b| {
        b.share
            .cmp(&a.share)
            .then(
                b.without
                    .saturating_sub(now)
                    .cmp(&a.without.saturating_sub(now)),
            )
            .then(b.findings.cmp(&a.findings))
            .then(a.name.cmp(&b.name))
    });
    buckets
}

fn short(file: &str) -> String {
    file.rsplit('/').next().unwrap_or(file).to_owned()
}

pub fn json(outcome: &ScanOutcome, by: By, out: &mut dyn Write) -> Result<()> {
    let now = crate::report::Report::everything(outcome).health();
    for bucket in buckets(outcome, by, now) {
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "bucket": bucket.name,
                "findings": bucket.findings,
                "debtShare": bucket.share,
                "health": now,
                "healthWithout": bucket.without,
                "gain": bucket.without.saturating_sub(now),
                "worst": bucket.worst,
            })
        )?;
    }
    Ok(())
}

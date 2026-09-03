use std::collections::BTreeSet;
use std::io::Write;

use anyhow::Result;
use rqx_core::rule::{Category, RuleId, RuleSpec};
use rqx_core::{Chapter, Coverage, FixTechnique, Group, Heuristic, Origin, Smell, Tally};

use crate::brand::{Ink, Palette, paint};

/// The rule catalogue prints itself the same way whether or not anything was
/// scanned, so it takes a palette rather than a report.
pub fn explain(rule: RuleId, palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let RuleSpec {
        slug,
        guru_smell,
        metric,
        fixes,
        confidence,
        severity,
        why,
        category,
        ..
    } = rule.spec();
    let dim = palette.style(Ink::Muted);
    writeln!(out, "\n  {}\n", paint(palette.style(Ink::Accent).bold(), slug))?;
    writeln!(
        out,
        "  {} {}",
        paint(dim, format!("{:18}", "Smell")),
        guru_smell.map_or("no direct catalog entry", rqx_core::Smell::name)
    )?;
    writeln!(out, "  {} {}", paint(dim, format!("{:18}", "Family")), category.title())?;
    writeln!(out, "  {} {}", paint(dim, format!("{:18}", "Clean Code")), rule.spec().citation())?;
    writeln!(out, "  {} {metric}", paint(dim, format!("{:18}", "Metric")))?;
    writeln!(
        out,
        "  {} {}",
        paint(dim, format!("{:18}", "Fix")),
        fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>().join(" / ")
    )?;
    writeln!(
        out,
        "  {} {} / {}",
        paint(dim, format!("{:18}", "Reported as")),
        severity.label(),
        confidence.label()
    )?;
    writeln!(out, "\n  {why}\n")?;
    Ok(())
}

pub fn rules(only: Option<&str>, palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    if let Some(name) = only
        && Category::parse(name).is_none()
    {
        anyhow::bail!(
            "unknown family `{name}`. Known families: {}",
            Category::ALL.map(Category::slug).join(", ")
        );
    }

    let dim = palette.style(Ink::Muted);
    for category in Category::ALL {
        if only.is_some_and(|name| name != category.slug()) {
            continue;
        }
        writeln!(
            out,
            "\n  {}  {}",
            paint(palette.style(Ink::Text).bold(), category.title()),
            paint(dim, category.slug())
        )?;
        for rule in category.rules() {
            let spec = rule.spec();
            let state = if spec.off_by_default { "  (off by default)" } else { "" };
            writeln!(
                out,
                "    {} {}{}",
                paint(palette.style(Ink::Accent), format!("{:26}", spec.slug)),
                paint(dim, spec.citation()),
                paint(dim, state)
            )?;
        }
    }
    writeln!(out, "\n  {} rules. `rqx explain <rule>` for any of them.\n", RuleId::ALL.len())?;
    Ok(())
}

const fn coverage_ink(coverage: Coverage) -> Ink {
    match coverage {
        Coverage::Implemented(_) => Ink::Good,
        Coverage::Absent(_) => Ink::Warn,
        Coverage::NeedsTypes(_) | Coverage::NotStatic(_) | Coverage::NotApplicable(_) => Ink::Muted,
    }
}

fn write_entry(
    out: &mut impl Write,
    palette: Palette,
    name: &str,
    coverage: Coverage,
) -> Result<()> {
    let detail = match coverage.reason() {
        Some(reason) => format!("{} \u{2014} {reason}", coverage.label()),
        None => coverage
            .rules()
            .iter()
            .map(|rule| rule.slug())
            .collect::<Vec<_>>()
            .join(", "),
    };
    writeln!(
        out,
        "    {} {}  {}",
        paint(palette.style(coverage_ink(coverage)), if coverage.is_implemented() { "\u{2713}" } else { "\u{25cb}" }),
        paint(palette.style(Ink::Text), format!("{name:44}")),
        paint(palette.style(Ink::Muted), detail)
    )?;
    Ok(())
}

fn write_tally(out: &mut impl Write, palette: Palette, label: &str, tally: Tally) -> Result<()> {
    let mut parts = vec![format!("{} implemented", tally.implemented)];
    for (count, name) in [
        (tally.absent, "not yet"),
        (tally.needs_types, "need types"),
        (tally.not_static, "not decidable"),
        (tally.not_applicable, "not applicable"),
    ] {
        if count > 0 {
            parts.push(format!("{count} {name}"));
        }
    }
    writeln!(
        out,
        "  {}  {}",
        paint(palette.style(Ink::Text).bold(), format!("{label:34}")),
        paint(palette.style(Ink::Muted), format!("{} of {}  \u{b7} {}", tally.implemented, tally.total(), parts[1..].join(" \u{b7} ")))
    )?;
    Ok(())
}

/// Traceability the tool can prove rather than assert: every entry in both
/// catalogues, with what Rqx does about it and why when it does nothing.
pub fn coverage(palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();

    writeln!(out, "\n  {}\n", paint(palette.style(Ink::Accent).bold(), "Coverage"))?;
    write_tally(&mut out, palette, "refactoring.guru smells", Smell::tally())?;
    write_tally(&mut out, palette, "Clean Code ch.17 heuristics", Heuristic::tally())?;
    writeln!(
        out,
        "  {}  {}",
        paint(palette.style(Ink::Text).bold(), format!("{:34}", "refactoring.guru techniques")),
        paint(
            palette.style(Ink::Muted),
            format!(
                "{} catalogued \u{b7} {} prescribed by a rule",
                FixTechnique::ALL.iter().filter(|t| t.origin() == Origin::Guru).count(),
                prescribed().len()
            )
        )
    )?;

    for family in Category::ALL {
        let smells: Vec<Smell> = Smell::in_family(family).collect();
        if smells.is_empty() {
            continue;
        }
        writeln!(out, "\n  {}", paint(palette.style(Ink::Text).bold(), family.title()))?;
        for smell in smells {
            write_entry(&mut out, palette, smell.name(), smell.coverage())?;
        }
    }

    for chapter in Chapter::ALL {
        writeln!(
            out,
            "\n  {}",
            paint(palette.style(Ink::Text).bold(), format!("Clean Code \u{2014} {}", chapter.title()))
        )?;
        for entry in chapter.heuristics() {
            write_entry(
                &mut out,
                palette,
                &format!("{} {}", entry.id(), entry.title()),
                entry.coverage(),
            )?;
        }
    }

    writeln!(out)?;
    Ok(())
}

fn prescribed() -> BTreeSet<FixTechnique> {
    RuleId::ALL.iter().flat_map(|rule| rule.spec().fixes.iter().copied()).collect()
}

/// The refactorings themselves, including the ones no rule prescribes yet — an
/// incomplete list is what let the wrong fix stay attached to a rule.
pub fn techniques(palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let prescribed = prescribed();
    let dim = palette.style(Ink::Muted);

    for group in Group::ALL {
        writeln!(out, "\n  {}", paint(palette.style(Ink::Text).bold(), group.title()))?;
        for technique in group.techniques() {
            let note = if prescribed.contains(&technique) {
                RuleId::ALL
                    .iter()
                    .filter(|rule| rule.spec().fixes.contains(&technique))
                    .map(|rule| rule.slug())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else if technique.origin() == Origin::Guru {
                String::new()
            } else {
                technique.origin().label().to_owned()
            };
            writeln!(
                out,
                "    {} {}",
                paint(
                    palette.style(if prescribed.contains(&technique) { Ink::Accent } else { Ink::Muted }),
                    format!("{:48}", technique.name())
                ),
                paint(dim, note)
            )?;
        }
    }
    writeln!(
        out,
        "\n  {} techniques, {} prescribed by a rule.\n",
        FixTechnique::ALL.len(),
        prescribed.len()
    )?;
    Ok(())
}

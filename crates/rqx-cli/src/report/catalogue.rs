use std::collections::BTreeSet;
use std::io::Write;

use anyhow::Result;
use rqx_core::rule::{Category, RuleId, RuleSpec};
use rqx_core::{Coverage, FixTechnique, Group, Tally};

use crate::brand::{Ink, Palette, coverage_ink, paint};

/// The rule catalogue prints itself the same way whether or not anything was
/// scanned, so it takes a palette rather than a report.
pub fn explain(rule: RuleId, palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let RuleSpec {
        slug,
        metric,
        fixes,
        confidence,
        severity,
        why,
        category,
        ..
    } = rule.spec();
    let dim = palette.style(Ink::Muted);
    writeln!(
        out,
        "\n  {}\n",
        paint(palette.style(Ink::Accent).bold(), slug)
    )?;
    writeln!(
        out,
        "  {} {}",
        paint(dim, format!("{:18}", "Family")),
        category.title()
    )?;
    writeln!(out, "  {} {metric}", paint(dim, format!("{:18}", "Metric")))?;
    writeln!(
        out,
        "  {} {}",
        paint(dim, format!("{:18}", "Fix")),
        fixes
            .iter()
            .map(|fix| fix.name())
            .collect::<Vec<_>>()
            .join(" / ")
    )?;
    writeln!(
        out,
        "  {} {}",
        paint(dim, format!("{:18}", "Auto-fixable")),
        if rule.spec().autofix.is_automatic() {
            format!("yes \u{2014} {}", rule.spec().autofix.slug())
        } else {
            "no \u{2014} the fix is a decision".to_owned()
        }
    )?;
    writeln!(
        out,
        "  {} {} / {}",
        paint(dim, format!("{:18}", "Reported as")),
        severity.label(),
        confidence.label()
    )?;
    let exempt: Vec<String> = rqx_core::Language::ALL
        .into_iter()
        .filter_map(|language| {
            rule.exemption(language)
                .map(|reason| format!("{}: {reason}", language.title()))
        })
        .collect();
    if !exempt.is_empty() {
        writeln!(out, "\n  {}", paint(dim, "Not applicable to"))?;
        for line in exempt {
            writeln!(out, "    {}", paint(dim, line))?;
        }
    }
    writeln!(out, "\n  {why}\n")?;
    Ok(())
}

pub fn rules(only: Option<&str>, language: Option<&str>, palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    if let Some(name) = only
        && Category::parse(name).is_none()
    {
        return Err(super::unknown(
            "family",
            name,
            &Category::ALL.map(Category::slug),
            &format!(
                "The families are: {}.",
                Category::ALL.map(Category::slug).join(", ")
            ),
        ));
    }
    let chosen = match language {
        Some(name) => Some(rqx_core::Language::parse(name).ok_or_else(|| {
            super::unknown(
                "language",
                name,
                &rqx_core::Language::ALL.map(rqx_core::Language::slug),
                &format!(
                    "rqx reads: {}.",
                    rqx_core::Language::ALL
                        .map(rqx_core::Language::slug)
                        .join(", ")
                ),
            )
        })?),
        None => None,
    };

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
            if chosen.is_some_and(|language| !rule.applies_to(language)) {
                continue;
            }
            let spec = rule.spec();
            let state = if spec.off_by_default {
                "  (off by default)"
            } else {
                ""
            };
            writeln!(
                out,
                "    {} {}{}",
                paint(palette.style(Ink::Accent), format!("{:26}", spec.slug)),
                paint(dim, spec.metric),
                paint(dim, state)
            )?;
        }
    }
    let total = match chosen {
        Some(language) => RuleId::for_language(language).count(),
        None => RuleId::ALL.len(),
    };
    let scope = chosen.map_or(String::new(), |language| {
        format!(" for {}", language.title())
    });
    writeln!(
        out,
        "\n  {total} rules{scope}. `rqx explain <rule>` for any of them.\n"
    )?;
    Ok(())
}

pub fn write_entry<Answer: Copy + std::fmt::Display + 'static>(
    out: &mut impl Write,
    palette: Palette,
    name: &str,
    coverage: Coverage<Answer>,
) -> Result<()> {
    let detail = coverage.detail();
    writeln!(
        out,
        "    {} {}  {}",
        paint(
            palette.style(coverage_ink(coverage)),
            if coverage.is_implemented() {
                "\u{2713}"
            } else {
                "\u{25cb}"
            }
        ),
        paint(palette.style(Ink::Text), format!("{name:44}")),
        paint(palette.style(Ink::Muted), detail)
    )?;
    Ok(())
}

pub fn write_tally(
    out: &mut impl Write,
    palette: Palette,
    label: &str,
    tally: Tally,
) -> Result<()> {
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
    let rest = parts[1..].join(" \u{b7} ");
    let counted = if rest.is_empty() {
        format!("{} of {}", tally.implemented, tally.total())
    } else {
        format!("{} of {}  \u{b7} {rest}", tally.implemented, tally.total())
    };
    writeln!(
        out,
        "  {}  {}",
        paint(palette.style(Ink::Text).bold(), format!("{label:34}")),
        paint(palette.style(Ink::Muted), counted)
    )?;
    Ok(())
}

fn prescribed() -> BTreeSet<FixTechnique> {
    RuleId::ALL
        .iter()
        .flat_map(|rule| rule.spec().fixes.iter().copied())
        .collect()
}

/// Each level printed as the TOML it is, because a preset that changed twelve
/// thresholds and nine rules invisibly would be the trap the config file avoids.
pub fn levels(palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let dim = palette.style(Ink::Muted);

    for level in rqx_core::config::Level::ALL {
        let config: rqx_core::RqxConfig = toml::from_str(level.layer()).unwrap_or_default();
        let on = RuleId::ALL
            .iter()
            .copied()
            .filter(|rule| config.rules.enabled(*rule))
            .count();
        writeln!(
            out,
            "\n  {}  {}",
            paint(palette.style(Ink::Accent).bold(), level.slug()),
            paint(dim, level.summary())
        )?;
        writeln!(
            out,
            "  {}",
            paint(
                dim,
                format!(
                    "{on} of {} rules \u{b7} gate fails on {}",
                    RuleId::ALL.len(),
                    config.gate.fail_on.slug()
                )
            )
        )?;
        if level.layer().is_empty() {
            writeln!(out, "\n    {}", paint(dim, "the catalogue's own defaults"))?;
            continue;
        }
        writeln!(out)?;
        for line in level.layer().lines() {
            let ink = if line.trim_start().starts_with('#') {
                dim
            } else {
                palette.style(Ink::Text)
            };
            writeln!(out, "    {}", paint(ink, line))?;
        }
    }
    writeln!(
        out,
        "\n  {}\n",
        paint(
            dim,
            "A level is merged under your own settings, so anything you write wins."
        )
    )?;
    Ok(())
}

pub fn techniques(palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let prescribed = prescribed();
    let dim = palette.style(Ink::Muted);

    for group in Group::ALL {
        writeln!(
            out,
            "\n  {}  {}",
            paint(palette.style(Ink::Text).bold(), group.title()),
            paint(dim, group.summary())
        )?;
        for technique in group.techniques() {
            let note = RuleId::ALL
                .iter()
                .filter(|rule| rule.spec().fixes.contains(&technique))
                .map(|rule| rule.slug())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                out,
                "    {} {}",
                paint(
                    palette.style(if prescribed.contains(&technique) {
                        Ink::Accent
                    } else {
                        Ink::Muted
                    }),
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

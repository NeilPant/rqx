use std::io::Write;

use anyhow::Result;
use rqx_core::rule::{Category, RuleId, RuleSpec};

use crate::brand::{Ink, Palette, paint};

/// The rule catalogue prints itself the same way whether or not anything was
/// scanned, so it takes a palette rather than a report.
pub fn explain(rule: RuleId, palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let RuleSpec {
        slug,
        guru_smell,
        clean_code,
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
        guru_smell.unwrap_or("no direct catalog entry")
    )?;
    writeln!(out, "  {} {}", paint(dim, format!("{:18}", "Family")), category.title())?;
    writeln!(out, "  {} {clean_code}", paint(dim, format!("{:18}", "Clean Code")))?;
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
                paint(dim, spec.clean_code),
                paint(dim, state)
            )?;
        }
    }
    writeln!(out, "\n  {} rules. `rqx explain <rule>` for any of them.\n", RuleId::ALL.len())?;
    Ok(())
}

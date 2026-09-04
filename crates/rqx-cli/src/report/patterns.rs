use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;
use rqx_core::{Advice, AdviceId, Pattern, PatternFamily, Stance};
use serde::Serialize;

use crate::brand::{Ink, Palette, paint};
use crate::scan::ScanOutcome;

use super::catalogue::{write_entry, write_tally};

const NAME_WIDTH: usize = 26;

const fn stance_ink(stance: Stance) -> Ink {
    match stance {
        Stance::Recognized => Ink::Good,
        Stance::Incomplete => Ink::Warn,
        Stance::Applicable => Ink::Accent,
    }
}

/// Recognised patterns collapse to a count because an inventory is a number, not a
/// list to read; the two stances that ask something of the reader get a line each.
pub fn render(outcome: &ScanOutcome, palette: Palette, out: &mut dyn Write) -> Result<()> {
    if outcome.advice.is_empty() {
        writeln!(
            out,
            "\n  {}\n",
            paint(
                palette.style(Ink::Muted),
                "No pattern advice. `rqx patterns --coverage` says what rqx looks for."
            )
        )?;
        return Ok(());
    }

    let mut by_pattern: BTreeMap<Pattern, Vec<&Advice>> = BTreeMap::new();
    for advice in &outcome.advice {
        by_pattern
            .entry(advice.advisory.pattern())
            .or_default()
            .push(advice);
    }

    for family in PatternFamily::ALL {
        let patterns: Vec<Pattern> = family
            .patterns()
            .filter(|pattern| by_pattern.contains_key(pattern))
            .collect();
        if patterns.is_empty() {
            continue;
        }
        writeln!(
            out,
            "\n  {}\n",
            paint(palette.style(Ink::Accent).bold(), family.title())
        )?;
        for pattern in patterns {
            let found = &by_pattern[&pattern];
            write_pattern(out, palette, pattern, found)?;
        }
    }

    let counted = |stance: Stance| {
        outcome
            .advice
            .iter()
            .filter(|advice| advice.advisory.stance() == stance)
            .count()
    };
    writeln!(
        out,
        "\n  {}  {}\n",
        paint(palette.style(Ink::Text).bold(), format!("{:NAME_WIDTH$}", "Patterns")),
        paint(
            palette.style(Ink::Muted),
            format!(
                "{} in use \u{b7} {} half applied \u{b7} {} would fit",
                counted(Stance::Recognized),
                counted(Stance::Incomplete),
                counted(Stance::Applicable),
            )
        )
    )?;
    Ok(())
}

fn write_pattern(
    out: &mut dyn Write,
    palette: Palette,
    pattern: Pattern,
    found: &[&Advice],
) -> Result<()> {
    writeln!(
        out,
        "  {}  {}",
        paint(
            palette.style(Ink::Text).bold(),
            format!("{:NAME_WIDTH$}", pattern.name())
        ),
        paint(palette.style(Ink::Muted), pattern.intent())
    )?;

    let in_use = found
        .iter()
        .filter(|advice| advice.advisory.stance() == Stance::Recognized)
        .count();
    if in_use > 0 {
        writeln!(
            out,
            "    {}  {}",
            paint(palette.style(stance_ink(Stance::Recognized)), "\u{2713}"),
            paint(
                palette.style(Ink::Muted),
                super::plural(in_use as u32, "place", "places")
            )
        )?;
    }

    for advice in found {
        let stance = advice.advisory.stance();
        if stance == Stance::Recognized {
            continue;
        }
        writeln!(
            out,
            "    {}  {}  {}",
            paint(palette.style(stance_ink(stance)), "\u{25cf}"),
            paint(palette.style(Ink::Text), advice.location.to_string()),
            paint(palette.style(Ink::Muted), stance.label())
        )?;
        writeln!(out, "        {}", advice.signal)?;
        writeln!(
            out,
            "        {}",
            paint(palette.style(Ink::Muted), &advice.note)
        )?;
    }
    writeln!(out)?;
    Ok(())
}

#[derive(Serialize)]
struct JsonAdvice<'a> {
    #[serde(flatten)]
    advice: &'a Advice,
    pattern: &'static str,
    family: &'static str,
    stance: &'static str,
    signature: &'static str,
    confidence: &'static str,
    techniques: Vec<&'static str>,
    why: &'static str,
}

/// One line per advisory, the same NDJSON shape the findings reporter uses, so the
/// two can be read by the same consumer without a second parser.
pub fn json(outcome: &ScanOutcome, out: &mut dyn Write) -> Result<()> {
    for advice in &outcome.advice {
        let spec = advice.advisory.spec();
        let line = JsonAdvice {
            advice,
            pattern: spec.pattern.slug(),
            family: spec.pattern.family().slug(),
            stance: spec.stance.slug(),
            signature: spec.signature,
            confidence: spec.confidence.label(),
            techniques: spec.techniques.iter().map(|fix| fix.name()).collect(),
            why: spec.why,
        };
        writeln!(out, "{}", serde_json::to_string(&line)?)?;
    }
    Ok(())
}

pub fn explain(pattern: Pattern, palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();
    let dim = palette.style(Ink::Muted);
    writeln!(
        out,
        "\n  {}\n",
        paint(palette.style(Ink::Accent).bold(), pattern.slug())
    )?;
    for (label, value) in [
        ("Pattern", pattern.name().to_owned()),
        ("Family", pattern.family().title().to_owned()),
        (
            "Cited as",
            format!("Design Patterns \u{a7}{}", pattern.section()),
        ),
        ("Intent", pattern.intent().to_owned()),
        ("Coverage", pattern.coverage().detail()),
    ] {
        writeln!(out, "  {} {value}", paint(dim, format!("{label:18}")))?;
    }

    let advisories: Vec<AdviceId> = pattern.advisories().collect();
    if advisories.is_empty() {
        writeln!(out)?;
        return Ok(());
    }
    for advisory in advisories {
        let spec = advisory.spec();
        writeln!(
            out,
            "\n  {}  {}",
            paint(palette.style(stance_ink(spec.stance)), spec.slug),
            paint(dim, format!("{} \u{b7} {}", spec.stance.label(), spec.confidence.label()))
        )?;
        writeln!(out, "  {} {}", paint(dim, format!("{:18}", "Signature")), spec.signature)?;
        if !spec.techniques.is_empty() {
            writeln!(
                out,
                "  {} {}",
                paint(dim, format!("{:18}", "Fix")),
                spec.techniques
                    .iter()
                    .map(|fix| fix.name())
                    .collect::<Vec<_>>()
                    .join(" / ")
            )?;
        }
        let exempt: Vec<String> = rqx_core::Language::ALL
            .into_iter()
            .filter_map(|language| {
                advisory
                    .exemption(language)
                    .map(|reason| format!("{}: {reason}", language.title()))
            })
            .collect();
        for line in exempt {
            writeln!(
                out,
                "  {} {}",
                paint(dim, format!("{:18}", "Not in")),
                paint(dim, line)
            )?;
        }
        writeln!(out, "\n  {}\n", spec.why)?;
    }
    Ok(())
}

/// Every pattern in the book, and what Rqx does about it. The point is the entries it
/// declines: a catalogue that listed only what it detects would make the gaps
/// invisible, which is the claim this table exists to prevent.
pub fn coverage(palette: Palette) -> Result<()> {
    let mut out = anstream::stdout().lock();

    writeln!(
        out,
        "\n  {}\n",
        paint(palette.style(Ink::Accent).bold(), "Design patterns")
    )?;
    write_tally(&mut out, palette, "Gang of Four patterns", Pattern::tally())?;

    for family in PatternFamily::ALL {
        writeln!(
            out,
            "\n  {}",
            paint(palette.style(Ink::Text).bold(), family.title())
        )?;
        for pattern in family.patterns() {
            write_entry(
                &mut out,
                palette,
                &format!("{}  {}", pattern.section(), pattern.name()),
                pattern.coverage(),
            )?;
        }
    }

    let advisories = AdviceId::ALL.len();
    writeln!(
        out,
        "\n  {advisories} advisories over {} patterns. `rqx explain <pattern>` for any of them.\n",
        Pattern::tally().implemented
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rqx_core::Tally;

    use super::*;

    /// Every advisory the tally counts as implemented has to be reachable by name, or
    /// the coverage table is promising a surface that does not answer.
    #[test]
    fn an_implemented_pattern_names_advisories_that_parse_back() {
        for pattern in Pattern::ALL.iter().copied() {
            for advisory in pattern.coverage().rules() {
                assert_eq!(AdviceId::parse(advisory.slug()), Some(*advisory));
            }
        }
    }

    #[test]
    fn the_tally_counts_every_pattern_once() {
        let tally: Tally = Pattern::tally();
        assert_eq!(tally.total(), Pattern::ALL.len() as u32);
    }
}

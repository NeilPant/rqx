//! A trend file as text: each series with its direction and a sparkline, and the rules
//! or owners that moved most.

use std::io::Write;

use anyhow::Result;

use crate::brand::{Ink, Palette, paint};
use crate::report::escape::markup;
use crate::report::{Align, column, plural};
use crate::trend::{By, Direction, Series, Snapshot, movers, overall};

/// How many movers to list: enough to see the pattern, few enough to read at a glance.
const MOVERS: usize = 10;

const fn ink(direction: Direction) -> Ink {
    match direction {
        Direction::Improving => Ink::Good,
        Direction::Worsening => Ink::Danger,
        Direction::Steady => Ink::Muted,
    }
}

pub fn render(snapshots: &[Snapshot], by: By, palette: Palette, out: &mut dyn Write) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.bold(Ink::Text), "Trend"),
        paint(dim, span(snapshots))
    )?;
    if snapshots.len() < 2 {
        writeln!(
            out,
            "\n    {}\n",
            paint(
                dim,
                "a direction needs two snapshots; `rqx scan --record` adds one"
            )
        )?;
        return Ok(());
    }

    writeln!(out)?;
    write_rows(out, palette, &overall(snapshots))?;
    let moved = movers(snapshots, by, MOVERS);
    writeln!(
        out,
        "\n  {}",
        paint(
            palette.bold(Ink::Text),
            format!("Biggest movers by {}", by.label())
        )
    )?;
    if moved.is_empty() {
        writeln!(out, "    {}", paint(dim, "nothing changed"))?;
    } else {
        write_rows(out, palette, &moved)?;
    }
    writeln!(out)?;
    Ok(())
}

fn span(snapshots: &[Snapshot]) -> String {
    match (snapshots.first(), snapshots.last()) {
        (Some(first), Some(last)) if first.date() != last.date() => format!(
            "{} \u{b7} {} to {}",
            plural(snapshots.len() as u32, "snapshot", "snapshots"),
            first.date(),
            last.date()
        ),
        (Some(only), _) => format!(
            "{} \u{b7} {}",
            plural(snapshots.len() as u32, "snapshot", "snapshots"),
            only.date()
        ),
        _ => "no snapshots".to_owned(),
    }
}

fn write_rows(out: &mut dyn Write, palette: Palette, rows: &[Series]) -> Result<()> {
    let width = rows
        .iter()
        .map(|series| series.name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 34);
    for series in rows {
        let direction = series.direction();
        writeln!(
            out,
            "    {} {}  {:>12}  {}",
            column(&series.name, width, Align::Left),
            paint(palette.style(ink(direction)), series.sparkline()),
            format!("{} \u{2192} {}", series.first(), series.last()),
            paint(
                palette.style(ink(direction)),
                format!("{:+} {}", series.change(), direction.label())
            )
        )?;
    }
    Ok(())
}

/// One object per series, then a summary, in the shapes `docs/trends.md` lists.
pub fn json(snapshots: &[Snapshot], by: By, out: &mut dyn Write) -> Result<()> {
    let line = |kind: &str, series: &Series| {
        serde_json::json!({
            "kind": kind,
            "name": series.name,
            "points": series.points,
            "first": series.first(),
            "last": series.last(),
            "change": series.change(),
            "direction": series.direction().label(),
        })
    };
    for series in overall(snapshots) {
        writeln!(out, "{}", line("series", &series))?;
    }
    for series in movers(snapshots, by, MOVERS) {
        let mut mover = line("mover", &series);
        mover["by"] = by.label().into();
        writeln!(out, "{mover}")?;
    }
    writeln!(
        out,
        "{}",
        serde_json::json!({
            "kind": "summary",
            "schema_version": crate::report::SCHEMA_VERSION,
            "snapshots": snapshots.len(),
            "from": snapshots.first().map(|snapshot| snapshot.time.as_str()),
            "to": snapshots.last().map(|snapshot| snapshot.time.as_str()),
            "by": by.label(),
        })
    )?;
    Ok(())
}

const CHART_WIDTH: f64 = 640.0;
const CHART_HEIGHT: f64 = 180.0;
const PLOT_LEFT: f64 = 40.0;
const PLOT_RIGHT: f64 = 624.0;
const PLOT_TOP: f64 = 16.0;
const PLOT_BOTTOM: f64 = 148.0;
/// Past this many snapshots the points merge into the line, and a dot on each only
/// makes it lumpy.
const MOST_DOTS: usize = 40;

/// The health score over the snapshots as an inline SVG, 0 to 100 on the vertical axis
/// and the snapshots evenly spaced across it. Colours are the page's own CSS classes,
/// so the chart follows its light and dark themes. Needs at least two snapshots to
/// have a line; the caller says so otherwise.
pub fn chart(snapshots: &[Snapshot]) -> String {
    let at = |index: usize, health: u32| {
        let across = if snapshots.len() < 2 {
            0.0
        } else {
            index as f64 / (snapshots.len() - 1) as f64
        };
        (
            PLOT_LEFT + across * (PLOT_RIGHT - PLOT_LEFT),
            PLOT_BOTTOM - f64::from(health.min(100)) / 100.0 * (PLOT_BOTTOM - PLOT_TOP),
        )
    };
    let points: Vec<(f64, f64)> = snapshots
        .iter()
        .enumerate()
        .map(|(index, snapshot)| at(index, snapshot.health))
        .collect();
    let line: Vec<String> = points
        .iter()
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect();

    let mut svg = format!(
        "<svg viewBox=\"0 0 {CHART_WIDTH} {CHART_HEIGHT}\" role=\"img\" aria-labelledby=\"trend-title trend-desc\" class=\"trend\">\n\
         <title id=\"trend-title\">Health over time</title>\n\
         <desc id=\"trend-desc\">{}</desc>\n",
        markup(&description(snapshots))
    );
    for level in [0_u32, 50, 100] {
        let (_, y) = at(0, level);
        svg.push_str(&format!(
            "<line class=\"trend-axis\" x1=\"{PLOT_LEFT}\" y1=\"{y:.1}\" x2=\"{PLOT_RIGHT}\" y2=\"{y:.1}\"/>\n\
             <text class=\"trend-label\" x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{level}</text>\n",
            PLOT_LEFT - 6.0,
            y + 4.0
        ));
    }
    svg.push_str(&format!(
        "<polyline class=\"trend-line\" points=\"{}\"/>\n",
        line.join(" ")
    ));
    if snapshots.len() <= MOST_DOTS {
        for (x, y) in &points {
            svg.push_str(&format!(
                "<circle class=\"trend-dot\" cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"3\"/>\n"
            ));
        }
    }
    if let (Some(first), Some(last)) = (snapshots.first(), snapshots.last()) {
        let base = PLOT_BOTTOM + 20.0;
        svg.push_str(&format!(
            "<text class=\"trend-label\" x=\"{PLOT_LEFT}\" y=\"{base}\">{}</text>\n\
             <text class=\"trend-label\" x=\"{PLOT_RIGHT}\" y=\"{base}\" text-anchor=\"end\">{}</text>\n",
            markup(first.date()),
            markup(last.date())
        ));
    }
    svg.push_str("</svg>");
    svg
}

/// What the chart says, for a reader who cannot see it.
fn description(snapshots: &[Snapshot]) -> String {
    match (snapshots.first(), snapshots.last()) {
        (Some(first), Some(last)) => format!(
            "Health across {}, from {} on {} to {} on {}.",
            plural(snapshots.len() as u32, "snapshot", "snapshots"),
            first.health,
            first.date(),
            last.health,
            last.date()
        ),
        _ => "No snapshots.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use rqx_core::finding::Severity;

    use super::*;
    use crate::brand::ColorDepth;
    use crate::report::tests::outcome_with;

    fn snapshots() -> Vec<Snapshot> {
        [(0, 50, 10), (1, 62, 6), (2, 70, 3)]
            .into_iter()
            .map(|(day, health, findings)| {
                let outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
                let mut snapshot = Snapshot::of(&outcome, day * 86_400);
                snapshot.health = health;
                snapshot.counts.total = findings;
                snapshot.rules.insert("long-function".into(), findings);
                snapshot
            })
            .collect()
    }

    fn text(snapshots: &[Snapshot]) -> String {
        let mut out = Vec::new();
        let palette = Palette::with_depth(ColorDepth::Monochrome);
        render(snapshots, By::Rule, palette, &mut out).expect("rendering to memory");
        String::from_utf8(out).expect("utf-8")
    }

    #[test]
    fn the_series_are_listed_with_direction_and_a_sparkline() {
        let text = text(&snapshots());
        let health = text.lines().find(|line| line.contains("health")).unwrap();
        assert!(health.contains("50 \u{2192} 70") && health.contains("+20 improving"));
        assert!(health.contains('\u{2581}') && health.contains('\u{2588}'));
        let findings = text.lines().find(|line| line.contains("findings")).unwrap();
        assert!(findings.contains("-7 improving"), "{findings}");
        assert!(text.contains("1970-01-01 to 1970-01-03"), "{text}");
        assert!(text.contains("long-function"), "{text}");
    }

    #[test]
    fn fewer_than_two_snapshots_say_what_is_missing() {
        let one = text(&snapshots()[..1]);
        assert!(one.contains("needs two snapshots"), "{one}");
        let none = text(&[]);
        assert!(none.contains("no snapshots"), "{none}");
    }

    #[test]
    fn the_json_has_a_line_per_series_and_a_summary() {
        let mut out = Vec::new();
        json(&snapshots(), By::Rule, &mut out).expect("json");
        let lines: Vec<serde_json::Value> = String::from_utf8(out)
            .expect("utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("json"))
            .collect();
        let kinds: Vec<&str> = lines
            .iter()
            .map(|line| line["kind"].as_str().expect("a kind"))
            .collect();
        assert_eq!(kinds, ["series", "series", "mover", "summary"]);
        assert_eq!(lines[0]["points"], serde_json::json!([50, 62, 70]));
        assert_eq!(lines[0]["direction"], "improving");
        assert_eq!(lines[2]["by"], "rule");
        assert_eq!(lines[3]["snapshots"], 3);
    }

    #[test]
    fn the_chart_is_an_svg_with_a_line_through_every_snapshot() {
        let svg = chart(&snapshots());
        assert!(svg.starts_with("<svg viewBox="), "{svg}");
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("role=\"img\""));
        assert_eq!(svg.matches("<circle").count(), 3);
        let points = svg
            .split("<polyline class=\"trend-line\" points=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("a polyline");
        assert_eq!(points.split(' ').count(), 3);
        assert!(svg.contains(">1970-01-01</text>") && svg.contains(">1970-01-03</text>"));
        assert!(
            svg.contains("from 50 on 1970-01-01 to 70 on 1970-01-03"),
            "{svg}"
        );
        assert!(!svg.contains("<script"));
    }

    #[test]
    fn a_rising_score_is_drawn_rising() {
        let svg = chart(&snapshots());
        let ys: Vec<f64> = svg
            .split("class=\"trend-dot\" cx=\"")
            .skip(1)
            .map(|dot| {
                let cy = dot.split("cy=\"").nth(1).expect("a cy");
                cy.split('"')
                    .next()
                    .expect("a value")
                    .parse()
                    .expect("a number")
            })
            .collect();
        assert!(ys[0] > ys[1] && ys[1] > ys[2], "{ys:?}");
    }
}

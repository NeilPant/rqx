//! The import graph as something you can read, paste into a document, or feed to a
//! layout tool.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write;

use anyhow::Result;
use clap::ValueEnum;

use crate::brand::{Ink, Palette, paint};
use crate::scan::ScanOutcome;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Shape {
    /// One line per edge, grouped by importer
    Text,
    /// Graphviz, for `dot -Tsvg`
    Dot,
    /// A fenced block GitHub and most wikis render on the spot
    Mermaid,
    /// Nodes and edges as data
    Json,
}

pub struct View<'a> {
    pub shape: Shape,
    pub cycles_only: bool,
    pub focus: Option<&'a str>,
    pub depth: u32,
}

pub fn render(
    outcome: &ScanOutcome,
    view: &View<'_>,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let edges = narrow(outcome, view);
    match view.shape {
        Shape::Text => text(outcome, &edges, palette, out),
        Shape::Dot => dot(outcome, &edges, out),
        Shape::Mermaid => mermaid(outcome, &edges, out),
        Shape::Json => json(outcome, &edges, out),
    }
}

/// Which edges to draw. A whole graph is unreadable past a few hundred modules, so
/// `--focus` and `--cycles` are how a reader asks about the part they mean.
fn narrow(outcome: &ScanOutcome, view: &View<'_>) -> BTreeSet<(String, String)> {
    let every: BTreeSet<(String, String)> = outcome
        .import_edges
        .iter()
        .flat_map(|(from, targets)| targets.iter().map(|to| (from.clone(), to.clone())))
        .collect();

    if view.cycles_only {
        let in_a_cycle: BTreeSet<(String, String)> = outcome
            .cycles
            .iter()
            .flat_map(|ring| {
                ring.windows(2)
                    .map(|edge| (edge[0].clone(), edge[1].clone()))
            })
            .collect();
        return in_a_cycle;
    }

    let Some(focus) = view.focus else {
        return every;
    };
    // Both directions: "what does this reach" and "what reaches this" are the same
    // question asked by someone about to change a file.
    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<(String, u32)> = VecDeque::new();
    queue.push_back((focus.to_owned(), 0));
    while let Some((here, away)) = queue.pop_front() {
        if !reached.insert(here.clone()) || away == view.depth {
            continue;
        }
        for (from, to) in &every {
            if *from == here {
                queue.push_back((to.clone(), away + 1));
            } else if *to == here {
                queue.push_back((from.clone(), away + 1));
            }
        }
    }
    every
        .into_iter()
        .filter(|(from, to)| reached.contains(from) && reached.contains(to))
        .collect()
}

fn cycle_edges(outcome: &ScanOutcome) -> BTreeSet<(String, String)> {
    outcome
        .cycles
        .iter()
        .flat_map(|ring| {
            ring.windows(2)
                .map(|edge| (edge[0].clone(), edge[1].clone()))
        })
        .collect()
}

fn text(
    outcome: &ScanOutcome,
    edges: &BTreeSet<(String, String)>,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let dim = palette.style(Ink::Muted);

    if !outcome.cycles.is_empty() {
        writeln!(
            out,
            "\n  {}  {}",
            paint(palette.style(Ink::Text).bold(), "Cycles"),
            paint(
                dim,
                crate::report::plural(outcome.cycles.len() as u32, "ring", "rings")
            )
        )?;
        for ring in &outcome.cycles {
            writeln!(
                out,
                "    {}",
                paint(palette.style(Ink::Danger), ring.join(" \u{2192} "))
            )?;
        }
    }

    let mut by_importer: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (from, to) in edges {
        by_importer.entry(from).or_default().push(to);
    }
    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.style(Ink::Text).bold(), "Imports"),
        paint(
            dim,
            format!(
                "{} from {}",
                crate::report::plural(edges.len() as u32, "edge", "edges"),
                crate::report::plural(by_importer.len() as u32, "module", "modules")
            )
        )
    )?;
    let in_a_cycle = cycle_edges(outcome);
    for (from, targets) in by_importer {
        writeln!(out, "\n    {from}")?;
        for to in targets {
            let looping = in_a_cycle.contains(&(from.to_owned(), to.to_owned()));
            writeln!(
                out,
                "      {} {}",
                paint(dim, "\u{2192}"),
                if looping {
                    paint(palette.style(Ink::Danger), format!("{to}  closes a cycle"))
                } else {
                    to.to_owned()
                }
            )?;
        }
    }
    Ok(())
}

/// An identifier for a module. Both formats reject the slashes and dots a path is
/// made of, so the path becomes a label and the node gets a number.
fn node(at: usize) -> String {
    format!("n{at}")
}

fn names(edges: &BTreeSet<(String, String)>) -> BTreeMap<String, usize> {
    let mut every: BTreeSet<&String> = BTreeSet::new();
    for (from, to) in edges {
        every.insert(from);
        every.insert(to);
    }
    every
        .into_iter()
        .enumerate()
        .map(|(at, name)| (name.clone(), at))
        .collect()
}

fn dot(
    outcome: &ScanOutcome,
    edges: &BTreeSet<(String, String)>,
    out: &mut dyn Write,
) -> Result<()> {
    let ids = names(edges);
    let in_a_cycle = cycle_edges(outcome);
    writeln!(out, "digraph imports {{")?;
    writeln!(out, "  rankdir=LR;")?;
    writeln!(out, "  node [shape=box, fontname=\"sans-serif\"];")?;
    for (name, at) in &ids {
        writeln!(out, "  {} [label={:?}];", node(*at), name)?;
    }
    for (from, to) in edges {
        let colour = if in_a_cycle.contains(&(from.clone(), to.clone())) {
            " [color=\"#e4574a\", penwidth=2]"
        } else {
            ""
        };
        writeln!(out, "  {} -> {}{colour};", node(ids[from]), node(ids[to]))?;
    }
    writeln!(out, "}}")?;
    Ok(())
}

fn mermaid(
    outcome: &ScanOutcome,
    edges: &BTreeSet<(String, String)>,
    out: &mut dyn Write,
) -> Result<()> {
    let ids = names(edges);
    let in_a_cycle = cycle_edges(outcome);
    writeln!(out, "```mermaid")?;
    writeln!(out, "graph LR")?;
    for (name, at) in &ids {
        writeln!(out, "  {}[\"{name}\"]", node(*at))?;
    }
    let mut looping = Vec::new();
    for (at, (from, to)) in edges.iter().enumerate() {
        writeln!(out, "  {} --> {}", node(ids[from]), node(ids[to]))?;
        if in_a_cycle.contains(&(from.clone(), to.clone())) {
            looping.push(at);
        }
    }
    for at in looping {
        writeln!(out, "  linkStyle {at} stroke:#e4574a,stroke-width:2px")?;
    }
    writeln!(out, "```")?;
    Ok(())
}

fn json(
    outcome: &ScanOutcome,
    edges: &BTreeSet<(String, String)>,
    out: &mut dyn Write,
) -> Result<()> {
    let in_a_cycle = cycle_edges(outcome);
    writeln!(
        out,
        "{}",
        serde_json::json!({
            "nodes": names(edges).keys().collect::<Vec<_>>(),
            "edges": edges
                .iter()
                .map(|(from, to)| serde_json::json!({
                    "from": from,
                    "to": to,
                    "inCycle": in_a_cycle.contains(&(from.clone(), to.clone())),
                }))
                .collect::<Vec<_>>(),
            "cycles": outcome.cycles,
        })
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(edges: &[(&str, &str)], cycles: &[&[&str]]) -> ScanOutcome {
        let mut built = crate::report::tests::outcome_with(&[], 0);
        for (from, to) in edges {
            built
                .import_edges
                .entry((*from).to_owned())
                .or_default()
                .push((*to).to_owned());
        }
        built.cycles = cycles
            .iter()
            .map(|ring| ring.iter().map(|at| (*at).to_owned()).collect())
            .collect();
        built
    }

    fn rendered(outcome: &ScanOutcome, view: &View<'_>) -> String {
        let mut out = Vec::new();
        render(
            outcome,
            view,
            Palette::with_depth(crate::brand::ColorDepth::Monochrome),
            &mut out,
        )
        .expect("render");
        String::from_utf8(out).expect("utf-8")
    }

    fn view(shape: Shape) -> View<'static> {
        View {
            shape,
            cycles_only: false,
            focus: None,
            depth: 2,
        }
    }

    /// A path is not an identifier in either format: unquoted slashes and dots make
    /// a document neither `dot` nor mermaid will read.
    #[test]
    fn a_path_never_reaches_either_format_as_an_identifier() {
        let built = outcome(&[("src/a.ts", "src/b.ts")], &[]);
        for shape in [Shape::Dot, Shape::Mermaid] {
            let text = rendered(&built, &view(shape));
            for line in text
                .lines()
                .filter(|line| line.contains("->") || line.contains("-->"))
            {
                assert!(!line.contains('/'), "an unquoted path in an edge: {line}");
            }
            assert!(text.contains("src/a.ts"), "the label is still the path");
        }
    }

    #[test]
    fn cycles_only_keeps_the_edges_that_close_one() {
        let built = outcome(&[("a", "b"), ("b", "a"), ("a", "z")], &[&["a", "b", "a"]]);
        let text = rendered(
            &built,
            &View {
                cycles_only: true,
                ..view(Shape::Json)
            },
        );
        assert!(text.contains("\"from\":\"a\""), "{text}");
        assert!(!text.contains("\"z\""), "an edge outside the cycle: {text}");
    }

    /// Following imports one way answers half the question; the reader is usually
    /// about to change the file and wants to know who is watching.
    #[test]
    fn focus_reaches_importers_as_well_as_imports() {
        let built = outcome(&[("caller", "middle"), ("middle", "callee")], &[]);
        let text = rendered(
            &built,
            &View {
                focus: Some("middle"),
                depth: 1,
                ..view(Shape::Json)
            },
        );
        assert!(text.contains("caller"), "{text}");
        assert!(text.contains("callee"), "{text}");
    }

    #[test]
    fn a_cycle_edge_is_marked_in_every_format() {
        let built = outcome(&[("a", "b"), ("b", "a")], &[&["a", "b", "a"]]);
        assert!(rendered(&built, &view(Shape::Json)).contains("\"inCycle\":true"));
        assert!(rendered(&built, &view(Shape::Dot)).contains("#e4574a"));
        assert!(rendered(&built, &view(Shape::Mermaid)).contains("linkStyle"));
        assert!(rendered(&built, &view(Shape::Text)).contains("closes a cycle"));
    }
}

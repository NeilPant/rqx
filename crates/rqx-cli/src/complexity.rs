//! The numbers the analyser measured, as a distribution rather than as a tail.
//!
//! Every rule with a threshold reports what crosses it and nothing else, which
//! answers "what is too complex" and never "compared to what". These are the same
//! measurements, all of them.

use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;
use clap::ValueEnum;

use crate::brand::{Ink, Palette, paint};
use crate::report::{Align, column};
use crate::scan::ScanOutcome;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum By {
    Function,
    File,
    Dir,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Sort {
    Complexity,
    Statements,
    Nesting,
    Params,
}

pub struct View {
    pub by: By,
    pub sort: Sort,
    pub top: usize,
    pub min: u32,
    pub histogram: bool,
}

/// Shared with the interface's Complexity screen, so the table there and the table
/// `rqx complexity` prints are one table measured once.
pub struct Row {
    pub name: String,
    pub at: String,
    pub complexity: u32,
    pub statements: u32,
    pub nesting: u32,
    pub parameters: u32,
    /// How many functions this row stands for: one, unless it is a file or a
    /// directory, where the numbers are the worst of what it holds.
    pub counted: u32,
}

impl Row {
    const fn key(&self, sort: Sort) -> u32 {
        match sort {
            Sort::Complexity => self.complexity,
            Sort::Statements => self.statements,
            Sort::Nesting => self.nesting,
            Sort::Params => self.parameters,
        }
    }
}

pub fn render(
    outcome: &ScanOutcome,
    view: &View,
    threshold: u32,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let functions = functions(outcome);
    let rows = group(&functions, view);
    let dim = palette.style(Ink::Muted);

    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.style(Ink::Text).bold(), title(view.by)),
        paint(
            dim,
            format!(
                "{} measured",
                crate::report::plural(functions.len() as u32, "function", "functions")
            )
        )
    )?;

    if functions.is_empty() {
        writeln!(
            out,
            "\n  {}\n",
            paint(dim, "nothing with a body was found to measure")
        )?;
        return Ok(());
    }

    let width = rows
        .iter()
        .take(view.top)
        .map(|row| row.name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 48);

    writeln!(
        out,
        "\n    {:<width$} {} {} {} {}  {}",
        "",
        paint(dim, column("cplx", 5, Align::Right)),
        paint(dim, column("stmts", 6, Align::Right)),
        paint(dim, column("nest", 5, Align::Right)),
        paint(dim, column("args", 4, Align::Right)),
        paint(dim, "where")
    )?;
    for row in rows.iter().take(view.top) {
        let ink = if row.complexity >= threshold {
            Ink::Warn
        } else {
            Ink::Text
        };
        writeln!(
            out,
            "    {} {:>5} {:>6} {:>5} {:>4}  {}",
            paint(
                palette.style(ink),
                column(&trim(&row.name, width), width, Align::Left)
            ),
            row.complexity,
            row.statements,
            row.nesting,
            row.parameters,
            paint(dim, &row.at)
        )?;
    }

    let over = functions
        .iter()
        .filter(|function| function.complexity >= threshold)
        .count();
    writeln!(
        out,
        "\n    {}",
        paint(
            dim,
            format!(
                "p50 {} \u{b7} p90 {} \u{b7} p99 {} \u{b7} {over} of {} over the threshold of {threshold}",
                percentile(&functions, 50),
                percentile(&functions, 90),
                percentile(&functions, 99),
                functions.len()
            )
        )
    )?;

    if view.histogram {
        draw(&functions, threshold, palette, out)?;
    }
    writeln!(out)?;
    Ok(())
}

const fn title(by: By) -> &'static str {
    match by {
        By::Function => "Complexity",
        By::File => "Complexity by file",
        By::Dir => "Complexity by directory",
    }
}

fn trim(name: &str, width: usize) -> String {
    if name.chars().count() <= width {
        return name.to_owned();
    }
    let kept: String = name.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}\u{2026}")
}

pub fn functions(outcome: &ScanOutcome) -> Vec<Row> {
    let mut rows: Vec<Row> = outcome
        .shape_by_file
        .iter()
        .flat_map(|(file, shape)| {
            shape.functions.iter().map(move |function| Row {
                name: function
                    .name
                    .clone()
                    .unwrap_or_else(|| "(anonymous)".to_owned()),
                at: format!("{file}:{}", function.range.line),
                complexity: function.cognitive_complexity,
                statements: function.statements,
                nesting: function.max_nesting,
                parameters: function.parameters,
                counted: 1,
            })
        })
        .collect();
    // A total order, so two functions with the same numbers do not swap places
    // between runs the way a thread-order-dependent sort would let them.
    rows.sort_by(|a, b| a.at.cmp(&b.at).then(a.name.cmp(&b.name)));
    rows
}

pub fn group(functions: &[Row], view: &View) -> Vec<Row> {
    let mut rows: Vec<Row> = match view.by {
        By::Function => functions
            .iter()
            .filter(|row| row.key(view.sort) >= view.min)
            .map(|row| Row {
                name: row.name.clone(),
                at: row.at.clone(),
                complexity: row.complexity,
                statements: row.statements,
                nesting: row.nesting,
                parameters: row.parameters,
                counted: row.counted,
            })
            .collect(),
        By::File | By::Dir => {
            let mut worst: BTreeMap<String, Row> = BTreeMap::new();
            for row in functions {
                let file = row
                    .at
                    .rsplit_once(':')
                    .map_or(row.at.as_str(), |(at, _)| at);
                let key = match view.by {
                    By::Dir => file.rsplit_once('/').map_or(".", |(at, _)| at).to_owned(),
                    _ => file.to_owned(),
                };
                let held = worst.entry(key.clone()).or_insert_with(|| Row {
                    name: key,
                    at: String::new(),
                    complexity: 0,
                    statements: 0,
                    nesting: 0,
                    parameters: 0,
                    counted: 0,
                });
                // The worst of what it holds, not the mean: an average hides one
                // terrible function behind twenty ordinary ones.
                held.complexity = held.complexity.max(row.complexity);
                held.statements = held.statements.max(row.statements);
                held.nesting = held.nesting.max(row.nesting);
                held.parameters = held.parameters.max(row.parameters);
                held.counted += 1;
            }
            worst
                .into_values()
                .filter(|row| row.key(view.sort) >= view.min)
                .map(|mut row| {
                    row.at = crate::report::plural(row.counted, "function", "functions");
                    row
                })
                .collect()
        }
    };
    rows.sort_by(|a, b| {
        b.key(view.sort)
            .cmp(&a.key(view.sort))
            .then(a.name.cmp(&b.name))
            .then(a.at.cmp(&b.at))
    });
    rows
}

pub fn percentile(functions: &[Row], at: u32) -> u32 {
    if functions.is_empty() {
        return 0;
    }
    let mut scores: Vec<u32> = functions.iter().map(|row| row.complexity).collect();
    scores.sort_unstable();
    let index = ((scores.len() - 1) * at as usize) / 100;
    scores[index]
}

/// Twenty buckets from the smallest measurement to the largest, so the shape of the
/// distribution is visible rather than only its ends.
fn draw(functions: &[Row], threshold: u32, palette: Palette, out: &mut dyn Write) -> Result<()> {
    const BUCKETS: usize = 20;
    let dim = palette.style(Ink::Muted);
    let highest = functions
        .iter()
        .map(|row| row.complexity)
        .max()
        .unwrap_or(0)
        .max(1);
    let width = (highest as usize).div_ceil(BUCKETS).max(1);

    let mut counts = [0_u32; BUCKETS];
    for row in functions {
        let at = ((row.complexity as usize) / width).min(BUCKETS - 1);
        counts[at] += 1;
    }
    let tallest = counts.iter().copied().max().unwrap_or(1).max(1);

    writeln!(
        out,
        "\n    {}",
        paint(dim, "how many functions score what, on a square-root scale")
    )?;
    for (at, count) in counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let from = at * width;
        // Square-rooted, because the distribution is not close to flat: nearly every
        // function is trivial, and a linear bar makes that one bucket the whole chart
        // and every bucket worth looking at a single character.
        let bar = ((f64::from(*count).sqrt() / f64::from(tallest).sqrt()) * 40.0).round() as usize;
        let bar = bar.max(1);
        let ink = if from as u32 >= threshold {
            Ink::Warn
        } else {
            Ink::Good
        };
        writeln!(
            out,
            "    {:>4}\u{2013}{:<4} {} {}",
            from,
            from + width - 1,
            paint(palette.style(ink), "\u{2588}".repeat(bar)),
            paint(dim, count.to_string())
        )?;
    }
    Ok(())
}

pub fn json(outcome: &ScanOutcome, view: &View, out: &mut dyn Write) -> Result<()> {
    for row in group(&functions(outcome), view).iter().take(view.top) {
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "name": row.name,
                "at": row.at,
                "complexity": row.complexity,
                "statements": row.statements,
                "nesting": row.nesting,
                "parameters": row.parameters,
                "functions": row.counted,
            })
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(functions: &[(&str, &str, u32, u32)]) -> ScanOutcome {
        let mut outcome = crate::report::tests::outcome_with(&[], 0);
        for (file, name, complexity, statements) in functions {
            let shape = outcome
                .shape_by_file
                .entry(std::sync::Arc::from(*file))
                .or_default();
            shape.functions.push(rqx_lang::FunctionShape {
                name: Some((*name).to_owned()),
                range: rqx_core::finding::SourceRange {
                    file: std::sync::Arc::from(*file),
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
                parameters: 1,
                statements: *statements,
                cognitive_complexity: *complexity,
                max_nesting: 1,
            });
        }
        outcome
    }

    fn view(by: By) -> View {
        View {
            by,
            sort: Sort::Complexity,
            top: 10,
            min: 0,
            histogram: false,
        }
    }

    fn rows_of(outcome: &ScanOutcome, view: &View) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        json(outcome, view, &mut out).expect("json");
        String::from_utf8(out)
            .expect("utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a row"))
            .collect()
    }

    #[test]
    fn the_worst_function_is_first() {
        let outcome = measured(&[("a.ts", "small", 2, 4), ("a.ts", "huge", 40, 90)]);
        let rows = rows_of(&outcome, &view(By::Function));
        assert_eq!(rows[0]["name"], "huge");
        assert_eq!(rows[0]["complexity"], 40);
    }

    /// A directory's mean hides one terrible function behind twenty ordinary ones,
    /// which is the opposite of what someone scanning a list of directories wants.
    #[test]
    fn a_directory_reports_the_worst_it_holds_not_the_average() {
        let outcome = measured(&[
            ("src/pay/a.ts", "fine", 1, 2),
            ("src/pay/b.ts", "awful", 60, 120),
            ("src/pay/c.ts", "fine too", 1, 2),
        ]);
        let rows = rows_of(&outcome, &view(By::Dir));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "src/pay");
        assert_eq!(rows[0]["complexity"], 60);
        assert_eq!(rows[0]["functions"], 3);
    }

    #[test]
    fn sorting_by_statements_asks_a_different_question() {
        let outcome = measured(&[("a.ts", "twisty", 30, 5), ("a.ts", "long", 3, 200)]);
        let by_statements = View {
            sort: Sort::Statements,
            ..view(By::Function)
        };
        assert_eq!(rows_of(&outcome, &by_statements)[0]["name"], "long");
        assert_eq!(rows_of(&outcome, &view(By::Function))[0]["name"], "twisty");
    }

    #[test]
    fn min_drops_everything_under_it() {
        let outcome = measured(&[("a.ts", "small", 2, 4), ("a.ts", "big", 40, 90)]);
        let rows = rows_of(
            &outcome,
            &View {
                min: 10,
                ..view(By::Function)
            },
        );
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0]["name"], "big");
    }

    /// The point of the command is the distribution, and a percentile that moved with
    /// the sort order or the row limit would not be one.
    #[test]
    fn the_percentiles_describe_every_function_not_the_rows_shown() {
        let mut functions: Vec<(String, String, u32, u32)> = (0..100)
            .map(|at| ("a.ts".to_owned(), format!("f{at}"), at, at))
            .collect();
        functions.push(("a.ts".to_owned(), "worst".to_owned(), 500, 500));
        let borrowed: Vec<(&str, &str, u32, u32)> = functions
            .iter()
            .map(|(file, name, complexity, statements)| {
                (file.as_str(), name.as_str(), *complexity, *statements)
            })
            .collect();
        let outcome = measured(&borrowed);

        // 101 functions scoring 0..=99 and one outlier at 500. The outlier is one
        // per cent of them, so it sits above p99 and does not drag the percentiles
        // the way a mean would.
        let rows = functions_of(&outcome);
        assert_eq!(percentile(&rows, 50), 50);
        assert_eq!(percentile(&rows, 99), 99);
        assert_eq!(percentile(&rows, 100), 500, "the top is the outlier");
    }

    fn functions_of(outcome: &ScanOutcome) -> Vec<Row> {
        super::functions(outcome)
    }
}

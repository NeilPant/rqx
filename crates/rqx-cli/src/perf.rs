//! Which functions to look at first when something is slow.
//!
//! `rqx scan` lists findings by file and `rqx complexity` measures how hard a
//! function is to read. Neither answers the question a lead asks about a slow
//! service: which body does work that grows faster than its input, weighted by how
//! much of the project depends on it — and, when a profile was read, whether
//! anything actually ran it.

use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;
use clap::ValueEnum;
use rqx_core::finding::Finding;
use rqx_core::rule::Category;

use crate::brand::{Ink, Palette, paint};
use crate::scan::ScanOutcome;

mod measure;

use measure::{Measure, align, measure_of};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum By {
    Function,
    File,
    Dir,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Sort {
    Weight,
    Order,
    Io,
    Reach,
    Sampled,
    Time,
    Space,
}

pub struct View {
    pub by: By,
    pub sort: Sort,
    pub top: usize,
    pub min_depth: u32,
}

pub struct Line {
    pub rule: &'static str,
    pub evidence: String,
    pub fix: &'static str,
}

pub struct Row {
    pub name: String,
    pub at: String,
    pub loops: u32,
    pub self_calls: u32,
    /// Counted apart from the other shapes because a round trip shows up as latency
    /// rather than as processor time.
    pub io: u32,
    pub weight: f64,
    pub reach: f64,
    pub sampled: Option<rqx_core::runtime::Share>,
    pub findings: Vec<Line>,
    pub counted: u32,
    /// What the cost engine composed for the function, beside the syntactic `loops`.
    pub measure: Measure,
}

impl Row {
    fn key(&self, sort: Sort) -> f64 {
        match sort {
            Sort::Weight => self.weight,
            Sort::Order | Sort::Time => f64::from(self.measure.time.rank()),
            Sort::Space => f64::from(self.measure.space.rank()),
            Sort::Io => f64::from(self.io),
            Sort::Reach => self.reach,
            Sort::Sampled => f64::from(self.sampled.map_or(0, |share| share.total)),
        }
    }

    /// `row` with the keys the cost engine adds. The keys it already has are not touched,
    /// so a reader that knows only the older ones reads the same row.
    pub fn with_cost(&self, mut row: serde_json::Value) -> serde_json::Value {
        if let Some(fields) = row.as_object_mut() {
            fields.extend(
                self.measure
                    .json_fields()
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value)),
            );
        }
        row
    }
}

/// What the innermost body costs as the input grows, read from the loop nesting alone.
/// Loop order, not a proof: three nested loops over the same short list are still three
/// passes of nothing. The composed time sits beside it in `Row::measure`.
pub const fn order(loops: u32, self_calls: u32) -> &'static str {
    if self_calls >= 2 {
        return "exp";
    }
    match loops {
        0 => "O(1)",
        1 => "O(n)",
        2 => "O(n\u{b2})",
        _ => "O(n\u{b3}+)",
    }
}

/// Every function carrying a performance finding, and — when a profile was read —
/// every function it put on the stack, because a hot body with no static shape is
/// still where to look.
pub fn rows(outcome: &ScanOutcome) -> Vec<Row> {
    const HOT_ENOUGH: u32 = 10;
    /// A cap on how much a function's time order adds to its weight, so that `exp`, which
    /// ranks above everything, counts as a little worse than cubic and not as infinity.
    const WEIGHED_RANK_CAP: u32 = 17;

    let mut rows: Vec<Row> = Vec::new();
    for (file, shape) in &outcome.shape_by_file {
        let reach = outcome.impact.get(file).multiplier();
        let aligned = std::cell::OnceCell::new();
        let mut carried: BTreeMap<u32, Vec<&Finding>> = BTreeMap::new();
        for finding in outcome
            .findings
            .iter()
            .filter(|finding| finding.rule.spec().category == Category::Performance)
            .filter(|finding| finding.location.file.as_ref() == file.as_ref())
        {
            if let Some(function) = innermost(shape, finding.location.start) {
                carried.entry(function).or_default().push(finding);
            }
        }

        for function in &shape.functions {
            let held = carried.get(&function.range.start);
            let sampled = outcome
                .profile
                .as_ref()
                .and_then(|profile| profile.of(file, function.range.line, function.last_line));
            let hot = sampled.is_some_and(|share| share.total >= HOT_ENOUGH);
            if held.is_none() && !hot {
                continue;
            }
            let findings = held.map(Vec::as_slice).unwrap_or_default();
            let weight: f64 = findings
                .iter()
                .filter(|finding| finding.confidence != rqx_core::finding::Confidence::Low)
                .map(|finding| finding.severity.weight())
                .sum();
            let io = findings
                .iter()
                .filter(|finding| {
                    matches!(
                        finding.rule.slug(),
                        "query-in-loop" | "await-in-loop" | "blocking-call-in-async"
                    )
                })
                .count() as u32;
            let measure = measure_of(
                outcome,
                file,
                aligned.get_or_init(|| {
                    outcome
                        .cost_facts
                        .as_ref()
                        .and_then(|facts| facts.of_file(file))
                        .map(|costs| align(shape, costs))
                        .unwrap_or_default()
                }),
                function.range.start,
            );
            let ranked = (1.0 + weight * reach)
                * (1.0 + f64::from(sampled.map_or(0, |share| share.total)) / 1000.0)
                * (1.0 + f64::from(measure.time.rank().min(WEIGHED_RANK_CAP)) / 4.0);
            rows.push(Row {
                name: function
                    .name
                    .clone()
                    .unwrap_or_else(|| "(anonymous)".to_owned()),
                at: format!("{file}:{}", function.range.line),
                loops: function.loop_depth,
                self_calls: function.self_calls,
                io,
                weight: ranked,
                reach,
                sampled,
                findings: findings
                    .iter()
                    .map(|finding| Line {
                        rule: finding.rule.slug(),
                        evidence: finding.evidence.to_string(),
                        fix: finding
                            .rule
                            .spec()
                            .fixes
                            .first()
                            .map_or("", |technique| technique.name()),
                    })
                    .collect(),
                counted: 1,
                measure,
            });
        }
    }
    // A total order before ranking, so two runs of the same scan cannot disagree
    // about two rows that weigh the same.
    rows.sort_by(|a, b| a.at.cmp(&b.at).then(a.name.cmp(&b.name)));
    rows
}

/// The function a finding sits in, innermost first: a callback inside a method is
/// where the work is, not the method around it.
fn innermost(shape: &rqx_lang::FileShape, offset: u32) -> Option<u32> {
    shape
        .functions
        .iter()
        .filter(|function| function.range.start <= offset && offset <= function.end)
        .min_by_key(|function| function.end.saturating_sub(function.range.start))
        .map(|function| function.range.start)
}

pub fn ranked(outcome: &ScanOutcome, view: &View) -> Vec<Row> {
    group(rows(outcome), view)
}

fn group(rows: Vec<Row>, view: &View) -> Vec<Row> {
    let mut grouped: Vec<Row> = match view.by {
        By::Function => rows
            .into_iter()
            .filter(|row| row.loops >= view.min_depth)
            .collect(),
        By::File | By::Dir => {
            let mut worst: BTreeMap<String, Row> = BTreeMap::new();
            for row in rows.into_iter().filter(|row| row.loops >= view.min_depth) {
                let file = row
                    .at
                    .rsplit_once(':')
                    .map_or(row.at.clone(), |(at, _)| at.to_owned());
                let key = match view.by {
                    By::Dir => file
                        .rsplit_once('/')
                        .map_or(".".to_owned(), |(at, _)| at.to_owned()),
                    _ => file,
                };
                let held = worst.entry(key.clone()).or_insert_with(|| Row {
                    name: key,
                    at: String::new(),
                    loops: 0,
                    self_calls: 0,
                    io: 0,
                    weight: 0.0,
                    reach: 0.0,
                    sampled: None,
                    findings: Vec::new(),
                    counted: 0,
                    measure: Measure::default(),
                });
                held.measure = Measure::worst([std::mem::take(&mut held.measure), row.measure]);
                held.loops = held.loops.max(row.loops);
                held.self_calls = held.self_calls.max(row.self_calls);
                held.reach = held.reach.max(row.reach);
                held.io += row.io;
                held.weight += row.weight;
                held.counted += 1;
            }
            worst
                .into_values()
                .map(|mut row| {
                    row.at = crate::report::plural(row.counted, "function", "functions");
                    row
                })
                .collect()
        }
    };
    grouped.sort_by(|a, b| {
        b.key(view.sort)
            .total_cmp(&a.key(view.sort))
            .then(b.loops.cmp(&a.loops))
            .then(a.name.cmp(&b.name))
            .then(a.at.cmp(&b.at))
    });
    grouped
}

const fn title(by: By) -> &'static str {
    match by {
        By::Function => "Bottlenecks",
        By::File => "Bottlenecks by file",
        By::Dir => "Bottlenecks by directory",
    }
}

pub fn render(
    outcome: &ScanOutcome,
    view: &View,
    threshold: u32,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<()> {
    let measured = rows(outcome);
    let found = measured.len() as u32;
    let profiled = outcome.profile.is_some();
    let shown = group(measured, view);
    let dim = palette.style(Ink::Muted);

    writeln!(
        out,
        "\n  {}  {}",
        paint(palette.bold(Ink::Text), title(view.by)),
        paint(
            dim,
            format!(
                "{} worth a look",
                crate::report::plural(found, "function", "functions")
            )
        )
    )?;

    if shown.is_empty() {
        writeln!(
            out,
            "\n  {}\n",
            paint(dim, "nothing here grows faster than its input")
        )?;
        return Ok(());
    }

    let width = shown
        .iter()
        .take(view.top)
        .map(|row| row.name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 40);

    let table = Table::of(&shown[..shown.len().min(view.top)], profiled);
    // Padded before it is painted: a colour escape counts towards a format width
    // and would push every column right by the length of the escape.
    writeln!(
        out,
        "\n    {}",
        paint(dim, format!("{:<width$} {}  where", "", table.header()))
    )?;

    for (row, cells) in shown.iter().zip(table.rows()) {
        let ink = if row.loops > threshold || row.self_calls >= 2 {
            Ink::Warn
        } else {
            Ink::Text
        };
        writeln!(
            out,
            "    {} {}  {}",
            paint(
                palette.style(ink),
                format!("{:<width$}", trim(&row.name, width))
            ),
            paint(palette.style(Ink::Text), cells),
            paint(dim, &row.at)
        )?;
        for line in &row.findings {
            writeln!(
                out,
                "    {:<width$} {}",
                "",
                paint(dim, format!("{}  \u{b7} Fix: {}", line.evidence, line.fix))
            )?;
        }
    }

    writeln!(out, "\n    {}", paint(dim, summary(outcome, threshold)))?;
    writeln!(out)?;
    Ok(())
}

/// The figures of the rows shown, each column as wide as its widest cell, so that a long
/// order such as `O(n² log n)` widens its own column and leaves its neighbours aligned.
struct Table {
    headers: Vec<&'static str>,
    cells: Vec<Vec<String>>,
    widths: Vec<usize>,
}

impl Table {
    fn of(rows: &[Row], profiled: bool) -> Self {
        let mut headers = vec![
            "order", "time", "space", "io", "alloc", "conf", "loops", "trips", "reach",
        ];
        if profiled {
            headers.push("sampled");
        }
        let cells: Vec<Vec<String>> = rows.iter().map(|row| figures(row, profiled)).collect();
        let widths = headers
            .iter()
            .enumerate()
            .map(|(column, header)| {
                cells
                    .iter()
                    .map(|cells| cells[column].chars().count())
                    .fold(header.len(), usize::max)
            })
            .collect();
        Self {
            headers,
            cells,
            widths,
        }
    }

    fn header(&self) -> String {
        self.line(self.headers.iter().copied())
    }

    fn rows(&self) -> impl Iterator<Item = String> + '_ {
        self.cells
            .iter()
            .map(|cells| self.line(cells.iter().map(String::as_str)))
    }

    fn line<'a>(&self, cells: impl Iterator<Item = &'a str>) -> String {
        cells
            .zip(&self.widths)
            .map(|(cell, width)| format!("{cell:>width$}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn figures(row: &Row, profiled: bool) -> Vec<String> {
    let measure = &row.measure;
    let mut figures = vec![
        order(row.loops, row.self_calls).to_owned(),
        measure.time.to_string(),
        measure.space.to_string(),
        measure.io.to_string(),
        match measure.alloc {
            0 => "\u{2013}".to_owned(),
            count => count.to_string(),
        },
        measure.conf.slug().to_owned(),
        row.loops.to_string(),
        row.io.to_string(),
        format!("\u{d7}{:.1}", row.reach),
    ];
    if profiled {
        figures.push(share_text(row.sampled, profiled));
    }
    figures
}

fn summary(outcome: &ScanOutcome, threshold: u32) -> String {
    let depths: Vec<u32> = outcome
        .shape_by_file
        .values()
        .flat_map(|shape| shape.functions.iter().map(|function| function.loop_depth))
        .collect();
    let over = depths.iter().filter(|depth| **depth > threshold).count();
    let with_io = outcome
        .findings
        .iter()
        .filter(|finding| matches!(finding.rule.slug(), "query-in-loop" | "await-in-loop"))
        .count();
    let mut text = format!(
        "p50 {} \u{b7} p90 {} \u{b7} p99 {} loop depth \u{b7} {over} over {threshold} \u{b7} {with_io} with a round trip inside a loop",
        percentile(&depths, 50),
        percentile(&depths, 90),
        percentile(&depths, 99)
    );
    if let Some(profile) = &outcome.profile {
        text.push_str(&format!(
            " \u{b7} profile: {} samples, {} matched, {} frames unplaced",
            profile.samples(),
            profile.matched(),
            profile.unmatched()
        ));
    }
    text
}

fn share_text(share: Option<rqx_core::runtime::Share>, profiled: bool) -> String {
    match (profiled, share) {
        (false, _) => String::new(),
        (true, None) => "\u{2013}".to_owned(),
        (true, Some(share)) => format!("{}/{}", tenths(share.own), tenths(share.total)),
    }
}

fn tenths(permille: u32) -> String {
    format!("{}.{}%", permille / 10, permille % 10)
}

fn percentile(depths: &[u32], at: u32) -> u32 {
    if depths.is_empty() {
        return 0;
    }
    let mut sorted = depths.to_vec();
    sorted.sort_unstable();
    let index = (sorted.len() as u64 * u64::from(at) / 100) as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn trim(name: &str, width: usize) -> String {
    if name.chars().count() <= width {
        return name.to_owned();
    }
    let kept: String = name.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}\u{2026}")
}

pub fn json(outcome: &ScanOutcome, view: &View, out: &mut dyn Write) -> Result<()> {
    for row in ranked(outcome, view).iter().take(view.top) {
        let line = row.with_cost(serde_json::json!({
            "name": row.name,
            "at": row.at,
            "order": order(row.loops, row.self_calls),
            "loops": row.loops,
            "io": row.io,
            "reach": row.reach,
            "weight": row.weight,
            "sampled": row.sampled.map(|share| serde_json::json!({
                "self": share.own,
                "inclusive": share.total,
            })),
            "findings": row.findings.iter().map(|line| serde_json::json!({
                "rule": line.rule,
                "evidence": line.evidence,
                "fix": line.fix,
            })).collect::<Vec<_>>(),
            "functions": row.counted,
        }));
        writeln!(out, "{line}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rqx_core::finding::{Confidence, Evidence, SourceRange};
    use rqx_core::rule::RuleId;

    use super::*;

    fn at(file: &str, start: u32, line: u32) -> SourceRange {
        SourceRange {
            file: std::sync::Arc::from(file),
            start,
            end: start + 1,
            line,
            column: 1,
            end_line: line,
        }
    }

    fn measured(functions: &[(&str, &str, u32, u32, u32, u32)]) -> ScanOutcome {
        let mut outcome = crate::report::tests::outcome_with(&[], 0);
        for (file, name, start, end, loops, calls) in functions {
            let shape = outcome
                .shape_by_file
                .entry(std::sync::Arc::from(*file))
                .or_default();
            shape.functions.push(rqx_lang::FunctionShape {
                name: Some((*name).to_owned()),
                range: at(file, *start, 1),
                end: *end,
                last_line: 40,
                parameters: 1,
                statements: 4,
                cognitive_complexity: 1,
                max_nesting: 1,
                loop_depth: *loops,
                self_calls: *calls,
                body: None,
            });
        }
        outcome
    }

    fn carrying(outcome: &mut ScanOutcome, rule: RuleId, file: &str, start: u32) {
        outcome.findings.push(Finding {
            rule,
            location: at(file, start, 2),
            severity: rule.spec().severity,
            confidence: rule.spec().confidence,
            evidence: Evidence::InLoop {
                subject: "client.get".to_owned(),
                depth: 1,
                sampled: None,
            },
            fix_hint: String::new(),
            related: Vec::new(),
            known: false,
        });
    }

    fn view(by: By, sort: Sort) -> View {
        View {
            by,
            sort,
            top: 10,
            min_depth: 0,
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
    fn a_function_with_no_finding_is_not_a_bottleneck() {
        let outcome = measured(&[("a.ts", "quiet", 0, 100, 3, 0)]);
        assert!(rows_of(&outcome, &view(By::Function, Sort::Weight)).is_empty());
    }

    #[test]
    fn the_heaviest_function_is_first() {
        let mut outcome = measured(&[
            ("a.ts", "light", 0, 50, 1, 0),
            ("a.ts", "heavy", 60, 120, 3, 0),
        ]);
        carrying(&mut outcome, RuleId::ScanInLoop, "a.ts", 10);
        carrying(&mut outcome, RuleId::QueryInLoop, "a.ts", 70);
        carrying(&mut outcome, RuleId::AwaitInLoop, "a.ts", 80);
        let rows = rows_of(&outcome, &view(By::Function, Sort::Weight));
        assert_eq!(rows[0]["name"], "heavy");
        assert_eq!(rows[0]["order"], "O(n\u{b3}+)");
    }

    /// A round trip is the cost a reader feels as latency, so it is counted apart
    /// from the shapes that only cost processor time.
    #[test]
    fn io_counts_the_round_trips_and_not_the_scans() {
        let mut outcome = measured(&[("a.ts", "mixed", 0, 100, 1, 0)]);
        carrying(&mut outcome, RuleId::ScanInLoop, "a.ts", 10);
        carrying(&mut outcome, RuleId::QueryInLoop, "a.ts", 20);
        carrying(&mut outcome, RuleId::AwaitInLoop, "a.ts", 30);
        let rows = rows_of(&outcome, &view(By::Function, Sort::Weight));
        assert_eq!(rows[0]["io"], 2);
        assert_eq!(rows[0]["findings"].as_array().expect("findings").len(), 3);
    }

    /// The innermost function is where the work is: a callback inside a method is
    /// what runs per element, not the method around it.
    #[test]
    fn a_finding_lands_in_the_innermost_function_that_holds_it() {
        let mut outcome = measured(&[
            ("a.ts", "outer", 0, 200, 1, 0),
            ("a.ts", "callback", 40, 80, 2, 0),
        ]);
        carrying(&mut outcome, RuleId::QueryInLoop, "a.ts", 50);
        let rows = rows_of(&outcome, &view(By::Function, Sort::Weight));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "callback");
    }

    #[test]
    fn a_finding_the_profile_contradicts_weighs_nothing() {
        let mut outcome = measured(&[("a.ts", "cold", 0, 100, 1, 0)]);
        carrying(&mut outcome, RuleId::QueryInLoop, "a.ts", 10);
        outcome.findings[0].confidence = Confidence::Low;
        let rows = rows_of(&outcome, &view(By::Function, Sort::Weight));
        assert_eq!(rows[0]["weight"], 1.0, "a low finding adds nothing to it");
    }

    #[test]
    fn min_depth_drops_the_shallow_rows() {
        let mut outcome = measured(&[
            ("a.ts", "flat", 0, 50, 1, 0),
            ("a.ts", "nested", 60, 120, 3, 0),
        ]);
        carrying(&mut outcome, RuleId::QueryInLoop, "a.ts", 10);
        carrying(&mut outcome, RuleId::QueryInLoop, "a.ts", 70);
        let mut asked = view(By::Function, Sort::Weight);
        asked.min_depth = 2;
        let rows = rows_of(&outcome, &asked);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "nested");
    }

    /// A directory reports the worst of what it holds and the sum of its round
    /// trips: an average hides one terrible function behind twenty ordinary ones.
    #[test]
    fn a_directory_reports_the_worst_it_holds() {
        let mut outcome = measured(&[
            ("src/a.ts", "one", 0, 50, 1, 0),
            ("src/b.ts", "two", 0, 50, 3, 0),
        ]);
        carrying(&mut outcome, RuleId::QueryInLoop, "src/a.ts", 10);
        carrying(&mut outcome, RuleId::QueryInLoop, "src/b.ts", 10);
        let rows = rows_of(&outcome, &view(By::Dir, Sort::Weight));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["loops"], 3);
        assert_eq!(rows[0]["io"], 2);
        assert_eq!(rows[0]["functions"], 2);
    }

    #[test]
    fn the_order_label_reads_the_loops_and_the_recursion() {
        assert_eq!(order(0, 0), "O(1)");
        assert_eq!(order(1, 0), "O(n)");
        assert_eq!(order(2, 0), "O(n\u{b2})");
        assert_eq!(order(5, 0), "O(n\u{b3}+)");
        assert_eq!(order(1, 2), "exp", "recursion outranks the loop count");
    }
}

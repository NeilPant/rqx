//! How a function's cost was worked out, for `rqx perf --explain`: the loops and calls
//! the time is made of, where each loop's bound came from, what each callee costs and
//! which library entry priced each library call, and why the analysis is as sure of it
//! as it is.

use anyhow::{Result, bail};
use rqx_core::cost::{ArgSize, Bound, Conf, Cost, FunctionCost, Order, Shrink, Summary};
use rqx_lang::FunctionShape;
use rqx_lang::cost::compose::Reached;
use rqx_lang::cost::local::{Dimension, entry_order};
use rqx_lang::cost::sizes::Sizes;
use rqx_lang::cost::tables::CostTables;
use serde_json::{Value, json};
use std::sync::Arc;

use super::measure::{Measure, align};
use crate::scan::ScanOutcome;

/// How many functions deep the derivation follows a call. A cost that comes from further
/// down is still in the callee's own summary; the tree stops being readable first.
const CALL_DEPTH: usize = 3;

/// One part of the derivation: a loop, a call or a library call, with what is under it.
pub struct Step {
    kind: &'static str,
    text: String,
    order: Option<String>,
    children: Vec<Step>,
}

struct Section {
    dimension: &'static str,
    order: String,
    steps: Vec<Step>,
}

pub struct Explanation {
    name: String,
    at: String,
    time: String,
    space: String,
    io: String,
    stack: String,
    conf: Conf,
    recursion: Option<String>,
    sections: Vec<Section>,
    reasons: Vec<String>,
}

/// What a derivation reads of a function: the facts it was costed from, and its summary.
struct Place<'a> {
    file: &'a str,
    facts: &'a FunctionCost,
    summary: Option<&'a Summary>,
    measure: Measure,
}

#[derive(Clone, Copy)]
enum Dim {
    Time,
    Space,
    RoundTrips,
}

impl Dim {
    const ALL: [Self; 3] = [Self::Time, Self::Space, Self::RoundTrips];

    const fn label(self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Space => "space",
            Self::RoundTrips => "io",
        }
    }

    fn root(self, facts: &FunctionCost) -> &Cost {
        match self {
            Self::Time => &facts.time,
            Self::Space => &facts.space,
            Self::RoundTrips => &facts.ind.round_trips,
        }
    }

    const fn library(self) -> Dimension {
        match self {
            Self::Space => Dimension::Space,
            Self::Time | Self::RoundTrips => Dimension::Time,
        }
    }

    fn of(self, measure: &Measure) -> &Order {
        match self {
            Self::Time => &measure.time,
            Self::Space => &measure.space,
            Self::RoundTrips => &measure.io,
        }
    }
}

/// A function of the scan that `--explain` may be asking about.
struct Candidate<'a> {
    file: &'a str,
    function: &'a FunctionShape,
    facts: Option<&'a FunctionCost>,
}

impl Candidate<'_> {
    fn name(&self) -> String {
        self.facts
            .filter(|facts| facts.name.is_some())
            .map(|facts| facts.qual.clone())
            .or_else(|| self.function.name.clone())
            .unwrap_or_else(|| "(anonymous)".to_owned())
    }

    fn at(&self) -> String {
        format!("{}:{}", self.file, self.function.range.line)
    }

    fn answers_to(&self, target: &str) -> bool {
        if let Some((file, line)) = target.rsplit_once(':')
            && let Ok(line) = line.parse::<u32>()
        {
            return self.in_file(file)
                && (self.function.range.line == line
                    || self.facts.is_some_and(|facts| facts.line == line));
        }
        self.function.name.as_deref() == Some(target)
            || self.facts.is_some_and(|facts| facts.qual == target)
    }

    fn in_file(&self, file: &str) -> bool {
        self.file == file || self.file.ends_with(&format!("/{file}"))
    }
}

fn candidates(outcome: &ScanOutcome) -> Vec<Candidate<'_>> {
    outcome
        .shape_by_file
        .keys()
        .flat_map(|file| candidates_in(outcome, file))
        .collect()
}

/// The functions of one file, in the order they are written.
fn candidates_in<'a>(outcome: &'a ScanOutcome, file: &str) -> Vec<Candidate<'a>> {
    let Some((file, shape)) = outcome.shape_by_file.get_key_value(file) else {
        return Vec::new();
    };
    let aligned = outcome
        .cost_facts
        .as_ref()
        .and_then(|facts| facts.of_file(file))
        .map(|costs| align(shape, costs))
        .unwrap_or_default();
    let mut functions: Vec<&FunctionShape> = shape.functions.iter().collect();
    functions.sort_by_key(|function| (function.range.line, function.range.start));
    functions
        .into_iter()
        .map(|function| Candidate {
            file,
            function,
            facts: aligned.get(&function.range.start).copied(),
        })
        .collect()
}

/// The function `target` names: a name, `Class.method`, or `file:line`. More than one is
/// an error that lists them, because a derivation for the wrong one is worse than none.
fn resolve<'a>(outcome: &'a ScanOutcome, target: &str) -> Result<Candidate<'a>> {
    let mut matching: Vec<Candidate<'a>> = candidates(outcome)
        .into_iter()
        .filter(|candidate| candidate.answers_to(target))
        .collect();
    match matching.len() {
        0 => bail!(
            "no function matches `{target}`. Name it as `name`, `Class.method` or `file:line`."
        ),
        1 => Ok(matching.remove(0)),
        _ => {
            let listed: Vec<String> = matching
                .iter()
                .map(|candidate| format!("  {}  {}", candidate.name(), candidate.at()))
                .collect();
            bail!(
                "`{target}` matches {} functions; name one by `file:line`:\n{}",
                matching.len(),
                listed.join("\n")
            )
        }
    }
}

pub fn explain(outcome: &ScanOutcome, target: &str) -> Result<Explanation> {
    resolve(outcome, target).map(|candidate| derive(outcome, &candidate))
}

/// The derivation for the function of `file` whose name starts at `start`, which is
/// where a row of the performance table says it is.
pub fn explain_at(outcome: &ScanOutcome, file: &str, start: u32) -> Option<Explanation> {
    candidates_in(outcome, file)
        .into_iter()
        .find(|candidate| candidate.function.range.start == start)
        .map(|candidate| derive(outcome, &candidate))
}

fn derive(outcome: &ScanOutcome, candidate: &Candidate<'_>) -> Explanation {
    let facts = candidate.facts;
    let summary = facts.and_then(|facts| outcome.summaries.get(candidate.file, facts.start));
    let measure = Measure::of(summary, facts);

    let mut walk = Walk {
        outcome,
        tables: outcome
            .cost_facts
            .as_ref()
            .map_or_else(CostTables::built_in, |facts| Arc::clone(&facts.tables)),
        notes: Vec::new(),
    };
    let mut sections = Vec::new();
    let mut recursion = None;
    if let Some(facts) = facts {
        let place = Place {
            file: candidate.file,
            facts,
            summary,
            measure: measure.clone(),
        };
        for dim in Dim::ALL {
            let order = dim.of(&measure);
            if order.is_one() {
                continue;
            }
            let steps = walk.steps(&place, dim.root(facts), dim, 0, &mut vec![place.key()]);
            // Space and round trips that nothing in the body accounts for are the
            // recursion's, which says so above.
            if steps.is_empty() && !matches!(dim, Dim::Time) {
                continue;
            }
            sections.push(Section {
                dimension: dim.label(),
                order: measure.expression(order),
                steps,
            });
        }
        recursion = recursion_note(&place);
    }
    let reasons = walk.reasons(candidate, summary, outcome);
    Explanation {
        name: candidate.name(),
        at: candidate.at(),
        time: measure.expression(&measure.time),
        space: measure.expression(&measure.space),
        io: measure.expression(&measure.io),
        stack: summary.map_or_else(|| "O(1)".to_owned(), |s| measure.expression(&s.stack)),
        conf: measure.conf,
        recursion,
        sections,
        reasons,
    }
}

impl Place<'_> {
    fn key(&self) -> (String, u32) {
        (self.file.to_owned(), self.facts.start)
    }
}

/// The part of the derivation that reads the call graph and notes where it had to guess.
struct Walk<'a> {
    outcome: &'a ScanOutcome,
    tables: Arc<CostTables>,
    /// What lowered the confidence, in the order it was met.
    notes: Vec<String>,
}

impl<'a> Walk<'a> {
    fn steps(
        &mut self,
        place: &Place<'a>,
        cost: &Cost,
        dim: Dim,
        depth: usize,
        visiting: &mut Vec<(String, u32)>,
    ) -> Vec<Step> {
        match cost {
            Cost::One => Vec::new(),
            Cost::Seq(items) | Cost::Branch(items) => items
                .iter()
                .flat_map(|item| self.steps(place, item, dim, depth, visiting))
                .collect(),
            Cost::Loop { bound, body, line } => {
                let children = self.steps(place, body, dim, depth, visiting);
                if *bound == Bound::Const && children.is_empty() {
                    return Vec::new();
                }
                let round_trip = matches!(dim, Dim::RoundTrips) && children.is_empty();
                vec![Step {
                    kind: "loop",
                    text: format!(":{line} loop {}", self.bound_text(place, *bound, *line)),
                    order: round_trip.then(|| "one round trip on each pass".to_owned()),
                    children,
                }]
            }
            Cost::Call(index) => self.call(place, usize::from(*index), dim, depth, visiting),
            Cost::Lib {
                entry,
                size,
                line,
                arg,
            } => {
                let Some(found) = self.tables.entry(*entry) else {
                    return Vec::new();
                };
                let mut sizes = Sizes::new(&place.facts.vars);
                let (on, with) = (sizes.of(*size), sizes.of(*arg));
                let order = entry_order(found, dim.library(), &on, &with);
                if order.is_one() {
                    return Vec::new();
                }
                vec![Step {
                    kind: "library",
                    text: format!(":{line} `{}` on {}", found.id, argument_text(place, *size)),
                    order: Some(place.measure.expression(&order)),
                    children: Vec::new(),
                }]
            }
        }
    }

    fn bound_text(&mut self, place: &Place<'_>, bound: Bound, line: u32) -> String {
        match bound {
            Bound::Const => "runs a constant number of times".to_owned(),
            Bound::Size(id) => {
                let (name, source) = place.measure.size(id);
                format!("is bounded by the size of `{name}` ({source})")
            }
            Bound::Log(id) => {
                let (name, source) = place.measure.size(id);
                format!(
                    "halves or doubles its counter, so it runs log of the size of `{name}` times ({source})"
                )
            }
            Bound::Unknown => {
                self.note(format!(
                    "the bound of the loop at {}:{line} is not a size the analysis could read, so it is taken as a size of its own",
                    place.file
                ));
                "has a bound the analysis could not read".to_owned()
            }
        }
    }

    fn call(
        &mut self,
        place: &Place<'a>,
        index: usize,
        dim: Dim,
        depth: usize,
        visiting: &mut Vec<(String, u32)>,
    ) -> Vec<Step> {
        let Some(site) = place.facts.calls.get(index) else {
            return Vec::new();
        };
        let reached = self
            .outcome
            .cost_facts
            .as_ref()
            .map_or(&[][..], |facts| {
                facts.links.of_function(place.file, place.facts.start)
            })
            .get(index)
            .map(Vec::as_slice)
            .unwrap_or_default();
        reached
            .iter()
            .filter_map(|target| {
                let callee = self.place_of(target)?;
                // A function calling itself is the recursion note's to describe.
                if visiting.contains(&callee.key()) {
                    return None;
                }
                let summary = callee.summary?;
                let args: Vec<String> = site
                    .args
                    .iter()
                    .map(|arg| argument_text(place, *arg))
                    .collect();
                if target.guessed {
                    self.note(format!(
                        "the call to `{}` at {}:{} was matched by name among {} functions",
                        callee.facts.qual,
                        place.file,
                        site.line,
                        reached.len()
                    ));
                }
                if summary.conf < Conf::Verified {
                    self.note(format!(
                        "`{}` ({}:{}) is itself only {}",
                        callee.facts.qual,
                        callee.file,
                        callee.facts.line,
                        summary.conf.slug()
                    ));
                }
                let children = if depth < CALL_DEPTH {
                    visiting.push(callee.key());
                    let below =
                        self.steps(&callee, dim.root(callee.facts), dim, depth + 1, visiting);
                    visiting.pop();
                    below
                } else {
                    Vec::new()
                };
                Some(Step {
                    kind: "call",
                    text: format!(
                        ":{} calls `{}` ({}:{}) with {}{}",
                        site.line,
                        callee.facts.qual,
                        callee.file,
                        callee.facts.line,
                        if args.is_empty() {
                            "nothing".to_owned()
                        } else {
                            args.join(", ")
                        },
                        if target.guessed {
                            ", matched by name"
                        } else {
                            ""
                        }
                    ),
                    order: Some(callee.measure.expression(dim.of(&callee.measure))),
                    children,
                })
            })
            .collect()
    }

    fn place_of(&self, target: &Reached) -> Option<Place<'a>> {
        let outcome: &'a ScanOutcome = self.outcome;
        let kept = outcome.cost_facts.as_ref()?;
        let file = kept
            .costs
            .iter()
            .find(|file| *file.name == *target.file)
            .map(|file| &*file.name)?;
        let facts = kept.function(file, target.start)?;
        let summary = outcome.summaries.get(file, target.start);
        Some(Place {
            file,
            facts,
            summary,
            measure: Measure::of(summary, Some(facts)),
        })
    }

    fn note(&mut self, text: String) {
        if !self.notes.contains(&text) {
            self.notes.push(text);
        }
    }

    /// Why the analysis is as sure as it is: what it met while walking, then what its
    /// own cost was read from, then how much of the project it saw.
    fn reasons(
        &mut self,
        candidate: &Candidate<'_>,
        summary: Option<&Summary>,
        outcome: &ScanOutcome,
    ) -> Vec<String> {
        let mut reasons = std::mem::take(&mut self.notes);
        let own = candidate.facts.map_or(Conf::Verified, |facts| facts.conf);
        if own < Conf::Verified && reasons.is_empty() {
            reasons.push(
                "part of the body's own cost was read from names, such as a method name whose receiver has no stated type"
                    .to_owned(),
            );
        }
        if summary.is_some_and(|summary| {
            summary
                .trace
                .iter()
                .any(|step| step.what == "mutual recursion")
        }) {
            reasons.push(
                "functions that call one another in a circle are costed together, one pass of the circle per level of depth, and how deep it goes is not known"
                    .to_owned(),
            );
        }
        if !outcome.summaries.complete {
            reasons.push(
                "the scan did not read the whole project, and a call into a file it did not read counts as constant"
                    .to_owned(),
            );
        }
        if reasons.is_empty() {
            reasons.push("every bound and library cost was read from the source".to_owned());
        }
        reasons
    }
}

fn argument_text(place: &Place<'_>, size: ArgSize) -> String {
    match size {
        ArgSize::Param(position) => place.facts.params.get(usize::from(position)).map_or_else(
            || "an argument".to_owned(),
            |param| format!("`{}`", param.name),
        ),
        ArgSize::Local(id) => format!("`{}`", place.measure.size(id).0),
        ArgSize::Const => "a constant".to_owned(),
        ArgSize::Unknown => "something of unknown size".to_owned(),
    }
}

fn recursion_note(place: &Place<'_>) -> Option<String> {
    let rec = place.facts.rec.as_ref()?;
    let shrink = match rec.shrink {
        Shrink::Dec1 => "one smaller",
        Shrink::Half => "half as large",
        Shrink::Other => "changed in a way the analysis did not read",
    };
    let argument = place.facts.params.get(usize::from(rec.param)).map_or_else(
        || "its argument".to_owned(),
        |param| format!("`{}`", param.name),
    );
    let mut note = format!(
        "calls itself {} {} per call, with {argument} {shrink}",
        rec.calls,
        if rec.calls == 1 { "time" } else { "times" }
    );
    if rec.copies_arg {
        note.push_str(", copying it each time");
    }
    if rec.memo {
        note.push_str(", and reads a cache before recursing");
    }
    if let Some(summary) = place.summary
        && !summary.stack.is_one()
    {
        let stack = place.measure.expression(&summary.stack);
        note.push_str(&format!(", so the stack grows to {stack}"));
    }
    Some(note)
}

impl Explanation {
    /// The explanation as plain text, one string per line.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("{}  {}", self.name, self.at),
            format!(
                "  time {}  space {}  io {}  stack {}  confidence {}",
                self.time,
                self.space,
                self.io,
                self.stack,
                self.conf.slug()
            ),
        ];
        if let Some(recursion) = &self.recursion {
            lines.push(String::new());
            lines.push(format!("recursion: {recursion}"));
        }
        for section in &self.sections {
            lines.push(String::new());
            lines.push(format!("{} {}", section.dimension, section.order));
            push_tree(&section.steps, "", &mut lines);
        }
        lines.push(String::new());
        lines.push(format!("confidence {}", self.conf.slug()));
        lines.extend(self.reasons.iter().map(|reason| format!("  - {reason}")));
        lines
    }

    pub fn json(&self) -> Value {
        json!({
            "name": self.name,
            "at": self.at,
            "time": self.time,
            "space": self.space,
            "io_order": self.io,
            "stack": self.stack,
            "confidence": self.conf.slug(),
            "recursion": self.recursion,
            "derivation": self.sections.iter().map(|section| json!({
                "dimension": section.dimension,
                "order": section.order,
                "steps": section.steps.iter().map(Step::json).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "reasons": self.reasons,
        })
    }
}

impl Step {
    fn json(&self) -> Value {
        json!({
            "kind": self.kind,
            "what": self.text,
            "order": self.order,
            "children": self.children.iter().map(Self::json).collect::<Vec<_>>(),
        })
    }

    fn line(&self) -> String {
        match &self.order {
            Some(order) => format!("{}  \u{2192} {order}", self.text),
            None => self.text.clone(),
        }
    }
}

fn push_tree(steps: &[Step], prefix: &str, lines: &mut Vec<String>) {
    for (position, step) in steps.iter().enumerate() {
        let last = position + 1 == steps.len();
        let (branch, carry) = if last {
            ("\u{2514}\u{2500} ", "   ")
        } else {
            ("\u{251c}\u{2500} ", "\u{2502}  ")
        };
        lines.push(format!("{prefix}{branch}{}", step.line()));
        push_tree(&step.children, &format!("{prefix}{carry}"), lines);
    }
}

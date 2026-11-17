//! What the cost engine says about one function, in the form `rqx perf` reports it: the
//! orders it composed, how sure it is, and the names the sizes in them stand for.

use std::collections::{BTreeSet, HashMap};

use rqx_core::cost::{
    Conf, FileCost, FunctionCost, Order, SizeId, SizeSrc, SizeVar, Summary, TraceStep,
    standard_name,
};
use rqx_lang::FileShape;
use serde_json::{Value, json};

use crate::scan::ScanOutcome;

/// How many functions on either side of a name are tried as the cost facts of a function.
const NEIGHBOURS: usize = 4;

/// The part of a [`Measure`] that only a single function has: the names its sizes were
/// written with and the path through its callees that made its time what it is.
#[derive(Clone, Debug, Default)]
pub struct Detail {
    vars: Vec<SizeVar>,
    trace: Vec<TraceStep>,
}

#[derive(Clone, Debug, Default)]
pub struct Measure {
    pub time: Order,
    pub space: Order,
    pub io: Order,
    /// Allocations the body makes on each pass of its innermost loop.
    pub alloc: u8,
    pub conf: Conf,
    /// Absent for a row that stands for several functions.
    detail: Option<Detail>,
}

/// One variable of a function's cost, as the report names it.
pub struct Variable {
    pub id: String,
    pub name: String,
    pub source: &'static str,
}

impl Measure {
    /// A function the extraction left out is plainly constant, which is an answer and not
    /// a gap.
    pub fn of(summary: Option<&Summary>, facts: Option<&FunctionCost>) -> Self {
        let Some(summary) = summary else {
            return Self {
                detail: Some(Detail::default()),
                ..Self::default()
            };
        };
        let mut vars = facts.map(|facts| facts.vars.clone()).unwrap_or_default();
        if let Some(facts) = facts {
            name_the_depth(&mut vars, facts, summary);
        }
        Self {
            time: summary.time.clone(),
            space: summary.space.clone(),
            io: summary.io.clone(),
            alloc: facts.map_or(0, |facts| facts.ind.allocs_per_iter),
            conf: summary.conf,
            detail: Some(Detail {
                vars,
                trace: summary.trace.clone(),
            }),
        }
    }

    /// What a row for several functions says: the heaviest of each order, and the least
    /// sure of the confidences. Sizes of different functions have nothing in common, so no
    /// names or trace are kept.
    pub fn worst(measures: impl IntoIterator<Item = Self>) -> Self {
        let mut worst = Self::default();
        for measure in measures {
            worst.time = heavier(worst.time, measure.time);
            worst.space = heavier(worst.space, measure.space);
            worst.io = heavier(worst.io, measure.io);
            worst.alloc = worst.alloc.max(measure.alloc);
            worst.conf = worst.conf.weaker(measure.conf);
        }
        worst
    }

    /// `order` written with the names of this function's sizes: `O(orders·prices)`.
    pub fn expression(&self, order: &Order) -> String {
        let vars = self.vars();
        order.named(&|id| name_of(vars, id)).to_string()
    }

    pub fn trace(&self) -> &[TraceStep] {
        self.detail
            .as_ref()
            .map_or(&[], |detail| detail.trace.as_slice())
    }

    /// The sizes the orders are written in, in the order of their letters.
    pub fn variables(&self) -> Vec<Variable> {
        let vars = self.vars();
        [&self.time, &self.space, &self.io]
            .into_iter()
            .flat_map(|order| order.terms())
            .flat_map(|term| term.factors())
            .map(|(id, _, _)| *id)
            .collect::<BTreeSet<SizeId>>()
            .into_iter()
            .map(|id| Variable {
                id: standard_name(id),
                name: name_of(vars, id),
                source: vars
                    .get(usize::from(id))
                    .map_or("not named", |var| source_of(&var.src)),
            })
            .collect()
    }

    fn vars(&self) -> &[SizeVar] {
        self.detail
            .as_ref()
            .map_or(&[], |detail| detail.vars.as_slice())
    }

    /// The keys a row of the JSON output gains, which are the same in `rqx perf --json`
    /// and the MCP tool. `io_order` is not `io`: that key has always been the count of
    /// round trips found inside loops.
    pub fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("time", json!(self.time.to_string())),
            ("space", json!(self.space.to_string())),
            ("io_order", json!(self.io.to_string())),
            ("alloc", json!(self.alloc)),
            ("confidence", json!(self.conf.slug())),
            ("cost", self.cost_json()),
        ]
    }

    fn cost_json(&self) -> Value {
        if self.detail.is_none() {
            return Value::Null;
        }
        json!({
            "expr": self.expression(&self.time),
            "trace": self.trace().iter().map(|step| json!({
                "file": step.file,
                "line": step.line,
                "what": step.what,
                "order": step.order.to_string(),
            })).collect::<Vec<_>>(),
            "vars": self.variables().iter().map(|variable| json!({
                "id": variable.id,
                "name": variable.name,
                "source": variable.source,
            })).collect::<Vec<_>>(),
        })
    }
}

fn heavier(left: Order, right: Order) -> Order {
    if right.rank() > left.rank() {
        right
    } else {
        left
    }
}

/// A recursion is measured in the parameter that shrinks, which the function's own sizes
/// leave out when its body never measures that parameter. The composition adds it as the
/// next variable, and the orders say so.
fn name_the_depth(vars: &mut Vec<SizeVar>, facts: &FunctionCost, summary: &Summary) {
    let next = SizeId::try_from(vars.len()).unwrap_or(SizeId::MAX);
    let used = |order: &Order| {
        order
            .terms()
            .iter()
            .flat_map(|term| term.factors())
            .any(|(id, _, _)| *id == next)
    };
    let mentioned = [&summary.time, &summary.space, &summary.io, &summary.stack]
        .into_iter()
        .any(used);
    let position = facts.rec.as_ref().map_or(0, |rec| rec.param);
    if let (true, Some(param)) = (mentioned, facts.params.get(usize::from(position))) {
        vars.push(SizeVar {
            name: param.name.clone(),
            src: SizeSrc::Param(position),
        });
    }
}

pub fn name_of(vars: &[SizeVar], id: SizeId) -> String {
    vars.get(usize::from(id))
        .map_or_else(|| format!("?{id}"), |var| var.name.clone())
}

const fn source_of(src: &SizeSrc) -> &'static str {
    match src {
        SizeSrc::Param(_) => "parameter",
        SizeSrc::Local => "local value",
        SizeSrc::Field => "field",
        SizeSrc::Global => "module-level value",
        SizeSrc::Unknown => "unknown",
    }
}

/// The cost facts of each function of a file, by where the function's name starts.
///
/// The shape of a function reports where its name is and the cost facts where the
/// function begins, and the two disagree for a function assigned to a name that comes
/// before it. So each shape takes the nearest facts, the ones with the same name first,
/// and facts belong to one shape only: a constant function the extraction left out then
/// cannot take the facts of the function beside it.
pub fn align<'a>(shape: &FileShape, costs: &'a FileCost) -> HashMap<u32, &'a FunctionCost> {
    let mut sorted: Vec<&FunctionCost> = costs.functions.iter().collect();
    sorted.sort_by_key(|function| function.start);

    let mut pairs: Vec<(bool, u32, u32, usize)> = Vec::new();
    for function in &shape.functions {
        let at = function.range.start;
        let after = sorted.partition_point(|facts| facts.start < at);
        let from = after.saturating_sub(NEIGHBOURS);
        let to = (after + NEIGHBOURS).min(sorted.len());
        for (position, facts) in sorted.iter().enumerate().take(to).skip(from) {
            let renamed = function.name != facts.name;
            pairs.push((renamed, facts.start.abs_diff(at), at, position));
        }
    }
    pairs.sort_unstable();

    let mut taken = vec![false; sorted.len()];
    let mut aligned = HashMap::new();
    for (_, _, at, position) in pairs {
        if taken[position] || aligned.contains_key(&at) {
            continue;
        }
        taken[position] = true;
        aligned.insert(at, sorted[position]);
    }
    aligned
}

/// The summary and facts of the function of `file` whose name starts at `at`.
pub fn measure_of(
    outcome: &ScanOutcome,
    file: &str,
    aligned: &HashMap<u32, &FunctionCost>,
    at: u32,
) -> Measure {
    let facts = aligned.get(&at).copied();
    let summary = facts.and_then(|facts| outcome.summaries.get(file, facts.start));
    Measure::of(summary, facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quadratic() -> Summary {
        Summary {
            time: "O(n·m)".parse().unwrap(),
            conf: Conf::Heuristic,
            ..Summary::default()
        }
    }

    fn facts(names: &[&str]) -> FunctionCost {
        FunctionCost {
            vars: names
                .iter()
                .enumerate()
                .map(|(position, name)| SizeVar {
                    name: (*name).to_owned(),
                    src: SizeSrc::Param(position as u8),
                })
                .collect(),
            ..FunctionCost::default()
        }
    }

    fn shape_of(functions: &[(&str, u32)]) -> FileShape {
        let mut shape = FileShape::default();
        for (name, start) in functions {
            shape.functions.push(rqx_lang::FunctionShape {
                name: Some((*name).to_owned()),
                range: rqx_core::finding::SourceRange {
                    file: std::sync::Arc::from("a.ts"),
                    start: *start,
                    end: start + 1,
                    line: 1,
                    column: 1,
                    end_line: 1,
                },
                end: start + 30,
                last_line: 2,
                parameters: 0,
                statements: 1,
                cognitive_complexity: 0,
                max_nesting: 0,
                loop_depth: 0,
                self_calls: 0,
                body: None,
            });
        }
        shape
    }

    fn costing(name: &str, start: u32) -> FunctionCost {
        FunctionCost {
            name: Some(name.to_owned()),
            qual: name.to_owned(),
            start,
            ..FunctionCost::default()
        }
    }

    #[test]
    fn facts_start_where_the_function_does_and_the_shape_where_its_name_does() {
        let shape = shape_of(&[("walk", 20), ("pad", 120)]);
        let costs = FileCost {
            functions: vec![costing("walk", 12)],
        };
        let aligned = align(&shape, &costs);
        assert_eq!(aligned.get(&20).map(|facts| facts.start), Some(12));
        assert!(
            !aligned.contains_key(&120),
            "a constant function the extraction left out takes nobody else's facts"
        );
    }

    #[test]
    fn a_function_assigned_to_a_name_before_it_is_still_found() {
        let shape = shape_of(&[("walk", 20)]);
        let costs = FileCost {
            functions: vec![costing("walk", 27)],
        };
        assert_eq!(
            align(&shape, &costs).get(&20).map(|facts| facts.start),
            Some(27)
        );
    }

    #[test]
    fn facts_go_to_the_function_with_the_same_name_when_two_are_close() {
        let shape = shape_of(&[("first", 20), ("second", 40)]);
        let costs = FileCost {
            functions: vec![costing("second", 30)],
        };
        let aligned = align(&shape, &costs);
        assert_eq!(aligned.get(&40).map(|facts| facts.start), Some(30));
        assert!(!aligned.contains_key(&20));
    }

    #[test]
    fn a_function_without_a_summary_is_constant_and_sure() {
        let measure = Measure::of(None, None);
        assert!(measure.time.is_one() && measure.space.is_one() && measure.io.is_one());
        assert_eq!(measure.conf, Conf::Verified);
    }

    #[test]
    fn sizes_are_written_with_the_names_the_function_gave_them() {
        let measure = Measure::of(Some(&quadratic()), Some(&facts(&["orders", "prices"])));
        assert_eq!(measure.expression(&measure.time), "O(orders·prices)");
        let variables = measure.variables();
        let named: Vec<(&str, &str, &str)> = variables
            .iter()
            .map(|v| (v.id.as_str(), v.name.as_str(), v.source))
            .collect();
        assert_eq!(
            named,
            [("n", "orders", "parameter"), ("m", "prices", "parameter")]
        );
    }

    #[test]
    fn a_size_the_function_never_named_is_marked_rather_than_invented() {
        let measure = Measure::of(Some(&quadratic()), Some(&facts(&["orders"])));
        assert_eq!(measure.expression(&measure.time), "O(orders·?1)");
    }

    #[test]
    fn a_row_for_several_functions_keeps_the_heaviest_order_and_the_least_sure() {
        let linear = Summary {
            time: "O(n)".parse().unwrap(),
            ..Summary::default()
        };
        let worst = Measure::worst([
            Measure::of(Some(&linear), None),
            Measure::of(Some(&quadratic()), None),
        ]);
        assert_eq!(worst.time.to_string(), "O(n·m)");
        assert_eq!(worst.conf, Conf::Heuristic);
        assert!(
            worst
                .json_fields()
                .iter()
                .any(|(key, value)| *key == "cost" && value.is_null())
        );
    }

    #[test]
    fn the_new_fields_leave_the_existing_keys_alone() {
        let keys: Vec<&str> = Measure::default()
            .json_fields()
            .iter()
            .map(|(key, _)| *key)
            .collect();
        for taken in [
            "name", "at", "order", "loops", "io", "reach", "weight", "sampled",
        ] {
            assert!(!keys.contains(&taken), "{taken} is already a key of a row");
        }
    }
}

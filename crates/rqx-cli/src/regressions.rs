//! Functions that cost more after a change than before it. The head's summaries are the
//! scan's own. The base's are composed over the same project with the files the change
//! touched read as they were at the base revision, so a caller nobody edited is compared
//! against what its callee cost then, and nothing is scanned or checked out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rqx_core::config::RegressionGate;
use rqx_core::cost::{Conf, FileCost, FunctionCost, Order, Summaries, TraceStep};
use rqx_lang::module_graph::ModuleEntry;

use crate::config::Resolved;
use crate::scan::{CostInputs, ScanOutcome};

const NO_BASE: &str = "there is no base revision to compare with";
const INCOMPLETE: &str = "the scan did not read the whole project, so a callee in a file it skipped would be costed as constant";
const NO_FACTS: &str = "the scan kept no cost facts to compare";

/// What the function is called when its source gives it no name, which says nothing
/// about which function it is.
const ANONYMOUS: &str = "<anonymous>";

/// One function whose time order grew. The function is the one in the changed tree; the
/// file need not be one the change touched, because a callee that got slower slows the
/// callers that nobody edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Regression {
    pub file: String,
    /// The name qualified by its class, which with the file identifies the function.
    pub qual: String,
    pub line: u32,
    pub before: Order,
    pub after: Order,
    /// How the function gets to `after`, from its loop or call down to the cost.
    pub trace: Vec<TraceStep>,
}

/// What a review found out about complexity, and how loudly the gate says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Regressions {
    /// `off` when the check ran because one run asked for it: the regressions are
    /// reported and cost nothing.
    pub gate: RegressionGate,
    pub state: State,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Found(Vec<Regression>),
    /// Why there is nothing to report: the comparison could not be made.
    Skipped(&'static str),
}

impl Regressions {
    pub fn found(&self) -> &[Regression] {
        match &self.state {
            State::Found(found) => found,
            State::Skipped(_) => &[],
        }
    }

    pub fn skipped(&self) -> Option<&'static str> {
        match &self.state {
            State::Skipped(reason) => Some(reason),
            State::Found(_) => None,
        }
    }

    /// A review of listed or staged files names no revision to compare with.
    pub fn without_base(gate: RegressionGate) -> Self {
        Self {
            gate,
            state: State::Skipped(NO_BASE),
        }
    }

    /// `head` against the same project with `held` in place of the changed files, where
    /// `held` is what the files the change touched contained at the base revision.
    pub fn between(
        head: &ScanOutcome,
        held: &BTreeMap<String, String>,
        changed: &BTreeSet<String>,
        root: &Path,
        settings: &Resolved,
    ) -> Self {
        let gate = settings.config.gate.complexity_regression;
        let state = match (&head.cost_inputs, head.summaries.complete) {
            (None, _) => State::Skipped(NO_FACTS),
            (Some(_), false) => State::Skipped(INCOMPLETE),
            (Some(inputs), true) => State::Found(regressions(
                head,
                inputs,
                &BaseFiles::read(held, changed, inputs, settings),
                root,
                settings,
            )),
        };
        Self { gate, state }
    }
}

/// Whether `order` grew past `limit`: it dominates it, or it ranks higher where the two
/// cannot be compared.
pub fn exceeds(order: &Order, limit: &Order) -> bool {
    order != limit && (limit.max(order) == *order || order.rank() > limit.rank())
}

/// A changed file as it was at the base revision.
struct BaseFile {
    name: String,
    module: ModuleEntry,
    cost: FileCost,
    /// The names that exactly one function of the file bears, which is what tells a
    /// function that was constant time from one that did not exist.
    sole_names: BTreeSet<String>,
}

/// The changed files that existed at the base revision, read as they were there.
struct BaseFiles {
    files: Vec<BaseFile>,
    /// Every file the change touched, whether or not it existed at the base.
    touched: BTreeSet<String>,
}

impl BaseFiles {
    fn read(
        held: &BTreeMap<String, String>,
        changed: &BTreeSet<String>,
        inputs: &CostInputs,
        settings: &Resolved,
    ) -> Self {
        let paths: BTreeMap<&str, &Path> = inputs
            .files()
            .map(|file| (file.name, file.module.path.as_path()))
            .collect();
        let files = held
            .iter()
            .filter_map(|(name, text)| {
                let path = paths.get(name.as_str())?;
                analysed_at_base(name, path, text, settings)
            })
            .collect();
        Self {
            files,
            touched: changed.iter().cloned().collect(),
        }
    }

    fn get(&self, name: &str) -> Option<&BaseFile> {
        self.files.iter().find(|file| file.name == name)
    }
}

fn analysed_at_base(name: &str, path: &Path, text: &str, settings: &Resolved) -> Option<BaseFile> {
    let config = settings.for_file(path);
    let analysis = rqx_lang::analyze_with(path, name, text, config, &settings.cost_tables).ok()?;
    Some(BaseFile {
        name: name.to_owned(),
        sole_names: sole_names(
            analysis
                .shape
                .functions
                .iter()
                .filter_map(|f| f.name.as_deref()),
        ),
        module: ModuleEntry {
            path: path.to_path_buf(),
            language: analysis.language,
            facts: analysis.module,
        },
        cost: analysis.cost,
    })
}

fn sole_names<'a>(names: impl Iterator<Item = &'a str>) -> BTreeSet<String> {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for name in names {
        *counts.entry(name).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|(name, _)| name.to_owned())
        .collect()
}

fn regressions(
    head: &ScanOutcome,
    inputs: &CostInputs,
    base: &BaseFiles,
    root: &Path,
    settings: &Resolved,
) -> Vec<Regression> {
    if base.touched.is_empty() {
        return Vec::new();
    }
    let before = base_summaries(inputs, base, root, settings);
    let mut found = Vec::new();
    for file in inputs.files() {
        let was = base.get(file.name);
        if base.touched.contains(file.name) && was.is_none() {
            continue;
        }
        let sole_in_head = head_sole_names(head, file.name);
        let against = Against {
            name: file.name,
            was: was.map(|was| (&was.cost, &was.sole_names)),
            sole_in_head: &sole_in_head,
            head: &head.summaries,
            before: &before,
        };
        found.extend(
            unique_by_qual(&file.cost.functions)
                .into_iter()
                .filter_map(|function| against.regression(function)),
        );
    }
    found.sort_by(|left, right| {
        (&left.file, left.line, &left.qual).cmp(&(&right.file, right.line, &right.qual))
    });
    found
}

/// Every function of the project's summary, with the changed files as they were at the
/// base revision and a file the change added left out.
fn base_summaries(
    inputs: &CostInputs,
    base: &BaseFiles,
    root: &Path,
    settings: &Resolved,
) -> Summaries {
    let files: Vec<rqx_lang::cost::compose::Analysed<'_>> = inputs
        .files()
        .filter_map(|file| match base.get(file.name) {
            Some(was) => Some(rqx_lang::cost::compose::Analysed {
                name: file.name,
                module: &was.module,
                cost: &was.cost,
            }),
            None if base.touched.contains(file.name) => None,
            None => Some(file),
        })
        .collect();
    rqx_lang::cost::compose::summarise(&files, root, &settings.cost_tables, true)
}

fn head_sole_names(head: &ScanOutcome, file: &str) -> BTreeSet<String> {
    head.shape_by_file
        .get(file)
        .map(|shape| {
            sole_names(
                shape
                    .functions
                    .iter()
                    .filter_map(|function| function.name.as_deref()),
            )
        })
        .unwrap_or_default()
}

/// The functions a qualified name picks out alone. Two functions that share one, such as
/// overloads, cannot be told apart from the other tree, so neither is compared.
fn unique_by_qual(functions: &[FunctionCost]) -> Vec<&FunctionCost> {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for function in functions {
        *counts.entry(function.qual.as_str()).or_default() += 1;
    }
    functions
        .iter()
        .filter(|function| function.qual != ANONYMOUS && counts[function.qual.as_str()] == 1)
        .collect()
}

/// What a function of one file is compared against.
struct Against<'a> {
    name: &'a str,
    /// The file at the base revision, when it is one the change touched.
    was: Option<(&'a FileCost, &'a BTreeSet<String>)>,
    sole_in_head: &'a BTreeSet<String>,
    head: &'a Summaries,
    before: &'a Summaries,
}

impl Against<'_> {
    fn regression(&self, function: &FunctionCost) -> Option<Regression> {
        let after = self.head.get(self.name, function.start)?;
        if after.conf == Conf::Unknown {
            return None;
        }
        let before = self.time_before(function)?;
        exceeds(&after.time, &before).then(|| Regression {
            file: self.name.to_owned(),
            qual: function.qual.clone(),
            line: function.line,
            before,
            after: after.time.clone(),
            trace: after.trace.clone(),
        })
    }

    /// What the function cost at the base revision, or `None` when it did not exist
    /// there. A file that was not touched holds the same function with the same
    /// facts; a touched one is looked up by qualified name, and a function the cost
    /// facts leave out was constant time if the file had exactly one function of that
    /// name then and has exactly one now.
    fn time_before(&self, function: &FunctionCost) -> Option<Order> {
        let Some((cost, sole_names)) = self.was else {
            let start = function.start;
            return self.before.get(self.name, start).map(|b| b.time.clone());
        };
        let known: Vec<&FunctionCost> = cost
            .functions
            .iter()
            .filter(|was| was.qual == function.qual)
            .collect();
        match known.as_slice() {
            [was] => self
                .before
                .get(self.name, was.start)
                .map(|b| b.time.clone()),
            [] => self
                .was_constant(function, cost, sole_names)
                .then(Order::one),
            _ => None,
        }
    }

    fn was_constant(
        &self,
        function: &FunctionCost,
        cost: &FileCost,
        sole_names: &BTreeSet<String>,
    ) -> bool {
        let Some(name) = function.name.as_deref() else {
            return false;
        };
        sole_names.contains(name)
            && self.sole_in_head.contains(name)
            && cost
                .functions
                .iter()
                .all(|was| was.name.as_deref() != Some(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(text: &str) -> Order {
        text.parse().expect("an order")
    }

    #[test]
    fn an_order_exceeds_the_ones_it_dominates() {
        assert!(exceeds(&order("O(n^2)"), &order("O(n)")));
        assert!(exceeds(&order("O(n)"), &order("O(1)")));
        assert!(exceeds(&order("exp"), &order("O(n^3)")));
        assert!(exceeds(&order("O(n log n)"), &order("O(n)")));
    }

    #[test]
    fn an_order_does_not_exceed_itself_or_a_larger_one() {
        assert!(!exceeds(&order("O(n)"), &order("O(n)")));
        assert!(!exceeds(&order("O(n)"), &order("O(n^2)")));
        assert!(!exceeds(&order("O(1)"), &order("O(log n)")));
    }

    #[test]
    fn a_scan_that_did_not_read_the_whole_project_compares_nothing() {
        let mut head = crate::report::tests::outcome_with(&[], 1);
        head.cost_inputs = Some(CostInputs {
            modules: Vec::new(),
            costs: Vec::new(),
        });
        head.summaries = Summaries::new(false);
        let settings = Resolved::of(rqx_core::config::RqxConfig::default());

        let compared = Regressions::between(
            &head,
            &BTreeMap::new(),
            &BTreeSet::new(),
            Path::new("."),
            &settings,
        );

        assert_eq!(compared.skipped(), Some(INCOMPLETE));
        assert!(compared.found().is_empty());
    }

    #[test]
    fn a_scan_that_kept_no_cost_facts_compares_nothing() {
        let head = crate::report::tests::outcome_with(&[], 1);
        let settings = Resolved::of(rqx_core::config::RqxConfig::default());

        let compared = Regressions::between(
            &head,
            &BTreeMap::new(),
            &BTreeSet::new(),
            Path::new("."),
            &settings,
        );

        assert_eq!(compared.skipped(), Some(NO_FACTS));
    }

    #[test]
    fn only_names_borne_once_are_sole() {
        let sole = sole_names(["a", "b", "a", "c"].into_iter());
        assert_eq!(sole.into_iter().collect::<Vec<_>>(), ["b", "c"]);
    }
}

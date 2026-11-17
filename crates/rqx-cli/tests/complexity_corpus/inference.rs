//! The composed inference run over the corpus, and what it gets right. A function on its
//! own in a file is analysed with that file alone; the files of a project are analysed
//! together, which is what lets a call in one reach a function in another.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rqx_core::config::RqxConfig;
use rqx_core::cost::{Conf, FileCost, Order, SizeId, Summaries, Summary};
use rqx_lang::cost::compose::{Analysed, summarise};
use rqx_lang::cost::tables::CostTables;
use rqx_lang::module_graph::ModuleEntry;

use super::baseline::Outcome;
use super::corpus::{self, Corpus, Function};

/// Everything the inference said about the corpus.
pub struct Inference {
    /// The extraction of each corpus file, by its path under the corpus root.
    extracted: BTreeMap<String, FileCost>,
    /// The composition of each group of files.
    composed: BTreeMap<String, Summaries>,
}

struct Read {
    name: String,
    module: ModuleEntry,
    cost: FileCost,
}

impl Inference {
    pub fn of(corpus: &Corpus) -> Self {
        let groups: BTreeSet<&str> = corpus
            .functions
            .iter()
            .map(|function| function.group.as_str())
            .collect();
        let tables = CostTables::built_in();
        let mut inference = Self {
            extracted: BTreeMap::new(),
            composed: BTreeMap::new(),
        };
        for group in groups {
            let analysed: Vec<Read> = corpus::files_of(&corpus::root(), group)
                .iter()
                .map(|path| analyse(path))
                .collect();
            let files: Vec<Analysed<'_>> = analysed
                .iter()
                .map(|file| Analysed {
                    name: &file.name,
                    module: &file.module,
                    cost: &file.cost,
                })
                .collect();
            let directory = corpus::root().join(group);
            let root = if directory.is_dir() {
                directory
            } else {
                directory.parent().unwrap_or(&directory).to_path_buf()
            };
            let summaries = summarise(&files, &root, &tables, true);
            for file in &analysed {
                inference
                    .extracted
                    .insert(file.name.clone(), file.cost.clone());
            }
            inference.composed.insert(group.to_owned(), summaries);
        }
        inference
    }

    /// The extraction of a function's file, for the checks that hold it to its own time.
    pub fn extraction(&self, file: &str) -> &FileCost {
        &self.extracted[file]
    }

    /// What the inference answered for the function, if it recorded one. A function the
    /// extraction left out has no loop, calls nothing and has nothing to say, and that is
    /// an answer of its own: constant.
    fn summary_of(&self, function: &Function) -> Option<&Summary> {
        let start = self.extracted[&function.file]
            .functions
            .iter()
            .find(|found| {
                found.line as usize == function.annotation.target_line
                    && function
                        .name
                        .as_ref()
                        .is_none_or(|name| found.name.as_ref() == Some(name))
            })?
            .start;
        self.composed[&function.group].get(&function.file, start)
    }

    pub fn outcome(&self, function: &Function) -> Outcome {
        let constant = (Order::one(), Order::one(), Conf::Verified);
        let (time, space, conf) = self.summary_of(function).map_or(constant, |summary| {
            (summary.time.clone(), summary.space.clone(), summary.conf)
        });
        if conf == Conf::Unknown {
            return Outcome::Abstained;
        }
        classify(
            [&time, &space],
            [&function.annotation.time, &function.annotation.space],
        )
    }

    /// A line for each function that is not exact, for whoever is improving the inference.
    pub fn explain(&self, function: &Function, id: &str) -> String {
        let found = self.summary_of(function).map_or_else(
            || "O(1), O(1) (nothing recorded)".to_owned(),
            |summary| {
                format!(
                    "{}, {} ({})",
                    summary.time,
                    summary.space,
                    summary.conf.slug()
                )
            },
        );
        format!(
            "{id}: inferred {found}; annotated {}, {}",
            function.annotation.time, function.annotation.space
        )
    }
}

fn analyse(path: &Path) -> Read {
    let root = corpus::root();
    let name = path
        .strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let source = std::fs::read_to_string(path).expect("a corpus file reads");
    let analysis = rqx_lang::analyze(path, &name, &source, &RqxConfig::default())
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    let canonical: PathBuf = path.canonicalize().expect("a corpus file exists");
    Read {
        name,
        module: ModuleEntry {
            path: canonical,
            language: analysis.language,
            facts: analysis.module,
        },
        cost: analysis.cost,
    }
}

/// How the inferred orders compare with the annotated ones. The variables are numbered
/// by where the extraction first meets them, so two orders are the same when some
/// renaming of the variables makes them equal.
fn classify(found: [&Order; 2], expected: [&Order; 2]) -> Outcome {
    if same_up_to_naming(found, expected) {
        return Outcome::Exact;
    }
    let smaller = (0..2).any(|at| found[at].max(expected[at]) != *found[at]);
    if smaller {
        Outcome::Under
    } else {
        Outcome::Over
    }
}

fn same_up_to_naming(found: [&Order; 2], expected: [&Order; 2]) -> bool {
    if found == expected {
        return true;
    }
    let (have, want) = (variables(found), variables(expected));
    if have.len() != want.len() || have.len() > 6 {
        return false;
    }
    permutations(&want)
        .into_iter()
        .any(|renaming| (0..2).all(|at| renamed(found[at], &have, &renaming) == *expected[at]))
}

fn variables(orders: [&Order; 2]) -> Vec<SizeId> {
    orders
        .iter()
        .flat_map(|order| order.terms())
        .flat_map(|term| term.factors())
        .map(|(variable, _, _)| *variable)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// `order` with each of `from` replaced by the variable at the same position in `to`.
fn renamed(order: &Order, from: &[SizeId], to: &[SizeId]) -> Order {
    const TEMPORARY: SizeId = SizeId::MAX / 2;
    let mut moved = order.clone();
    for (at, variable) in from.iter().enumerate() {
        moved = moved.subst(*variable, &Order::var(TEMPORARY + at as SizeId));
    }
    for (at, variable) in to.iter().enumerate() {
        moved = moved.subst(TEMPORARY + at as SizeId, &Order::var(*variable));
    }
    moved
}

fn permutations(items: &[SizeId]) -> Vec<Vec<SizeId>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut all = Vec::new();
    for at in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(at);
        for mut tail in permutations(&rest) {
            tail.insert(0, first);
            all.push(tail);
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(text: &str) -> Order {
        text.parse().unwrap()
    }

    fn outcome(found: (&str, &str), expected: (&str, &str)) -> Outcome {
        classify(
            [&order(found.0), &order(found.1)],
            [&order(expected.0), &order(expected.1)],
        )
    }

    #[test]
    fn equal_orders_are_exact_and_so_are_orders_that_only_name_their_sizes_differently() {
        assert_eq!(outcome(("O(n)", "O(1)"), ("O(n)", "O(1)")), Outcome::Exact);
        assert_eq!(
            outcome(("O(n·m²)", "O(1)"), ("O(n²·m)", "O(1)")),
            Outcome::Exact
        );
        assert_eq!(
            outcome(("O(n·m²)", "O(n)"), ("O(n²·m)", "O(m)")),
            Outcome::Exact
        );
    }

    #[test]
    fn the_naming_has_to_be_the_same_for_the_time_and_the_space() {
        assert_ne!(
            outcome(("O(n²·m)", "O(n)"), ("O(n²·m)", "O(m)")),
            Outcome::Exact
        );
    }

    #[test]
    fn a_larger_answer_is_over_and_a_smaller_one_is_under() {
        assert_eq!(outcome(("O(n²)", "O(1)"), ("O(n)", "O(1)")), Outcome::Over);
        assert_eq!(outcome(("O(n)", "O(1)"), ("O(n²)", "O(1)")), Outcome::Under);
        assert_eq!(outcome(("O(n)", "O(n)"), ("O(n)", "O(1)")), Outcome::Over);
        assert_eq!(outcome(("exp", "O(1)"), ("O(n)", "O(1)")), Outcome::Over);
    }

    #[test]
    fn missing_part_of_the_answer_is_under_even_when_something_else_is_extra() {
        assert_eq!(
            outcome(("O(n·m)", "O(1)"), ("O(n²)", "O(1)")),
            Outcome::Under
        );
    }
}

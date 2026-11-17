//! The complexity precision corpus. Every function under `fixtures/complexity` carries
//! an `@complexity` annotation saying what its time and space are, and the committed
//! `baseline.json` records how many of them the inference answered correctly.
//!
//! The composed inference does not exist yet, so for now this checks that the corpus is
//! well formed (every annotation parses, every language is represented, and the
//! baseline names exactly the corpus's functions) and that the per-file extraction
//! gets the time of every function marked `scope=local` right. `RQX_BLESS=1` rewrites
//! the baseline; until there is a composed inference it records every function as
//! abstained.

#[path = "complexity_corpus/annotation.rs"]
mod annotation;
#[path = "complexity_corpus/baseline.rs"]
mod baseline;
#[path = "complexity_corpus/corpus.rs"]
mod corpus;

use std::collections::BTreeMap;

use baseline::Baseline;
use corpus::{Corpus, Function, LANGUAGES};
use rqx_core::config::RqxConfig;
use rqx_core::cost::FileCost;
use rqx_lang::cost::{local, tables::CostTables};

const SEED_FUNCTIONS: usize = 30;

fn blessing() -> bool {
    std::env::var_os("RQX_BLESS").is_some()
}

fn baseline_path() -> std::path::PathBuf {
    corpus::root().join("baseline.json")
}

fn loaded() -> Corpus {
    let corpus = Corpus::load(&corpus::root());
    assert!(
        corpus.problems.is_empty(),
        "the corpus has mistakes:\n{}",
        corpus.problems.join("\n")
    );
    corpus
}

#[test]
fn every_annotation_in_the_corpus_is_well_formed() {
    let corpus = loaded();
    for language in LANGUAGES {
        eprintln!(
            "complexity corpus: {language:<10} {}",
            corpus.count_in(language)
        );
        assert!(corpus.count_in(language) >= 4, "{language} is too thin");
    }
    eprintln!(
        "complexity corpus: {:<10} {}",
        "total",
        corpus.functions.len()
    );
    assert!(corpus.functions.len() >= SEED_FUNCTIONS);
}

#[test]
fn function_ids_are_unique() {
    let ids = loaded().ids();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
}

#[test]
fn the_baseline_names_exactly_the_functions_of_the_corpus() {
    let corpus = loaded();
    let ids = corpus.ids();
    if blessing() {
        Baseline::all_abstained(&ids)
            .write(&baseline_path())
            .expect("the baseline is written");
    }
    let baseline = Baseline::read(&baseline_path()).expect("fixtures/complexity/baseline.json");
    let recorded: Vec<&String> = baseline.functions.keys().collect();
    let mut expected: Vec<&String> = ids.iter().collect();
    expected.sort();
    assert_eq!(
        recorded, expected,
        "the corpus and the baseline disagree; run `RQX_BLESS=1 cargo test -p rqx --test complexity_corpus`"
    );
    eprintln!(
        "complexity corpus baseline: exact {} over {} under {} abstained {}",
        baseline.count(baseline::Outcome::Exact),
        baseline.count(baseline::Outcome::Over),
        baseline.count(baseline::Outcome::Under),
        baseline.count(baseline::Outcome::Abstained),
    );
}

/// The extraction of each corpus file, read once.
struct Extractions {
    files: BTreeMap<String, FileCost>,
}

impl Extractions {
    fn of(corpus: &Corpus) -> Self {
        let config = RqxConfig::default();
        let mut files = BTreeMap::new();
        for function in &corpus.functions {
            files.entry(function.file.clone()).or_insert_with(|| {
                let path = corpus::root().join(&function.file);
                let source = std::fs::read_to_string(&path).expect("a corpus file reads");
                rqx_lang::analyze(&path, &function.file, &source, &config)
                    .unwrap_or_else(|error| panic!("{}: {error}", function.file))
                    .cost
            });
        }
        Self { files }
    }

    /// The function's own time. A function the extraction did not record is constant: it
    /// has no loop, calls nothing and has nothing to say.
    fn local_time(&self, function: &Function) -> String {
        let tables = CostTables::built_in();
        let file = &self.files[&function.file];
        file.functions
            .iter()
            .filter(|found| found.name == function.name)
            .filter(|found| found.line as usize >= function.annotation.target_line)
            .min_by_key(|found| found.line)
            .map_or_else(
                || rqx_core::cost::Order::one().to_string(),
                |found| local::time(found, &tables).to_string(),
            )
    }
}

#[test]
fn the_extraction_gets_the_own_time_of_every_local_function_right() {
    let corpus = loaded();
    let extractions = Extractions::of(&corpus);
    let ids = corpus.ids();
    let mut checked = 0;
    let mut wrong = Vec::new();
    for (function, id) in corpus.functions.iter().zip(&ids) {
        if !function.annotation.local {
            continue;
        }
        checked += 1;
        let found = extractions.local_time(function);
        let expected = function.annotation.time.to_string();
        if found != expected {
            wrong.push(format!("{id}: annotated {expected}, extracted {found}"));
        }
    }
    eprintln!("complexity corpus: {checked} local functions checked");
    assert!(checked >= 60, "only {checked} functions are marked local");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_language_has_local_functions_to_hold_the_extraction_to() {
    let corpus = loaded();
    for language in LANGUAGES {
        let local = corpus
            .functions
            .iter()
            .filter(|function| function.language == language && function.annotation.local)
            .count();
        assert!(local >= 6, "{language} marks only {local} functions local");
    }
}

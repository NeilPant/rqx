//! The complexity precision corpus. Every function under `fixtures/complexity` carries
//! an `@complexity` annotation saying what its time and space are, and the committed
//! `baseline.json` records how many of them the inference answered correctly.
//!
//! The inference does not exist yet, so for now this checks only that the corpus is
//! well formed: every annotation parses, every language is represented, and the
//! baseline names exactly the corpus's functions. `RQX_BLESS=1` rewrites the baseline;
//! until there is an inference it records every function as abstained.

#[path = "complexity_corpus/annotation.rs"]
mod annotation;
#[path = "complexity_corpus/baseline.rs"]
mod baseline;
#[path = "complexity_corpus/corpus.rs"]
mod corpus;

use baseline::Baseline;
use corpus::{Corpus, LANGUAGES};

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

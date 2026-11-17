//! The complexity precision corpus. Every function under `fixtures/complexity` carries
//! an `@complexity` annotation saying what its time and space are, and the committed
//! `baseline.json` records how many of them the inference answered correctly.
//!
//! The inference is the per-file extraction composed across calls and files. A function
//! on its own in a file is composed with that file alone; the files of a project under
//! `projects/` are composed together. Each function is scored exact, over, under or
//! abstained, per language, and the run fails when a function that was exact is not, or
//! when any language's precision or recall is lower than the baseline's. `RQX_BLESS=1`
//! rewrites the baseline on purpose.
//!
//! The extraction alone is also held to the time of every function marked `scope=local`.

#[path = "complexity_corpus/annotation.rs"]
mod annotation;
#[path = "complexity_corpus/baseline.rs"]
mod baseline;
#[path = "complexity_corpus/corpus.rs"]
mod corpus;
#[path = "complexity_corpus/inference.rs"]
mod inference;
#[path = "complexity_corpus/ratchet.rs"]
mod ratchet;

use std::collections::BTreeMap;
use std::sync::{Once, OnceLock};

use baseline::Baseline;
use corpus::{Corpus, LANGUAGES};
use inference::Inference;
use rqx_core::cost::Order;
use rqx_lang::cost::{local, tables::CostTables};

const SEED_FUNCTIONS: usize = 30;

fn blessing() -> bool {
    std::env::var_os("RQX_BLESS").is_some()
}

fn baseline_path() -> std::path::PathBuf {
    corpus::root().join("baseline.json")
}

fn loaded() -> &'static Corpus {
    static CORPUS: OnceLock<Corpus> = OnceLock::new();
    let corpus = CORPUS.get_or_init(|| Corpus::load(&corpus::root()));
    assert!(
        corpus.problems.is_empty(),
        "the corpus has mistakes:\n{}",
        corpus.problems.join("\n")
    );
    corpus
}

fn inferred() -> &'static Inference {
    static INFERENCE: OnceLock<Inference> = OnceLock::new();
    INFERENCE.get_or_init(|| Inference::of(loaded()))
}

/// What the inference says of every function of the corpus, by id.
fn current_baseline() -> Baseline {
    let corpus = loaded();
    let inference = inferred();
    Baseline::of(
        corpus
            .ids()
            .into_iter()
            .zip(&corpus.functions)
            .map(|(id, function)| (id, inference.outcome(function)))
            .collect(),
    )
}

/// Rewrites the baseline when asked to, once, before any test reads it.
fn bless_if_asked() {
    static BLESSED: Once = Once::new();
    BLESSED.call_once(|| {
        if blessing() {
            current_baseline()
                .write(&baseline_path())
                .expect("the baseline is written");
        }
    });
}

/// The language of every function, by id, for the metrics that are kept per language.
fn languages_by_id() -> BTreeMap<String, &'static str> {
    let corpus = loaded();
    corpus
        .ids()
        .into_iter()
        .zip(&corpus.functions)
        .map(|(id, function)| (id, function.language))
        .collect()
}

fn report(baseline: &Baseline, languages: &BTreeMap<String, &'static str>) {
    let tallies =
        baseline.tallies(&|id| languages.get(id).copied().unwrap_or("unknown").to_owned());
    for (group, tally) in &tallies {
        let ratio = |value: Option<f64>| value.map_or("-".to_owned(), |v| format!("{v:.3}"));
        eprintln!(
            "complexity corpus: {group:<10} {:>3} functions  exact {:>3} over {:>2} under {:>2} abstained {:>2}  precision {}  recall {}",
            tally.functions(),
            tally.exact,
            tally.over,
            tally.under,
            tally.abstained,
            ratio(tally.precision()),
            ratio(tally.recall()),
        );
    }
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
    bless_if_asked();
    let baseline = Baseline::read(&baseline_path()).expect("fixtures/complexity/baseline.json");
    let recorded: Vec<&String> = baseline.functions.keys().collect();
    let ids = corpus.ids();
    let mut expected: Vec<&String> = ids.iter().collect();
    expected.sort();
    assert_eq!(
        recorded, expected,
        "the corpus and the baseline disagree; run `RQX_BLESS=1 cargo test -p rqx --test complexity_corpus`"
    );
}

#[test]
fn the_composed_inference_holds_to_the_baseline() {
    let corpus = loaded();
    bless_if_asked();
    let recorded = Baseline::read(&baseline_path()).expect("fixtures/complexity/baseline.json");
    let current = current_baseline();
    let languages = languages_by_id();
    report(&current, &languages);

    let ids = corpus.ids();
    let mut missed = Vec::new();
    for (id, function) in ids.iter().zip(&corpus.functions) {
        if current.functions[id] != baseline::Outcome::Exact {
            missed.push(inferred().explain(function, id));
        }
    }
    eprintln!("complexity corpus: not exact:\n{}", missed.join("\n"));

    let violations = ratchet::violations(&recorded, &current, &|id| {
        languages.get(id).copied().unwrap_or("unknown").to_owned()
    });
    assert!(
        violations.is_empty(),
        "the inference is worse than the baseline; fix it, or bless a lower baseline on purpose \
         with `RQX_BLESS=1 cargo test -p rqx --test complexity_corpus`:\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_extraction_gets_the_own_time_of_every_local_function_right() {
    let corpus = loaded();
    let inference = inferred();
    let tables = CostTables::built_in();
    let ids = corpus.ids();
    let mut checked = 0;
    let mut wrong = Vec::new();
    for (function, id) in corpus.functions.iter().zip(&ids) {
        if !function.annotation.local {
            continue;
        }
        checked += 1;
        // A function the extraction did not record is constant: it has no loop, calls
        // nothing and has nothing to say.
        let found = inference
            .extraction(&function.file)
            .functions
            .iter()
            .filter(|found| found.name == function.name)
            .filter(|found| found.line as usize >= function.annotation.target_line)
            .min_by_key(|found| found.line)
            .map_or_else(
                || Order::one().to_string(),
                |found| local::time(found, &tables).to_string(),
            );
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

#[test]
fn the_projects_are_part_of_the_corpus() {
    let corpus = loaded();
    let in_projects: Vec<&corpus::Function> = corpus
        .functions
        .iter()
        .filter(|function| function.group.starts_with(corpus::PROJECTS))
        .collect();
    assert!(
        in_projects.len() >= 10,
        "only {} functions are in projects",
        in_projects.len()
    );
    assert!(
        in_projects
            .iter()
            .all(|function| function.file.starts_with("projects/") && !function.annotation.local),
        "a function whose cost depends on another file is not local"
    );
}

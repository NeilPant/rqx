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
//! The functions under `real/` were copied from public projects. They are scored the
//! same way, in a section of the baseline of their own, so that the number for real code
//! is not averaged into the number for the functions written for the corpus.
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
#[path = "complexity_corpus/rules.rs"]
mod rules;

use std::sync::{Once, OnceLock};

use baseline::{Baseline, RuleTable, Section};
use corpus::{Corpus, LANGUAGES, Set};
use inference::Inference;
use rqx_core::cost::Order;
use rqx_core::precision::{FORMAT, language_of_id};
use rqx_lang::cost::{local, tables::CostTables};

const SEED_FUNCTIONS: usize = 30;

fn blessing() -> bool {
    std::env::var_os("RQX_BLESS").is_some()
}

fn baseline_path() -> std::path::PathBuf {
    corpus::root().join("baseline.json")
}

/// The copy of the baseline's rule counts that the binary embeds.
fn rule_table_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../rqx-core/src/precision/rules.json")
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

/// What the inference says of every function of one set, and how its rules did on them.
fn section_of(set: Set) -> Section {
    let corpus = loaded();
    let inference = inferred();
    let functions = corpus
        .ids()
        .into_iter()
        .zip(&corpus.functions)
        .filter(|(_, function)| function.set == set)
        .map(|(id, function)| (id, inference.outcome(function)))
        .collect();
    Section {
        functions,
        rules: rules::counts(corpus, inference, set),
    }
}

/// What the inference says of every function of the corpus, by id.
fn current_baseline() -> Baseline {
    let written = section_of(Set::Written);
    Baseline {
        format: FORMAT,
        functions: written.functions,
        rules: written.rules,
        real: section_of(Set::Real),
    }
}

/// Rewrites the baseline when asked to, once, before any test reads it.
fn bless_if_asked() {
    static BLESSED: Once = Once::new();
    BLESSED.call_once(|| {
        if blessing() {
            let current = current_baseline();
            baseline::write(&current, &baseline_path()).expect("the baseline is written");
            baseline::write_rule_table(&current, &rule_table_path())
                .expect("the rule table is written");
        }
    });
}

fn report(baseline: &Baseline) {
    let sections = [("", baseline.synthetic()), ("real ", baseline.real.clone())];
    for (label, section) in sections {
        for (group, tally) in &section.tallies_by_language() {
            let ratio = |value: Option<f64>| value.map_or("-".to_owned(), |v| format!("{v:.3}"));
            eprintln!(
                "complexity corpus: {label}{group:<10} {:>3} functions  exact {:>3} over {:>2} under {:>2} abstained {:>2}  precision {}  recall {}",
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
}

#[test]
fn every_annotation_in_the_corpus_is_well_formed() {
    let corpus = loaded();
    for language in LANGUAGES {
        eprintln!(
            "complexity corpus: {language:<10} {} written, {} real",
            corpus.count_in(Set::Written, language),
            corpus.count_in(Set::Real, language)
        );
        assert!(
            corpus.count_in(Set::Written, language) >= 4,
            "{language} is too thin"
        );
    }
    eprintln!(
        "complexity corpus: {:<10} {} written, {} real",
        "total",
        corpus.count_of(Set::Written),
        corpus.count_of(Set::Real)
    );
    assert!(corpus.count_of(Set::Written) >= SEED_FUNCTIONS);
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
    let baseline = baseline::read(&baseline_path()).expect("fixtures/complexity/baseline.json");
    let mut recorded: Vec<&String> = baseline
        .functions
        .keys()
        .chain(baseline.real.functions.keys())
        .collect();
    recorded.sort();
    let ids = corpus.ids();
    let mut expected: Vec<&String> = ids.iter().collect();
    expected.sort();
    assert_eq!(
        recorded, expected,
        "the corpus and the baseline disagree; run `RQX_BLESS=1 cargo test -p rqx --test complexity_corpus`"
    );
}

#[test]
fn the_real_functions_are_scored_apart_from_the_written_ones() {
    let baseline = baseline::read(&baseline_path()).expect("fixtures/complexity/baseline.json");
    assert!(
        baseline
            .functions
            .keys()
            .all(|id| !id.starts_with(corpus::REAL)),
        "a real function is in the section of the written ones"
    );
    assert!(
        baseline
            .real
            .functions
            .keys()
            .all(|id| id.starts_with(corpus::REAL)),
        "a written function is in the real section"
    );
}

#[test]
fn the_binary_embeds_the_rule_counts_of_the_baseline() {
    bless_if_asked();
    let baseline = baseline::read(&baseline_path()).expect("fixtures/complexity/baseline.json");
    let text = std::fs::read_to_string(rule_table_path()).expect("the embedded rule table");
    assert_eq!(
        RuleTable::parse(&text).expect("the embedded rule table parses"),
        RuleTable::of(&baseline),
        "crates/rqx-core/src/precision/rules.json is out of step with the baseline; run \
         `RQX_BLESS=1 cargo test -p rqx --test complexity_corpus`"
    );
}

#[test]
fn the_composed_inference_holds_to_the_baseline() {
    let corpus = loaded();
    bless_if_asked();
    let recorded = baseline::read(&baseline_path()).expect("fixtures/complexity/baseline.json");
    let current = current_baseline();
    report(&current);

    let ids = corpus.ids();
    let mut missed = Vec::new();
    for (id, function) in ids.iter().zip(&corpus.functions) {
        let outcomes = match function.set {
            Set::Written => &current.functions,
            Set::Real => &current.real.functions,
        };
        if outcomes[id] != baseline::Outcome::Exact {
            missed.push(inferred().explain(function, id));
        }
    }
    eprintln!("complexity corpus: not exact:\n{}", missed.join("\n"));
    for (slug, tally) in &current.rules_overall() {
        eprintln!(
            "complexity corpus: {slug:<30} tp {:>2} fp {:>2} fn {:>2}",
            tally.tp, tally.fp, tally.missed
        );
    }
    eprintln!(
        "complexity corpus: rule mistakes:\n{}",
        rules::explain(corpus, inferred(), &ids).join("\n")
    );

    let violations = ratchet::violations(&recorded, &current, &|id| language_of_id(id).to_owned());
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
            .map_or_else(Order::one, |found| local::time(found, &tables));
        let expected = &function.annotation.time;
        if !inference::same_time_up_to_naming(&found, expected) {
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

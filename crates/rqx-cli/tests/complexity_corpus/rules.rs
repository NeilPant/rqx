//! How well the performance rules do on the corpus. A function annotated `rule=<slug>`
//! must be reported by that rule, which is a hit or a miss; a function the rule reports
//! without being annotated for it is a false alarm, whether the annotation says
//! `none=<slug>` or says nothing. Counts are per rule, kept in the baseline beside the
//! per-function outcomes.
//!
//! The rules scored are the ones that read composed costs across files and every rule
//! an annotation names, so a rule enters the baseline as soon as the corpus says what
//! it should and should not report.

use std::collections::{BTreeMap, BTreeSet};

use rqx_core::cost_sites::CROSS_FILE_RULES;

use super::baseline::RuleCounts;
use super::corpus::{Corpus, Function, Set};
use super::inference::Inference;

/// The rules the corpus scores: the cross-file ones and each one an annotation names.
pub fn scored(corpus: &Corpus) -> BTreeSet<String> {
    let named = corpus.functions.iter().flat_map(|function| {
        let annotation = &function.annotation;
        [annotation.rule.as_deref(), annotation.none.as_deref()]
            .into_iter()
            .flatten()
            .flat_map(|rules| rules.split(','))
    });
    CROSS_FILE_RULES
        .iter()
        .map(|rule| rule.slug())
        .chain(named)
        .map(str::to_owned)
        .collect()
}

/// Whether the annotation names the rule in a field.
fn names(field: Option<&str>, slug: &str) -> bool {
    field.is_some_and(|rules| rules.split(',').any(|rule| rule == slug))
}

/// Whether the function is annotated as one the rule must report.
fn is_wanted(function: &Function, slug: &str) -> bool {
    names(function.annotation.rule.as_deref(), slug)
}

/// Whether the function is annotated as one the rule may report without being wrong.
fn is_tolerated(function: &Function, slug: &str) -> bool {
    names(function.annotation.may.as_deref(), slug)
}

/// What each scored rule did on the functions of one set.
pub fn counts(corpus: &Corpus, inference: &Inference, set: Set) -> BTreeMap<String, RuleCounts> {
    let mut found: BTreeMap<String, RuleCounts> = scored(corpus)
        .into_iter()
        .map(|slug| (slug, RuleCounts::default()))
        .collect();
    for function in corpus.functions.iter().filter(|f| f.set == set) {
        for (slug, tally) in &mut found {
            match (is_wanted(function, slug), inference.fires(function, slug)) {
                (true, true) => tally.tp += 1,
                (true, false) => tally.missed += 1,
                (false, true) if is_tolerated(function, slug) => {}
                (false, true) => tally.fp += 1,
                (false, false) => {}
            }
        }
    }
    found
}

/// A line for each function a rule gets wrong, for whoever is improving the rule.
pub fn explain(corpus: &Corpus, inference: &Inference, ids: &[String]) -> Vec<String> {
    let scored = scored(corpus);
    let mut lines = Vec::new();
    for (function, id) in corpus.functions.iter().zip(ids) {
        for slug in &scored {
            let wanted = is_wanted(function, slug);
            let fired = inference.fires(function, slug);
            if wanted != fired && !(fired && is_tolerated(function, slug)) {
                let what = if wanted { "missed" } else { "fired wrongly" };
                lines.push(format!("{id}: {slug} {what}"));
            }
        }
    }
    lines
}

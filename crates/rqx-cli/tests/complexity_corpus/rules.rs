//! How well the rules that read composed costs do on the corpus. A function annotated
//! `rule=<slug>` must be reported by that rule, which is a hit or a miss; a function the
//! rule reports without being annotated for it is a false alarm, whether the annotation
//! says `none=<slug>` or says nothing. Counts are per rule, kept in the baseline beside
//! the per-function outcomes.

use std::collections::BTreeMap;

use rqx_core::cost_sites::CROSS_FILE_RULES;

use super::baseline::RuleCounts;
use super::corpus::{Corpus, Function};
use super::inference::Inference;

/// The rules the corpus scores here: the ones the composed inference answers.
pub fn scored() -> Vec<&'static str> {
    CROSS_FILE_RULES.iter().map(|rule| rule.slug()).collect()
}

/// Whether the function is annotated as one the rule must report.
fn is_wanted(function: &Function, slug: &str) -> bool {
    function
        .annotation
        .rule
        .as_deref()
        .is_some_and(|rules| rules.split(',').any(|rule| rule == slug))
}

pub fn counts(corpus: &Corpus, inference: &Inference) -> BTreeMap<String, RuleCounts> {
    let mut found: BTreeMap<String, RuleCounts> = scored()
        .into_iter()
        .map(|slug| (slug.to_owned(), RuleCounts::default()))
        .collect();
    for function in &corpus.functions {
        for (slug, tally) in &mut found {
            match (is_wanted(function, slug), inference.fires(function, slug)) {
                (true, true) => tally.tp += 1,
                (true, false) => tally.missed += 1,
                (false, true) => tally.fp += 1,
                (false, false) => {}
            }
        }
    }
    found
}

/// A line for each function a rule gets wrong, for whoever is improving the rule.
pub fn explain(corpus: &Corpus, inference: &Inference, ids: &[String]) -> Vec<String> {
    let mut lines = Vec::new();
    for (function, id) in corpus.functions.iter().zip(ids) {
        for slug in scored() {
            let wanted = is_wanted(function, slug);
            let fired = inference.fires(function, slug);
            if wanted != fired {
                let what = if wanted { "missed" } else { "fired wrongly" };
                lines.push(format!("{id}: {slug} {what}"));
            }
        }
    }
    lines
}

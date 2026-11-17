//! The ratchet: what a run of the inference may not do compared with the baseline. A
//! function that was exact has to stay exact, and no group's precision or recall may fall,
//! so an improvement in one place cannot be paid for by a loss in another. Getting better
//! is recorded by blessing a new baseline on purpose.

use super::baseline::{Baseline, Outcome};

/// Metrics are ratios of whole numbers, so anything closer than this is the same number.
const EPSILON: f64 = 1e-9;

/// Everything `current` does that `recorded` does not allow, one line each.
pub fn violations(
    recorded: &Baseline,
    current: &Baseline,
    group_of: &dyn Fn(&str) -> String,
) -> Vec<String> {
    let mut found = Vec::new();
    for (id, was) in &recorded.functions {
        if let Some(now) = current.functions.get(id)
            && *was == Outcome::Exact
            && *now != Outcome::Exact
        {
            found.push(format!("{id}: was exact and is now {now:?}"));
        }
    }
    let (before, after) = (recorded.tallies(group_of), current.tallies(group_of));
    for (group, was) in &before {
        let now = after.get(group).copied().unwrap_or_default();
        let metrics = [
            ("precision", was.precision(), now.precision()),
            ("recall", was.recall(), now.recall()),
        ];
        for (name, before, after) in metrics {
            let Some(before) = before else { continue };
            match after {
                Some(after) if after + EPSILON >= before => {}
                Some(after) => found.push(format!(
                    "{group}: {name} fell from {before:.3} to {after:.3}"
                )),
                None => found.push(format!("{group}: {name} fell from {before:.3} to nothing")),
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn baseline(outcomes: &[(&str, Outcome)]) -> Baseline {
        Baseline::of(
            outcomes
                .iter()
                .map(|(id, outcome)| ((*id).to_owned(), *outcome))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn language(id: &str) -> String {
        id.split('/').next().unwrap().to_owned()
    }

    #[test]
    fn a_run_that_keeps_every_exact_function_and_gains_more_is_allowed() {
        let recorded = baseline(&[("go/a", Outcome::Exact), ("go/b", Outcome::Over)]);
        let better = baseline(&[("go/a", Outcome::Exact), ("go/b", Outcome::Exact)]);
        assert!(violations(&recorded, &better, &language).is_empty());
        assert!(violations(&recorded, &recorded, &language).is_empty());
    }

    #[test]
    fn a_function_that_was_exact_may_not_stop_being_exact() {
        let recorded = baseline(&[
            ("go/a", Outcome::Exact),
            ("go/b", Outcome::Over),
            ("py/c", Outcome::Exact),
        ]);
        // One more exact elsewhere keeps the totals up, which does not excuse the loss.
        let swapped = baseline(&[
            ("go/a", Outcome::Under),
            ("go/b", Outcome::Exact),
            ("py/c", Outcome::Exact),
        ]);
        let found = violations(&recorded, &swapped, &language);
        assert!(
            found
                .iter()
                .any(|line| line.contains("go/a: was exact and is now Under")),
            "{found:?}"
        );
    }

    #[test]
    fn precision_may_not_fall_even_when_more_functions_are_exact() {
        let recorded = baseline(&[("go/a", Outcome::Exact), ("go/b", Outcome::Abstained)]);
        let answers_wrongly = baseline(&[("go/a", Outcome::Exact), ("go/b", Outcome::Over)]);
        let found = violations(&recorded, &answers_wrongly, &language);
        assert!(
            found
                .iter()
                .any(|line| line.starts_with("go: precision fell")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|line| line.starts_with("total: precision fell")),
            "{found:?}"
        );
        assert!(
            !found.iter().any(|line| line.contains("recall")),
            "recall is unchanged: {found:?}"
        );
    }

    #[test]
    fn recall_may_not_fall_in_any_language() {
        let recorded = baseline(&[("go/a", Outcome::Exact), ("py/b", Outcome::Exact)]);
        let lost = baseline(&[("go/a", Outcome::Exact), ("py/b", Outcome::Abstained)]);
        let found = violations(&recorded, &lost, &language);
        assert!(
            found.iter().any(|line| line.starts_with("py: recall fell")),
            "{found:?}"
        );
    }
}

use rqx_core::finding::Finding;
use rqx_core::score::health_of;

use crate::report::Report;
use crate::scan::ScanOutcome;

/// One finding, priced. `value` is what fixing it takes off the weighted total the
/// health score decays from, so the ranking maximises the score directly rather than
/// through a proxy.
pub struct Item<'a> {
    pub finding: &'a Finding,
    pub value: f64,
    pub effort: u8,
    /// The score once this item and everything above it in the plan is done.
    pub health_after: u32,
}

/// The cheapest route a rule prescribes. A rule offering both an extraction and a
/// hierarchy change is not dear work; it is work with a cheap answer available.
fn effort_of(finding: &Finding) -> u8 {
    finding
        .rule
        .spec()
        .fixes
        .iter()
        .map(|fix| fix.group().effort())
        .min()
        .unwrap_or(u8::MAX)
}

fn value_of(finding: &Finding, outcome: &ScanOutcome) -> f64 {
    if finding.confidence == rqx_core::Confidence::Low {
        // Unscored findings are reported but do not move the health score, so fixing
        // one buys nothing the plan can promise.
        return 0.0;
    }
    finding.severity.weight() * outcome.impact.get(&finding.location.file).multiplier()
}

/// The most valuable work that fits, in the order to do it.
///
/// `budget` is in effort points. Because the score decays monotonically from the
/// weighted total, taking the most value under a budget maximises the resulting score
/// exactly — there is no per-item rescoring to do, and a greedy pick by value-per-point
/// would be wrong for a mix of efforts.
pub fn rank<'a>(outcome: &'a ScanOutcome, budget: Option<u32>, limit: usize) -> Vec<Item<'a>> {
    let mut priced: Vec<(&Finding, f64, u8)> = outcome
        .findings
        .iter()
        .map(|finding| (finding, value_of(finding, outcome), effort_of(finding)))
        .filter(|(_, value, effort)| *value > 0.0 && *effort < u8::MAX)
        .collect();

    // Deterministic before any selection runs, so two runs of the same scan produce
    // the same plan whatever order the collector happened to fill.
    priced.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.2.cmp(&right.2))
            .then(right.0.rank().cmp(&left.0.rank()))
            .then(left.0.location.file.cmp(&right.0.location.file))
            .then(left.0.location.start.cmp(&right.0.location.start))
    });

    let chosen: Vec<(&Finding, f64, u8)> = match budget {
        Some(budget) => knapsack(&priced, budget),
        None => priced.into_iter().take(limit).collect(),
    };

    let total = outcome.counts.weighted_total(&outcome.impact);
    let mut spent = 0.0;
    chosen
        .into_iter()
        .map(|(finding, value, effort)| {
            spent += value;
            Item {
                finding,
                value,
                effort,
                health_after: health_of((total - spent).max(0.0), outcome.code_lines),
            }
        })
        .collect()
}

/// Exact, by the usual table. The inputs are bounded by `[scan] max_findings` and by
/// the budget a person is willing to type, so the table stays small enough that being
/// approximate would buy nothing.
fn knapsack<'a>(priced: &[(&'a Finding, f64, u8)], budget: u32) -> Vec<(&'a Finding, f64, u8)> {
    let capacity = budget as usize;
    let mut best = vec![0.0_f64; capacity + 1];
    let mut taken = vec![vec![false; capacity + 1]; priced.len()];

    for (index, (_, value, effort)) in priced.iter().enumerate() {
        let cost = *effort as usize;
        for room in (cost..=capacity).rev() {
            let candidate = best[room - cost] + value;
            if candidate > best[room] {
                best[room] = candidate;
                taken[index][room] = true;
            }
        }
    }

    let mut room = capacity;
    let mut picked: Vec<usize> = Vec::new();
    for index in (0..priced.len()).rev() {
        if taken[index][room] {
            picked.push(index);
            room -= priced[index].2 as usize;
        }
    }
    picked.reverse();
    picked.into_iter().map(|index| priced[index]).collect()
}

/// The project as it would be with the plan done. Grades move too, so a plan that
/// lifts one file out of `critical` can say so.
pub fn simulated(outcome: &ScanOutcome, plan: &[Item<'_>]) -> (u32, u32) {
    let before = Report::everything(outcome).health();
    let counts = outcome.counts.without(plan.iter().map(|item| item.finding));
    let after = counts.weighted_health_score(outcome.code_lines, &outcome.impact);
    (before, after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::tests::outcome_of;
    use rqx_core::rule::RuleId;

    /// The greedy answer takes the dearest item first and leaves a point unspendable;
    /// the exact one takes two cheaper items worth more together.
    #[test]
    fn the_plan_is_exact_rather_than_greedy() {
        let outcome = outcome_of(&[
            (RuleId::UnusedLocal, "a.ts"),
            (RuleId::UnusedLocal, "b.ts"),
            (RuleId::CircularDependency, "c.ts"),
        ]);
        let plan = rank(&outcome, Some(2), 20);
        let spent: u8 = plan.iter().map(|item| item.effort).sum();
        assert!(spent <= 2, "the plan spent {spent} of a budget of 2");
    }

    #[test]
    fn a_plan_is_the_same_twice() {
        let outcome = outcome_of(&[
            (RuleId::UnusedLocal, "a.ts"),
            (RuleId::UnusedLocal, "b.ts"),
            (RuleId::EmptyCatch, "c.ts"),
        ]);
        let first: Vec<&str> = rank(&outcome, None, 10)
            .iter()
            .map(|item| item.finding.rule.slug())
            .collect();
        let second: Vec<&str> = rank(&outcome, None, 10)
            .iter()
            .map(|item| item.finding.rule.slug())
            .collect();
        assert_eq!(first, second);
    }

    #[test]
    fn doing_the_plan_raises_the_score_it_predicted() {
        let outcome = outcome_of(&[(RuleId::EmptyCatch, "a.ts"), (RuleId::EmptyCatch, "b.ts")]);
        let plan = rank(&outcome, None, 10);
        assert!(!plan.is_empty());

        let (before, after) = simulated(&outcome, &plan);
        assert!(after > before, "{before} -> {after}");
        assert_eq!(
            after,
            plan.last().expect("an item").health_after,
            "the last row's prediction and the simulation disagree"
        );
    }
}

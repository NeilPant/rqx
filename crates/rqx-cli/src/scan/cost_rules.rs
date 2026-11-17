//! The findings of the rules that read composed costs. The cost stage finds the places;
//! this turns them into findings and offers them through the same admission as every
//! other finding, after dropping the ones another rule already reports.

use std::collections::HashMap;
use std::path::Path;

use rqx_core::cost_sites::{CROSS_FILE_RULES, CrossSite};
use rqx_core::{Language, RuleId};

use super::{CostFile, Positions, Resolved, Staged, build};

/// Rules that already report a call inside a loop. A place one of them reports on the same
/// line is that rule's, and is not reported a second time under a rule that generalises it.
pub(super) const REPORTED_IN_LOOPS: [&str; 7] = [
    "scan-in-loop",
    "query-in-loop",
    "await-in-loop",
    "invariant-in-loop",
    "accumulate-by-copy",
    "linear-op-in-loop",
    "sort-in-loop",
];

/// Whether the scan has any use for the places: some rule that reads them is on.
pub(super) fn wanted(settings: &Resolved) -> bool {
    CROSS_FILE_RULES
        .iter()
        .any(|rule| settings.enabled_anywhere(*rule))
}

/// The findings for `sites`, in the order the stage found them.
pub(super) fn staged(
    sites: Vec<CrossSite>,
    files: &[CostFile],
    languages: &HashMap<&str, Language>,
    root: &Path,
    settings: &Resolved,
) -> Staged {
    let covered: HashMap<&str, &[u32]> = files
        .iter()
        .map(|file| (&*file.name, file.covered.as_slice()))
        .collect();
    let mut at = Positions::new(root);
    let mut staged = Staged::default();
    for site in sites {
        let language = languages.get(site.file.as_str()).copied();
        if language.is_some_and(|language| !site.rule.applies_to(language))
            || already_reported(
                &site,
                covered.get(site.file.as_str()).copied().unwrap_or(&[]),
            )
        {
            continue;
        }
        let path = root.join(&site.file);
        let Some(range) = at.line_range(&path, site.line) else {
            continue;
        };
        let mut finding = build(
            site.rule,
            range,
            site.evidence(),
            site.fix_hint().to_owned(),
            Vec::new(),
        );
        finding.confidence = site.confidence();
        staged.accept_if_enabled(settings.for_file(&path), finding);
    }
    staged
}

fn already_reported(site: &CrossSite, lines: &[u32]) -> bool {
    matches!(
        site.rule,
        RuleId::QuadraticViaCall | RuleId::RepeatedComputationInLoop
    ) && lines.contains(&site.line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(rule: RuleId, line: u32) -> CrossSite {
        CrossSite {
            rule,
            file: "a.ts".to_owned(),
            function: 0,
            line,
            subject: String::new(),
            depth: 1,
            conf: rqx_core::cost::Conf::Verified,
            message: String::new(),
        }
    }

    #[test]
    fn every_rule_a_place_defers_to_exists() {
        for slug in REPORTED_IN_LOOPS {
            assert!(RuleId::parse(slug).is_some(), "{slug} is not a rule");
        }
    }

    #[test]
    fn a_line_another_rule_reports_is_not_reported_again_by_the_rules_that_generalise_it() {
        let lines = [7];
        assert!(already_reported(
            &site(RuleId::RepeatedComputationInLoop, 7),
            &lines
        ));
        assert!(already_reported(&site(RuleId::QuadraticViaCall, 7), &lines));
        assert!(!already_reported(
            &site(RuleId::QuadraticViaCall, 8),
            &lines
        ));
        assert!(!already_reported(&site(RuleId::DeepRecursion, 7), &lines));
        assert!(!already_reported(
            &site(RuleId::PairScanCouldHash, 7),
            &lines
        ));
    }
}

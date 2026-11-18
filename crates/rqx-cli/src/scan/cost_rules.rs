//! The findings of the rules that read composed costs. The cost stage finds the places,
//! leaving out the ones another rule already reports; this turns them into findings and
//! offers them through the same admission as every other finding.

use std::path::Path;

use rqx_core::cost_sites::{CROSS_FILE_RULES, CrossSite};

use super::{Positions, Resolved, Staged, build};

/// Whether the scan has any use for the places: some rule that reads them is on.
pub(super) fn wanted(settings: &Resolved) -> bool {
    CROSS_FILE_RULES
        .iter()
        .any(|rule| settings.enabled_anywhere(*rule))
}

/// The findings for `sites`, in the order the stage found them.
pub(super) fn staged(sites: Vec<CrossSite>, root: &Path) -> Staged {
    let mut at = Positions::new(root);
    let mut staged = Staged::default();
    for site in sites {
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
        staged.propose(finding);
    }
    staged
}

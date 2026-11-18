//! What each function in a file costs, read from its syntax alone: how many times its
//! loops run, which library calls it makes and what they cost, which of its own
//! functions it calls, and the signals that are not an order of growth. This produces
//! the per-file facts that the cache holds; `compose` joins them across calls and files.

pub mod bindings;
pub mod bounds;
pub mod builder;
pub mod cache;
pub mod compose;
pub mod env;
pub mod file;
pub mod indicators;
pub mod local;
pub mod recurrence;
pub mod remap;
pub mod shape;
pub mod sizes;
pub mod tables;
pub mod types;
pub mod written;

use rqx_core::config::Performance;
use rqx_core::cost::FileCost;
use rqx_core::facts::{CostSite, ModuleFacts, Spot};
use tree_sitter::Node;

use crate::names::Names;
use crate::profile::LanguageProfile;
use builder::{Shared, build};
use tables::CostTables;

/// Everything outside the file that its cost is read against.
pub struct Inputs<'a> {
    pub tables: &'a CostTables,
    pub performance: &'a Performance,
    /// What the file imports and exports, where the analysis read it.
    pub module: Option<&'a ModuleFacts>,
}

/// What reading a file's functions finds: the cost facts the cache holds, and the places
/// that performance rules report, which are findings already and so are not cached here.
pub struct Extracted {
    pub cost: FileCost,
    pub sites: Vec<CostSite>,
}

/// The cost facts of one file: each function that is not plainly constant time, or that
/// calls something, or that carries a signal. Functions come in the order they are
/// written, with the ones found inside a function right after it.
pub fn extract(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
    inputs: &Inputs<'_>,
) -> FileCost {
    read(profile, source, root, inputs).cost
}

/// As [`extract`], with the places the performance rules report.
pub fn read(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
    inputs: &Inputs<'_>,
) -> Extracted {
    let names = Names::new(profile, source);
    let (facts, mut waiting) = file::read(names, root, inputs.module);
    let shared = Shared {
        tables: inputs.tables,
        performance: inputs.performance,
        file: &facts,
    };
    let mut functions = Vec::new();
    let mut sites = Vec::new();
    // Read from the back, so that functions come out in the order they are written.
    waiting.reverse();
    while let Some(site) = waiting.pop() {
        let built = build(names, &shared, &site);
        functions.extend(built.cost);
        sites.extend(built.sites);
        waiting.extend(built.nested.into_iter().rev());
    }
    sites.extend(cache::unbounded_caches(names, root, &facts));
    Extracted {
        cost: FileCost { functions },
        sites,
    }
}

/// The sites no other rule already reports. A call that `scan-in-loop` flags is that
/// rule's finding, and a second rule naming the same call would say the same thing twice.
pub fn apart_from_scans(sites: Vec<CostSite>, scans: &[Spot]) -> Vec<CostSite> {
    sites
        .into_iter()
        .filter(|site| scans.iter().all(|scan| scan.span != site.span))
        .collect()
}

#[cfg(test)]
mod rules;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_repeats;

#[cfg(test)]
pub(crate) mod testing {
    use rqx_core::Language;
    use tree_sitter::{Node, Tree};

    use crate::names::Names;

    pub fn tree_of(language: Language, source: &str) -> Tree {
        let profile = crate::support(language).profile;
        crate::parse(language, profile, false, source).expect("the snippet parses")
    }

    pub fn names_of(language: Language, source: &str) -> Names<'_> {
        Names::new(crate::support(language).profile, source)
    }

    /// Every named node of one kind, in source order.
    pub fn nodes_of<'t>(tree: &'t Tree, kind: &str) -> Vec<Node<'t>> {
        crate::tree::descendants(tree.root_node())
            .into_iter()
            .filter(|node| node.kind() == kind)
            .collect()
    }
}

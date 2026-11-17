//! What each function in a file costs, read from its syntax alone: how many times its
//! loops run, which library calls it makes and what they cost, which of its own
//! functions it calls, and the signals that are not an order of growth. Composing
//! these across calls and files is a later stage; this one produces the per-file
//! facts that the cache holds.

pub mod bindings;
pub mod bounds;
pub mod env;
pub mod indicators;
pub mod recurrence;
pub mod tables;
pub mod types;

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

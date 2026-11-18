//! Which facts a scan builds before any rule reads them. Each rule names the facts it
//! reads, and a scan builds the union of what its enabled rules and advisories read,
//! so whether a rule fires never depends on which other rules are switched on.
//!
//! `Analysis` describes how a rule decides; it is not what the engine builds for it.
//! Most rules declared `Syntax` are read off the same collectors as the `Semantics`
//! rules, and building facts from the declared tier left them silent whenever the
//! deeper rules were switched off.

use crate::advice::AdviceId;
use crate::config::RqxConfig;
use crate::rule::{Analysis, RuleId};

/// A set of facts the engine builds, named by what a rule reads.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Fact {
    /// What the parse tree measures in a file: function, class and name metrics,
    /// clone candidates, function shapes, cost facts and waivers. Built for every
    /// file, whatever the rules are.
    Metrics,
    /// What the scope resolver and the collectors built on it find in a file: the
    /// hygiene walk, the module surface and its imports, classes, patterns, unused
    /// parameters, notes, and the cost sites they refine.
    Collected,
    /// `Collected`, in every file of the project and not only the one a finding is
    /// reported in: the import graph and the composed costs are built from all of them.
    CollectedEverywhere,
    /// The import graph, resolved across every file.
    ImportGraph,
}

const METRICS: &[Fact] = &[Fact::Metrics];
const COLLECTED: &[Fact] = &[Fact::Metrics, Fact::Collected];
const COLLECTED_EVERYWHERE: &[Fact] = &[Fact::Metrics, Fact::Collected, Fact::CollectedEverywhere];
const IMPORT_GRAPH: &[Fact] = &[
    Fact::Metrics,
    Fact::Collected,
    Fact::CollectedEverywhere,
    Fact::ImportGraph,
];

impl RuleId {
    /// The facts this rule reads. Derived from where the rule is decided rather than
    /// from its `Analysis`: only the rules listed here read nothing beyond the parse
    /// tree's measurements, and every other rule gets the collectors, so a rule added
    /// without a decision here can only be given more than it reads, never less.
    pub const fn facts(self) -> &'static [Fact] {
        match self {
            // Decided by `check_function`, `check_class` and `check_name`, which are
            // handed one function's, class's or name's metrics and nothing else.
            Self::LongFunction
            | Self::CognitiveComplexity
            | Self::CyclomaticComplexity
            | Self::DeepNesting
            | Self::DeepLoopNesting
            | Self::LongParameterList
            | Self::BranchingRecursion
            | Self::NoBaseCase
            | Self::LargeClass
            | Self::MeaninglessName
            // Clone candidates and function shapes compared across files.
            | Self::DuplicateCode
            | Self::CrossLanguageDuplicate
            | Self::ReimplementedExisting
            | Self::OversizedModule
            // Git history, beside the list of files the walk read.
            | Self::Hotspot
            | Self::SingleAuthorFile
            | Self::DivergentChange
            | Self::ShotgunSurgery
            // A tree-sitter query over the parse tree.
            | Self::Custom => METRICS,
            // Calls followed across imports, through every file's cost facts.
            Self::QuadraticViaCall
            | Self::DeepRecursion
            | Self::RepeatedComputationInLoop
            | Self::PairScanCouldHash => COLLECTED_EVERYWHERE,
            _ if matches!(self.spec().analysis, Analysis::Modules) => IMPORT_GRAPH,
            _ => COLLECTED,
        }
    }
}

impl AdviceId {
    /// The facts this advisory reads. Every advisory is drawn from the classes and
    /// patterns the collectors find, and the ones that need other files from the
    /// import graph as well.
    pub const fn facts(self) -> &'static [Fact] {
        match self.spec().analysis {
            Analysis::Modules => IMPORT_GRAPH,
            Analysis::Syntax | Analysis::Semantics => COLLECTED,
        }
    }
}

/// The facts one file's analysis builds.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FactPlan {
    collected: bool,
    collected_everywhere: bool,
    import_graph: bool,
}

impl FactPlan {
    /// The parse tree's measurements and nothing else.
    pub const METRICS: Self = Self {
        collected: false,
        collected_everywhere: false,
        import_graph: false,
    };

    /// Everything the engine can build.
    pub const ALL: Self = Self {
        collected: true,
        collected_everywhere: true,
        import_graph: true,
    };

    /// What the rules and advisories `config` enables read.
    pub fn of(config: &RqxConfig) -> Self {
        let rules = RuleId::ALL
            .iter()
            .filter(|rule| config.rules.enabled(**rule))
            .map(|rule| rule.facts());
        let advisories = AdviceId::ALL
            .iter()
            .filter(|advisory| config.patterns.enabled(**advisory))
            .map(|advisory| advisory.facts());
        rules
            .chain(advisories)
            .fold(Self::METRICS, |plan, facts| plan.with_all(facts))
    }

    /// Everything either plan builds.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            collected: self.collected || other.collected,
            collected_everywhere: self.collected_everywhere || other.collected_everywhere,
            import_graph: self.import_graph || other.import_graph,
        }
    }

    /// The part of this plan that every file has to build, not only the files whose
    /// own rules read it: what a rule reads from other files.
    #[must_use]
    pub const fn for_every_file(self) -> Self {
        Self {
            collected: self.collected_everywhere,
            collected_everywhere: self.collected_everywhere,
            import_graph: self.import_graph,
        }
    }

    pub const fn builds(self, fact: Fact) -> bool {
        match fact {
            Fact::Metrics => true,
            Fact::Collected => self.collected,
            Fact::CollectedEverywhere => self.collected_everywhere,
            Fact::ImportGraph => self.import_graph,
        }
    }

    #[must_use]
    fn with_all(self, facts: &[Fact]) -> Self {
        facts.iter().fold(self, |plan, fact| plan.with(*fact))
    }

    #[must_use]
    const fn with(mut self, fact: Fact) -> Self {
        match fact {
            Fact::Metrics => {}
            Fact::Collected => self.collected = true,
            Fact::CollectedEverywhere => self.collected_everywhere = true,
            Fact::ImportGraph => self.import_graph = true,
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(rule: RuleId) -> RqxConfig {
        let mut config = RqxConfig::default();
        config.rules.disabled = RuleId::ALL
            .iter()
            .filter(|other| **other != rule)
            .map(|other| other.slug().to_owned())
            .collect();
        config.rules.enabled.insert(rule.slug().to_owned());
        config.patterns.disabled = crate::pattern::PatternFamily::ALL
            .iter()
            .map(|family| family.slug().to_owned())
            .collect();
        config
    }

    #[test]
    fn a_rule_that_reads_the_collectors_has_them_built_when_it_is_the_only_rule_on() {
        for rule in [
            RuleId::BranchingTest,
            RuleId::DiscardedError,
            RuleId::NegatedName,
        ] {
            assert!(
                FactPlan::of(&only(rule)).builds(Fact::Collected),
                "{rule:?} reads the hygiene walk"
            );
        }
    }

    #[test]
    fn a_rule_that_reads_only_metrics_builds_nothing_more() {
        assert_eq!(FactPlan::of(&only(RuleId::LongFunction)), FactPlan::METRICS);
    }

    #[test]
    fn every_rule_that_reads_the_import_graph_needs_every_files_collectors() {
        for rule in RuleId::ALL.iter().copied() {
            let facts = rule.facts();
            assert!(facts.contains(&Fact::Metrics), "{rule:?}");
            if facts.contains(&Fact::ImportGraph) || facts.contains(&Fact::CollectedEverywhere) {
                assert!(facts.contains(&Fact::CollectedEverywhere), "{rule:?}");
                assert!(facts.contains(&Fact::Collected), "{rule:?}");
            }
        }
    }

    /// Building facts from the declared tier is what left most `Syntax` rules silent,
    /// so the tier may only ever ask for less than `facts` does.
    #[test]
    fn a_rule_reads_at_least_what_its_tier_declares() {
        for rule in RuleId::ALL.iter().copied() {
            let plan = FactPlan::METRICS.with_all(rule.facts());
            match rule.spec().analysis {
                Analysis::Syntax => {}
                Analysis::Semantics => assert!(plan.builds(Fact::Collected), "{rule:?}"),
                Analysis::Modules => assert!(plan.builds(Fact::ImportGraph), "{rule:?}"),
            }
        }
    }

    #[test]
    fn only_what_other_files_need_is_built_in_every_file() {
        let own = FactPlan::of(&only(RuleId::BranchingTest));
        assert_eq!(own.for_every_file(), FactPlan::METRICS);
        let graph = FactPlan::of(&only(RuleId::UnusedExport));
        assert!(graph.for_every_file().builds(Fact::Collected));
        assert!(graph.for_every_file().builds(Fact::ImportGraph));
    }

    #[test]
    fn the_default_configuration_builds_everything() {
        assert_eq!(FactPlan::of(&RqxConfig::default()), FactPlan::ALL);
    }
}

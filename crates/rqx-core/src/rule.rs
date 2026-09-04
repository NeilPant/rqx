use std::fmt;

use serde::{Deserialize, Serialize};

use crate::finding::{Confidence, Severity};
use crate::language::Language;
use crate::technique::FixTechnique;

/// The top-level organisation, because eight axes are what a reader can hold in
/// their head where sixty rules are not.
#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Size,
    Abstraction,
    Change,
    Waste,
    Coupling,
    Flow,
    Naming,
    Safety,
    Tests,
}

impl Category {
    pub const ALL: [Self; 9] = [
        Self::Size,
        Self::Abstraction,
        Self::Change,
        Self::Waste,
        Self::Coupling,
        Self::Flow,
        Self::Naming,
        Self::Safety,
        Self::Tests,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Abstraction => "abstraction",
            Self::Change => "change",
            Self::Waste => "waste",
            Self::Coupling => "coupling",
            Self::Flow => "flow",
            Self::Naming => "naming",
            Self::Safety => "safety",
            Self::Tests => "tests",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Size => "Size",
            Self::Abstraction => "Abstraction",
            Self::Change => "Change",
            Self::Waste => "Waste",
            Self::Coupling => "Coupling",
            Self::Flow => "Flow",
            Self::Naming => "Naming",
            Self::Safety => "Safety",
            Self::Tests => "Tests",
        }
    }

    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.slug() == slug)
    }

    pub fn rules(self) -> impl Iterator<Item = RuleId> {
        RuleId::ALL
            .iter()
            .copied()
            .filter(move |rule| rule.spec().category == self)
    }
}

/// How much of the engine a rule needs before it can say anything. Ordered, because
/// the module graph is built from the facts the semantic pass collects, so a rule
/// that wants modules wants semantics too.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Analysis {
    /// The AST alone.
    Syntax,
    /// The scope resolver, and the collectors built on it.
    Semantics,
    /// The resolved import graph across every file in the project.
    Modules,
}

/// Whether a rule's fix is an edit rqx can make or a decision only a person can.
/// Declared per rule rather than derived from `fixes`, because `RemoveDeadCode` is
/// prescribed by seven rules and deleting an unused local is safe where deleting an
/// empty catch changes which exceptions propagate.
#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Autofix {
    /// No edit is right without a design decision.
    Refactoring,
    /// Removes source nothing reaches.
    Deletion,
    /// Rewrites live source to a form that behaves identically.
    Rewrite,
}

impl Autofix {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Refactoring => "refactoring",
            Self::Deletion => "deletion",
            Self::Rewrite => "rewrite",
        }
    }

    pub const fn is_automatic(self) -> bool {
        !matches!(self, Self::Refactoring)
    }

    pub fn parse(slug: &str) -> Option<Self> {
        [Self::Refactoring, Self::Deletion, Self::Rewrite]
            .into_iter()
            .find(|kind| kind.slug() == slug)
    }
}

/// A rule that reaches output without every field of this struct is a rule whose
/// findings cannot be traced to the catalogue they claim, which is the failure mode
/// Rqx exists to avoid.
pub struct RuleSpec {
    pub id: RuleId,
    pub slug: &'static str,
    pub category: Category,
    /// Declared per rule rather than guessed from its family. The gate used to test
    /// three categories, while the collectors behind it also supply every Safety,
    /// Tests and Naming rule — so disabling Waste silently stopped them.
    pub analysis: Analysis,
    pub metric: &'static str,
    pub fixes: &'static [FixTechnique],
    /// What `rqx fix` may do about it. Defaults to `Refactoring`, so a new rule is
    /// never fixable by accident.
    pub autofix: Autofix,
    pub confidence: Confidence,
    pub severity: Severity,
    pub why: &'static str,
    /// Ships disabled: rules whose signal is real but noisy, or which encode a
    /// convention a given project may not share.
    pub off_by_default: bool,
    /// Languages this rule cannot exist in, each with the reason. Go has no
    /// inheritance, so Refused Bequest has nothing to measure there — and a rule that
    /// silently never fires is indistinguishable from a broken one, which is what
    /// declaring the exemption prevents.
    pub not_applicable: &'static [(Language, &'static str)],
}

/// Declares the whole catalogue in one table. Every field is mandatory, so a rule
/// that omits its metric or its fix fails to compile — the traceability
/// gate is the type system, not a review checklist.
macro_rules! catalogue {
    ($(
        $variant:ident {
            slug: $slug:literal,
            category: $category:ident,
            analysis: $analysis:ident,
            metric: $metric:literal,
            fixes: [$($fix:ident),+ $(,)?],
            confidence: $confidence:ident,
            severity: $severity:ident,
            why: $why:literal,
            $(autofix: $autofix:ident,)?
            $(off_by_default: $off:literal,)?
            $(not_applicable: [$(($language:ident, $reason:literal)),+ $(,)?],)?
        }
    ),+ $(,)?) => {
        #[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum RuleId { $($variant),+ }

        impl RuleId {
            pub const ALL: &'static [RuleId] = &[$(RuleId::$variant),+];

            pub const fn spec(self) -> &'static RuleSpec {
                match self {
                    $(Self::$variant => &RuleSpec {
                        id: Self::$variant,
                        slug: $slug,
                        category: Category::$category,
                        analysis: Analysis::$analysis,
                        metric: $metric,
                        fixes: &[$(FixTechnique::$fix),+],
                        autofix: catalogue!(@autofix $($autofix)?),
                        confidence: Confidence::$confidence,
                        severity: Severity::$severity,
                        why: $why,
                        off_by_default: catalogue!(@off $($off)?),
                        not_applicable: &[$($((Language::$language, $reason)),+)?],
                    }),+
                }
            }
        }
    };
    (@autofix) => { Autofix::Refactoring };
    (@autofix $variant:ident) => { Autofix::$variant };
    (@off) => { false };
    (@off $value:literal) => { $value };
}

impl RuleId {
    /// The rules one file's re-analysis cannot re-derive: the ones that read the
    /// import graph, and three that are computed across files by the scan although
    /// they declare `Syntax` — clones, and the two churn rules. Listed by id because
    /// `analysis` alone would miss those three.
    pub const PROJECT_SCOPED: [Self; 10] = [
        Self::ParallelInheritance,
        Self::UnusedExport,
        Self::UnusedFile,
        Self::SpeculativeGenerality,
        Self::CircularDependency,
        Self::LayerViolation,
        Self::ForbiddenImport,
        Self::DuplicateCode,
        Self::DivergentChange,
        Self::ShotgunSurgery,
    ];

    pub fn is_project_scoped(self) -> bool {
        Self::PROJECT_SCOPED.contains(&self)
    }

    /// Whether this rule has anything to measure in a language.
    pub fn applies_to(self, language: Language) -> bool {
        self.exemption(language).is_none()
    }

    /// Why it does not, when it does not.
    pub fn exemption(self, language: Language) -> Option<&'static str> {
        self.spec()
            .not_applicable
            .iter()
            .find(|(candidate, _)| *candidate == language)
            .map(|(_, reason)| *reason)
    }

    pub fn for_language(language: Language) -> impl Iterator<Item = RuleId> {
        Self::ALL
            .iter()
            .copied()
            .filter(move |rule| rule.applies_to(language))
    }
}

include!("catalogue.rs");

impl RuleId {
    pub const fn slug(self) -> &'static str {
        self.spec().slug
    }

    pub fn parse(slug: &str) -> Option<Self> {
        RuleId::ALL.iter().copied().find(|rule| rule.slug() == slug)
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_rule_has_a_traceable_spec() {
        for rule in RuleId::ALL.iter().copied() {
            let spec = rule.spec();
            assert_eq!(spec.id, rule, "spec for {rule} is wired to the wrong id");
            assert!(!spec.slug.is_empty());
            assert!(!spec.metric.is_empty(), "{rule} names no computable metric");
            assert!(!spec.fixes.is_empty(), "{rule} suggests no fix technique");
            assert!(!spec.why.is_empty(), "{rule} explains nothing");
            assert_eq!(RuleId::parse(spec.slug), Some(rule));
        }
    }

    /// A run of spaces inside a sentence is what a badly wrapped string literal leaves
    /// behind, and it reaches the reader verbatim in every reporter.
    #[test]
    fn no_rule_text_carries_the_marks_of_a_bad_line_wrap() {
        for rule in RuleId::ALL.iter().copied() {
            let spec = rule.spec();
            for (field, text) in [("why", spec.why), ("metric", spec.metric)] {
                assert!(
                    !text.contains("  "),
                    "{rule}'s {field} has a doubled space: {text}"
                );
                assert_eq!(text.trim(), text, "{rule}'s {field} is padded");
            }
        }
    }

    #[test]
    fn slugs_are_unique_and_kebab_case() {
        let mut seen = BTreeSet::new();
        for rule in RuleId::ALL.iter().copied() {
            let slug = rule.slug();
            assert!(seen.insert(slug), "duplicate slug {slug}");
            assert!(
                slug.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{slug} is not kebab-case"
            );
        }
    }

    #[test]
    fn project_scoped_names_every_modules_rule_and_only_the_three_collector_rules_besides() {
        for rule in RuleId::ALL.iter().copied() {
            let modules = rule.spec().analysis == Analysis::Modules;
            let collector = matches!(
                rule,
                RuleId::DuplicateCode | RuleId::DivergentChange | RuleId::ShotgunSurgery
            );
            assert_eq!(rule.is_project_scoped(), modules || collector, "{rule}");
        }
    }

    #[test]
    fn every_category_has_at_least_one_rule() {
        for category in Category::ALL {
            assert!(
                category.rules().next().is_some(),
                "{} has no rules; drop the category or fill it",
                category.title()
            );
        }
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn every_exemption_gives_a_reason_and_names_a_language_once() {
        for rule in RuleId::ALL {
            let mut seen = Vec::new();
            for (language, reason) in rule.spec().not_applicable {
                assert!(
                    !reason.trim().is_empty(),
                    "{} exempts {language:?} with no reason",
                    rule.slug()
                );
                assert!(
                    !seen.contains(language),
                    "{} names {language:?} twice",
                    rule.slug()
                );
                seen.push(*language);
            }
            assert!(
                seen.len() < Language::ALL.len(),
                "{} applies to no language at all",
                rule.slug()
            );
        }
    }

    #[test]
    fn typescript_is_the_reference_and_exempts_nothing() {
        let exempt: Vec<&str> = RuleId::ALL
            .iter()
            .filter(|rule| !rule.applies_to(Language::TypeScript))
            .map(|rule| rule.slug())
            .collect();
        assert!(
            exempt.is_empty(),
            "TypeScript should run every rule: {exempt:?}"
        );
    }
}

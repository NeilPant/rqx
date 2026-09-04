use std::fmt;

use serde::{Deserialize, Serialize};

use crate::finding::{Confidence, Severity};
use crate::heuristic::Heuristic;
use crate::language::Language;
use crate::smell::Smell;
use crate::technique::FixTechnique;

/// The refactoring.guru grouping, kept as the top-level organisation because it is
/// the one a reader can hold in their head: five families, not eighty-eight rules.
#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Bloaters,
    OoAbusers,
    ChangePreventers,
    Dispensables,
    Couplers,
    Naming,
    Safety,
    Tests,
}

impl Category {
    pub const ALL: [Self; 8] = [
        Self::Bloaters,
        Self::OoAbusers,
        Self::ChangePreventers,
        Self::Dispensables,
        Self::Couplers,
        Self::Naming,
        Self::Safety,
        Self::Tests,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Bloaters => "bloaters",
            Self::OoAbusers => "oo-abusers",
            Self::ChangePreventers => "change-preventers",
            Self::Dispensables => "dispensables",
            Self::Couplers => "couplers",
            Self::Naming => "naming",
            Self::Safety => "safety",
            Self::Tests => "tests",
        }
    }

    /// The last three are not refactoring.guru families. Saying so is the point:
    /// inventing a catalogue entry to make the table look tidy would be the exact
    /// false traceability this tool exists to avoid.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Bloaters => "Bloaters",
            Self::OoAbusers => "Object-Orientation Abusers",
            Self::ChangePreventers => "Change Preventers",
            Self::Dispensables => "Dispensables",
            Self::Couplers => "Couplers",
            Self::Naming => "Naming (Clean Code ch. 2)",
            Self::Safety => "Safety (Clean Code ch. 17 general)",
            Self::Tests => "Tests (Clean Code ch. 9)",
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

/// A rule that reaches output without every field of this struct is a rule whose
/// findings cannot be traced to the catalogue they claim, which is the failure mode
/// Rqx exists to avoid.
pub struct RuleSpec {
    pub id: RuleId,
    pub slug: &'static str,
    pub category: Category,
    /// Declared per rule rather than guessed from its family. The gate used to test
    /// three categories, while the collectors behind it also supply every Safety,
    /// Tests and Naming rule — so disabling Dispensables silently stopped them.
    pub analysis: Analysis,
    /// `None` where refactoring.guru genuinely has no matching smell. Stretching a
    /// category to fill the field would make the citation worthless.
    pub guru_smell: Option<Smell>,
    /// Typed rather than written out, so a citation cannot name a heuristic that does
    /// not exist and cannot drift from the coverage table that counts it.
    pub clean_code: &'static [Heuristic],
    /// A principle from outside chapter 17's numbered list, for the two rules whose
    /// justification is a chapter rather than a heuristic.
    pub principle: Option<&'static str>,
    pub metric: &'static str,
    pub fixes: &'static [FixTechnique],
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
/// that omits its smell, its metric or its fix fails to compile — the traceability
/// gate is the type system, not a review checklist.
macro_rules! catalogue {
    ($(
        $variant:ident {
            slug: $slug:literal,
            category: $category:ident,
            analysis: $analysis:ident,
            guru: $guru:expr,
            clean_code: [$($heuristic:ident),* $(,)?],
            metric: $metric:literal,
            fixes: [$($fix:ident),+ $(,)?],
            confidence: $confidence:ident,
            severity: $severity:ident,
            $(principle: $principle:literal,)?
            why: $why:literal,
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
                        guru_smell: $guru,
                        clean_code: &[$(Heuristic::$heuristic),*],
                        principle: catalogue!(@principle $($principle)?),
                        metric: $metric,
                        fixes: &[$(FixTechnique::$fix),+],
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
    (@off) => { false };
    (@off $value:literal) => { $value };
    (@principle) => { None };
    (@principle $value:literal) => { Some($value) };
}

impl RuleId {
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

impl RuleSpec {
    /// The citation as a reader expects to see it: the heuristics it names, then the
    /// principle for the rules whose justification is a chapter rather than a number.
    pub fn citation(&self) -> String {
        let mut parts: Vec<String> = self.clean_code.iter().map(ToString::to_string).collect();
        parts.extend(self.principle.map(ToOwned::to_owned));
        parts.join("; ")
    }
}

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
            assert!(
                !spec.clean_code.is_empty() || spec.principle.is_some(),
                "{rule} cites neither a heuristic nor a principle"
            );
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

    /// The two directions have to agree. A rule citing a heuristic the coverage table
    /// does not credit it for — or a table crediting a rule that cites something else —
    /// is exactly the drift that let five rules pile onto `G12 Clutter`.
    #[test]
    fn heuristic_coverage_matches_what_the_rules_cite() {
        for entry in crate::heuristic::Heuristic::ALL.iter().copied() {
            let claimed: BTreeSet<RuleId> = entry.coverage().rules().iter().copied().collect();
            let citing: BTreeSet<RuleId> = RuleId::ALL
                .iter()
                .copied()
                .filter(|rule| rule.spec().clean_code.contains(&entry))
                .collect();
            assert_eq!(
                claimed, citing,
                "{entry} is credited to {claimed:?} but cited by {citing:?}"
            );
        }
    }

    #[test]
    fn smell_coverage_matches_what_the_rules_cite() {
        for smell in crate::smell::Smell::ALL.iter().copied() {
            let claimed: BTreeSet<RuleId> = smell.coverage().rules().iter().copied().collect();
            let citing: BTreeSet<RuleId> = RuleId::ALL
                .iter()
                .copied()
                .filter(|rule| rule.spec().guru_smell == Some(smell))
                .collect();
            assert_eq!(
                claimed, citing,
                "{smell} is credited to {claimed:?} but cited by {citing:?}"
            );
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

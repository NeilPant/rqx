use std::fmt;

use serde::{Deserialize, Serialize};

use crate::finding::{Confidence, Severity};

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
        Self::ALL.into_iter().find(|category| category.slug() == slug)
    }

    pub fn rules(self) -> impl Iterator<Item = RuleId> {
        RuleId::ALL.iter().copied().filter(move |rule| rule.spec().category == self)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FixTechnique {
    ConsolidateConditional,
    DecomposeConditional,
    DeleteFile,
    EncapsulateField,
    ExtractClass,
    ExtractInterface,
    ExtractMethod,
    ExtractSharedModule,
    ExtractSubclass,
    ExtractVariable,
    HideDelegate,
    InlineClass,
    IntroduceParameterObject,
    InvertDependency,
    MoveMethod,
    PreserveWholeObject,
    PullUpMethod,
    PushDownMethod,
    RemoveAssignmentsToParameters,
    RemoveDeadCode,
    RemoveMiddleMan,
    RemoveParameter,
    RenameVariable,
    ReplaceConditionalWithPolymorphism,
    ReplaceMagicNumberWithConstant,
    ReplaceNestedConditionalWithGuardClauses,
    ReplaceParameterWithExplicitMethods,
    ReplaceTypeCodeWithClass,
    SeparateQueryFromModifier,
    CollapseHierarchy,
}

impl FixTechnique {
    /// Names are verbatim from refactoring.guru's catalogue so a finding can be
    /// looked up there directly.
    pub const fn name(self) -> &'static str {
        match self {
            Self::ConsolidateConditional => "Consolidate Conditional Expression",
            Self::DecomposeConditional => "Decompose Conditional",
            Self::DeleteFile => "Delete File",
            Self::EncapsulateField => "Encapsulate Field",
            Self::ExtractClass => "Extract Class",
            Self::ExtractInterface => "Extract Interface",
            Self::ExtractMethod => "Extract Method",
            Self::ExtractSharedModule => "Extract Shared Module",
            Self::ExtractSubclass => "Extract Subclass",
            Self::ExtractVariable => "Extract Variable",
            Self::HideDelegate => "Hide Delegate",
            Self::InlineClass => "Inline Class",
            Self::IntroduceParameterObject => "Introduce Parameter Object",
            Self::InvertDependency => "Invert Dependency",
            Self::MoveMethod => "Move Method",
            Self::PreserveWholeObject => "Preserve Whole Object",
            Self::PullUpMethod => "Pull Up Method",
            Self::PushDownMethod => "Push Down Method",
            Self::RemoveAssignmentsToParameters => "Remove Assignments to Parameters",
            Self::RemoveDeadCode => "Remove Dead Code",
            Self::RemoveMiddleMan => "Remove Middle Man",
            Self::RemoveParameter => "Remove Parameter",
            Self::RenameVariable => "Rename Variable",
            Self::ReplaceConditionalWithPolymorphism => "Replace Conditional with Polymorphism",
            Self::ReplaceMagicNumberWithConstant => "Replace Magic Number with Symbolic Constant",
            Self::ReplaceNestedConditionalWithGuardClauses => {
                "Replace Nested Conditional with Guard Clauses"
            }
            Self::ReplaceParameterWithExplicitMethods => "Replace Parameter with Explicit Methods",
            Self::ReplaceTypeCodeWithClass => "Replace Type Code with Class",
            Self::SeparateQueryFromModifier => "Separate Query from Modifier",
            Self::CollapseHierarchy => "Collapse Hierarchy",
        }
    }
}

impl fmt::Display for FixTechnique {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A rule that reaches output without every field of this struct is a rule whose
/// findings cannot be traced to the catalogue they claim, which is the failure mode
/// Rqx exists to avoid.
pub struct RuleSpec {
    pub id: RuleId,
    pub slug: &'static str,
    pub category: Category,
    /// `None` where refactoring.guru genuinely has no matching smell. Stretching a
    /// category to fill the field would make the citation worthless.
    pub guru_smell: Option<&'static str>,
    pub clean_code: &'static str,
    pub metric: &'static str,
    pub fixes: &'static [FixTechnique],
    pub confidence: Confidence,
    pub severity: Severity,
    pub why: &'static str,
    /// Ships disabled: rules whose signal is real but noisy, or which encode a
    /// convention a given project may not share.
    pub off_by_default: bool,
}

/// Declares the whole catalogue in one table. Every field is mandatory, so a rule
/// that omits its smell, its metric or its fix fails to compile — the traceability
/// gate is the type system, not a review checklist.
macro_rules! catalogue {
    ($(
        $variant:ident {
            slug: $slug:literal,
            category: $category:ident,
            guru: $guru:expr,
            clean_code: $clean:literal,
            metric: $metric:literal,
            fixes: [$($fix:ident),+ $(,)?],
            confidence: $confidence:ident,
            severity: $severity:ident,
            why: $why:literal,
            $(off_by_default: $off:literal,)?
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
                        guru_smell: $guru,
                        clean_code: $clean,
                        metric: $metric,
                        fixes: &[$(FixTechnique::$fix),+],
                        confidence: Confidence::$confidence,
                        severity: Severity::$severity,
                        why: $why,
                        off_by_default: catalogue!(@off $($off)?),
                    }),+
                }
            }
        }
    };
    (@off) => { false };
    (@off $value:literal) => { $value };
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
            assert!(!spec.clean_code.is_empty(), "{rule} cites no Clean Code heuristic");
            assert!(!spec.metric.is_empty(), "{rule} names no computable metric");
            assert!(!spec.fixes.is_empty(), "{rule} suggests no fix technique");
            assert!(!spec.why.is_empty(), "{rule} explains nothing");
            assert_eq!(RuleId::parse(spec.slug), Some(rule));
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

    /// A citation is only worth anything if it points at a real entry, so every
    /// Clean Code reference must carry an identifier from the book's own scheme.
    #[test]
    fn clean_code_citations_name_a_real_heuristic() {
        for rule in RuleId::ALL.iter().copied() {
            let citation = rule.spec().clean_code;
            let has_identifier = citation.split_whitespace().next().is_some_and(|first| {
                let mut chars = first.chars();
                matches!(chars.next(), Some('C' | 'E' | 'F' | 'G' | 'N' | 'T'))
                    && chars.next().is_some_and(|c| c.is_ascii_digit())
            }) || citation.starts_with("Ch.")
                || citation.starts_with("Acyclic");
            assert!(has_identifier, "{rule} cites `{citation}`, which names no heuristic");
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

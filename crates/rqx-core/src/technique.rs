use std::fmt;

/// Where a technique's name comes from. The catalogue used to claim every name was
/// verbatim from refactoring.guru; five of them never were, and a field is harder to
/// let drift than a doc comment.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Guru,
    /// Fowler's second edition, which refactoring.guru's catalogue predates.
    Fowler,
    /// Named by Rqx because the move it describes has no catalogue entry.
    Rqx,
}

impl Origin {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Guru => "refactoring.guru",
            Self::Fowler => "Refactoring, 2nd ed.",
            Self::Rqx => "rqx",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    ComposingMethods,
    MovingFeatures,
    OrganizingData,
    SimplifyingConditionals,
    SimplifyingCalls,
    Generalization,
    /// Not one of refactoring.guru's six groups, and labelled so.
    Outside,
}

impl Group {
    pub const ALL: [Self; 7] = [
        Self::ComposingMethods,
        Self::MovingFeatures,
        Self::OrganizingData,
        Self::SimplifyingConditionals,
        Self::SimplifyingCalls,
        Self::Generalization,
        Self::Outside,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::ComposingMethods => "Composing Methods",
            Self::MovingFeatures => "Moving Features Between Objects",
            Self::OrganizingData => "Organizing Data",
            Self::SimplifyingConditionals => "Simplifying Conditional Expressions",
            Self::SimplifyingCalls => "Simplifying Method Calls",
            Self::Generalization => "Dealing with Generalization",
            Self::Outside => "Outside the catalogue",
        }
    }

    pub fn techniques(self) -> impl Iterator<Item = FixTechnique> {
        FixTechnique::ALL
            .iter()
            .copied()
            .filter(move |technique| technique.group() == self)
    }
}

/// The whole technique catalogue, not only the entries some rule happens to cite.
/// A rule can only prescribe a fix that exists here, and the ones nothing prescribes
/// are still browsable — an incomplete list is what let the wrong fix get attached to
/// eleven rules.
macro_rules! techniques {
    ($(
        $variant:ident { name: $name:literal, group: $group:ident, origin: $origin:ident }
    ),+ $(,)?) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum FixTechnique { $($variant),+ }

        impl FixTechnique {
            pub const ALL: &'static [FixTechnique] = &[$(FixTechnique::$variant),+];

            pub const fn name(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }

            pub const fn group(self) -> Group {
                match self { $(Self::$variant => Group::$group),+ }
            }

            pub const fn origin(self) -> Origin {
                match self { $(Self::$variant => Origin::$origin),+ }
            }
        }
    };
}

techniques! {
    ExtractMethod { name: "Extract Method", group: ComposingMethods, origin: Guru },
    InlineMethod { name: "Inline Method", group: ComposingMethods, origin: Guru },
    ExtractVariable { name: "Extract Variable", group: ComposingMethods, origin: Guru },
    InlineTemp { name: "Inline Temp", group: ComposingMethods, origin: Guru },
    ReplaceTempWithQuery { name: "Replace Temp with Query", group: ComposingMethods, origin: Guru },
    SplitTemporaryVariable { name: "Split Temporary Variable", group: ComposingMethods, origin: Guru },
    RemoveAssignmentsToParameters { name: "Remove Assignments to Parameters", group: ComposingMethods, origin: Guru },
    ReplaceMethodWithMethodObject { name: "Replace Method with Method Object", group: ComposingMethods, origin: Guru },
    SubstituteAlgorithm { name: "Substitute Algorithm", group: ComposingMethods, origin: Guru },

    MoveMethod { name: "Move Method", group: MovingFeatures, origin: Guru },
    MoveField { name: "Move Field", group: MovingFeatures, origin: Guru },
    ExtractClass { name: "Extract Class", group: MovingFeatures, origin: Guru },
    InlineClass { name: "Inline Class", group: MovingFeatures, origin: Guru },
    HideDelegate { name: "Hide Delegate", group: MovingFeatures, origin: Guru },
    RemoveMiddleMan { name: "Remove Middle Man", group: MovingFeatures, origin: Guru },
    IntroduceForeignMethod { name: "Introduce Foreign Method", group: MovingFeatures, origin: Guru },
    IntroduceLocalExtension { name: "Introduce Local Extension", group: MovingFeatures, origin: Guru },

    ChangeValueToReference { name: "Change Value to Reference", group: OrganizingData, origin: Guru },
    ChangeReferenceToValue { name: "Change Reference to Value", group: OrganizingData, origin: Guru },
    DuplicateObservedData { name: "Duplicate Observed Data", group: OrganizingData, origin: Guru },
    SelfEncapsulateField { name: "Self Encapsulate Field", group: OrganizingData, origin: Guru },
    ReplaceDataValueWithObject { name: "Replace Data Value with Object", group: OrganizingData, origin: Guru },
    ReplaceArrayWithObject { name: "Replace Array with Object", group: OrganizingData, origin: Guru },
    ChangeUnidirectionalAssociationToBidirectional { name: "Change Unidirectional Association to Bidirectional", group: OrganizingData, origin: Guru },
    ChangeBidirectionalAssociationToUnidirectional { name: "Change Bidirectional Association to Unidirectional", group: OrganizingData, origin: Guru },
    EncapsulateField { name: "Encapsulate Field", group: OrganizingData, origin: Guru },
    EncapsulateCollection { name: "Encapsulate Collection", group: OrganizingData, origin: Guru },
    ReplaceMagicNumberWithConstant { name: "Replace Magic Number with Symbolic Constant", group: OrganizingData, origin: Guru },
    ReplaceTypeCodeWithClass { name: "Replace Type Code with Class", group: OrganizingData, origin: Guru },
    ReplaceTypeCodeWithSubclasses { name: "Replace Type Code with Subclasses", group: OrganizingData, origin: Guru },
    ReplaceTypeCodeWithStateStrategy { name: "Replace Type Code with State/Strategy", group: OrganizingData, origin: Guru },
    ReplaceSubclassWithFields { name: "Replace Subclass with Fields", group: OrganizingData, origin: Guru },

    ConsolidateConditional { name: "Consolidate Conditional Expression", group: SimplifyingConditionals, origin: Guru },
    ConsolidateDuplicateConditionalFragments { name: "Consolidate Duplicate Conditional Fragments", group: SimplifyingConditionals, origin: Guru },
    DecomposeConditional { name: "Decompose Conditional", group: SimplifyingConditionals, origin: Guru },
    ReplaceConditionalWithPolymorphism { name: "Replace Conditional with Polymorphism", group: SimplifyingConditionals, origin: Guru },
    RemoveControlFlag { name: "Remove Control Flag", group: SimplifyingConditionals, origin: Guru },
    ReplaceNestedConditionalWithGuardClauses { name: "Replace Nested Conditional with Guard Clauses", group: SimplifyingConditionals, origin: Guru },
    IntroduceNullObject { name: "Introduce Null Object", group: SimplifyingConditionals, origin: Guru },
    IntroduceAssertion { name: "Introduce Assertion", group: SimplifyingConditionals, origin: Guru },

    AddParameter { name: "Add Parameter", group: SimplifyingCalls, origin: Guru },
    RemoveParameter { name: "Remove Parameter", group: SimplifyingCalls, origin: Guru },
    RenameMethod { name: "Rename Method", group: SimplifyingCalls, origin: Guru },
    ReplaceParameterWithExplicitMethods { name: "Replace Parameter with Explicit Methods", group: SimplifyingCalls, origin: Guru },
    ParameterizeMethod { name: "Parameterize Method", group: SimplifyingCalls, origin: Guru },
    IntroduceParameterObject { name: "Introduce Parameter Object", group: SimplifyingCalls, origin: Guru },
    PreserveWholeObject { name: "Preserve Whole Object", group: SimplifyingCalls, origin: Guru },
    RemoveSettingMethod { name: "Remove Setting Method", group: SimplifyingCalls, origin: Guru },
    ReplaceParameterWithMethodCall { name: "Replace Parameter with Method Call", group: SimplifyingCalls, origin: Guru },
    HideMethod { name: "Hide Method", group: SimplifyingCalls, origin: Guru },
    ReplaceConstructorWithFactoryMethod { name: "Replace Constructor with Factory Method", group: SimplifyingCalls, origin: Guru },
    ReplaceErrorCodeWithException { name: "Replace Error Code with Exception", group: SimplifyingCalls, origin: Guru },
    ReplaceExceptionWithTest { name: "Replace Exception with Test", group: SimplifyingCalls, origin: Guru },
    SeparateQueryFromModifier { name: "Separate Query from Modifier", group: SimplifyingCalls, origin: Guru },

    PullUpField { name: "Pull Up Field", group: Generalization, origin: Guru },
    PullUpMethod { name: "Pull Up Method", group: Generalization, origin: Guru },
    PullUpConstructorBody { name: "Pull Up Constructor Body", group: Generalization, origin: Guru },
    PushDownField { name: "Push Down Field", group: Generalization, origin: Guru },
    PushDownMethod { name: "Push Down Method", group: Generalization, origin: Guru },
    ExtractSubclass { name: "Extract Subclass", group: Generalization, origin: Guru },
    ExtractSuperclass { name: "Extract Superclass", group: Generalization, origin: Guru },
    ExtractInterface { name: "Extract Interface", group: Generalization, origin: Guru },
    CollapseHierarchy { name: "Collapse Hierarchy", group: Generalization, origin: Guru },
    FormTemplateMethod { name: "Form Template Method", group: Generalization, origin: Guru },
    ReplaceInheritanceWithDelegation { name: "Replace Inheritance with Delegation", group: Generalization, origin: Guru },
    ReplaceDelegationWithInheritance { name: "Replace Delegation with Inheritance", group: Generalization, origin: Guru },

    RemoveDeadCode { name: "Remove Dead Code", group: Outside, origin: Fowler },
    RenameVariable { name: "Rename Variable", group: Outside, origin: Fowler },
    DeleteFile { name: "Delete File", group: Outside, origin: Rqx },
    ExtractSharedModule { name: "Extract Shared Module", group: Outside, origin: Rqx },
    InvertDependency { name: "Invert Dependency", group: Outside, origin: Rqx },
}

impl fmt::Display for FixTechnique {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn the_guru_catalogue_is_complete() {
        let guru = FixTechnique::ALL
            .iter()
            .filter(|technique| technique.origin() == Origin::Guru)
            .count();
        assert_eq!(guru, 66, "refactoring.guru's catalogue has 66 techniques");
    }

    #[test]
    fn each_guru_group_holds_the_count_the_catalogue_publishes() {
        for (group, expected) in [
            (Group::ComposingMethods, 9),
            (Group::MovingFeatures, 8),
            (Group::OrganizingData, 15),
            (Group::SimplifyingConditionals, 8),
            (Group::SimplifyingCalls, 14),
            (Group::Generalization, 12),
        ] {
            assert_eq!(group.techniques().count(), expected, "{}", group.title());
        }
    }

    /// Only the entries in `Outside` may claim an origin that is not the guru
    /// catalogue; anything else silently misattributes its name.
    #[test]
    fn only_the_outside_group_departs_from_the_catalogue() {
        for technique in FixTechnique::ALL.iter().copied() {
            assert_eq!(
                technique.origin() == Origin::Guru,
                technique.group() != Group::Outside,
                "{technique} claims {:?} in {}",
                technique.origin(),
                technique.group().title()
            );
        }
    }

    #[test]
    fn names_are_unique_and_present() {
        let mut seen = BTreeSet::new();
        for technique in FixTechnique::ALL.iter().copied() {
            assert!(!technique.name().is_empty(), "{technique:?} has no name");
            assert!(
                seen.insert(technique.name()),
                "duplicate name {}",
                technique.name()
            );
        }
    }
}

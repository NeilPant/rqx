use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Extraction,
    Placement,
    Data,
    Conditionals,
    Signatures,
    Hierarchy,
    Removal,
}

impl Group {
    pub const ALL: [Self; 7] = [
        Self::Extraction,
        Self::Placement,
        Self::Data,
        Self::Conditionals,
        Self::Signatures,
        Self::Hierarchy,
        Self::Removal,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Extraction => "Extraction",
            Self::Placement => "Placement",
            Self::Data => "Data",
            Self::Conditionals => "Conditionals",
            Self::Signatures => "Signatures",
            Self::Hierarchy => "Hierarchy",
            Self::Removal => "Removal",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::Extraction => "pull a unit out, or fold one back in",
            Self::Placement => "move a member to the type that uses it",
            Self::Data => "give state a shape and a gatekeeper",
            Self::Conditionals => "make a branch say what it decides",
            Self::Signatures => "change what a caller has to know",
            Self::Hierarchy => "move behaviour up or down a type tree",
            Self::Removal => "delete what nothing reaches",
        }
    }

    pub fn techniques(self) -> impl Iterator<Item = FixTechnique> {
        FixTechnique::ALL
            .iter()
            .copied()
            .filter(move |technique| technique.group() == self)
    }
}

/// A rule can only prescribe a fix that exists here, so a fix cannot drift into a
/// name nothing defines.
macro_rules! techniques {
    ($(
        $variant:ident { name: $name:literal, group: $group:ident }
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
        }
    };
}

techniques! {
    ExtractMethod { name: "Extract Method", group: Extraction },
    ExtractVariable { name: "Extract Variable", group: Extraction },
    InlineVariable { name: "Inline Variable", group: Extraction },
    ExtractClass { name: "Extract Class", group: Extraction },
    InlineClass { name: "Inline Class", group: Extraction },
    ExtractSharedModule { name: "Extract Shared Module", group: Extraction },

    MoveMethod { name: "Move Method", group: Placement },
    MoveField { name: "Move Field", group: Placement },
    EncapsulateDelegation { name: "Encapsulate Delegation", group: Placement },
    InlineDelegation { name: "Inline Delegation", group: Placement },
    InvertDependency { name: "Invert Dependency", group: Placement },
    ReplaceInheritanceWithComposition { name: "Replace Inheritance with Composition", group: Placement },

    EncapsulateField { name: "Encapsulate Field", group: Data },
    ReplaceDataValueWithObject { name: "Replace Data Value with Object", group: Data },
    ReplaceMagicNumberWithConstant { name: "Replace Magic Number with Constant", group: Data },
    ReplaceTypeCodeWithClass { name: "Replace Type Code with Class", group: Data },
    ReplaceTypeCodeWithSubclasses { name: "Replace Type Code with Subclasses", group: Data },
    ReplaceTypeCodeWithPolymorphicState { name: "Replace Type Code with Polymorphic State", group: Data },

    DecomposeConditional { name: "Decompose Conditional", group: Conditionals },
    ConsolidateConditional { name: "Consolidate Conditional", group: Conditionals },
    ReplaceNestedConditionalWithGuardClauses { name: "Replace Nested Conditional with Guard Clauses", group: Conditionals },
    ReplaceConditionalWithPolymorphism { name: "Replace Conditional with Polymorphism", group: Conditionals },
    IntroduceAssertion { name: "Introduce Assertion", group: Conditionals },

    RemoveParameter { name: "Remove Parameter", group: Signatures },
    RenameMethod { name: "Rename Method", group: Signatures },
    RenameVariable { name: "Rename Variable", group: Signatures },
    IntroduceParameterObject { name: "Introduce Parameter Object", group: Signatures },
    PassWholeObject { name: "Pass Whole Object", group: Signatures },
    ReplaceParameterWithExplicitMethods { name: "Replace Parameter with Explicit Methods", group: Signatures },
    SeparateQueryFromModifier { name: "Separate Query from Modifier", group: Signatures },
    ReplaceAlgorithm { name: "Replace Algorithm", group: Signatures },
    RemoveAssignmentsToParameters { name: "Remove Assignments to Parameters", group: Signatures },
    AddParameter { name: "Add Parameter", group: Signatures },
    HideMethod { name: "Hide Method", group: Signatures },
    RemoveSetter { name: "Remove Setter", group: Signatures },

    PullUpMethod { name: "Pull Up Method", group: Hierarchy },
    PushDownMethod { name: "Push Down Method", group: Hierarchy },
    ExtractSubclass { name: "Extract Subclass", group: Hierarchy },
    ExtractSuperclass { name: "Extract Superclass", group: Hierarchy },
    ExtractInterface { name: "Extract Interface", group: Hierarchy },
    CollapseHierarchy { name: "Collapse Hierarchy", group: Hierarchy },

    RemoveDeadCode { name: "Remove Dead Code", group: Removal },
    DeleteFile { name: "Delete File", group: Removal },
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

    #[test]
    fn every_group_holds_at_least_one_technique() {
        for group in Group::ALL {
            assert!(
                group.techniques().next().is_some(),
                "{} is empty; drop it or fill it",
                group.title()
            );
        }
    }

    /// A technique nothing prescribes is a menu entry the report can never reach.
    #[test]
    fn every_technique_is_prescribed() {
        let prescribed: BTreeSet<FixTechnique> = crate::rule::RuleId::ALL
            .iter()
            .flat_map(|rule| rule.spec().fixes)
            .chain(
                crate::advice::AdviceId::ALL
                    .iter()
                    .flat_map(|advice| advice.spec().techniques),
            )
            .copied()
            .collect();
        for technique in FixTechnique::ALL {
            assert!(prescribed.contains(technique), "{technique} is unreachable");
        }
    }
}

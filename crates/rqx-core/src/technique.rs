use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Extraction,
    Placement,
    Data,
    Conditionals,
    Errors,
    Signatures,
    Hierarchy,
    Removal,
    Performance,
}

impl Group {
    pub const ALL: [Self; 9] = [
        Self::Extraction,
        Self::Placement,
        Self::Data,
        Self::Conditionals,
        Self::Errors,
        Self::Signatures,
        Self::Hierarchy,
        Self::Removal,
        Self::Performance,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Extraction => "Extraction",
            Self::Placement => "Placement",
            Self::Data => "Data",
            Self::Conditionals => "Conditionals",
            Self::Errors => "Errors",
            Self::Signatures => "Signatures",
            Self::Hierarchy => "Hierarchy",
            Self::Removal => "Removal",
            Self::Performance => "Performance",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::Extraction => "extract a unit, or fold one back in",
            Self::Placement => "move a member to the type that uses it",
            Self::Data => "give state a type and control access to it",
            Self::Conditionals => "restructure a conditional so it states what it decides",
            Self::Errors => "decide what happens to a failure, and where",
            Self::Signatures => "change what a caller must know about a signature",
            Self::Hierarchy => "move behaviour up or down a type hierarchy",
            Self::Removal => "delete code that nothing reaches",
            Self::Performance => "produce the same result at lower cost",
        }
    }

    /// Roughly what the move costs, for ranking work rather than pricing it. Deleting
    /// is one edit; moving behaviour up or down a type tree touches every subclass.
    /// Exhaustive so a new group has to state its cost rather than inherit one.
    pub const fn effort(self) -> u8 {
        match self {
            Self::Removal => 1,
            Self::Extraction | Self::Conditionals | Self::Errors | Self::Performance => 2,
            Self::Data | Self::Signatures => 3,
            Self::Placement => 4,
            Self::Hierarchy => 5,
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
    ShareOwnership { name: "Share Ownership", group: Placement },
    ReplaceInheritanceWithComposition { name: "Replace Inheritance with Composition", group: Placement },

    EncapsulateField { name: "Encapsulate Field", group: Data },
    ReplaceDataValueWithObject { name: "Replace Data Value with Object", group: Data },
    ReplaceMagicNumberWithConstant { name: "Replace Magic Number with Constant", group: Data },
    ReplaceTypeCodeWithClass { name: "Replace Type Code with Class", group: Data },
    ReplaceTypeCodeWithSubclasses { name: "Replace Type Code with Subclasses", group: Data },
    ReplaceTypeCodeWithPolymorphicState { name: "Replace Type Code with Polymorphic State", group: Data },
    DeclareImmutable { name: "Declare Immutable", group: Data },
    NarrowTheType { name: "Narrow the Type", group: Data },

    DecomposeConditional { name: "Decompose Conditional", group: Conditionals },
    ConsolidateConditional { name: "Consolidate Conditional", group: Conditionals },
    ReplaceNestedConditionalWithGuardClauses { name: "Replace Nested Conditional with Guard Clauses", group: Conditionals },
    ReplaceConditionalWithPolymorphism { name: "Replace Conditional with Polymorphism", group: Conditionals },
    IntroduceAssertion { name: "Introduce Assertion", group: Conditionals },

    HandleTheFailure { name: "Handle the Failure", group: Errors },
    NarrowTheHandler { name: "Narrow the Handler", group: Errors },
    ReplaceExitWithError { name: "Replace Exit with Error", group: Errors },

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

    ReplaceLoopWithLookup { name: "Replace Loop with Lookup", group: Performance },
    BatchTheWork { name: "Batch the Work", group: Performance },
    AwaitConcurrently { name: "Await Concurrently", group: Performance },
    HoistInvariant { name: "Hoist Invariant", group: Performance },
    AccumulateInPlace { name: "Accumulate in Place", group: Performance },
    Memoize { name: "Memoize", group: Performance },
    ReplaceBlockingCall { name: "Replace Blocking Call", group: Performance },
    PresizeCollection { name: "Presize the Collection", group: Performance },
    BoundConcurrency { name: "Bound Concurrency", group: Performance },
    BoundTheCache { name: "Bound the Cache", group: Performance },
    UsePrimitiveType { name: "Use a Primitive Type", group: Performance },
    TakeByReference { name: "Take by Reference", group: Performance },
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

    /// For each technique, the rules whose problem it is a fix for. A rule may prescribe a
    /// technique only if the rule is listed here, so a pairing is a decision somebody made
    /// rather than a default somebody left.
    const APPLIES_TO: &[(FixTechnique, &[&str])] = &[
        (
            FixTechnique::AccumulateInPlace,
            &["loop-concatenation", "accumulate-by-copy"],
        ),
        (
            FixTechnique::AddParameter,
            &[
                "closure-over-loop-variable",
                "ambient-dependency",
                "self-built-collaborators",
            ],
        ),
        (
            FixTechnique::AwaitConcurrently,
            &["sequential-await", "await-in-loop"],
        ),
        (
            FixTechnique::BatchTheWork,
            &["query-in-loop", "await-in-loop"],
        ),
        (FixTechnique::BoundConcurrency, &["unbounded-concurrency"]),
        (FixTechnique::BoundTheCache, &["unbounded-cache"]),
        (
            FixTechnique::CollapseHierarchy,
            &[
                "refused-bequest",
                "parallel-inheritance",
                "speculative-generality",
                "abstract-nothing",
                "passthrough-override",
                "deep-inheritance",
                "empty-subclass",
            ],
        ),
        (
            FixTechnique::ConsolidateConditional,
            &[
                "collapsible-if",
                "boolean-from-branch",
                "literal-comparison",
                "self-comparison",
                "identical-branches",
                "duplicate-condition",
                "silent-fallthrough",
                "ambiguous-precedence",
                "negated-comparison",
                "tiny-switch",
                "complex-condition",
            ],
        ),
        (
            FixTechnique::DeclareImmutable,
            &["never-reassigned", "assign-once-field"],
        ),
        (
            FixTechnique::DecomposeConditional,
            &[
                "cognitive-complexity",
                "many-exits",
                "nested-ternary",
                "branching-test",
                "cyclomatic-complexity",
                "negative-conditional",
            ],
        ),
        (FixTechnique::DeleteFile, &["unused-file"]),
        (
            FixTechnique::EncapsulateDelegation,
            &[
                "message-chain",
                "inappropriate-intimacy",
                "reaches-into-private",
            ],
        ),
        (
            FixTechnique::EncapsulateField,
            &["data-class", "global-accessor", "mutable-module-state"],
        ),
        (
            FixTechnique::ExtractClass,
            &[
                "large-class",
                "data-clump",
                "temporary-field",
                "hotspot",
                "shotgun-surgery",
                "divergent-change",
                "temporal-coupling",
                "misleading-pattern-name",
                "inappropriate-intimacy",
                "wide-surface",
                "split-cohesion",
                "many-results",
                "single-method-field",
                "many-collaborators",
                "oversized-module",
                "mutable-module-state",
            ],
        ),
        (
            FixTechnique::ExtractInterface,
            &["refused-bequest", "wide-surface", "wide-fan-in"],
        ),
        (
            FixTechnique::ExtractMethod,
            &[
                "long-function",
                "cognitive-complexity",
                "temporary-field",
                "hotspot",
                "duplicate-code",
                "boundary-arithmetic",
                "obscured-expression",
                "message-chain",
                "feature-envy",
                "many-exits",
                "wide-switch",
                "callback-nesting",
                "closure-over-loop-variable",
                "unlabelled-assertions",
                "duplicate-function",
                "nested-then",
                "eager-test",
                "branching-test",
                "explanation-longer-than-code",
                "blanket-try",
                "cyclomatic-complexity",
                "many-locals",
                "wide-try",
                "working-constructor",
                "mixed-return",
                "import-time-work",
                "defer-in-loop",
            ],
        ),
        (
            FixTechnique::ExtractSharedModule,
            &[
                "reimplemented-existing",
                "mixed-language-file",
                "circular-dependency",
                "cross-language-duplicate",
                "oversized-module",
                "deep-relative-import",
                "wide-fan-out",
            ],
        ),
        (FixTechnique::ExtractSubclass, &["large-class"]),
        (FixTechnique::ExtractSuperclass, &["alternative-classes"]),
        (
            FixTechnique::ExtractVariable,
            &[
                "magic-number",
                "boundary-arithmetic",
                "obscured-expression",
                "buried-configuration",
                "assignment-in-condition",
                "nested-ternary",
                "ambiguous-precedence",
                "complex-condition",
            ],
        ),
        (
            FixTechnique::HandleTheFailure,
            &[
                "empty-catch",
                "dropped-promise",
                "discarded-error",
                "exit-from-finally",
            ],
        ),
        (
            FixTechnique::HoistInvariant,
            &[
                "invariant-in-loop",
                "sort-in-loop",
                "allocation-in-loop",
                "lock-in-loop",
                "repeated-computation-in-loop",
            ],
        ),
        (
            FixTechnique::InlineClass,
            &[
                "lazy-class",
                "speculative-generality",
                "middle-man",
                "abstract-nothing",
                "empty-subclass",
                "static-only-class",
            ],
        ),
        (
            FixTechnique::InlineDelegation,
            &["middle-man", "barrel-module", "passthrough-function"],
        ),
        (
            FixTechnique::InlineVariable,
            &[
                "distant-declaration",
                "return-of-fresh-local",
                "many-locals",
            ],
        ),
        (
            FixTechnique::IntroduceAssertion,
            &[
                "imprecise-type",
                "temporal-coupling",
                "no-default-branch",
                "self-asserting-test",
                "empty-test",
                "string-shape-assertion",
                "unchecked-stub",
                "hand-rolled-throws",
                "guard-cascade",
                "assertionless-test",
            ],
        ),
        (
            FixTechnique::IntroduceParameterObject,
            &[
                "long-parameter-list",
                "data-clump",
                "guard-cascade",
                "many-results",
            ],
        ),
        (
            FixTechnique::InvertDependency,
            &[
                "base-depends-on-derived",
                "slow-test",
                "circular-dependency",
                "layer-violation",
                "forbidden-import",
                "real-world-test",
                "global-accessor",
                "many-collaborators",
                "deep-relative-import",
                "ambient-dependency",
                "self-built-collaborators",
                "wide-fan-out",
            ],
        ),
        (FixTechnique::Memoize, &["branching-recursion"]),
        (FixTechnique::MoveField, &["parallel-inheritance"]),
        (
            FixTechnique::MoveMethod,
            &[
                "inappropriate-static",
                "shotgun-surgery",
                "parallel-inheritance",
                "data-class",
                "feature-envy",
                "inappropriate-intimacy",
                "alternative-classes",
                "split-cohesion",
                "method-ignores-instance",
                "single-method-field",
                "static-only-class",
                "reaches-into-private",
            ],
        ),
        (
            FixTechnique::NarrowTheHandler,
            &[
                "shadowed-handler",
                "blanket-try",
                "wide-try",
                "catch-all-handler",
            ],
        ),
        (FixTechnique::NarrowTheType, &["imprecise-type"]),
        (FixTechnique::PassWholeObject, &["long-parameter-list"]),
        (FixTechnique::PresizeCollection, &["missing-capacity"]),
        (FixTechnique::PullUpMethod, &["duplicate-code"]),
        (
            FixTechnique::PushDownMethod,
            &["refused-bequest", "base-depends-on-derived"],
        ),
        (
            FixTechnique::RemoveAssignmentsToParameters,
            &["output-parameter"],
        ),
        (
            FixTechnique::RemoveDeadCode,
            &[
                "unused-export",
                "unused-local",
                "unused-private-member",
                "unreachable-code",
                "commented-out-code",
                "journal-comment",
                "redundant-comment",
                "empty-block",
                "loop-runs-once",
                "redundant-jump",
                "empty-method",
                "barrel-module",
                "passthrough-override",
                "duplicate-function",
                "empty-test",
                "dead-store",
                "unused-fixture",
                "duplicate-assertion",
                "debug-output-in-test",
                "comment-per-line",
                "docstring-restates-signature",
                "over-logging",
                "stale-todo",
                "orphan-todo",
                "discarded-expression",
                "unused-import",
                "wildcard-import",
                "rethrow-only-catch",
                "placeholder-body",
                "debug-output",
                "duplicate-import",
                "banner-comment",
                "chat-residue",
                "apology-comment",
                "emoji-in-source",
                "unjustified-suppression",
                "suppressed-diagnostic",
                "skipped-test",
                "focused-test",
            ],
        ),
        (FixTechnique::RemoveParameter, &["unused-parameter"]),
        (
            FixTechnique::RenameMethod,
            &[
                "vague-function-name",
                "query-with-side-effect",
                "misleading-pattern-name",
                "inconsistent-vocabulary",
                "alternative-classes",
                "explanation-longer-than-code",
                "boolean-not-a-question",
                "unnamed-test",
            ],
        ),
        (
            FixTechnique::RenameVariable,
            &[
                "mixed-conventions",
                "meaningless-name",
                "short-name-wide-scope",
                "hungarian-notation",
                "negated-name",
                "shadows-builtin",
                "comment-per-line",
                "name-repeats-type",
                "plural-mismatch",
            ],
        ),
        (
            FixTechnique::ReplaceAlgorithm,
            &[
                "reimplemented-existing",
                "slow-test",
                "mutated-while-iterated",
                "nested-then",
                "integer-division",
                "float-equality",
                "deep-loop-nesting",
                "branching-recursion",
                "quadratic-via-call",
                "deep-recursion",
            ],
        ),
        (
            FixTechnique::ReplaceBlockingCall,
            &["sleep-in-production", "blocking-call-in-async"],
        ),
        (
            FixTechnique::ReplaceConditionalWithPolymorphism,
            &["type-code-switch", "wide-switch", "no-default-branch"],
        ),
        (
            FixTechnique::ReplaceDataValueWithObject,
            &["primitive-obsession", "mixed-return", "generic-throw"],
        ),
        (
            FixTechnique::ReplaceExitWithError,
            &["production-assert", "library-exit"],
        ),
        (
            FixTechnique::ReplaceInheritanceWithComposition,
            &["deep-inheritance"],
        ),
        (
            FixTechnique::ReplaceLoopWithLookup,
            &[
                "scan-in-loop",
                "deep-loop-nesting",
                "linear-op-in-loop",
                "quadratic-via-call",
                "pair-scan-could-hash",
            ],
        ),
        (
            FixTechnique::ReplaceMagicNumberWithConstant,
            &[
                "magic-number",
                "buried-configuration",
                "unexplained-expected-value",
                "example-value",
            ],
        ),
        (
            FixTechnique::ReplaceNestedConditionalWithGuardClauses,
            &[
                "deep-nesting",
                "redundant-else",
                "loop-runs-once",
                "no-base-case",
            ],
        ),
        (
            FixTechnique::ReplaceParameterWithExplicitMethods,
            &["boolean-literal-argument", "flag-argument"],
        ),
        (
            FixTechnique::ReplaceTypeCodeWithClass,
            &["primitive-obsession"],
        ),
        (
            FixTechnique::ReplaceTypeCodeWithPolymorphicState,
            &["type-code-switch"],
        ),
        (
            FixTechnique::ReplaceTypeCodeWithSubclasses,
            &["type-code-switch"],
        ),
        (
            FixTechnique::SeparateQueryFromModifier,
            &["query-with-side-effect", "output-parameter"],
        ),
        (FixTechnique::ShareOwnership, &["single-author-file"]),
        (
            FixTechnique::TakeByReference,
            &["copy-in-range-for", "large-by-value"],
        ),
        (FixTechnique::UsePrimitiveType, &["boxing-in-loop"]),
    ];

    /// A rule prescribing several techniques is ranked by the cheapest route, so a
    /// finding is never made to look dearer than its easiest honest answer.
    #[test]
    fn every_rule_has_an_effort_and_the_cheapest_route_decides_it() {
        for rule in crate::rule::RuleId::ALL.iter().copied() {
            let cheapest = rule
                .spec()
                .fixes
                .iter()
                .map(|fix| fix.group().effort())
                .min();
            assert!(cheapest.is_some(), "{rule} prescribes nothing");
        }
        assert_eq!(Group::Removal.effort(), 1);
        assert!(Group::Hierarchy.effort() > Group::Extraction.effort());
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

    /// A finding names the technique that fixes it, so a pairing that no one chose reads
    /// as advice from the tool: `discarded-error` once suggested Decompose Conditional.
    /// Adding a technique to a rule means adding the rule to the technique below, which
    /// is where the pairing gets reviewed.
    #[test]
    fn every_technique_a_rule_prescribes_is_one_the_table_says_applies_to_it() {
        let mut unlisted = Vec::new();
        for rule in crate::rule::RuleId::ALL.iter().copied() {
            for technique in rule.spec().fixes {
                let listed = APPLIES_TO
                    .iter()
                    .any(|(name, rules)| name == technique && rules.contains(&rule.slug()));
                if !listed {
                    unlisted.push(format!("{} prescribes {technique}", rule.slug()));
                }
            }
        }
        assert!(unlisted.is_empty(), "{}", unlisted.join("\n"));
    }

    #[test]
    fn the_table_names_only_rules_that_prescribe_the_technique_beside_them() {
        let mut stale = Vec::new();
        for (technique, rules) in APPLIES_TO {
            for slug in *rules {
                match crate::rule::RuleId::parse(slug) {
                    None => stale.push(format!("{technique} lists {slug}, which is not a rule")),
                    Some(rule) if !rule.spec().fixes.contains(technique) => {
                        stale.push(format!(
                            "{technique} lists {slug}, which does not prescribe it"
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
        assert!(stale.is_empty(), "{}", stale.join("\n"));
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

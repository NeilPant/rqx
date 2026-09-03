use std::fmt;

use crate::coverage::{Coverage, Tally};
use crate::rule::{Category, RuleId};

/// refactoring.guru's twenty-two smells, all of them, each with what Rqx does about
/// it. The catalogue is the claim this tool makes; holding it as data is what makes
/// the claim checkable.
macro_rules! smells {
    ($(
        $variant:ident { name: $name:literal, family: $family:ident, coverage: $coverage:expr }
    ),+ $(,)?) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Smell { $($variant),+ }

        impl Smell {
            pub const ALL: &'static [Smell] = &[$(Smell::$variant),+];

            pub const fn name(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }

            pub const fn family(self) -> Category {
                match self { $(Self::$variant => Category::$family),+ }
            }

            pub const fn coverage(self) -> Coverage {
                match self { $(Self::$variant => $coverage),+ }
            }
        }
    };
}

smells! {
    LongMethod {
        name: "Long Method",
        family: Bloaters,
        coverage: Coverage::Implemented(&[
            RuleId::LongFunction,
            RuleId::CognitiveComplexity,
            RuleId::DeepNesting,
        ])
    },
    LargeClass {
        name: "Large Class",
        family: Bloaters,
        coverage: Coverage::Implemented(&[RuleId::LargeClass])
    },
    PrimitiveObsession {
        name: "Primitive Obsession",
        family: Bloaters,
        coverage: Coverage::Implemented(&[RuleId::PrimitiveObsession, RuleId::MagicNumber])
    },
    LongParameterList {
        name: "Long Parameter List",
        family: Bloaters,
        coverage: Coverage::Implemented(&[RuleId::LongParameterList])
    },
    DataClumps {
        name: "Data Clumps",
        family: Bloaters,
        coverage: Coverage::Implemented(&[RuleId::DataClump])
    },

    SwitchStatements {
        name: "Switch Statements",
        family: OoAbusers,
        coverage: Coverage::Implemented(&[RuleId::TypeCodeSwitch])
    },
    TemporaryField {
        name: "Temporary Field",
        family: OoAbusers,
        coverage: Coverage::Implemented(&[RuleId::TemporaryField])
    },
    RefusedBequest {
        name: "Refused Bequest",
        family: OoAbusers,
        coverage: Coverage::Implemented(&[RuleId::RefusedBequest])
    },
    AlternativeClasses {
        name: "Alternative Classes with Different Interfaces",
        family: OoAbusers,
        coverage: Coverage::Absent(
            "two classes doing the same job under different signatures; the clone shape \
             hash can compare their method bodies"
        )
    },

    DivergentChange {
        name: "Divergent Change",
        family: ChangePreventers,
        coverage: Coverage::Implemented(&[RuleId::DivergentChange])
    },
    ShotgunSurgery {
        name: "Shotgun Surgery",
        family: ChangePreventers,
        coverage: Coverage::Implemented(&[RuleId::ShotgunSurgery])
    },
    ParallelInheritanceHierarchies {
        name: "Parallel Inheritance Hierarchies",
        family: ChangePreventers,
        coverage: Coverage::Absent(
            "needs a cross-file parent-to-child map; the module graph carries only a \
             flat list of inherited names with no pairing"
        )
    },

    Comments {
        name: "Comments",
        family: Dispensables,
        coverage: Coverage::Implemented(&[RuleId::CommentedOutCode])
    },
    DuplicateCode {
        name: "Duplicate Code",
        family: Dispensables,
        coverage: Coverage::Implemented(&[RuleId::DuplicateCode])
    },
    DataClass {
        name: "Data Class",
        family: Dispensables,
        coverage: Coverage::Implemented(&[RuleId::DataClass])
    },
    DeadCode {
        name: "Dead Code",
        family: Dispensables,
        coverage: Coverage::Implemented(&[
            RuleId::UnusedExport,
            RuleId::UnusedFile,
            RuleId::UnusedParameter,
            RuleId::UnusedLocal,
            RuleId::UnusedPrivateMember,
            RuleId::UnreachableCode,
        ])
    },
    LazyClass {
        name: "Lazy Class",
        family: Dispensables,
        coverage: Coverage::Implemented(&[RuleId::LazyClass])
    },
    SpeculativeGenerality {
        name: "Speculative Generality",
        family: Dispensables,
        coverage: Coverage::Implemented(&[RuleId::SpeculativeGenerality])
    },

    FeatureEnvy {
        name: "Feature Envy",
        family: Couplers,
        coverage: Coverage::Implemented(&[RuleId::FeatureEnvy])
    },
    InappropriateIntimacy {
        name: "Inappropriate Intimacy",
        family: Couplers,
        coverage: Coverage::Absent(
            "peer classes reaching into each other's members; the pattern collector \
             already tags every reference with its enclosing class"
        )
    },
    MessageChains {
        name: "Message Chains",
        family: Couplers,
        coverage: Coverage::Implemented(&[RuleId::MessageChain])
    },
    MiddleMan {
        name: "Middle Man",
        family: Couplers,
        coverage: Coverage::Implemented(&[RuleId::MiddleMan])
    },
}

impl Smell {
    pub fn tally() -> Tally {
        Tally::of(Self::ALL.iter().map(|smell| smell.coverage()))
    }

    pub fn in_family(family: Category) -> impl Iterator<Item = Self> {
        Self::ALL.iter().copied().filter(move |smell| smell.family() == family)
    }
}

impl fmt::Display for Smell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn the_catalogue_is_complete() {
        assert_eq!(Smell::ALL.len(), 22, "refactoring.guru names 22 smells");
        for (family, expected) in [
            (Category::Bloaters, 5),
            (Category::OoAbusers, 4),
            (Category::ChangePreventers, 3),
            (Category::Dispensables, 6),
            (Category::Couplers, 4),
        ] {
            assert_eq!(Smell::in_family(family).count(), expected, "{}", family.title());
        }
    }

    /// The last three categories are Rqx's own. A smell claiming one would be
    /// inventing a refactoring.guru family that does not exist.
    #[test]
    fn no_smell_claims_a_family_the_catalogue_does_not_have() {
        for smell in Smell::ALL.iter().copied() {
            assert!(
                matches!(
                    smell.family(),
                    Category::Bloaters
                        | Category::OoAbusers
                        | Category::ChangePreventers
                        | Category::Dispensables
                        | Category::Couplers
                ),
                "{smell} claims {}",
                smell.family().title()
            );
        }
    }

    #[test]
    fn every_entry_is_accounted_for() {
        for smell in Smell::ALL.iter().copied() {
            let coverage = smell.coverage();
            match coverage.reason() {
                Some(reason) => assert!(!reason.is_empty(), "{smell} declines without saying why"),
                None => assert!(!coverage.rules().is_empty(), "{smell} names no rule"),
            }
        }
        assert_eq!(Smell::tally().total(), 22);
    }

    #[test]
    fn names_are_unique() {
        let mut seen = BTreeSet::new();
        for smell in Smell::ALL.iter().copied() {
            assert!(seen.insert(smell.name()), "duplicate smell {smell}");
        }
    }

    /// Burns down to zero as the rules land. Every entry here is decidable today —
    /// it is a shortfall, not a limit.
    #[test]
    fn the_decidable_shortfall_is_the_expected_size() {
        assert_eq!(Smell::tally().absent, 3);
    }
}

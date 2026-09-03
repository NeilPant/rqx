use std::fmt;

use crate::coverage::{Coverage, Tally};
use crate::rule::RuleId;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Chapter {
    Comments,
    Environment,
    Functions,
    General,
    Java,
    Names,
    Tests,
}

impl Chapter {
    pub const ALL: [Self; 7] = [
        Self::Comments,
        Self::Environment,
        Self::Functions,
        Self::General,
        Self::Java,
        Self::Names,
        Self::Tests,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Comments => "Comments",
            Self::Environment => "Environment",
            Self::Functions => "Functions",
            Self::General => "General",
            Self::Java => "Java",
            Self::Names => "Names",
            Self::Tests => "Tests",
        }
    }

    pub fn heuristics(self) -> impl Iterator<Item = Heuristic> {
        Heuristic::ALL.iter().copied().filter(move |entry| entry.chapter() == self)
    }
}

/// Every numbered heuristic in Clean Code chapter 17, whether or not Rqx reports it.
/// The identifier is the variant, so a rule cannot cite one that does not exist and
/// a citation cannot drift into prose.
macro_rules! heuristics {
    ($(
        $variant:ident { title: $title:literal, chapter: $chapter:ident, coverage: $coverage:expr }
    ),+ $(,)?) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Heuristic { $($variant),+ }

        impl Heuristic {
            pub const ALL: &'static [Heuristic] = &[$(Heuristic::$variant),+];

            pub const fn id(self) -> &'static str {
                match self { $(Self::$variant => stringify!($variant)),+ }
            }

            pub const fn title(self) -> &'static str {
                match self { $(Self::$variant => $title),+ }
            }

            pub const fn chapter(self) -> Chapter {
                match self { $(Self::$variant => Chapter::$chapter),+ }
            }

            pub const fn coverage(self) -> Coverage {
                match self { $(Self::$variant => $coverage),+ }
            }
        }
    };
}

heuristics! {
    C1 { title: "Inappropriate Information", chapter: Comments,
         coverage: Coverage::Implemented(&[RuleId::JournalComment]) },
    C2 { title: "Obsolete Comment", chapter: Comments,
         coverage: Coverage::NotStatic("nothing decides whether a comment still matches the code beneath it") },
    C3 { title: "Redundant Comment", chapter: Comments,
         coverage: Coverage::Implemented(&[RuleId::RedundantComment]) },
    C4 { title: "Poorly Written Comment", chapter: Comments,
         coverage: Coverage::NotStatic("prose quality is not a metric") },
    C5 { title: "Commented-Out Code", chapter: Comments,
         coverage: Coverage::Implemented(&[RuleId::CommentedOutCode]) },

    E1 { title: "Build Requires More Than One Step", chapter: Environment,
         coverage: Coverage::NotStatic("a property of the build, not of the source") },
    E2 { title: "Tests Require More Than One Step", chapter: Environment,
         coverage: Coverage::NotStatic("a property of the harness, not of the source") },

    F1 { title: "Too Many Arguments", chapter: Functions,
         coverage: Coverage::Implemented(&[RuleId::LongParameterList]) },
    F2 { title: "Output Arguments", chapter: Functions,
         coverage: Coverage::Implemented(&[RuleId::OutputParameter]) },
    F3 { title: "Flag Arguments", chapter: Functions,
         coverage: Coverage::Implemented(&[RuleId::FlagArgument]) },
    F4 { title: "Dead Function", chapter: Functions,
         coverage: Coverage::Implemented(&[RuleId::UnusedExport, RuleId::UnusedPrivateMember]) },

    G1 { title: "Multiple Languages in One Source File", chapter: General,
         coverage: Coverage::Implemented(&[RuleId::MixedLanguageFile]) },
    G2 { title: "Obvious Behavior Is Unimplemented", chapter: General,
         coverage: Coverage::NotStatic("no metric knows what a reader expected to find") },
    G3 { title: "Incorrect Behavior at the Boundaries", chapter: General,
         coverage: Coverage::NotStatic("correctness at a boundary is what a test decides, not a metric") },
    G4 { title: "Overridden Safeties", chapter: General,
         coverage: Coverage::Implemented(&[RuleId::SuppressedDiagnostic, RuleId::EmptyCatch]) },
    G5 { title: "Duplication", chapter: General,
         coverage: Coverage::Implemented(&[RuleId::DuplicateCode]) },
    G6 { title: "Code at Wrong Level of Abstraction", chapter: General,
         coverage: Coverage::NotStatic("which level a statement belongs at is a design judgement") },
    G7 { title: "Base Classes Depending on Their Derivatives", chapter: General,
         coverage: Coverage::Implemented(&[RuleId::BaseDependsOnDerived, RuleId::RefusedBequest]) },
    G8 { title: "Too Much Information", chapter: General,
         coverage: Coverage::Implemented(&[RuleId::TemporaryField]) },
    G9 { title: "Dead Code", chapter: General,
         coverage: Coverage::Implemented(&[
             RuleId::UnusedFile,
             RuleId::UnusedLocal,
             RuleId::UnusedParameter,
             RuleId::UnreachableCode,
         ]) },
    G10 { title: "Vertical Separation", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::DistantDeclaration]) },
    G11 { title: "Inconsistency", chapter: General,
          coverage: Coverage::Absent("one concept named three ways in the same file") },
    G12 { title: "Clutter", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::LazyClass, RuleId::SpeculativeGenerality]) },
    G13 { title: "Artificial Coupling", chapter: General,
          coverage: Coverage::Absent("a declaration whose own module has no use for it") },
    G14 { title: "Feature Envy", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::FeatureEnvy]) },
    G15 { title: "Selector Arguments", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::FlagArgument]) },
    G16 { title: "Obscured Intent", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::ObscuredExpression]) },
    G17 { title: "Misplaced Responsibility", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::DataClass, RuleId::MiddleMan]) },
    G18 { title: "Inappropriate Static", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::InappropriateStatic]) },
    G19 { title: "Use Explanatory Variables", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::ComplexCondition]) },
    G20 { title: "Function Names Should Say What They Do", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::VagueFunctionName]) },
    G21 { title: "Understand the Algorithm", chapter: General,
          coverage: Coverage::NotStatic("no metric knows whether the author understood it") },
    G22 { title: "Make Logical Dependencies Physical", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::DataClump, RuleId::ShotgunSurgery]) },
    G23 { title: "Prefer Polymorphism to If/Else or Switch/Case", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::TypeCodeSwitch]) },
    G24 { title: "Follow Standard Conventions", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::MixedConventions]) },
    G25 { title: "Replace Magic Numbers with Named Constants", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::MagicNumber, RuleId::PrimitiveObsession]) },
    G26 { title: "Be Precise", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::ImpreciseType]) },
    G27 { title: "Structure over Convention", chapter: General,
          coverage: Coverage::NotStatic("whether a convention should have been a type is a design judgement") },
    G28 { title: "Encapsulate Conditionals", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::ComplexCondition]) },
    G29 { title: "Avoid Negative Conditionals", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::NegativeConditional]) },
    G30 { title: "Functions Should Do One Thing", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::LongFunction]) },
    G31 { title: "Hidden Temporal Couplings", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::TemporalCoupling]) },
    G32 { title: "Don't Be Arbitrary", chapter: General,
          coverage: Coverage::NotStatic("no metric separates a considered structure from an arbitrary one") },
    G33 { title: "Encapsulate Boundary Conditions", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::BoundaryArithmetic]) },
    G34 { title: "Functions Should Descend Only One Level of Abstraction", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::CognitiveComplexity, RuleId::DeepNesting]) },
    G35 { title: "Keep Configurable Data at High Levels", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::BuriedConfiguration]) },
    G36 { title: "Avoid Transitive Navigation", chapter: General,
          coverage: Coverage::Implemented(&[RuleId::MessageChain]) },

    J1 { title: "Avoid Long Import Lists by Using Wildcards", chapter: Java,
         coverage: Coverage::NotApplicable("TypeScript has no wildcard import of names") },
    J2 { title: "Don't Inherit Constants", chapter: Java,
         coverage: Coverage::NotApplicable("TypeScript interfaces carry no constants to inherit") },
    J3 { title: "Constants versus Enums", chapter: Java,
         coverage: Coverage::NotApplicable("a choice between Java's int constants and typed enums; TypeScript's analogue is the literal union, which primitive-obsession already reports") },

    N1 { title: "Choose Descriptive Names", chapter: Names,
         coverage: Coverage::Implemented(&[RuleId::MeaninglessName]) },
    N2 { title: "Choose Names at the Appropriate Level of Abstraction", chapter: Names,
         coverage: Coverage::NotStatic("which level a name belongs at is a design judgement") },
    N3 { title: "Use Standard Nomenclature Where Possible", chapter: Names,
         coverage: Coverage::Implemented(&[RuleId::MisleadingPatternName]) },
    N4 { title: "Unambiguous Names", chapter: Names,
         coverage: Coverage::Implemented(&[RuleId::MeaninglessName]) },
    N5 { title: "Use Long Names for Long Scopes", chapter: Names,
         coverage: Coverage::Implemented(&[RuleId::ShortNameWideScope]) },
    N6 { title: "Avoid Encodings", chapter: Names,
         coverage: Coverage::Implemented(&[RuleId::HungarianNotation]) },
    N7 { title: "Names Should Describe Side-Effects", chapter: Names,
         coverage: Coverage::Implemented(&[RuleId::QueryWithSideEffect]) },

    T1 { title: "Insufficient Tests", chapter: Tests,
         coverage: Coverage::Implemented(&[RuleId::AssertionlessTest, RuleId::FocusedTest]) },
    T2 { title: "Use a Coverage Tool!", chapter: Tests,
         coverage: Coverage::NotStatic("coverage is measured by running the tests") },
    T3 { title: "Don't Skip Trivial Tests", chapter: Tests,
         coverage: Coverage::NotStatic("a test nobody wrote leaves nothing in the source to find") },
    T4 { title: "An Ignored Test Is a Question about an Ambiguity", chapter: Tests,
         coverage: Coverage::Implemented(&[RuleId::SkippedTest]) },
    T5 { title: "Test Boundary Conditions", chapter: Tests,
         coverage: Coverage::NotStatic("where the boundary lies is known to the author, not to a parser") },
    T6 { title: "Exhaustively Test Near Bugs", chapter: Tests,
         coverage: Coverage::NotStatic("needs the bug history, not the source") },
    T7 { title: "Patterns of Failure Are Revealing", chapter: Tests,
         coverage: Coverage::NotStatic("a property of test runs, not of test code") },
    T8 { title: "Test Coverage Patterns Can Be Revealing", chapter: Tests,
         coverage: Coverage::NotStatic("needs coverage data") },
    T9 { title: "Tests Should Be Fast", chapter: Tests,
         coverage: Coverage::Implemented(&[RuleId::SlowTest]) },
}

impl Heuristic {
    pub fn tally() -> Tally {
        Tally::of(Self::ALL.iter().map(|entry| entry.coverage()))
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|entry| entry.id() == id)
    }
}

impl fmt::Display for Heuristic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.id(), self.title())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn the_chapter_is_complete() {
        assert_eq!(Heuristic::ALL.len(), 66, "chapter 17 numbers 66 heuristics");
        for (chapter, expected) in [
            (Chapter::Comments, 5),
            (Chapter::Environment, 2),
            (Chapter::Functions, 4),
            (Chapter::General, 36),
            (Chapter::Java, 3),
            (Chapter::Names, 7),
            (Chapter::Tests, 9),
        ] {
            assert_eq!(chapter.heuristics().count(), expected, "{}", chapter.title());
        }
    }

    #[test]
    fn identifiers_are_unique_and_parse_back() {
        let mut seen = BTreeSet::new();
        for entry in Heuristic::ALL.iter().copied() {
            assert!(seen.insert(entry.id()), "duplicate identifier {}", entry.id());
            assert_eq!(Heuristic::parse(entry.id()), Some(entry));
            assert!(!entry.title().is_empty(), "{} has no title", entry.id());
        }
    }

    #[test]
    fn every_entry_is_accounted_for() {
        for entry in Heuristic::ALL.iter().copied() {
            let coverage = entry.coverage();
            match coverage.reason() {
                Some(reason) => assert!(!reason.is_empty(), "{entry} declines without saying why"),
                None => assert!(!coverage.rules().is_empty(), "{entry} names no rule"),
            }
        }
        assert_eq!(Heuristic::tally().total(), 66);
    }

    /// Burns down as the rules land. Everything counted here is decidable from the
    /// source Rqx already parses.
    #[test]
    fn the_decidable_shortfall_is_the_expected_size() {
        assert_eq!(Heuristic::tally().absent, 2);
    }
}

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::finding::{Confidence, SourceRange};
use crate::language::Language;
use crate::pattern::Pattern;
use crate::rule::Analysis;
use crate::technique::FixTechnique;

/// Not a severity and deliberately not ordered by one: a recognised pattern is
/// inventory, an incomplete one is broken, and an applicable one is not there at all,
/// which are three different questions rather than three degrees of the same one.
#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stance {
    Recognized,
    Incomplete,
    Applicable,
}

impl Stance {
    pub const ALL: [Self; 3] = [Self::Recognized, Self::Incomplete, Self::Applicable];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Recognized => "recognized",
            Self::Incomplete => "incomplete",
            Self::Applicable => "applicable",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Recognized => "in use",
            Self::Incomplete => "half applied",
            Self::Applicable => "would fit",
        }
    }

    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|stance| stance.slug() == slug)
    }
}

impl fmt::Display for Stance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// An advisory that reaches output without every field of this struct is one whose
/// claim cannot be traced to the pattern it names, which is the failure this table
/// exists to prevent. Same discipline as `RuleSpec`, and deliberately not the same
/// type: an advisory has no severity, because nothing here is a defect to be scored.
pub struct AdviceSpec {
    pub id: AdviceId,
    pub slug: &'static str,
    pub pattern: Pattern,
    pub stance: Stance,
    pub analysis: Analysis,
    /// The counterpart of `RuleSpec::metric`: stated so a reader can argue with the
    /// claim rather than take it.
    pub signature: &'static str,
    pub confidence: Confidence,
    /// Empty for a recognised pattern, where nothing needs to move.
    pub techniques: &'static [FixTechnique],
    pub why: &'static str,
    pub off_by_default: bool,
    /// Languages this advisory cannot exist in, each with the reason. Go has no
    /// inheritance, so nothing there can be a Template Method — and an advisory that
    /// silently never fires is indistinguishable from a broken one.
    pub not_applicable: &'static [(Language, &'static str)],
}

/// Declares the whole advisory table in one place, every field mandatory, so an
/// advisory that omits its pattern, its signature or its confidence fails to compile.
macro_rules! advisories {
    ($(
        $variant:ident {
            slug: $slug:literal,
            pattern: $pattern:ident,
            stance: $stance:ident,
            analysis: $analysis:ident,
            signature: $signature:literal,
            techniques: [$($technique:ident),* $(,)?],
            confidence: $confidence:ident,
            why: $why:literal,
            $(off_by_default: $off:literal,)?
            $(not_applicable: [$(($language:ident, $reason:literal)),+ $(,)?],)?
        }
    ),+ $(,)?) => {
        #[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum AdviceId { $($variant),+ }

        impl AdviceId {
            pub const ALL: &'static [AdviceId] = &[$(AdviceId::$variant),+];

            pub const fn spec(self) -> &'static AdviceSpec {
                match self {
                    $(Self::$variant => &AdviceSpec {
                        id: Self::$variant,
                        slug: $slug,
                        pattern: Pattern::$pattern,
                        stance: Stance::$stance,
                        analysis: Analysis::$analysis,
                        signature: $signature,
                        confidence: Confidence::$confidence,
                        techniques: &[$(FixTechnique::$technique),*],
                        why: $why,
                        off_by_default: advisories!(@off $($off)?),
                        not_applicable: &[$($((Language::$language, $reason)),+)?],
                    }),+
                }
            }
        }
    };
    (@off) => { false };
    (@off $value:literal) => { $value };
}

include!("advisories.rs");

impl AdviceId {
    pub const fn slug(self) -> &'static str {
        self.spec().slug
    }

    pub const fn pattern(self) -> Pattern {
        self.spec().pattern
    }

    pub const fn stance(self) -> Stance {
        self.spec().stance
    }

    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|advisory| advisory.slug() == slug)
    }

    pub fn applies_to(self, language: Language) -> bool {
        self.exemption(language).is_none()
    }

    pub fn exemption(self, language: Language) -> Option<&'static str> {
        self.spec()
            .not_applicable
            .iter()
            .find(|(candidate, _)| *candidate == language)
            .map(|(_, reason)| *reason)
    }

    pub fn for_language(language: Language) -> impl Iterator<Item = AdviceId> {
        Self::ALL
            .iter()
            .copied()
            .filter(move |advisory| advisory.applies_to(language))
    }
}

impl fmt::Display for AdviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// Beside `Evidence` rather than inside it: the findings JSON flattens `Evidence`
/// into its own schema, and adding to it would change that schema for every consumer.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "signal", rename_all = "snake_case")]
pub enum Signal {
    Discriminant {
        subject: String,
        sites: u32,
        branches: u32,
    },
    Chain {
        topics: String,
        branches: u32,
    },
    Reach {
        name: String,
        sites: u32,
    },
    Cluster {
        names: String,
        members: u32,
    },
    /// A class and what its shape says about it.
    Shape {
        class: String,
        detail: String,
    },
    Family {
        base: String,
        detail: String,
    },
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discriminant {
                subject,
                sites,
                branches,
            } => write!(
                f,
                "`{subject}` decides {} across {}",
                crate::finding::plural(*branches, "branch", "branches"),
                crate::finding::plural(*sites, "place", "places"),
            ),
            Self::Chain { topics, branches } => write!(
                f,
                "{} in order, each asking something different: {topics}",
                crate::finding::plural(*branches, "branch", "branches"),
            ),
            Self::Reach { name, sites } => write!(
                f,
                "`{name}` is reached for directly from {}",
                crate::finding::plural(*sites, "file", "files"),
            ),
            Self::Cluster { names, members } => write!(
                f,
                "{} that all depend on each other: {names}",
                crate::finding::plural(*members, "module", "modules"),
            ),
            Self::Shape { class, detail } => write!(f, "`{class}` {detail}"),
            Self::Family { base, detail } => write!(f, "`{base}` has {detail}"),
        }
    }
}

/// One place a pattern was recognised, found half-built, or would fit. Deliberately
/// not a `Finding`: nothing here reaches `FindingCounts`, so no advisory can move the
/// health score however a project configures it.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Advice {
    pub advisory: AdviceId,
    pub location: SourceRange,
    pub signal: Signal,
    pub note: String,
    pub related: Vec<SourceRange>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn slugs_are_unique_and_round_trip() {
        let mut seen = BTreeSet::new();
        for advisory in AdviceId::ALL.iter().copied() {
            assert!(seen.insert(advisory.slug()), "duplicate slug {advisory}");
            assert_eq!(AdviceId::parse(advisory.slug()), Some(advisory));
        }
    }

    /// A claim this weak is worth making only when a project asks for it; shipping it
    /// on would spend the reader's trust on the least certain thing Rqx says.
    #[test]
    fn a_low_confidence_advisory_ships_off() {
        for advisory in AdviceId::ALL.iter().copied() {
            let spec = advisory.spec();
            if spec.confidence == Confidence::Low {
                assert!(spec.off_by_default, "{advisory} is Low but ships on");
            }
        }
    }

    #[test]
    fn every_advisory_is_traceable() {
        for advisory in AdviceId::ALL.iter().copied() {
            let spec = advisory.spec();
            assert!(!spec.signature.is_empty(), "{advisory} has no signature");
            assert!(!spec.why.is_empty(), "{advisory} says nothing about why");
            for (language, reason) in spec.not_applicable {
                assert!(
                    !reason.is_empty(),
                    "{advisory} exempts {language:?} without saying why"
                );
            }
        }
    }

    /// Nothing needs moving where nothing is wrong, and a suggestion that names no
    /// refactoring is a suggestion a reader cannot act on.
    #[test]
    fn only_a_recognised_pattern_prescribes_nothing() {
        for advisory in AdviceId::ALL.iter().copied() {
            let spec = advisory.spec();
            assert_eq!(
                spec.techniques.is_empty(),
                spec.stance == Stance::Recognized,
                "{advisory} is {} and names {} techniques",
                spec.stance,
                spec.techniques.len()
            );
        }
    }
}

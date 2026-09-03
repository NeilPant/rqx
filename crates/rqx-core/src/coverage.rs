use crate::rule::RuleId;

/// What Rqx does about one entry in a canonical catalogue. Every entry carries one,
/// so "42 of 88 implemented" stops being a sentence in a README that nothing checks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Coverage {
    Implemented(&'static [RuleId]),
    /// Decidable, but no rule reports it yet.
    Absent(&'static str),
    /// Needs a type checker; oxc has none, so guessing is the only alternative.
    NeedsTypes(&'static str),
    /// No metric can decide it. A tool that claimed otherwise would invent findings.
    NotStatic(&'static str),
    /// Structurally cannot apply to TypeScript.
    NotApplicable(&'static str),
}

impl Coverage {
    pub const fn rules(self) -> &'static [RuleId] {
        match self {
            Self::Implemented(rules) => rules,
            _ => &[],
        }
    }

    pub const fn is_implemented(self) -> bool {
        matches!(self, Self::Implemented(_))
    }

    pub const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Implemented(_) => None,
            Self::Absent(reason)
            | Self::NeedsTypes(reason)
            | Self::NotStatic(reason)
            | Self::NotApplicable(reason) => Some(reason),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Implemented(_) => "implemented",
            Self::Absent(_) => "not yet",
            Self::NeedsTypes(_) => "needs types",
            Self::NotStatic(_) => "not decidable",
            Self::NotApplicable(_) => "not applicable",
        }
    }
}

/// How much of one catalogue is covered, counted rather than asserted.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub implemented: u32,
    pub absent: u32,
    pub needs_types: u32,
    pub not_static: u32,
    pub not_applicable: u32,
}

impl Tally {
    pub fn of(entries: impl Iterator<Item = Coverage>) -> Self {
        let mut tally = Self::default();
        for entry in entries {
            match entry {
                Coverage::Implemented(_) => tally.implemented += 1,
                Coverage::Absent(_) => tally.absent += 1,
                Coverage::NeedsTypes(_) => tally.needs_types += 1,
                Coverage::NotStatic(_) => tally.not_static += 1,
                Coverage::NotApplicable(_) => tally.not_applicable += 1,
            }
        }
        tally
    }

    pub const fn total(self) -> u32 {
        self.implemented + self.absent + self.needs_types + self.not_static + self.not_applicable
    }

    /// Entries a rule could reach. Excludes the ones no static tool can decide, so
    /// the number is a target rather than a permanent shortfall.
    pub const fn reachable(self) -> u32 {
        self.implemented + self.absent
    }
}

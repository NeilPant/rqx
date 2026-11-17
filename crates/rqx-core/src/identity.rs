use std::fmt;

use serde::{Deserialize, Serialize};

use crate::finding::{Evidence, Finding};
use crate::rule::RuleId;

/// What a finding is about, independent of where in the file it currently sits.
/// Two findings with the same identity are the same problem before and after an
/// edit moved it, which is what lets a fixer tell "still there" from "new".
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Identity(u64);

impl Identity {
    /// Rule, file and subject. Never the line, never the measured value, so a
    /// finding survives being moved and survives its metric shrinking.
    pub fn of(rule: RuleId, file: &str, subject: &str) -> Self {
        Self(fnv1a(&[rule.slug(), file, subject]))
    }

    /// Rule, file and the evidence as printed. A changed metric is a changed
    /// identity, which is what a reporter that re-reports on drift wants.
    pub fn exact(finding: &Finding) -> Self {
        Self(fnv1a(&[
            finding.slug(),
            &finding.location.file,
            &finding.evidence.to_string(),
        ]))
    }

    /// A value read back from a file rqx wrote. Only a baseline does this, and only
    /// with a number it wrote itself.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// `of`, with the span's own text standing in for findings whose evidence
    /// carries no name. Trimmed, so reindenting the line a finding sits on does
    /// not rename it.
    pub fn resolved(finding: &Finding, source: &str) -> Self {
        let span = crate::span::Span::new(finding.location.start, finding.location.end);
        let subject = finding
            .evidence
            .subject()
            .map_or_else(|| span.text(source).trim().to_owned(), ToOwned::to_owned);
        Self::keyed(finding, &subject)
    }

    /// `of` for a finding that has already been given its subject. A rule the project
    /// wrote shares the slug `custom`, which is what baselines recorded before
    /// external findings existed, so only an external finding is keyed on its own
    /// rule id: two of another tool's rules must not collapse into one identity.
    pub fn keyed(finding: &Finding, subject: &str) -> Self {
        let rule = if finding.is_external() {
            finding.slug()
        } else {
            finding.rule.slug()
        };
        Self(fnv1a(&[rule, &finding.location.file, subject]))
    }
}

/// FNV-1a, written out rather than hashed with `DefaultHasher`, because every
/// value it produces is persisted — in a GitLab report between CI runs, in a
/// checked-in baseline, in a cache that outlives a toolchain upgrade — and
/// `DefaultHasher` promises nothing across Rust versions.
pub fn fnv1a(parts: &[&str]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for (at, part) in parts.iter().enumerate() {
        if at > 0 {
            eat(0xff);
        }
        for byte in part.bytes() {
            eat(byte);
        }
    }
    hash
}

impl fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl Evidence {
    /// What a runtime profile said about the function this evidence sits in, in parts
    /// per thousand of the samples it took.
    pub fn mark_sampled(&mut self, permille: u32) {
        match self {
            Self::InLoop { sampled, .. }
            | Self::LoopDepth { sampled, .. }
            | Self::Recursion { sampled, .. }
            | Self::BlockingCall { sampled, .. } => *sampled = Some(permille),
            _ => {}
        }
    }

    pub fn sampled(&self) -> Option<u32> {
        match self {
            Self::InLoop { sampled, .. }
            | Self::LoopDepth { sampled, .. }
            | Self::Recursion { sampled, .. }
            | Self::BlockingCall { sampled, .. } => *sampled,
            _ => None,
        }
    }

    /// The named thing the evidence is about, when it carries one. Exhaustive so a
    /// new variant has to say whether it names something.
    pub fn subject(&self) -> Option<&str> {
        match self {
            // A query names whatever it matched; the span's own text is the subject,
            // and `resolved` supplies it.
            Self::Custom { .. } => None,
            // The comment itself is the subject, and its text is the span's text.
            Self::Noise { .. } => None,
            Self::Name { identifier, .. } => Some(identifier),
            Self::UnusedExport { symbol, .. } => Some(symbol),
            Self::UnusedParameter { name, .. }
            | Self::UnusedBinding { name, .. }
            | Self::BooleanParameter { name, .. }
            | Self::MutatedParameter { name }
            | Self::NoAssertion { name }
            | Self::Discriminant { name, .. }
            | Self::FieldLifetime { name, .. }
            | Self::RefusedOverride { name }
            | Self::DerivedReference { name }
            | Self::StaticOnOwnType { name }
            | Self::SideEffectingQuery { name }
            | Self::VagueName { name }
            | Self::DistantDeclaration { name, .. }
            | Self::MisleadingName { name, .. } => Some(name),
            Self::MagicNumber { literal, .. } => Some(literal),
            Self::RepeatedGroup { names, .. } => Some(names),
            Self::CoChange { partner, .. } => Some(partner),
            Self::RedundantComment { subject } | Self::BoundaryArithmetic { subject, .. } => {
                Some(subject)
            }
            Self::SlowTest { call, .. } => Some(call),
            Self::MixedConventions { minority, .. } => Some(minority),
            Self::TemporalCoupling { field, .. } => Some(field),
            Self::InconsistentVocabulary { pair, .. } | Self::IntimatePair { pair } => Some(pair),
            Self::AlternativeClasses { other, .. } => Some(other),
            Self::ParallelHierarchy { twin, .. } => Some(twin),
            Self::ForeignAccess { owner, .. } => Some(owner),
            Self::EmptyBody { construct } | Self::ImpreciseType { construct } => Some(construct),
            Self::ForeignLanguage { language } => Some(language),
            Self::LayerViolation { target, .. } | Self::ForbiddenImport { target, .. } => {
                Some(target)
            }
            Self::BranchShape { shape } => Some(shape),
            Self::InLoop { subject, .. } => Some(subject),
            Self::BlockingCall { call, .. } => Some(call),
            Self::BuriedConfiguration { kind, .. } => Some(kind),
            // A kind ("return", "TODO", "only") names a category of finding, not this
            // one, so the span's own text is the more faithful subject.
            Self::Unreachable { .. }
            | Self::JournalComment { .. }
            | Self::Suppression { .. }
            | Self::TestDirective { .. }
            | Self::CognitiveComplexity { .. }
            | Self::StatementCount { .. }
            | Self::NestingDepth { .. }
            | Self::ParameterCount { .. }
            | Self::ClassSize { .. }
            | Self::ClonedBlock { .. }
            | Self::UnusedFile { .. }
            | Self::ImportCycle { .. }
            | Self::CommentedCode { .. }
            | Self::ClassShape { .. }
            | Self::MessageChain { .. }
            | Self::Delegation { .. }
            | Self::ConditionOperands { .. }
            | Self::NegatedCondition
            | Self::Breadth { .. }
            | Self::LoopDepth { .. }
            | Self::Recursion { .. }
            | Self::ObscuredExpression { .. } => None,
        }
    }

    /// The one number a threshold compares against, when there is one.
    pub fn magnitude(&self) -> Option<u32> {
        match self {
            // A query either matched or did not; there is no number to shrink.
            Self::Custom { .. } => None,
            Self::Noise { .. } => None,
            Self::CognitiveComplexity { score, .. } => Some(*score),
            Self::StatementCount { count, .. } | Self::ParameterCount { count, .. } => Some(*count),
            Self::NestingDepth { depth, .. } | Self::MessageChain { depth, .. } => Some(*depth),
            Self::ClassSize { statements, .. } => Some(*statements),
            Self::ClonedBlock { occurrences, .. } => Some(*occurrences),
            Self::UnusedFile { exports, .. } => Some(*exports),
            Self::ImportCycle { modules, .. } => Some(*modules),
            Self::CommentedCode { lines } => Some(*lines),
            Self::MagicNumber { occurrences, .. } => Some(*occurrences),
            Self::ClassShape { trivial, .. } => Some(*trivial),
            Self::ForeignAccess { foreign, .. } => Some(*foreign),
            Self::Delegation { delegating, .. } => Some(*delegating),
            Self::ConditionOperands { operands, .. } => Some(*operands),
            Self::RepeatedGroup { sites, .. }
            | Self::Discriminant { sites, .. }
            | Self::BoundaryArithmetic { sites, .. }
            | Self::InconsistentVocabulary { sites, .. } => Some(*sites),
            Self::CoChange { together, .. } => Some(*together),
            Self::SlowTest { pause, .. } => Some(*pause),
            Self::MixedConventions { count, .. } => Some(*count),
            Self::DistantDeclaration { lines, .. } => Some(*lines),
            Self::ObscuredExpression { operations, .. } => Some(*operations),
            Self::Breadth { found, .. } => Some(*found),
            Self::LoopDepth { depth, .. } | Self::InLoop { depth, .. } => Some(*depth),
            Self::Recursion { calls, .. } => Some(*calls),
            Self::BuriedConfiguration { depth, .. } => Some(*depth),
            Self::AlternativeClasses { methods, .. } => Some(*methods),
            Self::Name { .. }
            | Self::UnusedExport { .. }
            | Self::UnusedParameter { .. }
            | Self::UnusedBinding { .. }
            | Self::Unreachable { .. }
            | Self::EmptyBody { .. }
            | Self::Suppression { .. }
            | Self::BooleanParameter { .. }
            | Self::MutatedParameter { .. }
            | Self::NegatedCondition
            | Self::TestDirective { .. }
            | Self::NoAssertion { .. }
            | Self::FieldLifetime { .. }
            | Self::RefusedOverride { .. }
            | Self::DerivedReference { .. }
            | Self::StaticOnOwnType { .. }
            | Self::ImpreciseType { .. }
            | Self::JournalComment { .. }
            | Self::RedundantComment { .. }
            | Self::ForeignLanguage { .. }
            | Self::SideEffectingQuery { .. }
            | Self::VagueName { .. }
            | Self::TemporalCoupling { .. }
            | Self::MisleadingName { .. }
            | Self::IntimatePair { .. }
            | Self::LayerViolation { .. }
            | Self::ForbiddenImport { .. }
            | Self::BranchShape { .. }
            | Self::BlockingCall { .. }
            | Self::ParallelHierarchy { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value is checked in rather than recomputed, because a GitLab report and a
    /// checked-in baseline both persist it: if this number ever moves, every waiver
    /// and every existing annotation moves with it.
    #[test]
    fn an_identity_is_the_same_number_on_every_toolchain() {
        assert_eq!(
            Identity::of(RuleId::LongFunction, "src/a.ts", "parse").to_string(),
            "e8288dc7aedd23a6"
        );
    }

    /// The separator is what keeps two different splits of the same text apart.
    #[test]
    fn the_parts_of_an_identity_cannot_run_together() {
        assert_ne!(
            Identity::of(RuleId::LongFunction, "a", "bc"),
            Identity::of(RuleId::LongFunction, "ab", "c")
        );
    }
    use crate::finding::{Confidence, Severity, SourceRange};

    fn finding(rule: RuleId, start: u32, evidence: Evidence) -> Finding {
        Finding {
            rule,
            location: SourceRange {
                file: "a.ts".into(),
                start,
                end: start + 3,
                line: 1,
                column: 1,
                end_line: 1,
            },
            severity: Severity::Warning,
            confidence: Confidence::Verified,
            evidence,
            fix_hint: String::new(),
            related: Vec::new(),
            known: false,
        }
    }

    #[test]
    fn a_finding_that_moved_keeps_its_identity_and_a_shrunken_metric_keeps_of() {
        let before = finding(
            RuleId::LongFunction,
            10,
            Evidence::StatementCount {
                count: 45,
                threshold: 40,
            },
        );
        let after = finding(
            RuleId::LongFunction,
            80,
            Evidence::StatementCount {
                count: 44,
                threshold: 40,
            },
        );
        let source = "x".repeat(200);
        assert_eq!(
            Identity::resolved(&before, &source),
            Identity::resolved(&after, &source)
        );
        assert_ne!(Identity::exact(&before), Identity::exact(&after));
    }

    #[test]
    fn a_different_subject_is_a_different_identity() {
        let a = finding(
            RuleId::UnusedLocal,
            0,
            Evidence::UnusedBinding {
                name: "a".into(),
                kind: "local".into(),
            },
        );
        let b = finding(
            RuleId::UnusedLocal,
            0,
            Evidence::UnusedBinding {
                name: "b".into(),
                kind: "local".into(),
            },
        );
        assert_ne!(Identity::resolved(&a, ""), Identity::resolved(&b, ""));
    }
}

use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::finding::{Evidence, Finding};
use crate::rule::RuleId;

/// What a finding is about, independent of where in the file it currently sits.
/// Two findings with the same identity are the same problem before and after an
/// edit moved it, which is what lets a fixer tell "still there" from "new".
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Identity(u64);

impl Identity {
    /// Rule, file and subject. Never the line, never the measured value, so a
    /// finding survives being moved and survives its metric shrinking.
    pub fn of(rule: RuleId, file: &str, subject: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        rule.slug().hash(&mut hasher);
        file.hash(&mut hasher);
        subject.hash(&mut hasher);
        Self(hasher.finish())
    }

    /// Rule, file and the evidence as printed. A changed metric is a changed
    /// identity, which is what a reporter that re-reports on drift wants.
    pub fn exact(finding: &Finding) -> Self {
        let mut hasher = DefaultHasher::new();
        finding.rule.slug().hash(&mut hasher);
        finding.location.file.hash(&mut hasher);
        finding.evidence.to_string().hash(&mut hasher);
        Self(hasher.finish())
    }

    /// `of`, with the span's own text standing in for findings whose evidence
    /// carries no name.
    pub fn resolved(finding: &Finding, source: &str) -> Self {
        let span = crate::span::Span::new(finding.location.start, finding.location.end);
        let subject = finding
            .evidence
            .subject()
            .map_or_else(|| span.text(source).to_owned(), ToOwned::to_owned);
        Self::of(finding.rule, &finding.location.file, &subject)
    }
}

impl fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl Evidence {
    /// The named thing the evidence is about, when it carries one. Exhaustive so a
    /// new variant has to say whether it names something.
    pub fn subject(&self) -> Option<&str> {
        match self {
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
            | Self::ObscuredExpression { .. } => None,
        }
    }

    /// The one number a threshold compares against, when there is one.
    pub fn magnitude(&self) -> Option<u32> {
        match self {
            Self::CognitiveComplexity { score, .. } => Some(*score),
            Self::StatementCount { count, .. } | Self::ParameterCount { count, .. } => Some(*count),
            Self::NestingDepth { depth, .. } | Self::MessageChain { depth, .. } => Some(*depth),
            Self::ClassSize { statements, .. } => Some(*statements),
            Self::ClonedBlock { occurrences, .. } => Some(*occurrences),
            Self::UnusedFile { exports } => Some(*exports),
            Self::ImportCycle { modules } => Some(*modules),
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
            | Self::ParallelHierarchy { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::rule::RuleId;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SourceRange {
    pub file: Arc<str>,
    pub start: u32,
    pub end: u32,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
}

impl SourceRange {
    pub fn contains(&self, other: &SourceRange) -> bool {
        self.file == other.file && self.start <= other.start && self.end >= other.end
    }
}

impl fmt::Display for SourceRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub const COUNT: usize = 3;

    pub fn weight(self) -> f64 {
        match self {
            Self::Info => 1.0,
            Self::Warning => 3.0,
            Self::Error => 9.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Heuristic,
    Verified,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "low confidence",
            Self::Heuristic => "measured threshold",
            Self::Verified => "verified",
        }
    }
}

fn plural(count: u32, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "metric", rename_all = "snake_case")]
pub enum Evidence {
    CognitiveComplexity { score: u32, threshold: u32, nesting_depth: u32 },
    StatementCount { count: u32, threshold: u32 },
    NestingDepth { depth: u32, threshold: u32 },
    ParameterCount { count: u32, threshold: u32 },
    ClassSize { fields: u32, methods: u32, statements: u32, threshold: u32 },
    ClonedBlock { statements: u32, nodes: u32, occurrences: u32 },
    Name { identifier: String, reason: String },
    UnusedExport { symbol: String, kind: String },
    UnusedFile { exports: u32 },
    UnusedParameter { name: String, position: u32, of: u32, trailing: bool },
    ImportCycle { modules: u32 },
    UnusedBinding { name: String, kind: String },
    Unreachable { after: String },
    CommentedCode { lines: u32 },
    MagicNumber { literal: String, occurrences: u32 },
    EmptyBody { construct: String },
    ClassShape { kind: String, fields: u32, methods: u32, trivial: u32 },
    MessageChain { depth: u32, limit: u32 },
    ForeignAccess { owner: String, foreign: u32, own: u32 },
    Delegation { delegating: u32, total: u32 },
    Suppression { directive: String },
    BooleanParameter { name: String, position: u32 },
    MutatedParameter { name: String },
    ConditionOperands { operands: u32, limit: u32 },
    NegatedCondition,
    TestDirective { directive: String },
    NoAssertion { name: String },
    RepeatedGroup { names: String, sites: u32 },
    Discriminant { name: String, sites: u32, branches: u32 },
    FieldLifetime { name: String, method: String },
    RefusedOverride { name: String },
    DerivedReference { name: String },
    StaticOnOwnType { name: String },
    CoChange { partner: String, together: u32, total: u32 },
    ImpreciseType { construct: String },
    JournalComment { marker: String },
    RedundantComment { subject: String },
    SlowTest { call: String, pause: u32 },
    ForeignLanguage { language: String },
    BoundaryArithmetic { subject: String, sites: u32 },
    MixedConventions { minority: String, count: u32 },
}

impl fmt::Display for Evidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CognitiveComplexity { score, threshold, nesting_depth } => write!(
                f,
                "Cognitive complexity {score} (threshold {threshold}), max nesting depth {nesting_depth}."
            ),
            Self::StatementCount { count, threshold } => {
                write!(f, "{count} statements in one body (threshold {threshold}).")
            }
            Self::NestingDepth { depth, threshold } => {
                write!(f, "Nesting reaches {depth} levels (threshold {threshold}).")
            }
            Self::ParameterCount { count, threshold } => {
                write!(f, "{count} parameters (threshold {threshold}).")
            }
            // A class of plain field declarations has no statements at all, and
            // "over 0 statements" reads as a defect in the tool rather than a fact
            // about the code.
            Self::ClassSize { fields, methods, statements: 0, threshold } => write!(
                f,
                "{} members: {fields} fields and {methods} methods (threshold {threshold}).",
                fields + methods
            ),
            Self::ClassSize { fields, methods, statements, threshold } => write!(
                f,
                "{} members ({fields} fields, {methods} methods) across {statements} statements (threshold {threshold}).",
                fields + methods
            ),
            Self::ClonedBlock { statements, nodes, occurrences } => write!(
                f,
                "{statements}-statement block ({nodes} AST nodes) repeated at {occurrences} sites."
            ),
            Self::Name { identifier, reason } => write!(f, "`{identifier}` {reason}."),
            Self::UnusedExport { symbol, kind } => {
                write!(f, "Exported {kind} `{symbol}` is imported nowhere in the project.")
            }
            Self::UnusedFile { exports } => write!(
                f,
                "No module imports this file, and none of its {exports} exports are used."
            ),
            Self::UnusedParameter { name, position, of, trailing: true } if position == of => {
                write!(f, "Parameter `{name}` ({position} of {of}, last in the list) is never read.")
            }
            // Trailing without being last means every parameter after it is dead too,
            // which is why deleting from here is still a safe edit.
            Self::UnusedParameter { name, position, of, trailing: true } => write!(
                f,
                "Parameter `{name}` ({position} of {of}) is never read, and nothing after it is either."
            ),
            Self::UnusedParameter { name, position, of, trailing: false } => write!(
                f,
                "Parameter `{name}` ({position} of {of}) is never read, but {} follow it.",
                of - position
            ),
            Self::ImportCycle { modules } => {
                write!(f, "{modules} modules import each other in a cycle.")
            }
            Self::UnusedBinding { name, kind } => {
                write!(f, "The {kind} `{name}` is declared and never read.")
            }
            Self::Unreachable { after } => {
                write!(f, "This statement cannot run: the block always {after} before reaching it.")
            }
            Self::CommentedCode { lines } => write!(
                f,
                "{} of commented-out code, which version control already remembers.",
                plural(*lines, "line", "lines")
            ),
            Self::MagicNumber { literal, occurrences: 1 } => {
                write!(f, "Unnamed literal `{literal}`.")
            }
            Self::MagicNumber { literal, occurrences } => {
                write!(f, "Unnamed literal `{literal}`, repeated {occurrences} times in this file.")
            }
            Self::EmptyBody { construct } => write!(f, "This {construct} has an empty body."),
            Self::ClassShape { kind, fields, methods, trivial } => write!(
                f,
                "{kind}: {fields} fields, {methods} methods, {trivial} of them trivial."
            ),
            Self::MessageChain { depth, limit } => {
                write!(f, "Reaches {depth} objects deep in one expression (limit {limit}).")
            }
            Self::ForeignAccess { owner, foreign, own } => write!(
                f,
                "Touches `{owner}` {foreign} times and its own members {own} times."
            ),
            Self::Delegation { delegating, total } => write!(
                f,
                "{delegating} of {total} methods do nothing but forward the call."
            ),
            Self::Suppression { directive } => {
                write!(f, "`{directive}` turns off a check that covers this code.")
            }
            Self::BooleanParameter { name, position } => write!(
                f,
                "Boolean parameter `{name}` ({position} in the list) selects between behaviours."
            ),
            Self::MutatedParameter { name } => {
                write!(f, "Parameter `{name}` is reassigned inside the function.")
            }
            Self::ConditionOperands { operands, limit } => {
                write!(f, "{operands} boolean operands in one condition (limit {limit}).")
            }
            Self::NegatedCondition => {
                write!(f, "A negated condition with an else branch, so both arms read backwards.")
            }
            Self::TestDirective { directive } => write!(f, "Declared with `{directive}`."),
            Self::NoAssertion { name } => {
                write!(f, "The test `{name}` makes no assertion.")
            }
            Self::RepeatedGroup { names, sites } => {
                write!(f, "The group ({names}) appears in {sites} signatures.")
            }
            Self::Discriminant { name, sites, branches } => write!(
                f,
                "Branches on `{name}` in {sites} places, {branches} cases at the widest."
            ),
            Self::FieldLifetime { name, method } => write!(
                f,
                "Field `{name}` is only ever used by `{method}`, and never set in the constructor."
            ),
            Self::RefusedOverride { name } => {
                write!(f, "`{name}` overrides an inherited member only to refuse it.")
            }
            Self::DerivedReference { name } => {
                write!(f, "This base class names its own subclass `{name}`.")
            }
            Self::StaticOnOwnType { name } => write!(
                f,
                "`{name}` is static but takes an instance of its own class as its first parameter."
            ),
            Self::CoChange { partner, together, total } => write!(
                f,
                "Changed with `{partner}` in {together} of its {total} commits."
            ),
            Self::ImpreciseType { construct } => {
                write!(f, "`{construct}` throws away what the type checker had already worked out.")
            }
            Self::JournalComment { marker } => {
                write!(f, "This comment carries `{marker}`, which version control already records.")
            }
            Self::RedundantComment { subject } => {
                write!(f, "This comment says no more than the name `{subject}` below it.")
            }
            Self::SlowTest { call, pause } => {
                write!(f, "`{call}` pauses this test for {pause}ms on every run.")
            }
            Self::ForeignLanguage { language } => {
                write!(f, "A template literal in this file holds {language}.")
            }
            Self::BoundaryArithmetic { subject, sites } => write!(
                f,
                "`{subject}` is adjusted by one in {sites} places, each of them a chance to be off by one."
            ),
            Self::MixedConventions { minority, count } => write!(
                f,
                "{} in this file use {minority} while the rest do not.",
                plural(*count, "name", "names")
            ),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Finding {
    pub rule: RuleId,
    pub location: SourceRange,
    pub severity: Severity,
    pub confidence: Confidence,
    pub evidence: Evidence,
    /// Why this matters is not stored per finding: it is `rule.spec().why`, so a
    /// rule cannot ship a finding whose rationale drifted from its taxonomy entry.
    pub fix_hint: String,
    pub related: Vec<SourceRange>,
}

impl Finding {
    pub fn rank(&self) -> (Severity, Confidence, RuleId) {
        (self.severity, self.confidence, self.rule)
    }
}

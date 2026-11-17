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

#[derive(
    Copy,
    Clone,
    Debug,
    Deserialize,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    schemars::JsonSchema,
)]
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

#[derive(
    Copy,
    Clone,
    Debug,
    Deserialize,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    schemars::JsonSchema,
)]
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

pub(crate) fn plural(count: u32, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "metric", rename_all = "snake_case")]
pub enum Evidence {
    CognitiveComplexity {
        score: u32,
        threshold: u32,
        nesting_depth: u32,
    },
    StatementCount {
        count: u32,
        threshold: u32,
    },
    NestingDepth {
        depth: u32,
        threshold: u32,
    },
    ParameterCount {
        count: u32,
        threshold: u32,
    },
    ClassSize {
        fields: u32,
        methods: u32,
        statements: u32,
        threshold: u32,
    },
    ClonedBlock {
        statements: u32,
        nodes: u32,
        occurrences: u32,
    },
    Name {
        identifier: String,
        reason: String,
    },
    UnusedExport {
        symbol: String,
        kind: String,
        /// Lines of this export a test run executed, when a coverage file was read.
        /// `Some(0)` corroborates the finding; anything higher contradicts it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ran: Option<u32>,
    },
    UnusedFile {
        exports: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ran: Option<u32>,
    },
    UnusedParameter {
        name: String,
        position: u32,
        of: u32,
        trailing: bool,
    },
    ImportCycle {
        modules: u32,
        /// The way round, as a closed walk. Additive, so a consumer reading the old
        /// shape still finds `modules` where it was.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ring: Vec<String>,
    },
    UnusedBinding {
        name: String,
        kind: String,
    },
    Unreachable {
        after: String,
    },
    CommentedCode {
        lines: u32,
    },
    MagicNumber {
        literal: String,
        occurrences: u32,
    },
    EmptyBody {
        construct: String,
    },
    ClassShape {
        kind: String,
        fields: u32,
        methods: u32,
        trivial: u32,
    },
    MessageChain {
        depth: u32,
        limit: u32,
    },
    ForeignAccess {
        owner: String,
        foreign: u32,
        own: u32,
    },
    Delegation {
        delegating: u32,
        total: u32,
    },
    Suppression {
        directive: String,
    },
    /// Text that costs a reader and buys nothing. `marker` is the words that gave it
    /// away, empty where the shape alone did.
    Noise {
        marker: String,
    },
    /// A rule the project wrote. Its identity and its family travel with the finding
    /// because no `RuleSpec` was compiled for it.
    Custom {
        id: Arc<str>,
        message: String,
        family: crate::rule::Category,
    },
    BooleanParameter {
        name: String,
        position: u32,
    },
    MutatedParameter {
        name: String,
    },
    ConditionOperands {
        operands: u32,
        limit: u32,
    },
    NegatedCondition,
    TestDirective {
        directive: String,
    },
    NoAssertion {
        name: String,
    },
    RepeatedGroup {
        names: String,
        sites: u32,
    },
    Discriminant {
        name: String,
        sites: u32,
        branches: u32,
    },
    FieldLifetime {
        name: String,
        method: String,
    },
    RefusedOverride {
        name: String,
    },
    DerivedReference {
        name: String,
    },
    StaticOnOwnType {
        name: String,
    },
    CoChange {
        partner: String,
        together: u32,
        total: u32,
    },
    ImpreciseType {
        construct: String,
    },
    JournalComment {
        marker: String,
    },
    RedundantComment {
        subject: String,
    },
    SlowTest {
        call: String,
        pause: u32,
    },
    ForeignLanguage {
        language: String,
    },
    BoundaryArithmetic {
        subject: String,
        sites: u32,
    },
    MixedConventions {
        minority: String,
        count: u32,
    },
    SideEffectingQuery {
        name: String,
    },
    VagueName {
        name: String,
    },
    DistantDeclaration {
        name: String,
        lines: u32,
    },
    ObscuredExpression {
        operations: u32,
        limit: u32,
    },
    BuriedConfiguration {
        kind: String,
        depth: u32,
    },
    TemporalCoupling {
        field: String,
        methods: String,
    },
    MisleadingName {
        name: String,
        promise: String,
    },
    InconsistentVocabulary {
        pair: String,
        sites: u32,
    },
    IntimatePair {
        pair: String,
    },
    AlternativeClasses {
        other: String,
        methods: u32,
    },
    BranchShape {
        shape: String,
    },
    Breadth {
        counted: String,
        found: u32,
        limit: u32,
    },
    LayerViolation {
        from: String,
        to: String,
        target: String,
    },
    ForbiddenImport {
        target: String,
        by: String,
    },
    ParallelHierarchy {
        parent: String,
        twin: String,
        shared: String,
    },
    /// Work repeated on every pass of a loop. `sampled` is filled from a runtime
    /// profile: `Some(0)` means the profile never reached this function.
    InLoop {
        subject: String,
        depth: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sampled: Option<u32>,
    },
    LoopDepth {
        depth: u32,
        threshold: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sampled: Option<u32>,
    },
    Recursion {
        calls: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sampled: Option<u32>,
    },
    BlockingCall {
        call: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sampled: Option<u32>,
    },
}

/// How much of a runtime profile's samples landed in the function a finding sits in.
/// Nothing is said when no profile was read.
fn sampled_note(sampled: Option<u32>) -> String {
    match sampled {
        None => String::new(),
        Some(0) => " A profile never sampled this function.".to_owned(),
        Some(permille) => format!(
            " On the stack in {}.{}% of samples.",
            permille / 10,
            permille % 10
        ),
    }
}

impl fmt::Display for Evidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CognitiveComplexity {
                score,
                threshold,
                nesting_depth,
            } => write!(
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
            Self::ClassSize {
                fields,
                methods,
                statements: 0,
                threshold,
            } => write!(
                f,
                "{} members: {fields} fields and {methods} methods (threshold {threshold}).",
                fields + methods
            ),
            Self::ClassSize {
                fields,
                methods,
                statements,
                threshold,
            } => write!(
                f,
                "{} members ({fields} fields, {methods} methods) across {statements} statements (threshold {threshold}).",
                fields + methods
            ),
            Self::ClonedBlock {
                statements,
                nodes,
                occurrences,
            } => write!(
                f,
                "{statements}-statement block ({nodes} AST nodes) repeated at {occurrences} sites."
            ),
            Self::Name { identifier, reason } => write!(f, "`{identifier}` {reason}."),
            Self::UnusedExport {
                symbol,
                kind,
                ran: Some(ran),
            } if *ran > 0 => write!(
                f,
                "The {kind} `{symbol}` is exported and never imported, but a test run \
                 reached it {ran} times \u{2014} something the import graph cannot see \
                 uses it."
            ),
            Self::UnusedFile {
                exports,
                ran: Some(ran),
            } if *ran > 0 => write!(
                f,
                "Nothing imports this file and all {exports} of its exports are unused, \
                 but a test run reached it \u{2014} something the import graph cannot \
                 see loads it."
            ),
            Self::UnusedExport { symbol, kind, .. } => {
                write!(
                    f,
                    "Exported {kind} `{symbol}` is imported nowhere in the project."
                )
            }
            Self::UnusedFile { exports, .. } => write!(
                f,
                "No module imports this file, and none of its {exports} exports are used."
            ),
            Self::UnusedParameter {
                name,
                position,
                of,
                trailing: true,
            } if position == of => {
                write!(
                    f,
                    "Parameter `{name}` ({position} of {of}, last in the list) is never read."
                )
            }
            // Trailing without being last means every parameter after it is dead too,
            // which is why deleting from here is still a safe edit.
            Self::UnusedParameter {
                name,
                position,
                of,
                trailing: true,
            } => write!(
                f,
                "Parameter `{name}` ({position} of {of}) is never read, and nothing after it is either."
            ),
            Self::UnusedParameter {
                name,
                position,
                of,
                trailing: false,
            } => write!(
                f,
                "Parameter `{name}` ({position} of {of}) is never read, but {} follow it.",
                of - position
            ),
            Self::ImportCycle { modules, ring } => match ring.as_slice() {
                [] => write!(f, "{modules} modules import each other in a cycle."),
                ring => write!(f, "{}.", ring.join(" \u{2192} ")),
            },
            Self::UnusedBinding { name, kind } => {
                write!(f, "The {kind} `{name}` is declared and never read.")
            }
            Self::Unreachable { after } => {
                write!(
                    f,
                    "This statement cannot run: the block always {after} before reaching it."
                )
            }
            Self::CommentedCode { lines } => write!(
                f,
                "{} of commented-out code, which version control already remembers.",
                plural(*lines, "line", "lines")
            ),
            Self::MagicNumber {
                literal,
                occurrences: 1,
            } => {
                write!(f, "Unnamed literal `{literal}`.")
            }
            Self::MagicNumber {
                literal,
                occurrences,
            } => {
                write!(
                    f,
                    "Unnamed literal `{literal}`, repeated {occurrences} times in this file."
                )
            }
            Self::EmptyBody { construct } => write!(f, "This {construct} has an empty body."),
            Self::ClassShape {
                kind,
                fields,
                methods,
                trivial,
            } => write!(
                f,
                "{kind}: {fields} fields, {methods} methods, {trivial} of them trivial."
            ),
            Self::MessageChain { depth, limit } => {
                write!(
                    f,
                    "Reaches {depth} objects deep in one expression (limit {limit})."
                )
            }
            Self::ForeignAccess {
                owner,
                foreign,
                own,
            } => write!(
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
            Self::Custom { message, .. } => f.write_str(message),
            Self::InLoop {
                subject,
                depth,
                sampled,
            } => {
                if *depth > 1 {
                    write!(
                        f,
                        "`{subject}` runs on every pass of a loop nested {depth} deep.{}",
                        sampled_note(*sampled)
                    )
                } else {
                    write!(
                        f,
                        "`{subject}` runs on every pass of the loop.{}",
                        sampled_note(*sampled)
                    )
                }
            }
            Self::LoopDepth {
                depth,
                threshold,
                sampled,
            } => write!(
                f,
                "Loops nest {depth} deep (threshold {threshold}), so the innermost body runs \
                 O(n^{depth}) times.{}",
                sampled_note(*sampled)
            ),
            Self::Recursion { calls, sampled } => write!(
                f,
                "Calls itself {calls} times per invocation with no memo in sight.{}",
                sampled_note(*sampled)
            ),
            Self::BlockingCall { call, sampled } => write!(
                f,
                "`{call}` blocks the thread inside an `async` function.{}",
                sampled_note(*sampled)
            ),
            Self::Noise { marker } => match marker.as_str() {
                "" => f.write_str("This comment says nothing about the code."),
                marker => write!(f, "`{marker}` reads as something other than the code."),
            },
            Self::BooleanParameter { name, position } => write!(
                f,
                "Boolean parameter `{name}` ({position} in the list) selects between behaviours."
            ),
            Self::MutatedParameter { name } => {
                write!(f, "Parameter `{name}` is reassigned inside the function.")
            }
            Self::ConditionOperands { operands, limit } => {
                write!(
                    f,
                    "{operands} boolean operands in one condition (limit {limit})."
                )
            }
            Self::NegatedCondition => {
                write!(
                    f,
                    "A negated condition with an else branch, so both arms read backwards."
                )
            }
            Self::TestDirective { directive } => write!(f, "Declared with `{directive}`."),
            Self::NoAssertion { name } => {
                write!(f, "The test `{name}` makes no assertion.")
            }
            Self::RepeatedGroup { names, sites } => {
                write!(f, "The group ({names}) appears in {sites} signatures.")
            }
            Self::Discriminant {
                name,
                sites,
                branches,
            } => write!(
                f,
                "Branches on `{name}` in {sites} places, {branches} cases at the widest."
            ),
            Self::FieldLifetime { name, method } => write!(
                f,
                "Field `{name}` is only ever used by `{method}`, and never set in the constructor."
            ),
            Self::RefusedOverride { name } => {
                write!(
                    f,
                    "`{name}` overrides an inherited member only to refuse it."
                )
            }
            Self::DerivedReference { name } => {
                write!(f, "This base class names its own subclass `{name}`.")
            }
            Self::StaticOnOwnType { name } => write!(
                f,
                "`{name}` is static but takes an instance of its own class as its first parameter."
            ),
            Self::CoChange {
                partner,
                together,
                total,
            } => write!(
                f,
                "Changed with `{partner}` in {together} of its {total} commits."
            ),
            Self::ImpreciseType { construct } => {
                write!(
                    f,
                    "`{construct}` throws away what the type checker had already worked out."
                )
            }
            Self::JournalComment { marker } => {
                write!(
                    f,
                    "This comment carries `{marker}`, which version control already records."
                )
            }
            Self::RedundantComment { subject } => {
                write!(
                    f,
                    "This comment says no more than the name `{subject}` below it."
                )
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
            Self::SideEffectingQuery { name } => write!(
                f,
                "`{name}` reads as a question but writes to the object it was asked about."
            ),
            Self::VagueName { name } => {
                write!(f, "`{name}` names the act of running, not what is done.")
            }
            Self::DistantDeclaration { name, lines } => write!(
                f,
                "`{name}` is declared {lines} lines above the line that first reads it."
            ),
            Self::ObscuredExpression { operations, limit } => write!(
                f,
                "{operations} operations in one unnamed expression (limit {limit})."
            ),
            Self::BuriedConfiguration { kind, depth } => write!(
                f,
                "{kind} written {depth} blocks deep, where nothing configures it."
            ),
            Self::TemporalCoupling { field, methods } => write!(
                f,
                "`{field}` is tested before acting by {methods}, and no constructor sets it."
            ),
            Self::MisleadingName { name, promise } => {
                write!(
                    f,
                    "`{name}` is named for a {promise} but implements none of one."
                )
            }
            Self::InconsistentVocabulary { pair, sites } => write!(
                f,
                "This file uses {pair} across {sites} names for one idea."
            ),
            Self::IntimatePair { pair } => {
                write!(f, "{pair} each reach for the other by name.")
            }
            Self::AlternativeClasses { other, methods } => write!(
                f,
                "Same shape as `{other}` across {methods} methods, with no method name in common."
            ),
            Self::ParallelHierarchy {
                parent,
                twin,
                shared,
            } => write!(
                f,
                "Subclasses of `{parent}` and of `{twin}` are both named for {shared}."
            ),
            Self::BranchShape { shape } => write!(f, "{shape}"),
            Self::Breadth {
                counted,
                found,
                limit,
            } => write!(f, "{found} {counted} (limit {limit})."),
            Self::LayerViolation { from, to, target } => write!(
                f,
                "The `{from}` layer imports `{target}`, which is in `{to}`."
            ),
            Self::ForbiddenImport { target, by } => {
                write!(f, "`{by}` says this subtree may not import `{target}`.")
            }
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
    /// Whether a baseline already recorded this exact problem. Kept on the finding
    /// rather than in a set beside it because a reporter is handed findings and
    /// nothing else, and the display cap means the set and the list disagree.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub known: bool,
}

impl Finding {
    pub fn rank(&self) -> (Severity, Confidence, RuleId) {
        (self.severity, self.confidence, self.rule)
    }

    /// What to call this rule anywhere a reader or a machine sees it. A rule the
    /// project wrote carries its own name on the evidence, because the enum has one
    /// variant for all of them.
    pub fn slug(&self) -> &str {
        match &self.evidence {
            Evidence::Custom { id, .. } => id,
            _ => self.rule.slug(),
        }
    }

    pub fn category(&self) -> crate::rule::Category {
        match &self.evidence {
            Evidence::Custom { family, .. } => *family,
            _ => self.rule.spec().category,
        }
    }
}

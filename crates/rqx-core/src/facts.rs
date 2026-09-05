//! What a language backend measures about one file, before any rule has an opinion
//! about it. Every backend fills these in from its own parse tree; `crate::rules`
//! turns them into findings without knowing which language produced them.

use serde::{Deserialize, Serialize};

use crate::span::Span;

#[derive(Debug)]
pub struct FunctionMetrics {
    pub name: Option<String>,
    pub report: Span,
    pub parameters: u32,
    pub statements: u32,
    pub cognitive_complexity: u32,
    pub max_nesting: u32,
}

#[derive(Debug)]
pub struct ClassMetrics {
    pub name: Option<String>,
    pub report: Span,
    pub fields: u32,
    pub methods: u32,
    pub statements: u32,
}

#[derive(Debug)]
pub struct CloneCandidate {
    pub hash: u64,
    /// The same block in a vocabulary no grammar owns, for finding logic ported
    /// between two languages.
    pub shape: u64,
    /// The constants it holds, order-independent.
    pub literals: u64,
    pub span: Span,
    pub statements: u32,
    pub nodes: u32,
}

#[derive(Debug)]
pub struct SuspiciousName {
    pub identifier: String,
    pub span: Span,
    pub reason: &'static str,
}

#[derive(Debug, Default)]
pub struct FileMetrics {
    pub functions: Vec<FunctionMetrics>,
    pub classes: Vec<ClassMetrics>,
    pub clones: Vec<CloneCandidate>,
    pub names: Vec<SuspiciousName>,
}

/// One place in a file, with whatever text the rule needs to name it. `count` carries
/// the repetition for rules that only fire above a repeat threshold.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Spot {
    pub span: Span,
    pub detail: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Hygiene {
    /// Waivers a developer wrote next to the code. Not a `Spot`: a waiver carries a
    /// reason and a date, and nothing else here does.
    pub waivers: Vec<crate::suppress::Suppression>,
    pub unused_locals: Vec<Spot>,
    pub unreachable: Vec<Spot>,
    pub commented_code: Vec<Spot>,
    pub magic_numbers: Vec<Spot>,
    pub empty_catches: Vec<Spot>,
    pub suppressions: Vec<Spot>,
    pub banner_comments: Vec<Spot>,
    pub chat_residue: Vec<Spot>,
    pub apologies: Vec<Spot>,
    pub emoji: Vec<Spot>,
    pub example_values: Vec<Spot>,
    pub flag_parameters: Vec<Spot>,
    pub mutated_parameters: Vec<Spot>,
    pub complex_conditions: Vec<Spot>,
    pub negative_conditionals: Vec<Spot>,
    pub message_chains: Vec<Spot>,
    pub skipped_tests: Vec<Spot>,
    pub focused_tests: Vec<Spot>,
    pub assertionless_tests: Vec<Spot>,
    pub short_names: Vec<Spot>,
    pub imprecise_types: Vec<Spot>,
    pub journal_comments: Vec<Spot>,
    pub redundant_comments: Vec<Spot>,
    pub slow_tests: Vec<Spot>,
    pub mixed_languages: Vec<Spot>,
    pub boundary_arithmetic: Vec<Spot>,
    pub mixed_conventions: Vec<Spot>,
    pub side_effecting_queries: Vec<Spot>,
    pub vague_names: Vec<Spot>,
    pub distant_declarations: Vec<Spot>,
    pub obscured_expressions: Vec<Spot>,
    pub buried_configuration: Vec<Spot>,
    pub inconsistent_vocabulary: Vec<Spot>,
    pub redundant_else: Vec<Spot>,
    pub collapsible_ifs: Vec<Spot>,
    pub boolean_from_branch: Vec<Spot>,
    pub literal_comparisons: Vec<Spot>,
    pub self_comparisons: Vec<Spot>,
    pub identical_branches: Vec<Spot>,
    pub empty_blocks: Vec<Spot>,
    pub many_exits: Vec<Spot>,
    pub wide_switches: Vec<Spot>,
    pub callback_nesting: Vec<Spot>,
    pub assignments_in_conditions: Vec<Spot>,
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub max_exits: u32,
    pub max_cases: u32,
    pub callback_depth: u32,
    pub condition_operands: u32,
    pub message_chain_depth: u32,
    pub commented_code_lines: u32,
    pub magic_number_repeats: u32,
    pub short_name_scope_lines: u32,
    pub slow_test_ms: u32,
    pub boundary_repeats: u32,
    pub declaration_distance_lines: u32,
    pub expression_operations: u32,
    pub configuration_depth: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Named {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Pair {
    pub name: String,
    pub other: String,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Envy {
    pub method: String,
    pub owner: String,
    pub foreign: u32,
    pub own: u32,
    pub span: Span,
}

/// One piece of a class's state. The type is what separates a Decorator from a Proxy
/// from an Adapter: all three delegate, and only what they hold says which.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Field {
    pub name: String,
    pub type_name: Option<String>,
    /// The inner type of a collection, for the patterns built on holding many of
    /// something rather than one.
    pub element_type: Option<String>,
    pub is_static: bool,
    pub is_public: bool,
    pub span: Span,
}

/// One member a class offers. `returns` is what tells a builder's fluent step from an
/// ordinary setter, and a factory method from a getter.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Method {
    pub name: String,
    pub returns: Option<String>,
    pub arity: u32,
    pub is_static: bool,
    pub is_constructor: bool,
    pub is_abstract: bool,
    pub is_public: bool,
    /// Calls made on the receiver from this method's body, which is how a template
    /// method is told from an ordinary one.
    pub calls_own: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClassReport {
    pub name: String,
    pub span: Span,
    pub fields: u32,
    pub methods: u32,
    /// Methods excluding the constructor, which is not a behaviour the class offers.
    pub real_methods: u32,
    /// Getters, setters and one-line pass-throughs: members that hold no decisions.
    pub trivial: u32,
    pub public_members: u32,
    pub delegating: u32,
    pub has_heritage: bool,
    /// Overrides whose body only throws or is empty.
    pub refused: Vec<Named>,
    /// Private members nothing inside the class ever reads.
    pub dead_private: Vec<Named>,
    /// Fields written and read by exactly one method, never in the constructor.
    pub temporary_fields: Vec<Pair>,
    /// Subclass names mentioned by their own base class.
    pub derived_references: Vec<Named>,
    /// Statics whose first parameter is an instance of their own class.
    pub static_on_own_type: Vec<Named>,
    /// Methods leaning on another object far more than on the receiver.
    pub envious: Vec<Envy>,
    /// Fields no constructor sets that several methods test before they act.
    pub guarded_fields: Vec<Pair>,
    /// A name promising a pattern the members do not implement.
    pub misleading_name: Option<Named>,
    /// Public method names, and their parameter counts sorted. Two classes with the
    /// same shape and no name in common are two answers to one question.
    pub method_names: Vec<String>,
    pub arities: Vec<u32>,
    pub held: Vec<Field>,
    pub declared: Vec<Method>,
    /// Names this class extends or implements, so a structural pattern can ask what
    /// it promises rather than only whether it promises anything.
    pub supertypes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Clump {
    pub names: Vec<String>,
    pub sites: u32,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Discriminant {
    pub name: String,
    pub sites: u32,
    pub branches: u32,
    /// A branch writing back to the value it dispatched on, which is what separates a
    /// choice of algorithm from a state machine spelled out in conditionals.
    pub assigns_subject: bool,
    /// Every branch calling one function of the same shape, which is a dispatch table
    /// written as a conditional.
    pub uniform_dispatch: bool,
    pub span: Span,
}

/// An if/else chain where each branch asks a different question. `topics` are those
/// questions in order, so the advice can name them rather than point at a line.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Chain {
    pub topics: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Mention {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Patterns {
    pub clumps: Vec<Clump>,
    pub discriminants: Vec<Discriminant>,
    pub handler_chains: Vec<Chain>,
    pub derived_references: Vec<Mention>,
    pub encoded_names: Vec<Mention>,
    pub type_codes: Vec<Mention>,
    /// Pairs of classes in this file that each reach for the other by name.
    pub intimate_pairs: Vec<Mention>,
}

pub struct PatternLimits {
    pub clump_size: u32,
    pub clump_sites: u32,
}

/// What kind of thing a name exported from a file is. The words are the ones a
/// reader of that language would use, so `unused-export` can say "class `Foo`" in
/// Java and "value `foo`" in Go without the rule knowing either language.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub enum ExportKind {
    Function,
    Class,
    Interface,
    TypeAlias,
    Enum,
    Variable,
    Default,
}

impl ExportKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::TypeAlias => "type",
            Self::Enum => "enum",
            Self::Variable => "value",
            Self::Default => "default",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExportedSymbol {
    pub name: String,
    pub span: Span,
    pub kind: ExportKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ImportRequest {
    pub specifier: String,
    pub names: Vec<String>,
    pub span: Span,
    /// A namespace import or `export *` uses every name the target offers, so the
    /// target's exports cannot be judged unused from the names list alone.
    pub takes_everything: bool,
    /// `export … from "./x"`, which reads as an import to the graph and is not one
    /// to a reader. A barrel re-exporting from one module across several lines is
    /// doing that deliberately, and `export type` cannot be merged with `export`
    /// at all.
    #[serde(default)]
    pub re_export: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UnusedParameter {
    pub name: String,
    pub span: Span,
    pub function: Option<String>,
    /// One-based, so it reads the way a person counts arguments.
    pub position: u32,
    pub total: u32,
    pub exported: bool,
    /// Deleting a trailing parameter is a local edit; deleting one with parameters
    /// after it renumbers every call site, which is a different-sized change.
    pub trailing: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Heritage {
    pub child: String,
    pub parent: String,
    pub span: Span,
}

/// What one class looks like to the rest of the project. `ClassReport` never leaves
/// the file it was measured in, and the patterns built on inheritance need to compare
/// types across files.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TypeFacts {
    pub name: String,
    pub span: Span,
    pub supertypes: Vec<String>,
    pub methods: Vec<Method>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ModuleFacts {
    pub exports: Vec<ExportedSymbol>,
    pub imports: Vec<ImportRequest>,
    pub unused_parameters: Vec<UnusedParameter>,
    pub has_default_export: bool,
    /// Interfaces and abstract classes declared here, and the names anything in this
    /// file implements or extends. The graph pass subtracts one from the other.
    pub abstractions: Vec<ExportedSymbol>,
    pub implemented: Vec<String>,
    /// Every `extends` in this file, paired. `implemented` flattens the same names
    /// away, which answers "is anything a subclass" but never "a subclass of what".
    pub heritage: Vec<Heritage>,
    pub types: Vec<TypeFacts>,
}

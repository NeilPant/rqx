//! One analysis engine for every language Rqx reads. A language contributes a
//! `LanguageProfile` of node kinds, a module surface reader and an import resolver;
//! the walkers, the scope resolver and the rules are shared.

pub mod classes;
pub mod cpp;
pub mod custom;
pub mod go;
pub mod hierarchy;
pub mod hygiene;
pub mod java;
pub mod metrics;
pub mod module_graph;
pub mod parameters;
pub mod patterns;
pub mod profile;
pub mod python;
pub mod rewrite;
pub mod scope;
pub mod tree;
pub mod typescript;
pub mod vocabulary;

use std::path::Path;
use std::sync::Arc;

use rqx_core::config::RqxConfig;
use rqx_core::facts::{Limits, ModuleFacts, PatternLimits};
use rqx_core::finding::{Finding, SourceRange};
use rqx_core::line_index::LineIndex;
use rqx_core::span::Span;
use rqx_core::{Language, rules};
use tree_sitter::Node;

use crate::module_graph::Resolver;
use crate::profile::LanguageProfile;

/// What the analyser measured about one function, kept rather than discarded once
/// the thresholds have been compared against it: the numbers a rule fires on above a
/// limit are the same numbers a reader wants the distribution of.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct FunctionShape {
    pub name: Option<String>,
    pub range: SourceRange,
    /// Where the declaration ends, so a finding's offset and a profile's sample line
    /// can both be placed in the function they belong to.
    #[serde(default)]
    pub end: u32,
    #[serde(default)]
    pub last_line: u32,
    pub parameters: u32,
    pub statements: u32,
    pub cognitive_complexity: u32,
    pub max_nesting: u32,
    #[serde(default)]
    pub loop_depth: u32,
    #[serde(default)]
    pub self_calls: u32,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct ClassShape {
    pub name: Option<String>,
    pub range: SourceRange,
    pub fields: u32,
    pub methods: u32,
    pub statements: u32,
}

#[derive(Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct FileShape {
    pub functions: Vec<FunctionShape>,
    pub classes: Vec<ClassShape>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct CloneSite {
    pub hash: u64,
    /// Unsalted, and in a vocabulary no grammar owns: two languages sharing one of
    /// these are sharing the logic, not an accident of their syntax trees.
    pub shape: u64,
    pub literals: u64,
    pub range: SourceRange,
    pub statements: u32,
    pub nodes: u32,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct FileAnalysis {
    pub language: Language,
    pub findings: Vec<Finding>,
    pub advice: Vec<rqx_core::Advice>,
    pub clones: Vec<CloneSite>,
    pub module: ModuleFacts,
    pub code_lines: u32,
    #[serde(default)]
    pub shape: FileShape,
    /// The waivers this file asks for. Applied by the collector rather than here,
    /// because a waiver can name a rule only the whole project can decide.
    #[serde(default)]
    pub suppressions: Vec<rqx_core::suppress::Suppression>,
}

#[derive(Debug)]
pub enum AnalysisError {
    UnsupportedFile,
    Parse(String),
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFile => f.write_str("not a source file Rqx reads"),
            Self::Parse(message) => write!(f, "parse failed: {message}"),
        }
    }
}

/// What a table of node kinds cannot say: which names a file offers, how a written
/// import becomes a path, and how a class is put together where the grammar does not
/// hold it in one node.
pub struct Support {
    pub profile: &'static LanguageProfile,
    pub surface: fn(&str, Node<'_>) -> ModuleFacts,
    pub resolver: fn(&Path) -> Box<dyn Resolver>,
    /// How a class is assembled from the tree. Go declares a type in one place and
    /// its methods in another, so the default — one node with a body — is not enough.
    pub units: for<'tree> fn(
        &'static LanguageProfile,
        &str,
        Node<'tree>,
    ) -> Vec<classes::ClassUnit<'tree>>,
}

pub fn support(language: Language) -> &'static Support {
    match language {
        Language::TypeScript => &TYPESCRIPT_SUPPORT,
        Language::Python => &PYTHON_SUPPORT,
        Language::Go => &GO_SUPPORT,
        Language::Java => &JAVA_SUPPORT,
        Language::Cpp => &CPP_SUPPORT,
    }
}

static TYPESCRIPT_SUPPORT: Support = Support {
    profile: &typescript::TYPESCRIPT,
    surface: typescript::module_facts,
    resolver: |root| Box::new(typescript::TypeScriptResolver::new(root)),
    units: classes::units,
};

static GO_SUPPORT: Support = Support {
    profile: &go::GO,
    surface: go::module_facts,
    resolver: |root| Box::new(go::GoResolver::new(root)),
    units: go::units,
};

static CPP_SUPPORT: Support = Support {
    profile: &cpp::CPP,
    surface: cpp::module_facts,
    resolver: |root| Box::new(cpp::CppResolver::new(root)),
    units: classes::units,
};

static JAVA_SUPPORT: Support = Support {
    profile: &java::JAVA,
    surface: java::module_facts,
    resolver: |root| Box::new(java::JavaResolver::new(root)),
    units: classes::units,
};

static PYTHON_SUPPORT: Support = Support {
    profile: &python::PYTHON,
    surface: python::module_facts,
    resolver: |root| Box::new(python::PythonResolver::new(root)),
    units: classes::units,
};

thread_local! {
    /// A `Parser` allocates and loads its language's tables on construction, which on
    /// a twelve-thousand-file walk is twelve thousand of each. The tree it produces
    /// borrows nothing from it, so one per language per worker is enough.
    static PARSERS: std::cell::RefCell<
        std::collections::HashMap<(Language, bool), tree_sitter::Parser>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Extensions whose contents are markup as well as code, and so need the second
/// grammar where the language has one.
pub fn holds_markup(display: &str) -> bool {
    display.ends_with(".tsx") || display.ends_with(".jsx")
}

fn parse(
    language: Language,
    profile: &'static LanguageProfile,
    markup: bool,
    source: &str,
) -> Result<tree_sitter::Tree, AnalysisError> {
    let grammar = match (markup, profile.markup_grammar) {
        (true, Some(grammar)) => grammar,
        _ => profile.grammar,
    };
    PARSERS.with_borrow_mut(|parsers| {
        let parser = match parsers.entry((language, markup)) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&grammar())
                    .map_err(|error| AnalysisError::Parse(error.to_string()))?;
                entry.insert(parser)
            }
        };
        parser
            .parse(source, None)
            .ok_or_else(|| AnalysisError::Parse("unrecoverable syntax error".to_owned()))
    })
}

pub fn analyze(
    path: &Path,
    display: &str,
    source: &str,
    config: &RqxConfig,
) -> Result<FileAnalysis, AnalysisError> {
    let language = Language::of_path(path).ok_or(AnalysisError::UnsupportedFile)?;
    analyze_as(language, display, source, config)
}

pub fn analyze_as(
    language: Language,
    display: &str,
    source: &str,
    config: &RqxConfig,
) -> Result<FileAnalysis, AnalysisError> {
    let support = support(language);
    let profile = support.profile;
    let tree = parse(language, profile, holds_markup(display), source)?;
    let root = tree.root_node();

    let index = LineIndex::new(source);
    let measured = metrics::measure(profile, source, root);

    // Scope and symbol data is only built when a rule will read it: it is the most
    // expensive stage in the per-file pipeline and the structural rules need none of
    // it.
    let deep = config.needs_semantics();
    let scopes = deep.then(|| scope::resolve(profile, source, root));
    let thresholds = &config.thresholds;

    let (classes, patterns) = if deep {
        (
            classes::collect(
                profile,
                source,
                &(support.units)(profile, source, root),
                thresholds.feature_envy_ratio,
            ),
            patterns::collect(
                profile,
                source,
                root,
                &PatternLimits {
                    clump_size: thresholds.data_clump_size,
                    clump_sites: thresholds.data_clump_sites,
                },
            ),
        )
    } else {
        (Vec::new(), rqx_core::facts::Patterns::default())
    };

    let (mut module, hygiene) = match &scopes {
        Some(scopes) => {
            let mut module = (support.surface)(source, root);
            module.unused_parameters = parameters::unused(profile, source, root, scopes);
            let hygiene = hygiene::collect(
                profile,
                source,
                root,
                scopes,
                Limits {
                    function_locals: thresholds.function_locals,
                    try_statements: thresholds.try_statements,
                    max_exits: thresholds.max_exits,
                    max_cases: thresholds.max_cases,
                    min_cases: thresholds.min_cases,
                    callback_depth: thresholds.callback_depth,
                    condition_operands: thresholds.condition_operands,
                    message_chain_depth: thresholds.message_chain_depth,
                    commented_code_lines: thresholds.commented_code_lines,
                    magic_number_repeats: thresholds.magic_number_repeats,
                    short_name_scope_lines: thresholds.short_name_scope_lines,
                    slow_test_ms: thresholds.slow_test_ms,
                    boundary_repeats: thresholds.boundary_repeats,
                    declaration_distance_lines: thresholds.declaration_distance_lines,
                    expression_operations: thresholds.expression_operations,
                    configuration_depth: thresholds.configuration_depth,
                },
                &config.performance,
            );
            (module, hygiene)
        }
        None => (
            ModuleFacts::default(),
            rqx_core::facts::Hygiene {
                waivers: hygiene::waivers(profile, source, root),
                ..Default::default()
            },
        ),
    };

    // An exported binding has a reader this file cannot see, and whether that reader
    // exists is the module graph's question, not this one's.
    let exported: std::collections::HashSet<&str> = module
        .exports
        .iter()
        .map(|export| export.name.as_str())
        .collect();
    let mut hygiene = hygiene;
    hygiene
        .unused_locals
        .retain(|spot| !exported.contains(spot.detail.as_str()));
    hygiene.unused_imports = hygiene::unused_imports(
        profile,
        source,
        root,
        holds_markup(display),
        &module.imports,
    );
    // A waiver reaches by line, and only the index knows which line a byte is on.
    for waiver in &mut hygiene.waivers {
        waiver.line = index.locate(waiver.span.start).0;
    }

    let file: Arc<str> = Arc::from(display);
    let builder = rules::FindingBuilder {
        file: Arc::clone(&file),
        index: &index,
        config,
    };

    let mut findings = Vec::new();
    for function in &measured.functions {
        rules::check_function(function, &builder, &mut findings);
    }
    for class in &measured.classes {
        rules::check_class(class, &builder, &mut findings);
    }
    for name in &measured.names {
        rules::check_name(name, &builder, &mut findings);
    }
    for parameter in &module.unused_parameters {
        rules::check_parameter(parameter, &builder, &mut findings);
    }
    rules::check_imports(&module, &builder, &mut findings);
    rules::check_hygiene(&hygiene, &builder, &mut findings);
    custom::run(
        language,
        source,
        root,
        &config.custom,
        &builder,
        &mut findings,
    );
    for class in &classes {
        rules::check_class_shape(class, &builder, &mut findings);
    }
    rules::check_class_pairs(&classes, &builder, &mut findings);
    rules::check_patterns(&patterns, &builder, &mut findings);

    let advisor = rqx_core::signature::AdviceBuilder {
        file,
        index: &index,
        config,
        language,
    };
    let mut advice = Vec::new();
    rqx_core::signature::from_patterns(&patterns, &advisor, &mut advice);
    rqx_core::signature::from_classes(&classes, &advisor, &mut advice);
    module.types = classes
        .iter()
        .map(|class| rqx_core::facts::TypeFacts {
            name: class.name.clone(),
            span: class.span,
            supertypes: class.supertypes.clone(),
            methods: class.declared.clone(),
        })
        .collect();
    rqx_core::signature::iterating(&classes, profile.iteration_members, &advisor, &mut advice);

    // Two languages can produce the same shape hash for unrelated code, and a finding
    // saying a Go block duplicates a Java one would be true and useless.
    let salt = language as u64 + 1;
    let clones = measured
        .clones
        .iter()
        .filter(|candidate| {
            candidate.statements >= thresholds.clone_min_statements
                && candidate.nodes >= thresholds.clone_min_nodes
        })
        .map(|candidate| CloneSite {
            hash: candidate.hash.wrapping_mul(salt).rotate_left(salt as u32),
            shape: candidate.shape,
            literals: candidate.literals,
            range: builder.range(candidate.span),
            statements: candidate.statements,
            nodes: candidate.nodes,
        })
        .collect();

    // Moved out of `measured` rather than copied: the names are already owned here
    // and this runs for every function in the project.
    let shape = FileShape {
        functions: measured
            .functions
            .into_iter()
            .map(|function| FunctionShape {
                name: function.name,
                range: builder.range(function.report),
                end: function.extent.end,
                last_line: index.locate(function.extent.end).0,
                parameters: function.parameters,
                statements: function.statements,
                cognitive_complexity: function.cognitive_complexity,
                max_nesting: function.max_nesting,
                loop_depth: function.loop_depth,
                self_calls: function.self_calls,
            })
            .collect(),
        classes: measured
            .classes
            .into_iter()
            .map(|class| ClassShape {
                name: class.name,
                range: builder.range(class.report),
                fields: class.fields,
                methods: class.methods,
                statements: class.statements,
            })
            .collect(),
    };

    Ok(FileAnalysis {
        language,
        findings,
        advice,
        clones,
        module,
        code_lines: index.code_lines(),
        shape,
        suppressions: hygiene.waivers,
    })
}

/// Re-exported so a caller can build a `SourceRange` for a finding discovered after
/// the file's text was dropped — the module graph's, which only knows spans.
pub fn range_of(span: Span, file: &str, index: &LineIndex<'_>) -> SourceRange {
    let (line, column) = index.locate(span.start);
    let (end_line, _) = index.locate(span.end);
    SourceRange {
        file: Arc::from(file),
        start: span.start,
        end: span.end,
        line,
        column,
        end_line,
    }
}

/// The grammar a language parses with, for tools that want the tree without the
/// analysis.
pub fn grammar(language: Language) -> tree_sitter::Language {
    (support(language).profile.grammar)()
}

//! One analysis engine for every language Rqx reads. A language contributes a
//! `LanguageProfile` of node kinds, a module surface reader and an import resolver;
//! the walkers, the scope resolver and the rules are shared.

pub mod classes;
pub mod cost;
pub mod cpp;
pub mod custom;
pub mod go;
pub mod hierarchy;
pub mod highlight;
pub mod hygiene;
pub mod invariants;
pub mod java;
pub mod javascript;
pub mod library;
pub mod metrics;
pub mod module_graph;
pub mod names;
pub mod notices;
pub mod outline;
pub mod parameters;
pub mod patterns;
pub mod profile;
pub mod python;
pub mod removal;
pub mod rewrite;
pub mod scope;
pub mod tree;
pub mod typescript;
pub mod visibility;
pub mod vocabulary;

use std::path::Path;
use std::sync::Arc;

use rqx_core::config::RqxConfig;
use rqx_core::fact_plan::{Fact, FactPlan};
use rqx_core::facts::{Limits, ModuleFacts, PatternLimits};
use rqx_core::finding::{Finding, SourceRange};
use rqx_core::line_index::LineIndex;
use rqx_core::span::Span;
use rqx_core::{Language, rules};
use tree_sitter::Node;

use crate::cost::tables::CostTables;
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
    /// The clone detector's fingerprints of the function's body, when the body holds
    /// enough to be worth comparing.
    #[serde(default)]
    pub body: Option<BodyPrint>,
}

/// A function body as the clone detector fingerprints it. `hash` is salted by
/// language family, so equal hashes mean the same normalised syntax in one language.
/// `shape` and `literals` are the vocabulary every language shares, so equal pairs
/// can come from two languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct BodyPrint {
    pub hash: u64,
    pub shape: u64,
    pub literals: u64,
    pub statements: u32,
    pub nodes: u32,
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
    /// Where this file records work still to do. Carried out of the file because how
    /// old a note is can only be answered by the repository.
    #[serde(default)]
    pub todos: Vec<rqx_core::facts::Spot>,
    /// What the file's functions cost, for the complexity analysis to compose across
    /// files.
    #[serde(default)]
    pub cost: rqx_core::cost::FileCost,
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
            Self::Parse(message) if self.too_deep() || self.unparseable() => f.write_str(message),
            Self::Parse(message) => write!(f, "parse failed: {message}"),
        }
    }
}

/// How a parse that ran out of time begins its message. A method rather than a variant,
/// so code that matches on `AnalysisError` keeps compiling.
const GAVE_UP: &str = "gave up after";

impl AnalysisError {
    /// Whether the grammar ran out of time rather than failing outright.
    pub fn timed_out(&self) -> bool {
        matches!(self, Self::Parse(message) if message.starts_with(GAVE_UP))
    }

    /// Whether the file nests deeper than [`MAX_NESTING`], and so was left unanalysed.
    pub fn too_deep(&self) -> bool {
        matches!(self, Self::Parse(message) if message.starts_with(NESTED))
    }
}

/// How a parse refused for its depth begins its message.
const NESTED: &str = "nested";

/// How a file refused for its syntax errors begins its message.
const NOT_PARSEABLE: &str = "not parseable:";

impl AnalysisError {
    /// Whether the file is mostly text the grammar could not read, and so was left
    /// unanalysed rather than graded on the little that parsed.
    pub fn unparseable(&self) -> bool {
        matches!(self, Self::Parse(message) if message.starts_with(NOT_PARSEABLE))
    }
}

/// Why a parsed file is not worth analysing, if it is not: most of its text sits in
/// syntax errors, or it holds the markers of a merge conflict nobody resolved. A file
/// like that has next to nothing for the rules to read, and grading what little
/// parsed would call it pristine.
fn unreadable_syntax(tree: &tree_sitter::Tree, source: &str) -> Option<String> {
    let root = tree.root_node();
    if !root.has_error() {
        return None;
    }
    if holds_conflict_markers(source) {
        return Some(format!(
            "{NOT_PARSEABLE} it holds unresolved merge conflict markers"
        ));
    }
    let visible = |text: &str| text.chars().filter(|c| !c.is_whitespace()).count();
    let mut in_errors = 0;
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.is_error() {
            in_errors += visible(tree::text_of(node, source));
            continue;
        }
        if node.has_error() {
            let mut cursor = node.walk();
            pending.extend(node.children(&mut cursor));
        }
    }
    let total = visible(source).max(1);
    let percent = in_errors * 100 / total;
    (percent >= UNPARSEABLE_PERCENT).then(|| {
        format!("{NOT_PARSEABLE} {percent}% of its text is syntax the grammar could not read")
    })
}

/// The share of a file's text, in percent, that has to sit in syntax errors before the
/// file is skipped. Below it, the rules read what did parse, as they always have.
const UNPARSEABLE_PERCENT: usize = 50;

/// A `<<<<<<<` line, a `=======` line and a `>>>>>>>` line, in that order.
fn holds_conflict_markers(source: &str) -> bool {
    let mut lines = source.lines();
    lines.any(|line| line.starts_with("<<<<<<< "))
        && lines.any(|line| line.trim_end() == "=======")
        && lines.any(|line| line.starts_with(">>>>>>> "))
}

/// The deepest syntax tree Rqx analyses, in levels. Code a person writes stays under a
/// hundred; a generated `else if` chain of thousands of branches or a concatenation
/// of thousands of terms does not, and a deeper tree is skipped with that reason
/// rather than analysed. The analysis of every tree this deep fits [`DEEP_STACK`].
pub const MAX_NESTING: usize = 1_500;

/// Deeper than this, a tree is analysed on a thread of its own with [`DEEP_STACK`],
/// because some of the analysis recurses once per level and a worker thread's
/// default stack holds a few thousand levels at most.
const SHALLOW_NESTING: usize = 256;

/// The stack a deep tree is analysed on. Reserved, not committed: the memory is only
/// used as deep as the analysis goes.
const DEEP_STACK: usize = 512 << 20;

/// Runs `work` where a tree `depth` levels deep cannot exhaust the stack: on the
/// calling thread when the tree is of ordinary depth, and otherwise on a thread with
/// [`DEEP_STACK`].
fn on_stack_for<T: Send>(
    depth: usize,
    work: impl FnOnce() -> Result<T, AnalysisError> + Send,
) -> Result<T, AnalysisError> {
    if depth <= SHALLOW_NESTING {
        return work();
    }
    std::thread::scope(|scope| {
        let spawned = std::thread::Builder::new()
            .name("rqx-deep-file".into())
            .stack_size(DEEP_STACK)
            .spawn_scoped(scope, work);
        match spawned {
            Ok(thread) => thread
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            Err(error) => Err(AnalysisError::Parse(format!(
                "{NESTED} {depth} levels deep, and no thread could be started to analyse it: {error}"
            ))),
        }
    })
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
        Language::JavaScript => &JAVASCRIPT_SUPPORT,
    }
}

static TYPESCRIPT_SUPPORT: Support = Support {
    profile: &typescript::TYPESCRIPT,
    surface: typescript::module_facts,
    resolver: |root| Box::new(typescript::TypeScriptResolver::new(root)),
    units: classes::units,
};

/// The surface reader and the resolver are TypeScript's: a project mixing the two
/// resolves an import from either into either.
static JAVASCRIPT_SUPPORT: Support = Support {
    profile: &javascript::JAVASCRIPT,
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
/// grammar where the language has one. Any JavaScript file may hold JSX, whatever its
/// extension says.
pub fn holds_markup(display: &str) -> bool {
    display.rsplit_once('.').is_some_and(|(_, extension)| {
        extension == "tsx" || Language::JavaScript.extensions().contains(&extension)
    })
}

/// A file whose name says it is the data-access layer: `users.repository.ts`,
/// `OrderDao.java`, `queries.go`, `report.sql.ts`. Holding the SQL is that file's job.
/// Matched on the end of each part of the name, so `report.ts` is not a repository.
fn is_data_access_file(display: &str) -> bool {
    const LAYERS: [&str; 6] = ["repository", "repositories", "repo", "repos", "dao", "daos"];
    let name = display
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(display)
        .to_ascii_lowercase();
    let stem = name
        .rsplit_once('.')
        .map_or(name.as_str(), |(stem, _)| stem);
    stem.split(['.', '_', '-']).any(|part| {
        part == "sql" || part == "queries" || LAYERS.iter().any(|layer| part.ends_with(layer))
    })
}

/// Long enough that no file anybody maintains reaches it, short enough that one
/// pathological file costs a scan seconds rather than the whole run.
const PARSE_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// The file's syntax tree, refused when it nests deeper than [`MAX_NESTING`] levels.
pub(crate) fn parse(
    language: Language,
    profile: &'static LanguageProfile,
    markup: bool,
    source: &str,
) -> Result<tree_sitter::Tree, AnalysisError> {
    parse_measured(language, profile, markup, source).map(|(tree, _)| tree)
}

/// As [`parse`], with how many levels deep the tree goes.
fn parse_measured(
    language: Language,
    profile: &'static LanguageProfile,
    markup: bool,
    source: &str,
) -> Result<(tree_sitter::Tree, usize), AnalysisError> {
    let tree = parse_unchecked(language, profile, markup, source)?;
    let depth = tree::depth(tree.root_node());
    if depth > MAX_NESTING {
        return Err(AnalysisError::Parse(format!(
            "{NESTED} {depth} levels deep, past the {MAX_NESTING} Rqx analyses"
        )));
    }
    Ok((tree, depth))
}

fn parse_unchecked(
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
        // A grammar can go quadratic on input no human wrote — a minified bundle, a
        // generated blob that slipped the marker check — and a worker stuck in one
        // takes the whole scan with it. A deadline rather than a size limit, because
        // size does not predict which files do it.
        let deadline = std::time::Instant::now() + PARSE_BUDGET;
        let mut spent = |_: &tree_sitter::ParseState| std::time::Instant::now() > deadline;
        let bytes = source.as_bytes();
        parser
            .parse_with_options(
                &mut |at, _| bytes.get(at..).unwrap_or_default(),
                None,
                Some(tree_sitter::ParseOptions::new().progress_callback(&mut spent)),
            )
            .ok_or_else(|| {
                AnalysisError::Parse(format!(
                    "{GAVE_UP} {}s; the grammar was still working",
                    PARSE_BUDGET.as_secs()
                ))
            })
    })
}

pub fn analyze(
    path: &Path,
    display: &str,
    source: &str,
    config: &RqxConfig,
) -> Result<FileAnalysis, AnalysisError> {
    analyze_with(
        path,
        display,
        source,
        config,
        &CostTables::for_performance(&config.performance),
    )
}

/// As [`analyze`], for a caller that already holds the cost tables for the project, so
/// that a scan of thousands of files builds them once.
pub fn analyze_with(
    path: &Path,
    display: &str,
    source: &str,
    config: &RqxConfig,
    tables: &CostTables,
) -> Result<FileAnalysis, AnalysisError> {
    analyze_planned(path, display, source, config, tables, config.facts())
}

/// As [`analyze_with`], building the facts `plan` names. A scan passes more than this
/// file's own rules read when a rule reads the same facts from every file, as the
/// import graph does. `plan` must include `config.facts()`.
pub fn analyze_planned(
    path: &Path,
    display: &str,
    source: &str,
    config: &RqxConfig,
    tables: &CostTables,
    plan: FactPlan,
) -> Result<FileAnalysis, AnalysisError> {
    let language = Language::of_path(path).ok_or(AnalysisError::UnsupportedFile)?;
    analyze_as_planned(language, display, source, config, tables, plan)
}

pub fn analyze_as(
    language: Language,
    display: &str,
    source: &str,
    config: &RqxConfig,
) -> Result<FileAnalysis, AnalysisError> {
    analyze_as_with(
        language,
        display,
        source,
        config,
        &CostTables::for_performance(&config.performance),
    )
}

pub fn analyze_as_with(
    language: Language,
    display: &str,
    source: &str,
    config: &RqxConfig,
    tables: &CostTables,
) -> Result<FileAnalysis, AnalysisError> {
    analyze_as_planned(language, display, source, config, tables, config.facts())
}

/// As [`analyze_as_with`], building the facts `plan` names. `plan` must include
/// `config.facts()`.
pub fn analyze_as_planned(
    language: Language,
    display: &str,
    source: &str,
    config: &RqxConfig,
    tables: &CostTables,
    plan: FactPlan,
) -> Result<FileAnalysis, AnalysisError> {
    debug_assert_eq!(
        plan.union(config.facts()),
        plan,
        "a plan that builds less than the enabled rules read"
    );
    let profile = support(language).profile;
    let (tree, depth) = parse_measured(language, profile, holds_markup(display), source)?;
    if let Some(why) = unreadable_syntax(&tree, source) {
        return Err(AnalysisError::Parse(why));
    }
    on_stack_for(depth, || {
        analyze_tree(language, display, source, config, tables, plan, &tree)
    })
}

fn analyze_tree(
    language: Language,
    display: &str,
    source: &str,
    config: &RqxConfig,
    tables: &CostTables,
    plan: FactPlan,
    tree: &tree_sitter::Tree,
) -> Result<FileAnalysis, AnalysisError> {
    let support = support(language);
    let profile = support.profile;
    let root = tree.root_node();

    let index = LineIndex::new(source);
    let measured = metrics::measure(profile, source, root);

    // Scope and symbol data is only built when a rule will read it: it is the most
    // expensive stage in the per-file pipeline and the metric rules need none of it.
    let deep = plan.builds(Fact::Collected);
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

    let nodes = tree::Nodes::of(root);
    let (mut module, hygiene) = match &scopes {
        Some(scopes) => {
            let mut module = (support.surface)(source, root);
            module.unused_parameters = parameters::unused(profile, source, root, scopes);
            let hygiene = hygiene::collect_over(
                profile,
                source,
                &nodes,
                scopes,
                Limits {
                    class_collaborators: thresholds.class_collaborators,
                    log_percent: thresholds.log_percent,
                    max_results: thresholds.max_results,
                    narrated_run: thresholds.narrated_run,
                    explanation_ratio: thresholds.explanation_ratio,
                    guard_cascade: thresholds.guard_cascade,
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
                waivers: hygiene::waivers_over(profile, source, &nodes),
                ..Default::default()
            },
        ),
    };

    let extracted = cost::read(
        profile,
        source,
        root,
        &cost::Inputs {
            tables,
            performance: &config.performance,
            module: deep.then_some(&module),
        },
    );
    let cost = extracted.cost;
    let cost_sites = cost::apart_from_scans(extracted.sites, &hygiene.scans_in_loops);

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
    if is_data_access_file(display) {
        hygiene.mixed_languages.clear();
    }
    hygiene.unused_imports = hygiene::unused_imports_over(
        profile,
        source,
        &nodes,
        holds_markup(display),
        &module.imports,
    );
    // A file whose every import comes straight back out of it is an index: it adds a
    // name to every path through the project and nothing else.
    let forwarded = module
        .imports
        .iter()
        .filter(|request| request.re_export)
        .count();
    if forwarded > 1 && forwarded == module.imports.len() && !module.exports.is_empty() {
        hygiene.barrel_modules.push(rqx_core::facts::Spot {
            span: rqx_core::span::Span::new(0, 0),
            detail: module.exports.len().to_string(),
            count: forwarded as u32,
        });
    }
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
    rules::check_cost_sites(&cost_sites, &builder, &mut findings);
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
    findings.retain(|finding| rules::applies_in(finding, Some(language)));
    rules::without_repeats(&mut findings);

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
    // saying a Go block duplicates a Java one would be true and useless. TypeScript
    // and JavaScript share a salt because a block copied between them is a copy.
    let salt = language.family() as u64 + 1;
    let is_comparable = |candidate: &&rqx_core::facts::CloneCandidate| {
        candidate.statements >= thresholds.clone_min_statements
            && candidate.nodes >= thresholds.clone_min_nodes
    };
    let salted = |hash: u64| hash.wrapping_mul(salt).rotate_left(salt as u32);
    let bodies: Vec<Option<BodyPrint>> = measured
        .functions
        .iter()
        .map(|function| {
            body_of(
                function.extent,
                measured.clones.iter().filter(is_comparable),
            )
            .map(|candidate| BodyPrint {
                hash: salted(candidate.hash),
                shape: candidate.shape,
                literals: candidate.literals,
                statements: candidate.statements,
                nodes: candidate.nodes,
            })
        })
        .collect();
    let clones = measured
        .clones
        .iter()
        .filter(is_comparable)
        .map(|candidate| CloneSite {
            hash: salted(candidate.hash),
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
            .zip(bodies)
            .map(|(function, body)| FunctionShape {
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
                body,
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
        todos: hygiene.todos,
        cost,
    })
}

/// The block that is a function's body: the largest one inside the function, because
/// the body contains every block that a branch or a loop in it opens.
fn body_of<'a>(
    function: Span,
    candidates: impl Iterator<Item = &'a rqx_core::facts::CloneCandidate>,
) -> Option<&'a rqx_core::facts::CloneCandidate> {
    candidates
        .filter(|candidate| {
            function.start <= candidate.span.start && candidate.span.end <= function.end
        })
        .max_by_key(|candidate| candidate.nodes)
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

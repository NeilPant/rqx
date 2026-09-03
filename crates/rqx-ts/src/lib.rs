use std::path::Path;
use std::sync::Arc;

use oxc::allocator::Allocator;
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::SourceType;
use rqx_core::config::RqxConfig;
use rqx_core::finding::{Finding, SourceRange};

pub mod classes;
pub mod hygiene;
pub mod metrics;
pub mod patterns;
pub mod module_graph;
pub mod semantics;
pub mod span_io;
pub mod naming;
pub mod rules;

pub struct LineIndex<'src> {
    source: &'src str,
    starts: Vec<u32>,
}

impl<'src> LineIndex<'src> {
    pub fn new(source: &'src str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset as u32 + 1),
        );
        Self { source, starts }
    }

    pub fn locate(&self, offset: u32) -> (u32, u32) {
        let line = self.starts.partition_point(|start| *start <= offset).max(1);
        let line_start = self.starts[line - 1] as usize;
        let end = (offset as usize).min(self.source.len());
        let column = self.source[line_start..end].chars().count() as u32 + 1;
        (line as u32, column)
    }

    pub fn line_text(&self, line: u32) -> &'src str {
        let start = self.starts.get(line as usize - 1).copied().unwrap_or(0) as usize;
        let end = self
            .starts
            .get(line as usize)
            .map_or(self.source.len(), |next| *next as usize);
        self.source[start..end].trim_end_matches(['\n', '\r'])
    }

    pub fn code_lines(&self) -> u32 {
        self.source.lines().filter(|line| !line.trim().is_empty()).count() as u32
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct CloneSite {
    pub hash: u64,
    pub range: SourceRange,
    pub statements: u32,
    pub nodes: u32,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct FileAnalysis {
    pub findings: Vec<Finding>,
    pub clones: Vec<CloneSite>,
    pub module: semantics::ModuleFacts,
    pub hygiene: hygiene::Hygiene,
    pub code_lines: u32,
}

#[derive(Debug)]
pub enum AnalysisError {
    UnsupportedFile,
    Parse(String),
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFile => f.write_str("not a TypeScript source file"),
            Self::Parse(message) => write!(f, "parse failed: {message}"),
        }
    }
}

pub fn analyze(
    path: &Path,
    display: &str,
    source: &str,
    config: &RqxConfig,
) -> Result<FileAnalysis, AnalysisError> {
    let source_type = SourceType::from_path(path).map_err(|_| AnalysisError::UnsupportedFile)?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if parsed.panicked {
        let message = parsed
            .diagnostics
            .iter()
            .next()
            .map_or_else(|| "unrecoverable syntax error".to_owned(), |error| error.message.to_string());
        return Err(AnalysisError::Parse(message));
    }

    let index = LineIndex::new(source);
    let measured = metrics::measure(source, &parsed.program);

    // Scope and symbol data is only built when a rule will read it: it is the most
    // expensive stage in the per-file pipeline and the structural rules need none
    // of it.
    let (classes, patterns) = if config.rules.needs_semantics() {
        (
            classes::collect(&parsed.program, config.thresholds.feature_envy_ratio),
            patterns::collect(
                &parsed.program,
                &patterns::PatternLimits {
                    clump_size: config.thresholds.data_clump_size,
                    clump_sites: config.thresholds.data_clump_sites,
                },
            ),
        )
    } else {
        (Vec::new(), patterns::Patterns::default())
    };

    let (module, mut hygiene) = if config.rules.needs_semantics() {
        let built = SemanticBuilder::new().with_check_syntax_error(false).build(&parsed.program);
        let thresholds = &config.thresholds;
        (
            semantics::collect(&parsed.program, &built.semantic),
            hygiene::collect(
                &parsed.program,
                &built.semantic,
                source,
                hygiene::Limits {
                    condition_operands: thresholds.condition_operands,
                    message_chain_depth: thresholds.message_chain_depth,
                    commented_code_lines: thresholds.commented_code_lines,
                    magic_number_repeats: thresholds.magic_number_repeats,
                    short_name_scope_lines: thresholds.short_name_scope_lines,
                    slow_test_ms: thresholds.slow_test_ms,
                    boundary_repeats: thresholds.boundary_repeats,
                },
            ),
        )
    } else {
        (semantics::ModuleFacts::default(), hygiene::Hygiene::default())
    };
    // An exported binding has a reader this file cannot see, and whether that reader
    // exists is the module graph's question, not this one's.
    let exported: std::collections::HashSet<&str> =
        module.exports.iter().map(|export| export.name.as_str()).collect();
    hygiene.unused_locals.retain(|spot| !exported.contains(spot.detail.as_str()));

    let builder = rules::FindingBuilder {
        file: Arc::from(display),
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
    rules::check_hygiene(&hygiene, &builder, &mut findings);
    for class in &classes {
        rules::check_class_shape(class, &builder, &mut findings);
    }
    rules::check_patterns(&patterns, &builder, &mut findings);

    let thresholds = &config.thresholds;
    let clones = measured
        .clones
        .iter()
        .filter(|candidate| {
            candidate.statements >= thresholds.clone_min_statements
                && candidate.nodes >= thresholds.clone_min_nodes
        })
        .map(|candidate| CloneSite {
            hash: candidate.hash,
            range: builder.range(candidate.span),
            statements: candidate.statements,
            nodes: candidate.nodes,
        })
        .collect();

    Ok(FileAnalysis {
        findings,
        clones,
        module,
        hygiene,
        code_lines: index.code_lines(),
    })
}

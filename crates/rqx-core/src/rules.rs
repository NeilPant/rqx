use std::sync::Arc;

use crate::config::RqxConfig;
use crate::facts::{ClassMetrics, FunctionMetrics, SuspiciousName, UnusedParameter};
use crate::finding::{Confidence, Evidence, Finding, SourceRange};
use crate::line_index::LineIndex;
use crate::rule::RuleId;
use crate::span::Span;

pub struct FindingBuilder<'a> {
    pub file: Arc<str>,
    pub index: &'a LineIndex<'a>,
    pub config: &'a RqxConfig,
}

impl FindingBuilder<'_> {
    pub fn range(&self, span: Span) -> SourceRange {
        let (line, column) = self.index.locate(span.start);
        let (end_line, _) = self.index.locate(span.end);
        SourceRange {
            file: Arc::clone(&self.file),
            start: span.start,
            end: span.end,
            line,
            column,
            end_line,
        }
    }

    pub fn emit(
        &self,
        out: &mut Vec<Finding>,
        rule: RuleId,
        span: Span,
        evidence: Evidence,
        fix_hint: String,
    ) {
        self.emit_with(out, rule, span, evidence, fix_hint, rule.spec().confidence);
    }

    /// Some rules can only claim `Verified` for a subset of their findings, so the
    /// spec's confidence is a default the rule may lower per finding — never raise.
    pub fn emit_with(
        &self,
        out: &mut Vec<Finding>,
        rule: RuleId,
        span: Span,
        evidence: Evidence,
        fix_hint: String,
        confidence: Confidence,
    ) {
        if !self.config.rules.enabled(rule) {
            return;
        }
        let spec = rule.spec();
        out.push(Finding {
            rule,
            location: self.range(span),
            severity: spec.severity,
            confidence: confidence.min(spec.confidence),
            evidence,
            fix_hint,
            related: Vec::new(),
            known: false,
        });
    }
}

/// The one sentence each flow rule says about what it found. Written here rather than
/// in the collector so the facts stay a shape and the prose stays a report.
fn describe(rule: RuleId, detail: &str) -> String {
    match rule {
        RuleId::RedundantElse => {
            "The branch above always returns, so this `else` is not a decision.".to_owned()
        }
        RuleId::CollapsibleIf => "This `if` is the only statement inside another one.".to_owned(),
        RuleId::BooleanFromBranch => {
            "Both arms return a boolean literal, so the branch restates the condition.".to_owned()
        }
        RuleId::LiteralComparison => format!("Compared against the literal `{detail}`."),
        RuleId::SelfComparison => format!("`{detail}` is compared against itself."),
        RuleId::IdenticalBranches => "Both arms of this branch do the same thing.".to_owned(),
        RuleId::EmptyBlock => format!("This {detail} has an empty body."),
        _ => detail.to_owned(),
    }
}

fn subject(name: Option<&String>) -> String {
    name.map_or_else(|| "this function".to_owned(), |name| format!("`{name}`"))
}

pub fn check_function(
    metrics: &FunctionMetrics,
    builder: &FindingBuilder<'_>,
    out: &mut Vec<Finding>,
) {
    let thresholds = &builder.config.thresholds;
    let name = subject(metrics.name.as_ref());

    if metrics.statements > thresholds.long_function_statements {
        builder.emit(
            out,
            RuleId::LongFunction,
            metrics.report,
            Evidence::StatementCount {
                count: metrics.statements,
                threshold: thresholds.long_function_statements,
            },
            format!("split {name} into named steps, each one nameable without the word \"and\""),
        );
    }

    if metrics.cognitive_complexity > thresholds.cognitive_complexity {
        builder.emit(
            out,
            RuleId::CognitiveComplexity,
            metrics.report,
            Evidence::CognitiveComplexity {
                score: metrics.cognitive_complexity,
                threshold: thresholds.cognitive_complexity,
                nesting_depth: metrics.max_nesting,
            },
            format!(
                "lift the conditions in {name} into named predicates, then extract the \
                 deepest block into its own function"
            ),
        );
    }

    if metrics.max_nesting > thresholds.nesting_depth {
        builder.emit(
            out,
            RuleId::DeepNesting,
            metrics.report,
            Evidence::NestingDepth {
                depth: metrics.max_nesting,
                threshold: thresholds.nesting_depth,
            },
            format!(
                "invert the outer conditions of {name} into early returns so the innermost \
                 code sits at depth 1"
            ),
        );
    }

    if metrics.loop_depth > thresholds.loop_nesting {
        builder.emit(
            out,
            RuleId::DeepLoopNesting,
            metrics.report,
            Evidence::LoopDepth {
                depth: metrics.loop_depth,
                threshold: thresholds.loop_nesting,
                sampled: None,
            },
            format!(
                "index what the inner loops search, so {name} walks the data once instead \
                 of once per pass"
            ),
        );
    }

    if metrics.self_calls >= 2 && !metrics.memo_guard {
        builder.emit(
            out,
            RuleId::BranchingRecursion,
            metrics.report,
            Evidence::Recursion {
                calls: metrics.self_calls,
                sampled: None,
            },
            format!("keep what {name} has already answered, or rewrite it as a loop over a table"),
        );
    }

    if metrics.parameters > thresholds.max_params {
        builder.emit(
            out,
            RuleId::LongParameterList,
            metrics.report,
            Evidence::ParameterCount {
                count: metrics.parameters,
                threshold: thresholds.max_params,
            },
            format!(
                "group the {} parameters of {name} into one options object, or pass the \
                 object the caller already has",
                metrics.parameters
            ),
        );
    }
}

pub fn check_class(metrics: &ClassMetrics, builder: &FindingBuilder<'_>, out: &mut Vec<Finding>) {
    let thresholds = &builder.config.thresholds;
    let members = metrics.fields + metrics.methods;
    if members <= thresholds.class_members && metrics.statements <= thresholds.class_statements {
        return;
    }
    let name = metrics
        .name
        .as_ref()
        .map_or_else(|| "this class".to_owned(), |name| format!("`{name}`"));
    builder.emit(
        out,
        RuleId::LargeClass,
        metrics.report,
        Evidence::ClassSize {
            fields: metrics.fields,
            methods: metrics.methods,
            statements: metrics.statements,
            threshold: thresholds.class_members,
        },
        format!(
            "extract the fields of {name} that only a subset of its {} methods touch into \
             their own class",
            metrics.methods
        ),
    );
}

pub fn check_name(name: &SuspiciousName, builder: &FindingBuilder<'_>, out: &mut Vec<Finding>) {
    builder.emit(
        out,
        RuleId::MeaninglessName,
        name.span,
        Evidence::Name {
            identifier: name.identifier.clone(),
            reason: name.reason.to_owned(),
        },
        format!("rename `{}` to what it holds", name.identifier),
    );
}

pub fn check_parameter(
    parameter: &UnusedParameter,
    builder: &FindingBuilder<'_>,
    out: &mut Vec<Finding>,
) {
    let owner = parameter
        .function
        .as_ref()
        .map_or_else(|| "this function".to_owned(), |name| format!("`{name}`"));

    // An exported function's signature is a contract with callers this scan cannot
    // see, and a middle parameter cannot be dropped without renumbering every call
    // site. Neither is a verified-safe deletion, so neither claims to be one.
    let confidence = match (parameter.exported, parameter.trailing) {
        (false, true) => Confidence::Verified,
        _ => Confidence::Heuristic,
    };

    let fix_hint = if parameter.trailing {
        format!(
            "drop `{}` from {owner} and from its call sites",
            parameter.name
        )
    } else {
        format!(
            "rename `{}` to `_{}` to record that it is deliberately ignored, or drop it and \
             renumber the {} after it",
            parameter.name,
            parameter.name,
            match parameter.total - parameter.position {
                1 => "one parameter".to_owned(),
                rest => format!("{rest} parameters"),
            }
        )
    };

    builder.emit_with(
        out,
        RuleId::UnusedParameter,
        parameter.span,
        Evidence::UnusedParameter {
            name: parameter.name.clone(),
            position: parameter.position,
            of: parameter.total,
            trailing: parameter.trailing,
        },
        fix_hint,
        confidence,
    );
}

/// Straight-line mapping from a detected spot to a finding. Each arm names the rule,
/// the evidence it carries and the specific instruction; the shared spec supplies the
/// catalogue citation, so nothing here can invent one.
/// What a file's import list says about itself. Read here rather than in a collector
/// because the module facts are already assembled by the time rules run.
pub fn check_imports(
    module: &crate::facts::ModuleFacts,
    builder: &FindingBuilder<'_>,
    out: &mut Vec<Finding>,
) {
    let mut seen: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
    for request in module.imports.iter().filter(|request| !request.re_export) {
        *seen.entry(request.specifier.as_str()).or_default() += 1;
    }
    for request in &module.imports {
        let climbed = climb(&request.specifier);
        if climbed > builder.config.thresholds.relative_climb {
            builder.emit(
                out,
                RuleId::DeepRelativeImport,
                request.span,
                Evidence::Breadth {
                    counted: "levels up".to_owned(),
                    found: climbed,
                    limit: builder.config.thresholds.relative_climb,
                },
                format!(
                    "move what `{}` reaches for somewhere both ends can import, or turn \
                     the path into one the project resolves by name",
                    request.specifier
                ),
            );
        }
    }
    for request in module.imports.iter().filter(|request| !request.re_export) {
        let count = seen
            .get(request.specifier.as_str())
            .copied()
            .unwrap_or_default();
        // Reported on the second and later statements, so the finding sits on the one
        // to delete rather than on the one that was there first.
        if count < 2 || request.span.start == first_of(module, &request.specifier) {
            continue;
        }
        builder.emit(
            out,
            RuleId::DuplicateImport,
            request.span,
            Evidence::Name {
                identifier: request.specifier.clone(),
                reason: "imported more than once".to_owned(),
            },
            format!(
                "fold these names into the first `{}` import",
                request.specifier
            ),
        );
    }
}

/// How far a relative specifier climbs before it descends. `../../shared/log` is
/// two, and so is Python's `...shared.log` — one leading dot is the current package,
/// and every dot after it is a level. `./sibling` and a bare package name are none.
fn climb(specifier: &str) -> u32 {
    let dots = specifier.chars().take_while(|c| *c == '.').count();
    if dots > 1 && !specifier[dots..].starts_with(['/', '\\']) {
        return dots as u32 - 1;
    }
    specifier
        .split(['/', '\\'])
        .take_while(|segment| *segment == ".." || *segment == ".")
        .filter(|segment| *segment == "..")
        .count() as u32
}

fn first_of(module: &crate::facts::ModuleFacts, specifier: &str) -> u32 {
    module
        .imports
        .iter()
        .filter(|request| !request.re_export && request.specifier == specifier)
        .map(|request| request.span.start)
        .min()
        .unwrap_or_default()
}

pub fn check_hygiene(
    hygiene: &crate::facts::Hygiene,
    builder: &FindingBuilder<'_>,
    out: &mut Vec<Finding>,
) {
    use crate::facts::Spot;

    let limits = &builder.config.thresholds;
    let emit = |out: &mut Vec<Finding>, rule: RuleId, spot: &Spot, evidence, hint: String| {
        builder.emit(out, rule, spot.span, evidence, hint);
    };

    for spot in &hygiene.unused_locals {
        emit(
            out,
            RuleId::UnusedLocal,
            spot,
            Evidence::UnusedBinding {
                name: spot.detail.clone(),
                kind: "local".to_owned(),
            },
            format!(
                "delete `{}`, or use it if the work computing it still matters",
                spot.detail
            ),
        );
    }
    for spot in &hygiene.unreachable {
        emit(
            out,
            RuleId::UnreachableCode,
            spot,
            Evidence::Unreachable {
                after: spot.detail.clone(),
            },
            "delete it, or move it above the exit if it was meant to run".to_owned(),
        );
    }
    for spot in &hygiene.commented_code {
        emit(
            out,
            RuleId::CommentedOutCode,
            spot,
            Evidence::CommentedCode { lines: spot.count },
            "delete it; git already has it, and a comment cannot say why it is still here"
                .to_owned(),
        );
    }
    for spot in &hygiene.magic_numbers {
        emit(
            out,
            RuleId::MagicNumber,
            spot,
            Evidence::MagicNumber {
                literal: spot.detail.clone(),
                occurrences: spot.count,
            },
            format!(
                "name `{}` once and refer to the name at all {} sites",
                spot.detail, spot.count
            ),
        );
    }
    for spot in &hygiene.empty_catches {
        emit(
            out,
            RuleId::EmptyCatch,
            spot,
            Evidence::EmptyBody {
                construct: spot.detail.clone(),
            },
            "log it, rethrow it, or write the reason ignoring it is correct".to_owned(),
        );
    }
    for spot in &hygiene.imprecise_types {
        emit(
            out,
            RuleId::ImpreciseType,
            spot,
            Evidence::ImpreciseType {
                construct: spot.detail.clone(),
            },
            match spot.detail.as_str() {
                "any" => {
                    "name the shape you actually accept, or `unknown` and narrow it".to_owned()
                }
                "!" => "check for the absence and handle it, so the guarantee is real".to_owned(),
                _ => "narrow the value with a check the checker can follow".to_owned(),
            },
        );
    }
    for (spots, rule, counted, limit, hint) in [
        (
            &hygiene.many_exits,
            RuleId::ManyExits,
            "ways out",
            limits.max_exits,
            "give the function one exit, or extract the branches that end early",
        ),
        (
            &hygiene.wide_switches,
            RuleId::WideSwitch,
            "arms",
            limits.max_cases,
            "give the thing being switched on a type, and let it answer for itself",
        ),
        (
            &hygiene.callback_nesting,
            RuleId::CallbackNesting,
            "levels deep",
            limits.callback_depth,
            "name the inner steps and call them, so the order they run in is the order they read in",
        ),
    ] {
        for spot in spots {
            emit(
                out,
                rule,
                spot,
                Evidence::Breadth {
                    counted: counted.to_owned(),
                    found: spot.count,
                    limit,
                },
                hint.to_owned(),
            );
        }
    }
    for spot in &hygiene.assignments_in_conditions {
        emit(
            out,
            RuleId::AssignmentInCondition,
            spot,
            Evidence::BranchShape {
                shape: format!("`{}` assigns where a condition is expected.", spot.detail),
            },
            "assign above the branch, or compare if that is what was meant".to_owned(),
        );
    }
    for (spots, rule, hint) in [
        (
            &hygiene.scans_in_loops,
            RuleId::ScanInLoop,
            "build a set or a map of what the scan looks for once, above the loop",
        ),
        (
            &hygiene.queries_in_loops,
            RuleId::QueryInLoop,
            "gather the keys first and ask for all of them in one call",
        ),
        (
            &hygiene.awaits_in_loops,
            RuleId::AwaitInLoop,
            "start every call before waiting on any of them",
        ),
        (
            &hygiene.invariants_in_loops,
            RuleId::InvariantInLoop,
            "compile it once above the loop and reuse it",
        ),
        (
            &hygiene.copied_accumulators,
            RuleId::AccumulateByCopy,
            "add to the accumulator in place instead of rebuilding it",
        ),
    ] {
        for spot in spots {
            emit(
                out,
                rule,
                spot,
                Evidence::InLoop {
                    subject: spot.detail.clone(),
                    depth: spot.count,
                    sampled: None,
                },
                hint.to_owned(),
            );
        }
    }
    for spot in &hygiene.blocking_in_async {
        emit(
            out,
            RuleId::BlockingCallInAsync,
            spot,
            Evidence::BlockingCall {
                call: spot.detail.clone(),
                sampled: None,
            },
            format!(
                "call the asynchronous form of `{}`, or move the work off this thread",
                spot.detail
            ),
        );
    }
    for (spots, rule, hint) in [
        (
            &hygiene.redundant_else,
            RuleId::RedundantElse,
            "drop the `else` and outdent its body; the branch above already returned",
        ),
        (
            &hygiene.collapsible_ifs,
            RuleId::CollapsibleIf,
            "join the two conditions with `&&` and keep one branch",
        ),
        (
            &hygiene.boolean_from_branch,
            RuleId::BooleanFromBranch,
            "return the condition itself",
        ),
        (
            &hygiene.literal_comparisons,
            RuleId::LiteralComparison,
            "use the value directly, or negate it",
        ),
        (
            &hygiene.self_comparisons,
            RuleId::SelfComparison,
            "one side was meant to be something else; decide which",
        ),
        (
            &hygiene.identical_branches,
            RuleId::IdenticalBranches,
            "keep one body and delete the branch, or make the arms differ",
        ),
        (
            &hygiene.empty_blocks,
            RuleId::EmptyBlock,
            "write what should happen, or delete the branch",
        ),
    ] {
        for spot in spots {
            emit(
                out,
                rule,
                spot,
                Evidence::BranchShape {
                    shape: describe(rule, &spot.detail),
                },
                hint.to_owned(),
            );
        }
    }
    for spot in &hygiene.journal_comments {
        emit(
            out,
            RuleId::JournalComment,
            spot,
            Evidence::JournalComment {
                marker: spot.detail.clone(),
            },
            "delete it; `git log` and `git blame` answer this and stay correct".to_owned(),
        );
    }
    for spot in &hygiene.redundant_comments {
        emit(
            out,
            RuleId::RedundantComment,
            spot,
            Evidence::RedundantComment {
                subject: spot.detail.clone(),
            },
            format!(
                "delete it, or say why `{}` exists rather than what it is",
                spot.detail
            ),
        );
    }
    for spot in &hygiene.slow_tests {
        emit(
            out,
            RuleId::SlowTest,
            spot,
            Evidence::SlowTest {
                call: spot.detail.clone(),
                pause: spot.count,
            },
            "wait on the thing itself rather than on the clock, or fake the timer".to_owned(),
        );
    }
    for spot in &hygiene.mixed_languages {
        emit(
            out,
            RuleId::MixedLanguageFile,
            spot,
            Evidence::ForeignLanguage {
                language: spot.detail.clone(),
            },
            format!(
                "move the {} into its own file, where its own tooling can read it",
                spot.detail
            ),
        );
    }
    for spot in &hygiene.boundary_arithmetic {
        emit(
            out,
            RuleId::BoundaryArithmetic,
            spot,
            Evidence::BoundaryArithmetic {
                subject: spot.detail.clone(),
                sites: spot.count,
            },
            format!(
                "name the boundary once, so `{}` is adjusted in one place",
                spot.detail
            ),
        );
    }
    for spot in &hygiene.mixed_conventions {
        emit(
            out,
            RuleId::MixedConventions,
            spot,
            Evidence::MixedConventions {
                minority: spot.detail.clone(),
                count: spot.count,
            },
            "pick the convention the rest of the project uses and rename to match".to_owned(),
        );
    }
    for spot in &hygiene.side_effecting_queries {
        emit(
            out,
            RuleId::QueryWithSideEffect,
            spot,
            Evidence::SideEffectingQuery {
                name: spot.detail.clone(),
            },
            format!(
                "split `{}` into the question and the change, or rename it for what it does",
                spot.detail
            ),
        );
    }
    for spot in &hygiene.vague_names {
        emit(
            out,
            RuleId::VagueFunctionName,
            spot,
            Evidence::VagueName {
                name: spot.detail.clone(),
            },
            format!("rename `{}` after what it does to what", spot.detail),
        );
    }
    for spot in &hygiene.distant_declarations {
        emit(
            out,
            RuleId::DistantDeclaration,
            spot,
            Evidence::DistantDeclaration {
                name: spot.detail.clone(),
                lines: spot.count,
            },
            format!("move `{}` down to where it is first used", spot.detail),
        );
    }
    for spot in &hygiene.obscured_expressions {
        emit(
            out,
            RuleId::ObscuredExpression,
            spot,
            Evidence::ObscuredExpression {
                operations: spot.count,
                limit: builder.config.thresholds.expression_operations,
            },
            "lift the steps into named locals, so the expression reads as its own summary"
                .to_owned(),
        );
    }
    for spot in &hygiene.buried_configuration {
        emit(
            out,
            RuleId::BuriedConfiguration,
            spot,
            Evidence::BuriedConfiguration {
                kind: spot.detail.clone(),
                depth: spot.count,
            },
            "lift it to a named constant at the top of the module, or into config".to_owned(),
        );
    }
    for spot in &hygiene.inconsistent_vocabulary {
        emit(
            out,
            RuleId::InconsistentVocabulary,
            spot,
            Evidence::InconsistentVocabulary {
                pair: spot.detail.clone(),
                sites: spot.count,
            },
            "pick one verb for the idea and rename the rest to match".to_owned(),
        );
    }
    for spot in &hygiene.discarded_expressions {
        emit(
            out,
            RuleId::DiscardedExpression,
            spot,
            Evidence::BranchShape {
                shape: format!("`{}` is computed and dropped", spot.detail),
            },
            "use the value, or delete the line".to_owned(),
        );
    }
    for spot in &hygiene.float_equalities {
        emit(
            out,
            RuleId::FloatEquality,
            spot,
            Evidence::BranchShape {
                shape: format!("`{}` is compared for exact equality", spot.detail),
            },
            "compare the difference against a tolerance the domain decides".to_owned(),
        );
    }
    for spot in &hygiene.redundant_jumps {
        emit(
            out,
            RuleId::RedundantJump,
            spot,
            Evidence::BranchShape {
                shape: format!("`{}` is the last statement of the loop", spot.detail),
            },
            "delete it; control was going there anyway".to_owned(),
        );
    }
    for spot in &hygiene.nested_ternaries {
        emit(
            out,
            RuleId::NestedTernary,
            spot,
            Evidence::BranchShape {
                shape: format!("`{}` is the inner half of another conditional", spot.detail),
            },
            "give the inner condition a named value, or write the whole thing as \
             statements"
                .to_owned(),
        );
    }
    for spot in &hygiene.ambiguous_precedence {
        emit(
            out,
            RuleId::AmbiguousPrecedence,
            spot,
            Evidence::BranchShape {
                shape: format!("`{}` with nothing to say which binds first", spot.detail),
            },
            "put the tighter half in parentheses, or name it".to_owned(),
        );
    }
    for spot in &hygiene.negated_comparisons {
        emit(
            out,
            RuleId::NegatedComparison,
            spot,
            Evidence::BranchShape {
                shape: format!("a negation wrapped around `{}`", spot.detail),
            },
            "invert the operator and drop the negation".to_owned(),
        );
    }
    for spot in &hygiene.defaultless_switches {
        emit(
            out,
            RuleId::NoDefaultBranch,
            spot,
            Evidence::Breadth {
                counted: "arms, none of them the last resort".to_owned(),
                found: spot.count,
                limit: spot.count,
            },
            "add the arm that says what an unnamed value means, even if it only \
             raises"
                .to_owned(),
        );
    }
    for spot in &hygiene.single_pass_loops {
        emit(
            out,
            RuleId::LoopRunsOnce,
            spot,
            Evidence::BranchShape {
                shape: format!("the body always `{}` before the next pass", spot.detail),
            },
            "drop the loop and take the first match, or move the exit behind the \
             condition that should end it"
                .to_owned(),
        );
    }
    for spot in &hygiene.tiny_switches {
        emit(
            out,
            RuleId::TinySwitch,
            spot,
            Evidence::Breadth {
                counted: "arms".to_owned(),
                found: spot.count,
                limit: 0,
            },
            "write it as an if, or give the switch the arms that justify it".to_owned(),
        );
    }
    for spot in &hygiene.empty_methods {
        emit(
            out,
            RuleId::EmptyMethod,
            spot,
            Evidence::Name {
                identifier: spot.detail.clone(),
                reason: "has an empty body".to_owned(),
            },
            "give it a body, delete it, or make it explicit that doing nothing is the \
             answer here"
                .to_owned(),
        );
    }
    for (spots, rule, hint) in [
        (
            &hygiene.banner_comments,
            RuleId::BannerComment,
            "delete it; if the file needs sections, it needs splitting",
        ),
        (
            &hygiene.chat_residue,
            RuleId::ChatResidue,
            "say what the code does that its name does not, or delete the comment",
        ),
        (
            &hygiene.apologies,
            RuleId::ApologyComment,
            "fix it, or record why it stays and what would let it go",
        ),
        (&hygiene.emoji, RuleId::EmojiInSource, "write it in words"),
        (
            &hygiene.example_values,
            RuleId::ExampleValue,
            "name it as configuration, or move it to a test where a stand-in belongs",
        ),
    ] {
        for spot in spots {
            emit(
                out,
                rule,
                spot,
                Evidence::Noise {
                    marker: spot.detail.clone(),
                },
                hint.to_owned(),
            );
        }
    }
    for waiver in &hygiene.waivers {
        let Some(refusal) = waiver.refusal(&crate::suppress::today()) else {
            continue;
        };
        emit(
            out,
            RuleId::UnjustifiedSuppression,
            &Spot {
                span: waiver.span,
                detail: refusal.clone(),
                count: 1,
            },
            Evidence::Suppression { directive: refusal },
            "say why the finding is acceptable here, or fix it".to_owned(),
        );
    }
    for spot in &hygiene.suppressions {
        emit(
            out,
            RuleId::SuppressedDiagnostic,
            spot,
            Evidence::Suppression {
                directive: spot.detail.clone(),
            },
            "fix what the check is reporting, or record why suppressing it is right".to_owned(),
        );
    }
    for spot in &hygiene.flag_parameters {
        emit(
            out,
            RuleId::FlagArgument,
            spot,
            Evidence::BooleanParameter {
                name: spot.detail.clone(),
                position: spot.count,
            },
            format!(
                "split into two functions named for what each does, so no call site reads \
                 `(…, true)` and leaves `{}` a mystery",
                spot.detail
            ),
        );
    }
    for spot in &hygiene.mutated_parameters {
        emit(
            out,
            RuleId::OutputParameter,
            spot,
            Evidence::MutatedParameter {
                name: spot.detail.clone(),
            },
            format!(
                "assign to a local instead of `{}`, and return the result",
                spot.detail
            ),
        );
    }
    for spot in &hygiene.complex_conditions {
        emit(
            out,
            RuleId::ComplexCondition,
            spot,
            Evidence::ConditionOperands {
                operands: spot.count,
                limit: builder.config.thresholds.condition_operands,
            },
            "name the condition in a variable or a predicate that says what it means".to_owned(),
        );
    }
    for spot in &hygiene.negative_conditionals {
        emit(
            out,
            RuleId::NegativeConditional,
            spot,
            Evidence::NegatedCondition,
            "invert the condition and swap the branches so the common case reads first".to_owned(),
        );
    }
    for spot in &hygiene.message_chains {
        emit(
            out,
            RuleId::MessageChain,
            spot,
            Evidence::MessageChain {
                depth: spot.count,
                limit: builder.config.thresholds.message_chain_depth,
            },
            "ask the nearest object for what you need and let it do the reaching".to_owned(),
        );
    }
    for spot in &hygiene.skipped_tests {
        emit(
            out,
            RuleId::SkippedTest,
            spot,
            Evidence::TestDirective {
                directive: spot.detail.clone(),
            },
            "fix it and re-enable it, or delete it and record the question it was asking"
                .to_owned(),
        );
    }
    for spot in &hygiene.focused_tests {
        emit(
            out,
            RuleId::FocusedTest,
            spot,
            Evidence::TestDirective {
                directive: spot.detail.clone(),
            },
            "remove the focus before this lands; it is almost never meant to be committed"
                .to_owned(),
        );
    }
    for spot in &hygiene.short_names {
        emit(
            out,
            RuleId::ShortNameWideScope,
            spot,
            Evidence::Name {
                identifier: spot.detail.clone(),
                reason: format!("lives across {} lines", spot.count),
            },
            format!(
                "give `{}` a name that survives the distance to its last use",
                spot.detail
            ),
        );
    }
    for spot in &hygiene.assertionless_tests {
        emit(
            out,
            RuleId::AssertionlessTest,
            spot,
            Evidence::NoAssertion {
                name: spot.detail.clone(),
            },
            "assert the behaviour it names, or delete it so the suite stops claiming coverage"
                .to_owned(),
        );
    }
}

pub fn check_class_shape(
    report: &crate::facts::ClassReport,
    builder: &FindingBuilder<'_>,
    out: &mut Vec<Finding>,
) {
    let name = &report.name;
    let members = report.fields + report.methods;

    if report.public_members > builder.config.thresholds.class_public_members {
        builder.emit(
            out,
            RuleId::WideSurface,
            report.span,
            Evidence::Breadth {
                counted: "members exposed".to_owned(),
                found: report.public_members,
                limit: builder.config.thresholds.class_public_members,
            },
            format!(
                "narrow what `{name}` promises: an interface for what callers use, and \
                 the rest kept inside"
            ),
        );
    }

    for guarded in &report.guarded_fields {
        builder.emit(
            out,
            RuleId::TemporalCoupling,
            guarded.span,
            Evidence::TemporalCoupling {
                field: guarded.name.clone(),
                methods: guarded.other.clone(),
            },
            format!(
                "set `{}` in the constructor, or split the stages into objects that \
                 cannot be used before they are ready",
                guarded.name
            ),
        );
    }

    if let Some(misleading) = &report.misleading_name {
        let promise = if misleading.name.ends_with("Builder") {
            "builder"
        } else {
            "factory"
        };
        builder.emit(
            out,
            RuleId::MisleadingPatternName,
            misleading.span,
            Evidence::MisleadingName {
                name: misleading.name.clone(),
                promise: promise.to_owned(),
            },
            format!(
                "rename `{}` after what it holds, or give it the members a {promise} has",
                misleading.name
            ),
        );
    }

    for refused in &report.refused {
        builder.emit(
            out,
            RuleId::RefusedBequest,
            refused.span,
            Evidence::RefusedOverride {
                name: refused.name.clone(),
            },
            format!(
                "if `{name}` cannot honour `{}`, it is not a subtype — split the interface \
                 so it only promises what it can do",
                refused.name
            ),
        );
    }
    for dead in &report.dead_private {
        builder.emit(
            out,
            RuleId::UnusedPrivateMember,
            dead.span,
            Evidence::UnusedBinding {
                name: dead.name.clone(),
                kind: "private member".to_owned(),
            },
            format!(
                "delete `{}`; nothing inside `{name}` can reach it",
                dead.name
            ),
        );
    }
    for field in &report.temporary_fields {
        builder.emit(
            out,
            RuleId::TemporaryField,
            field.span,
            Evidence::FieldLifetime {
                name: field.name.clone(),
                method: field.other.clone(),
            },
            format!(
                "make `{}` a local in `{}`, or move both into a class that owns that step",
                field.name, field.other
            ),
        );
    }
    for statik in &report.static_on_own_type {
        builder.emit(
            out,
            RuleId::InappropriateStatic,
            statik.span,
            Evidence::StaticOnOwnType {
                name: statik.name.clone(),
            },
            format!("make `{}` an instance method on `{name}`", statik.name),
        );
    }
    for envy in &report.envious {
        builder.emit(
            out,
            RuleId::FeatureEnvy,
            envy.span,
            Evidence::ForeignAccess {
                owner: envy.owner.clone(),
                foreign: envy.foreign,
                own: envy.own,
            },
            format!(
                "move `{}` onto `{}`, where the data it wants already lives",
                envy.method, envy.owner
            ),
        );
    }

    // Shape rules are mutually exclusive by construction: a class is at most one of
    // lazy, data-only or a middle man, and reporting two of them about the same class
    // would be two names for one observation.
    // Lazy means the class holds nothing worth a file — one substantial method is
    // still a reason to exist, so triviality decides this, not member count alone.
    if members <= 1 && !report.has_heritage && report.trivial >= report.real_methods {
        builder.emit(
            out,
            RuleId::LazyClass,
            report.span,
            Evidence::ClassShape {
                kind: "Does almost nothing".to_owned(),
                fields: report.fields,
                methods: report.methods,
                trivial: report.trivial,
            },
            format!("fold `{name}` into its only caller until it earns its own file"),
        );
    } else if report.real_methods > 0 && report.delegating * 2 > report.real_methods {
        builder.emit(
            out,
            RuleId::MiddleMan,
            report.span,
            Evidence::Delegation {
                delegating: report.delegating,
                total: report.real_methods,
            },
            format!(
                "let callers talk to the delegate directly and delete the pass-throughs in `{name}`"
            ),
        );
    } else if report.fields > 0 && report.real_methods == report.trivial && members > 1 {
        builder.emit(
            out,
            RuleId::DataClass,
            report.span,
            Evidence::ClassShape {
                kind: "Holds data but no behaviour".to_owned(),
                fields: report.fields,
                methods: report.methods,
                trivial: report.trivial,
            },
            format!("move the logic that operates on `{name}`'s fields onto `{name}`"),
        );
    }
}

/// Compares classes against each other rather than against a threshold, so it takes
/// the whole file's worth at once.
pub fn check_class_pairs(
    reports: &[crate::facts::ClassReport],
    builder: &FindingBuilder<'_>,
    out: &mut Vec<Finding>,
) {
    const MIN_METHODS: usize = 3;

    for (index, left) in reports.iter().enumerate() {
        if left.method_names.len() < MIN_METHODS {
            continue;
        }
        for right in &reports[index + 1..] {
            if left.arities != right.arities
                || left.method_names.len() != right.method_names.len()
                || left
                    .method_names
                    .iter()
                    .any(|name| right.method_names.contains(name))
            {
                continue;
            }
            builder.emit(
                out,
                RuleId::AlternativeClasses,
                left.span,
                Evidence::AlternativeClasses {
                    other: right.name.clone(),
                    methods: left.method_names.len() as u32,
                },
                format!(
                    "give `{}` and `{}` one interface, or one superclass, so a caller \
                     learns the job once",
                    left.name, right.name
                ),
            );
        }
    }
}

pub fn check_patterns(
    patterns: &crate::facts::Patterns,
    builder: &FindingBuilder<'_>,
    out: &mut Vec<Finding>,
) {
    for clump in &patterns.clumps {
        builder.emit(
            out,
            RuleId::DataClump,
            clump.span,
            Evidence::RepeatedGroup {
                names: clump.names.join(", "),
                sites: clump.sites,
            },
            format!(
                "give ({}) a name and pass that object instead",
                clump.names.join(", ")
            ),
        );
    }
    for discriminant in &patterns.discriminants {
        builder.emit(
            out,
            RuleId::TypeCodeSwitch,
            discriminant.span,
            Evidence::Discriminant {
                name: discriminant.name.clone(),
                sites: discriminant.sites,
                branches: discriminant.branches,
            },
            format!(
                "give `{}` a type per case and let each one carry its own behaviour, so \
                 adding a case is one new class rather than {} edits",
                discriminant.name, discriminant.sites
            ),
        );
    }
    for pair in &patterns.intimate_pairs {
        builder.emit(
            out,
            RuleId::InappropriateIntimacy,
            pair.span,
            Evidence::IntimatePair {
                pair: pair.name.clone(),
            },
            "move what they share into one of them, or behind an interface both use".to_owned(),
        );
    }
    for reference in &patterns.derived_references {
        builder.emit(
            out,
            RuleId::BaseDependsOnDerived,
            reference.span,
            Evidence::DerivedReference {
                name: reference.name.clone(),
            },
            format!(
                "move what the base needs from `{}` behind a method the subclass \
                 implements, so the base stops naming its own children",
                reference.name
            ),
        );
    }
    for name in &patterns.encoded_names {
        builder.emit(
            out,
            RuleId::HungarianNotation,
            name.span,
            Evidence::Name {
                identifier: name.name.clone(),
                reason: "encodes its type in its name".to_owned(),
            },
            "drop the prefix; the declaration already states the type and the compiler \
             checks it"
                .to_owned(),
        );
    }
    for code in &patterns.type_codes {
        builder.emit(
            out,
            RuleId::PrimitiveObsession,
            code.span,
            Evidence::Name {
                identifier: code.name.clone(),
                reason: "is a union of string literals standing in for a type".to_owned(),
            },
            format!(
                "give `{}` a type that owns its rules, so validation lives with the data \
                 instead of at every use",
                code.name
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::climb;

    /// Two spellings of the same thing. A path climbs with `..` segments, and Python
    /// climbs with dots where the first one only means "this package".
    #[test]
    fn a_relative_import_climbs_the_same_amount_in_either_spelling() {
        assert_eq!(climb("../../shared/log"), 2);
        assert_eq!(climb("...shared.log"), 2);
        assert_eq!(climb("../../../shared/log.hpp"), 3);
        assert_eq!(climb("....shared.log"), 3);
    }

    /// A sibling, the current package and a package name all reach across nothing.
    #[test]
    fn nothing_that_stays_put_counts_as_a_climb() {
        assert_eq!(climb("./sibling"), 0);
        assert_eq!(climb(".sibling"), 0);
        assert_eq!(climb("lodash"), 0);
        assert_eq!(climb("@scope/package"), 0);
        assert_eq!(climb("github.com/stripe/stripe-go"), 0);
        // A file whose name begins with a dot is not a climb either.
        assert_eq!(climb("./.eslintrc"), 0);
    }
}

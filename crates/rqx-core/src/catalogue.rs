// The rule catalogue. Every entry names the metric that decides it and the
// refactoring that fixes it.

catalogue! {
    // ---- Size -------------------------------------------------------------
    LongFunction {
        slug: "long-function",
        category: Size,
        analysis: Syntax,
        metric: "statement count in a function body",
        fixes: [ExtractMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "a body this long is doing several things, so a reader has to hold all of \
              them at once to change any one of them",
    },
    CognitiveComplexity {
        slug: "cognitive-complexity",
        category: Size,
        analysis: Syntax,
        metric: "SonarSource cognitive complexity",
        fixes: [DecomposeConditional, ExtractMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "every nested decision point multiplies the paths a reader has to trace, \
              and untraced paths are where regressions survive review",
    },
    DeepNesting {
        slug: "deep-nesting",
        category: Size,
        analysis: Syntax,
        metric: "maximum block nesting depth",
        fixes: [ReplaceNestedConditionalWithGuardClauses],
        confidence: Heuristic,
        severity: Warning,
        why: "at this depth the conditions that led here are off-screen, so the \
              preconditions of the innermost code are invisible where it matters",
    },
    LongParameterList {
        slug: "long-parameter-list",
        category: Size,
        analysis: Syntax,
        metric: "parameter count",
        fixes: [IntroduceParameterObject, PassWholeObject],
        confidence: Heuristic,
        severity: Warning,
        why: "callers have to remember an order this long, and same-typed neighbours \
              make a silent transposition possible at every call site",
    },
    LargeClass {
        slug: "large-class",
        category: Size,
        analysis: Syntax,
        metric: "member count and body statement count",
        fixes: [ExtractClass, ExtractSubclass],
        confidence: Heuristic,
        severity: Warning,
        why: "a class this size has more than one reason to change, so unrelated work \
              lands in the same file and the same merge conflicts",
    },
    DataClump {
        slug: "data-clump",
        category: Size,
        analysis: Semantics,
        metric: "the same group of three or more parameter names recurring across signatures",
        fixes: [IntroduceParameterObject, ExtractClass],
        confidence: Heuristic,
        severity: Info,
        why: "these values already travel together everywhere, so they are a concept \
              the code has not named yet, and every new use has to rediscover it",
    },
    PrimitiveObsession {
        slug: "primitive-obsession",
        category: Size,
        analysis: Semantics,
        metric: "a type alias that is a union of three or more string literals",
        fixes: [ReplaceTypeCodeWithClass, IntroduceParameterObject],
        confidence: Low,
        severity: Info,
        why: "a raw string or number carries no rules of its own, so every place that \
              uses it re-implements the same validation, and one of them gets it wrong",
        off_by_default: true,
        not_applicable: [
            (Go, "Go has no union types, so the string-literal alias this measures cannot be written"),
            (Java, "Java has no union types, so the string-literal alias this measures cannot be written"),
            (Cpp, "C++ has no union types, so the string-literal alias this measures cannot be written"),
        ],
    },

    // ---- Abstraction -------------------------------------------
    TypeCodeSwitch {
        slug: "type-code-switch",
        category: Abstraction,
        analysis: Semantics,
        metric: "switch or if/else chain branching on the same discriminant in more than one place",
        fixes: [
            ReplaceConditionalWithPolymorphism,
            ReplaceTypeCodeWithSubclasses,
            ReplaceTypeCodeWithPolymorphicState,
        ],
        confidence: Heuristic,
        severity: Warning,
        why: "the same set of cases is now spelled out in several places, so adding a \
              case means finding all of them and the compiler will not remind you",
    },
    TemporaryField {
        slug: "temporary-field",
        category: Abstraction,
        analysis: Semantics,
        metric: "instance field written and read by exactly one method, never in the constructor",
        fixes: [ExtractClass, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "the field is empty for most of the object's life, so every other method \
              has to know it must not look at it",
    },
    RefusedBequest {
        slug: "refused-bequest",
        category: Abstraction,
        analysis: Semantics,
        metric: "override whose body only throws or is empty",
        fixes: [PushDownMethod, ExtractInterface, CollapseHierarchy],
        confidence: Verified,
        severity: Warning,
        why: "the subclass is inheriting an interface it cannot honour, so a caller \
              holding the base type has no safe way to know which methods actually work",
        not_applicable: [
            (Go, "nothing is inherited in Go, so nothing can be refused: a type either has a method or does not"),
        ],
    },
    BaseDependsOnDerived {
        slug: "base-depends-on-derived",
        category: Abstraction,
        analysis: Semantics,
        metric: "base class referencing one of its own subclasses by name",
        fixes: [InvertDependency, PushDownMethod],
        confidence: Verified,
        severity: Warning,
        why: "the base cannot be deployed, tested or understood without its children, \
              which is the entire benefit of having a base class given away",
        not_applicable: [
            (Go, "Go has no inheritance: embedding is a field, so no type is a base another derives from"),
        ],
    },
    InappropriateStatic {
        slug: "inappropriate-static",
        category: Abstraction,
        analysis: Semantics,
        metric: "static method taking an instance of its own class as its first parameter",
        fixes: [MoveMethod],
        confidence: Heuristic,
        severity: Info,
        why: "a static can never be overridden, so behaviour that clearly belongs to the \
              instance is frozen for every subclass and every test that wants to fake it",
        not_applicable: [
            (Go, "Go has no methods on the type itself, only on values of it, so the static-on-own-type shape cannot be written"),
        ],
    },

    // ---- Change ----------------------------------------------------
    ShotgunSurgery {
        slug: "shotgun-surgery",
        category: Change,
        analysis: Syntax,
        metric: "files that git history shows almost always change together",
        fixes: [ExtractClass, MoveMethod],
        confidence: Heuristic,
        severity: Info,
        why: "one decision is spread across files that must be edited in lockstep, and \
              nothing in the code says so, so the day someone edits four of the five is \
              the day it breaks",
    },
    DivergentChange {
        slug: "divergent-change",
        category: Change,
        analysis: Syntax,
        metric: "a file with many commits and no single frequent co-change partner",
        fixes: [ExtractClass],
        confidence: Low,
        severity: Info,
        why: "this file is edited for many unrelated reasons, which is the observable \
              signature of more than one responsibility living in it",
        off_by_default: true,
    },

    ParallelInheritance {
        slug: "parallel-inheritance",
        category: Change,
        analysis: Modules,
        metric: "two class hierarchies whose subclasses are named for the same set of things",
        fixes: [MoveMethod, MoveField, CollapseHierarchy],
        confidence: Low,
        severity: Info,
        why: "every new case means a new subclass in both trees, and the day someone \
              adds only one the two hierarchies quietly stop lining up",
        off_by_default: true,
        not_applicable: [
            (Go, "hierarchies need inheritance, and Go has none to build one from"),
        ],
    },

    // ---- Waste ---------------------------------------------------------
    DuplicateCode {
        slug: "duplicate-code",
        category: Waste,
        analysis: Syntax,
        metric: "identical normalized-AST block hash across sites",
        fixes: [ExtractMethod, PullUpMethod],
        confidence: Verified,
        severity: Warning,
        why: "a fix applied to one copy leaves the others wrong, and nothing in the code \
              points from the copy you edited to the ones you did not",
    },
    UnusedExport {
        slug: "unused-export",
        category: Waste,
        analysis: Modules,
        metric: "exported symbol with no importer in the resolved module graph",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "an export nobody imports still has to be read, typechecked and kept working \
              during every refactor that touches it",
        autofix: Deletion,
    },
    UnusedFile {
        slug: "unused-file",
        category: Waste,
        analysis: Modules,
        metric: "module reachable from no entry point, with no importer",
        fixes: [DeleteFile],
        confidence: Verified,
        severity: Warning,
        why: "nothing in the build reaches this file, so every hour spent maintaining it \
              buys nothing",
        autofix: Deletion,
    },
    UnusedParameter {
        slug: "unused-parameter",
        category: Waste,
        analysis: Semantics,
        metric: "declared parameter with no resolved reference in its own scope",
        fixes: [RemoveParameter],
        confidence: Verified,
        severity: Warning,
        why: "every caller is passing a value that is read nowhere, so the argument list \
              is teaching readers something untrue about the function",
        autofix: Rewrite,
    },
    UnusedLocal {
        slug: "unused-local",
        category: Waste,
        analysis: Semantics,
        metric: "local binding with no resolved reference",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "the value is computed and thrown away, which either wastes the work or \
              hides the fact that somebody meant to use it",
        autofix: Deletion,
    },
    UnusedPrivateMember {
        slug: "unused-private-member",
        category: Waste,
        analysis: Semantics,
        metric: "private class member never referenced inside its own class",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "a private member can only be used from inside its class, so one nothing in \
              the class touches is unreachable by construction",
        autofix: Deletion,
        not_applicable: [
            (Go, "Go scopes visibility to the package, not the type, so a field unused in one file may be read by another beside it"),
        ],
    },
    UnreachableCode {
        slug: "unreachable-code",
        category: Waste,
        analysis: Semantics,
        metric: "statement following an unconditional return, throw, break or continue",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Error,
        why: "this code cannot run, so whatever it was meant to do is not happening and \
              has not been happening",
        autofix: Deletion,
    },
    CommentedOutCode {
        slug: "commented-out-code",
        category: Waste,
        analysis: Semantics,
        metric: "consecutive comment lines that read as statements rather than prose",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Warning,
        why: "nobody dares delete it because nobody knows if it matters, so it sits there \
              rotting while version control already remembers it perfectly",
        autofix: Deletion,
    },
    LazyClass {
        slug: "lazy-class",
        category: Waste,
        analysis: Semantics,
        metric: "class with at most one member and no heritage",
        fixes: [InlineClass],
        confidence: Heuristic,
        severity: Info,
        why: "the class costs a file, an import and a name, and does not yet do enough to \
              earn any of them",
    },
    DataClass {
        slug: "data-class",
        category: Waste,
        analysis: Semantics,
        metric: "class whose members are all fields or trivial accessors",
        fixes: [MoveMethod, EncapsulateField],
        confidence: Heuristic,
        severity: Info,
        why: "the behaviour that belongs with this data lives somewhere else, so the \
              rules about the data are enforced everywhere except where the data is",
    },
    SpeculativeGenerality {
        slug: "speculative-generality",
        category: Waste,
        analysis: Modules,
        metric: "abstract class with no subclass anywhere in the project",
        fixes: [CollapseHierarchy, InlineClass],
        confidence: Verified,
        severity: Info,
        why: "it was built for a case that never arrived, and until it does it is only an \
              extra indirection every reader has to step through. Interfaces are not \
              reported: TypeScript's are structural, so nothing implementing one says \
              nothing about whether it is speculative",
    },
    EmptyCatch {
        slug: "empty-catch",
        category: Waste,
        analysis: Semantics,
        metric: "catch clause with an empty body",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Error,
        why: "the failure is swallowed silently, so the first sign of it is wrong data \
              somewhere far away with nothing in the logs to connect the two",
        not_applicable: [
            (Go, "Go has no exceptions and so no catch clause; an ignored error is a different shape this rule does not measure"),
        ],
    },
    MagicNumber {
        slug: "magic-number",
        category: Waste,
        analysis: Semantics,
        metric: "unnamed numeric literal outside a declaration, excluding conventional values",
        fixes: [ReplaceMagicNumberWithConstant, ExtractVariable],
        confidence: Heuristic,
        severity: Info,
        why: "the number encodes a decision nobody wrote down, so the next reader cannot \
              tell whether changing it is safe or what else shares it",
    },

    ImpreciseType {
        slug: "imprecise-type",
        category: Safety,
        analysis: Semantics,
        metric: "an `any` annotation, a non-null assertion, or a cast that is not `as const`",
        fixes: [ReplaceDataValueWithObject, IntroduceAssertion],
        confidence: Verified,
        severity: Warning,
        why: "each of these tells the checker to stop checking, so the guarantee the \
              rest of the file is written against quietly stops holding here",
    },

    MixedLanguageFile {
        slug: "mixed-language-file",
        category: Waste,
        analysis: Semantics,
        metric: "SQL or HTML markup inside a template literal",
        fixes: [ExtractClass, ExtractVariable],
        confidence: Heuristic,
        severity: Info,
        why: "a second language in the file gets none of the tooling the first one \
              has, so nothing checks it until it fails at run time",
    },
    BoundaryArithmetic {
        slug: "boundary-arithmetic",
        category: Safety,
        analysis: Semantics,
        metric: "the same subject adjusted by one at more sites than the threshold",
        fixes: [ExtractVariable, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "every place the boundary is recomputed is a place it can be recomputed \
              wrongly, and off-by-one errors are found by readers rather than types",
    },
    MixedConventions {
        slug: "mixed-conventions",
        category: Naming,
        analysis: Semantics,
        metric: "camelCase and snake_case names declared in one file",
        fixes: [RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "a reader who has to guess which convention a name follows cannot skim, \
              and the mixture spreads every time somebody copies a neighbouring line",
        autofix: Rewrite,
    },
    SlowTest {
        slug: "slow-test",
        category: Tests,
        analysis: Semantics,
        metric: "a sleep or timeout inside a test body above the configured pause",
        fixes: [ReplaceAlgorithm],
        confidence: Verified,
        severity: Warning,
        why: "a suite slow enough to skip stops being run, and a test nobody runs is \
              worth less than the time it took to write",
    },

    DistantDeclaration {
        slug: "distant-declaration",
        category: Waste,
        analysis: Semantics,
        metric: "lines between a local's declaration and the line that first reads it",
        fixes: [ExtractVariable, InlineVariable],
        confidence: Heuristic,
        severity: Info,
        why: "everything in between has to be read while still holding this name in \
              mind, and the reader has no way to know it will matter",
    },

    ObscuredExpression {
        slug: "obscured-expression",
        category: Size,
        analysis: Semantics,
        metric: "operations packed into one returned expression beyond the limit",
        fixes: [ExtractVariable, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "nothing in the expression is named, so a reader has to re-derive every \
              intermediate step the author already worked out",
    },
    BuriedConfiguration {
        slug: "buried-configuration",
        category: Waste,
        analysis: Semantics,
        metric: "a URL or absolute path written below the configured block depth",
        fixes: [ReplaceMagicNumberWithConstant, ExtractVariable],
        confidence: Heuristic,
        severity: Info,
        why: "changing where the program points means finding this line first, and \
              nothing about its position suggests it is a setting",
    },

    TemporalCoupling {
        slug: "temporal-coupling",
        category: Abstraction,
        analysis: Semantics,
        metric: "a field no constructor sets that two or more methods test before acting",
        fixes: [IntroduceAssertion, ExtractClass],
        confidence: Heuristic,
        severity: Warning,
        why: "the order the methods must be called in is real but unwritten, so the \
              only way to learn it is to call them wrongly first",
    },

    // ---- Coupling -------------------------------------------------------------
    CircularDependency {
        slug: "circular-dependency",
        category: Coupling,
        analysis: Modules,
        metric: "strongly connected component of more than one module in the import graph",
        fixes: [ExtractSharedModule, InvertDependency],
        confidence: Verified,
        severity: Warning,
        why: "no module in the cycle can be understood, tested or loaded on its own, and \
              the initialisation order that makes it work today is accidental",
    },
    LayerViolation {
        slug: "layer-violation",
        category: Coupling,
        analysis: Modules,
        metric: "an import pointing from an inner layer to an outer one",
        fixes: [InvertDependency],
        confidence: Verified,
        severity: Error,
        why: "the layer the project put underneath now depends on the one above it, so \
              the thing it was separated from cannot be replaced without changing it",
    },
    ForbiddenImport {
        slug: "forbidden-import",
        category: Coupling,
        analysis: Modules,
        metric: "an import the project's own rules say this subtree may not make",
        fixes: [InvertDependency],
        confidence: Verified,
        severity: Error,
        why: "the boundary was written down and this import crosses it, which is the \
              one kind of architectural drift a reader cannot infer from the code",
    },
    MessageChain {
        slug: "message-chain",
        category: Coupling,
        analysis: Semantics,
        metric: "chained member access depth beyond the configured limit",
        fixes: [EncapsulateDelegation, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "this line depends on the shape of every object along the chain, so a change \
              to any one of them breaks a caller that never mentioned it",
    },
    FeatureEnvy {
        slug: "feature-envy",
        category: Coupling,
        analysis: Semantics,
        metric: "method touching another object's members more than its own",
        fixes: [MoveMethod, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "the method wants to live on the other class, and while it does not, both \
              classes have to stay in step for it to keep working",
    },
    MiddleMan {
        slug: "middle-man",
        category: Coupling,
        analysis: Semantics,
        metric: "class where most methods do nothing but delegate",
        fixes: [InlineDelegation, InlineClass],
        confidence: Heuristic,
        severity: Info,
        why: "every call pays for a hop that adds nothing, and every new method on the \
              delegate needs a twin here before anyone can reach it",
    },

    JournalComment {
        slug: "journal-comment",
        category: Waste,
        analysis: Semantics,
        metric: "a comment carrying an author, a date or a change log",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "version control already answers who and when, and unlike the comment it \
              is not left behind by the next edit",
        autofix: Deletion,
    },
    RedundantComment {
        slug: "redundant-comment",
        category: Waste,
        analysis: Semantics,
        metric: "a comment whose every word already appears in the name below it",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "it takes space and attention to say what the name has already said, and \
              it is one more thing to keep true when the name changes",
        autofix: Deletion,
    },

    VagueFunctionName {
        slug: "vague-function-name",
        category: Naming,
        analysis: Semantics,
        metric: "a function named with a bare verb and no object",
        fixes: [RenameMethod],
        confidence: Heuristic,
        severity: Info,
        why: "the name has to be read past to learn anything, so every call site \
              forces a trip to the definition",
    },
    QueryWithSideEffect {
        slug: "query-with-side-effect",
        category: Safety,
        analysis: Semantics,
        metric: "a get, is, has, should or can function that assigns to a member",
        fixes: [SeparateQueryFromModifier, RenameMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "callers assume asking is free, so they ask twice, order the calls \
              freely, and are surprised when that changes the answer",
    },

    MisleadingPatternName {
        slug: "misleading-pattern-name",
        category: Naming,
        analysis: Semantics,
        metric: "a class named Factory or Builder with no method that makes or builds",
        fixes: [RenameMethod, ExtractClass],
        confidence: Heuristic,
        severity: Info,
        why: "a pattern name is a promise about the interface, and a reader who trusts \
              it goes looking for members that are not there",
        off_by_default: true,
    },

    InconsistentVocabulary {
        slug: "inconsistent-vocabulary",
        category: Naming,
        analysis: Semantics,
        metric: "two interchangeable verbs used for the same object in one file",
        fixes: [RenameMethod],
        confidence: Heuristic,
        severity: Info,
        why: "a reader who has met `fetchUser` cannot tell whether `getUser` is the \
              same thing or a second one, so both have to be read before either is used",
        off_by_default: true,
    },

    InappropriateIntimacy {
        slug: "inappropriate-intimacy",
        category: Coupling,
        analysis: Semantics,
        metric: "two classes in one file that each reference the other by name",
        fixes: [MoveMethod, ExtractClass, EncapsulateDelegation],
        confidence: Heuristic,
        severity: Warning,
        why: "neither class can be read, tested or moved without the other coming \
              with it, so the pair behaves as one class written in two places",
    },
    AlternativeClasses {
        slug: "alternative-classes",
        category: Abstraction,
        analysis: Semantics,
        metric: "two classes with the same method count and arities and no shared name",
        fixes: [RenameMethod, ExtractSuperclass, MoveMethod],
        confidence: Low,
        severity: Info,
        why: "callers have to learn both interfaces to do one job, and a fix applied \
              to one of them silently leaves the other wrong",
        off_by_default: true,
    },

    // ---- Flow -------------------------------------------------------------
    RedundantElse {
        slug: "redundant-else",
        category: Flow,
        analysis: Syntax,
        metric: "an `else` following a branch whose every path already exits",
        fixes: [ReplaceNestedConditionalWithGuardClauses],
        confidence: Verified,
        severity: Warning,
        why: "the branch already left, so everything after it is indented one level \
              deeper than the logic actually is",
    },
    CollapsibleIf {
        slug: "collapsible-if",
        category: Flow,
        analysis: Syntax,
        metric: "an `if` that is the only statement inside another, neither with an else",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Warning,
        why: "it is one condition written as two, so a reader holds a branch that \
              never branches",
    },
    BooleanFromBranch {
        slug: "boolean-from-branch",
        category: Flow,
        analysis: Syntax,
        metric: "a branch whose arms return opposite boolean literals",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Warning,
        why: "the condition is already the answer, and branching on it to restate it \
              is four lines saying what one says",
    },
    LiteralComparison {
        slug: "literal-comparison",
        category: Flow,
        analysis: Syntax,
        metric: "an equality comparison against a boolean literal",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Info,
        why: "comparing a boolean to a boolean asks a question that was already \
              answered, and reads as though the value might be something else",
    },
    SelfComparison {
        slug: "self-comparison",
        category: Flow,
        analysis: Syntax,
        metric: "an equality comparison whose operands are written identically",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Error,
        why: "the answer is decided before it runs, so either the comparison does \
              nothing or one side was meant to be something else",
    },
    IdenticalBranches {
        slug: "identical-branches",
        category: Flow,
        analysis: Syntax,
        metric: "two arms of a branch whose bodies are written identically",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Error,
        why: "the decision changes nothing, so either it is not a decision or one arm \
              was meant to differ and does not",
    },
    EmptyBlock {
        slug: "empty-block",
        category: Flow,
        analysis: Syntax,
        metric: "a branch or loop body with no statements in it",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "a decision was written and then nothing was done about it, which reads \
              as unfinished work or as a condition somebody inverted by accident",
        not_applicable: [(
            Python,
            "Python has no empty block: a body needs `pass` or an ellipsis, which is a \
             statement somebody wrote on purpose"
        )],
    },
    // ---- Naming ---------------------------------------------------------------
    MeaninglessName {
        slug: "meaningless-name",
        category: Naming,
        analysis: Syntax,
        metric: "identifier matched against a curated non-name set",
        fixes: [RenameVariable],
        confidence: Low,
        severity: Info,
        why: "a name that describes nothing forces every reader to re-derive the value's \
              purpose from its uses",
        off_by_default: true,
    },
    ShortNameWideScope {
        slug: "short-name-wide-scope",
        category: Naming,
        analysis: Semantics,
        metric: "binding of one or two characters living across many lines",
        fixes: [RenameVariable],
        confidence: Low,
        severity: Info,
        why: "a one-letter name is readable while its whole life fits on screen, and stops \
              being readable the moment it does not",
        off_by_default: true,
    },
    HungarianNotation {
        slug: "hungarian-notation",
        category: Naming,
        analysis: Semantics,
        metric: "identifier carrying a type prefix, or an interface named with a leading I",
        fixes: [RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "the type is already in the declaration and checked by the compiler, so the \
              prefix is a second copy that nothing keeps honest",
        off_by_default: true,
    },

    // ---- Safety ---------------------------------------------------------------
    SuppressedDiagnostic {
        slug: "suppressed-diagnostic",
        category: Safety,
        analysis: Semantics,
        metric: "ts-ignore, ts-nocheck, eslint-disable, or a cast through any",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "a check that would have caught the next mistake here has been turned off, \
              and the comment turning it off outlives the reason for it",
        autofix: Deletion,
    },
    FlagArgument {
        slug: "flag-argument",
        category: Safety,
        analysis: Semantics,
        metric: "parameter declared with a boolean type",
        fixes: [ReplaceParameterWithExplicitMethods],
        confidence: Heuristic,
        severity: Info,
        why: "the function does two things and the caller picks which, so `f(x, true)` \
              tells a reader nothing about what will happen",
    },
    OutputParameter {
        slug: "output-parameter",
        category: Safety,
        analysis: Semantics,
        metric: "parameter reassigned or mutated inside the function body",
        fixes: [RemoveAssignmentsToParameters, SeparateQueryFromModifier],
        confidence: Verified,
        severity: Warning,
        why: "the caller's value changes without the call site saying so, which is the \
              hardest kind of change to find when it turns out to be wrong",
    },
    ComplexCondition {
        slug: "complex-condition",
        category: Safety,
        analysis: Semantics,
        metric: "boolean operands in a single condition beyond the configured limit",
        fixes: [ConsolidateConditional, ExtractVariable],
        confidence: Heuristic,
        severity: Info,
        why: "nobody can hold this many clauses at once, so the reader takes the branch on \
              trust and the bug in it survives every review",
    },
    NegativeConditional {
        slug: "negative-conditional",
        category: Safety,
        analysis: Semantics,
        metric: "negated condition that has an else branch",
        fixes: [DecomposeConditional],
        confidence: Heuristic,
        severity: Info,
        why: "the reader has to invert it mentally to follow the else, and inverting a \
              compound condition by hand is where off-by-one logic errors come from",
        autofix: Rewrite,
        off_by_default: true,
    },

    // ---- Tests ----------------------------------------------------------------
    SkippedTest {
        slug: "skipped-test",
        category: Tests,
        analysis: Semantics,
        metric: "test declared with skip, xit, xdescribe or todo",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "a skipped test is a question nobody answered; it reports green forever while \
              the behaviour it named goes unchecked",
        autofix: Deletion,
    },
    FocusedTest {
        slug: "focused-test",
        category: Tests,
        analysis: Semantics,
        metric: "test declared with only, fit or fdescribe",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Error,
        why: "this silently disables every other test in the file, so the suite goes green \
              while almost nothing ran",
        autofix: Deletion,
        not_applicable: [
            (Python, "pytest and unittest focus a run with `-k` on the command line; \
                      there is nothing in the source to find"),
            (Go, "`go test` focuses a run with `-run` on the command line; there is nothing in the source to find"),
            (Java, "JUnit has no focus annotation; a run is narrowed by the build tool, not by the test"),
            (Cpp, "GoogleTest focuses a run with `--gtest_filter` on the command line; there is nothing in the source to find"),
        ],
    },
    AssertionlessTest {
        slug: "assertionless-test",
        category: Tests,
        analysis: Semantics,
        metric: "test body containing no assertion call",
        fixes: [IntroduceAssertion],
        confidence: Heuristic,
        severity: Warning,
        why: "it passes as long as nothing throws, so it reports success for behaviour it \
              never actually checked",
    },
}

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
    Hotspot {
        slug: "hotspot",
        category: Change,
        analysis: Syntax,
        metric: "a file that is edited often and is not clean",
        fixes: [ExtractClass, ExtractMethod],
        confidence: Verified,
        severity: Warning,
        why: "the cost of what is wrong here is paid on every change, and this file \
              changes more than any other, so it is where cleaning up buys the most",
    },
    SingleAuthorFile {
        slug: "single-author-file",
        category: Change,
        analysis: Syntax,
        metric: "a file with a history of its own and only one name in it",
        fixes: [ExtractMethod],
        confidence: Verified,
        severity: Info,
        why: "nobody else has read it closely enough to change it, so whatever it \
              knows leaves with the person who wrote it",
    },
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

    WideSurface {
        slug: "wide-surface",
        category: Abstraction,
        analysis: Semantics,
        metric: "count of members a type exposes outside itself",
        fixes: [ExtractInterface, ExtractClass],
        confidence: Heuristic,
        severity: Warning,
        why: "every public member is a promise to keep working, so the surface is what \
              the type costs to change rather than what it does",
    },
    ManyExits {
        slug: "many-exits",
        category: Size,
        analysis: Syntax,
        metric: "count of returns, throws and raises in one body, past the guard clauses",
        fixes: [ExtractMethod, DecomposeConditional],
        confidence: Heuristic,
        severity: Warning,
        why: "a reader has to find every way out before they can say what the function \
              answers, and the ones written deep in the body are the easy ones to miss",
    },
    WideSwitch {
        slug: "wide-switch",
        category: Size,
        analysis: Syntax,
        metric: "count of arms in one switch or match",
        fixes: [ReplaceConditionalWithPolymorphism, ExtractMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "past a certain width the arms stop being a table a reader scans and \
              become a list they have to search",
    },
    CallbackNesting {
        slug: "callback-nesting",
        category: Size,
        analysis: Syntax,
        metric: "depth of function literals declared inside other function literals",
        fixes: [ExtractMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "the order the steps run in is the opposite of the order they are written \
              in, so following the sequence means reading inside out",
    },
    AssignmentInCondition {
        slug: "assignment-in-condition",
        category: Safety,
        analysis: Syntax,
        metric: "an assignment written where a condition is expected",
        fixes: [ExtractVariable],
        confidence: Verified,
        severity: Error,
        why: "it reads as a comparison and behaves as a write, which is why it is the \
              typo that survives review",
        not_applicable: [
            (
                Python,
                "the only assignment Python allows in a condition is `:=`, which exists \
                 to be written there and is read as deliberate"
            ),
            (
                Go,
                "Go allows an initialiser before the condition rather than inside it, so \
                 the two are never confused"
            )
        ],
    },
    CrossLanguageDuplicate {
        slug: "cross-language-duplicate",
        category: Waste,
        analysis: Syntax,
        metric: "one block's control-flow shape and constants found in another language",
        fixes: [ExtractSharedModule],
        confidence: Low,
        severity: Info,
        why: "the same rule is written twice in two languages, so the day it changes it \
              changes in one of them and the two quietly disagree",
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
    ReturnOfFreshLocal {
        slug: "return-of-fresh-local",
        category: Waste,
        analysis: Syntax,
        metric: "a name introduced on one line and returned on the next",
        fixes: [InlineVariable],
        confidence: Verified,
        severity: Info,
        why: "the name is read once, by the line under it, and says nothing the \
              expression it holds did not already say",
    },
    DuplicateCondition {
        slug: "duplicate-condition",
        category: Flow,
        analysis: Syntax,
        metric: "the same question asked twice in one chain of branches",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Error,
        why: "the second arm cannot be reached, so whatever it was written to do has \
              never run and nobody has noticed",
    },
    LoopConcatenation {
        slug: "loop-concatenation",
        category: Performance,
        analysis: Syntax,
        metric: "text built one piece at a time inside a loop",
        fixes: [ReplaceAlgorithm],
        confidence: Heuristic,
        severity: Warning,
        why: "each pass copies everything written so far, so the work grows with the \
              square of the number of pieces",
    },
    AbstractNothing {
        slug: "abstract-nothing",
        category: Abstraction,
        analysis: Syntax,
        metric: "a type declared abstract with nothing left for anyone to fill in",
        fixes: [CollapseHierarchy, InlineClass],
        confidence: Verified,
        severity: Info,
        why: "the word says a subclass has work to do and there is none, so the only \
              thing it stops is somebody constructing it",
        not_applicable: [
            (Go, "there is no inheritance and no abstract keyword"),
            (Cpp, "abstractness is a member with no body, which the type never declares"),
            (Python, "abstractness is a base class it inherits, not a word on the type"),
        ],
    },
    SilentFallthrough {
        slug: "silent-fallthrough",
        category: Flow,
        analysis: Syntax,
        metric: "a switch arm that does work and then runs into the next one",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Error,
        why: "the arm below runs too, and nothing written here says whether that was \
              wanted or forgotten",
        not_applicable: [
            (Go, "an arm ends where the next begins unless it says otherwise"),
            (Python, "an arm ends where the next begins"),
        ],
    },
    NestedTernary {
        slug: "nested-ternary",
        category: Flow,
        analysis: Syntax,
        metric: "a conditional expression written inside another one",
        fixes: [DecomposeConditional, ExtractVariable],
        confidence: Verified,
        severity: Warning,
        why: "two conditions folded into one expression have to be unfolded in the \
              reader's head before either can be checked",
        not_applicable: [(Go, "Go has no conditional expression")],
    },
    AmbiguousPrecedence {
        slug: "ambiguous-precedence",
        category: Flow,
        analysis: Syntax,
        metric: "two different logical operators in one condition with no parentheses \
                 between them",
        fixes: [ExtractVariable, ConsolidateConditional],
        confidence: Verified,
        severity: Warning,
        why: "the language knows which one binds tighter and the reader has to \
              remember, so the parentheses cost nothing and settle it",
    },
    NegatedComparison {
        slug: "negated-comparison",
        category: Flow,
        analysis: Syntax,
        metric: "a negation wrapped around an equality test",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Info,
        why: "the language already has the operator this spells out the long way, and \
              the short one is read without inverting anything",
    },
    NoDefaultBranch {
        slug: "no-default-branch",
        category: Flow,
        analysis: Syntax,
        metric: "a switch with arms enough to be one and no arm for the values it \
                 does not name",
        fixes: [ConsolidateConditional, ReplaceConditionalWithPolymorphism],
        confidence: Heuristic,
        severity: Warning,
        why: "a value the switch never anticipated falls straight through it, and \
              nothing in the code says whether that was the intent",
    },
    LoopRunsOnce {
        slug: "loop-runs-once",
        category: Flow,
        analysis: Syntax,
        metric: "a loop whose body always leaves before a second iteration",
        fixes: [RemoveDeadCode, ReplaceNestedConditionalWithGuardClauses],
        confidence: Verified,
        severity: Warning,
        why: "it is written as repetition and runs once, so either the exit is \
              misplaced or the loop is a lookup wearing a loop's clothes",
    },
    RedundantJump {
        slug: "redundant-jump",
        category: Flow,
        analysis: Syntax,
        metric: "a continue or break as the last statement of a loop body",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "control was going there anyway, so the keyword makes a reader stop and \
              look for the condition that is not there",
        autofix: Deletion,
    },
    TinySwitch {
        slug: "tiny-switch",
        category: Flow,
        analysis: Syntax,
        metric: "arms in a switch, below the count that earns the shape",
        fixes: [ConsolidateConditional],
        confidence: Heuristic,
        severity: Info,
        why: "a switch is a table a reader scans, and one with two arms is an if \
              spelled in four more lines",
        off_by_default: true,
    },
    EmptyMethod {
        slug: "empty-method",
        category: Waste,
        analysis: Semantics,
        metric: "a function or method with nothing in its body",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "a caller that reaches this gets silence and no way to tell it apart from \
              work that succeeded, and the next reader cannot tell whether it is a stub \
              or a decision",
    },
    ClosureOverLoopVariable {
        slug: "closure-over-loop-variable",
        category: Safety,
        analysis: Syntax,
        metric: "a function written inside a loop that reads the name the loop moves",
        fixes: [ExtractMethod, IntroduceParameterObject],
        confidence: Heuristic,
        severity: Warning,
        why: "whatever runs it later reads whichever value the loop stopped on, so \
              every one of them sees the last element and none sees its own",
        not_applicable: [(Java, "a captured local must already be effectively final")],
    },
    MutatedWhileIterated {
        slug: "mutated-while-iterated",
        category: Safety,
        analysis: Syntax,
        metric: "changing the collection a loop is walking, from inside the walk",
        fixes: [ReplaceAlgorithm],
        confidence: Verified,
        severity: Error,
        why: "the walk and the change disagree about what is left, so elements are \
              skipped or seen twice depending on where the change landed",
    },
    SequentialAwait {
        slug: "sequential-await",
        category: Performance,
        analysis: Syntax,
        metric: "two waits in a row where the second does not read the first's answer",
        fixes: [ReplaceAlgorithm],
        confidence: Heuristic,
        severity: Warning,
        why: "nothing makes them an order, so the call takes the sum of two waits \
              where it could have taken the longer one",
        not_applicable: [
            (Go, "waiting is a channel receive, which the language does not mark"),
            (Java, "waiting is a call like any other"),
            (Cpp, "waiting is a call like any other"),
        ],
    },
    ProductionAssert {
        slug: "production-assert",
        category: Safety,
        analysis: Syntax,
        metric: "an assertion outside a test",
        fixes: [IntroduceAssertion],
        confidence: Heuristic,
        severity: Warning,
        why: "the language is free to compile it away, so the check a reader is \
              relying on may not be running where it matters most",
        not_applicable: [(Go, "there is no assert; a failure is a value that is returned")],
    },
    RealWorldTest {
        slug: "real-world-test",
        category: Tests,
        analysis: Syntax,
        metric: "a test reaching a database, a network or a disk",
        fixes: [InvertDependency],
        confidence: Heuristic,
        severity: Warning,
        why: "it fails for reasons that have nothing to do with the code, and it is \
              slow enough that somebody eventually stops running it",
        off_by_default: true,
    },
    BranchingTest {
        slug: "branching-test",
        category: Tests,
        analysis: Syntax,
        metric: "a decision inside a test body",
        fixes: [ExtractMethod, DecomposeConditional],
        confidence: Verified,
        severity: Warning,
        why: "a test that decides has paths of its own, and nothing says which one \
              ran, so a green result no longer means the case was checked",
    },
    DuplicateAssertion {
        slug: "duplicate-assertion",
        category: Tests,
        analysis: Syntax,
        metric: "the same assertion written twice in one test",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "the second one can never fail while the first passes, so it costs a \
              reader a check they have already read",
    },
    HandRolledThrows {
        slug: "hand-rolled-throws",
        category: Tests,
        analysis: Syntax,
        metric: "a test asserting a failure with a handler of its own",
        fixes: [IntroduceAssertion],
        confidence: Heuristic,
        severity: Warning,
        why: "when the call stops failing the handler never runs and the test still \
              passes, which is the one outcome it was written to catch",
        not_applicable: [(Go, "failure is a returned value the test compares directly")],
    },
    DebugOutputInTest {
        slug: "debug-output-in-test",
        category: Tests,
        analysis: Syntax,
        metric: "a write to the terminal from inside a test",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "it was how somebody watched this test once, and it now prints on every \
              run for people who are not watching",
    },
    NegatedName {
        slug: "negated-name",
        category: Naming,
        analysis: Syntax,
        metric: "a name that states what a thing is not",
        fixes: [RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "every test of it that is itself negated has to be undone twice in the \
              reader's head before the condition means anything",
    },
    ShadowsBuiltin {
        slug: "shadows-builtin",
        category: Naming,
        analysis: Syntax,
        metric: "a declared name the language or its standard library already owns",
        fixes: [RenameVariable],
        confidence: Verified,
        severity: Warning,
        why: "the original is unreachable for the rest of the scope, and the next line \
              that wanted it fails somewhere that names neither",
    },
    CommentPerLine {
        slug: "comment-per-line",
        category: Noise,
        analysis: Syntax,
        metric: "a run of statements each carrying a comment of its own",
        fixes: [RemoveDeadCode, RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "the code is being read aloud rather than explained, so the reader now \
              has two things to keep in step and only one of them is checked",
    },
    ExplanationLongerThanCode {
        slug: "explanation-longer-than-code",
        category: Noise,
        analysis: Syntax,
        metric: "lines of comment above a function against statements inside it",
        fixes: [ExtractMethod, RenameMethod],
        confidence: Heuristic,
        severity: Info,
        why: "a body that needs more prose than code is a body whose names are not \
              carrying it, and the prose is what goes stale",
    },
    DocstringRestatesSignature {
        slug: "docstring-restates-signature",
        category: Noise,
        analysis: Syntax,
        metric: "a doc comment whose every word is already in the name or the \
                 parameters",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "it is the signature written twice, so it costs a reader a second pass \
              and goes wrong the first time the signature changes",
    },
    GuardCascade {
        slug: "guard-cascade",
        category: Safety,
        analysis: Syntax,
        metric: "checks at the top of a body that all answer the same way",
        fixes: [IntroduceAssertion, IntroduceParameterObject],
        confidence: Heuristic,
        severity: Info,
        why: "the body has not started and the reader has read several decisions, \
              none of which says anything about what the function is for",
    },
    BlanketTry {
        slug: "blanket-try",
        category: Safety,
        analysis: Syntax,
        metric: "a guarded block holding nearly every statement of its function",
        fixes: [DecomposeConditional, ExtractMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "the handler is a net under the whole body rather than an answer to one \
              failure, so a mistake anywhere inside arrives as the failure it was \
              written for",
        not_applicable: [(Go, "failure is a returned value, so no block is guarded")],
    },
    CyclomaticComplexity {
        slug: "cyclomatic-complexity",
        category: Size,
        analysis: Syntax,
        metric: "paths through one function: one plus every decision in it",
        fixes: [ExtractMethod, DecomposeConditional],
        confidence: Verified,
        severity: Warning,
        why: "it is the number of tests it takes to cover the body, and past a point \
              nobody writes them, so most of the paths have never run",
    },
    ManyResults {
        slug: "many-results",
        category: Size,
        analysis: Syntax,
        metric: "values handed back by one return",
        fixes: [IntroduceParameterObject, ExtractClass],
        confidence: Heuristic,
        severity: Info,
        why: "the caller has to know the order to read them, and nothing at either \
              end says what any of them is",
        not_applicable: [
            (Java, "a method answers with one value"),
            (Cpp, "a function answers with one value"),
        ],
    },
    NoBaseCase {
        slug: "no-base-case",
        category: Safety,
        analysis: Syntax,
        metric: "a function that calls itself with no decision anywhere in the body",
        fixes: [DecomposeConditional],
        confidence: Verified,
        severity: Error,
        why: "there is nothing that can stop it, so the first call takes the stack \
              down with it",
    },
    BooleanNotAQuestion {
        slug: "boolean-not-a-question",
        category: Naming,
        analysis: Syntax,
        metric: "something answering true or false under a name that is not a question",
        fixes: [RenameMethod],
        confidence: Heuristic,
        severity: Info,
        why: "a reader has to open it to learn which way round the answer runs, and \
              a name that asks the question says it in one word",
    },
    UnnamedTest {
        slug: "unnamed-test",
        category: Tests,
        analysis: Syntax,
        metric: "a test whose name says nothing about what it checks",
        fixes: [RenameMethod],
        confidence: Verified,
        severity: Info,
        why: "a failing run names the test and nothing else, so the name is the whole \
              of what the reader is given before they open the file",
    },
    NameRepeatsType {
        slug: "name-repeats-type",
        category: Naming,
        analysis: Syntax,
        metric: "a name ending in the word for what it holds",
        fixes: [RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "the declaration beside it already says what it is, so the word is read \
              twice and goes wrong the day the type changes",
    },
    PluralMismatch {
        slug: "plural-mismatch",
        category: Naming,
        analysis: Syntax,
        metric: "a name whose number disagrees with what it was given",
        fixes: [RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "a reader counts on the name to say whether there is one of something \
              or many, and here it says the opposite",
    },
    SingleMethodField {
        slug: "single-method-field",
        category: Abstraction,
        analysis: Syntax,
        metric: "a field only one method of its type ever reads",
        fixes: [ExtractClass, InlineVariable],
        confidence: Heuristic,
        severity: Info,
        why: "it is that method's local kept where every other method can reach it, \
              so every one of them has to be read before it can be changed",
        not_applicable: [
            (Go, "a struct field is read by whatever holds the struct"),
            (Cpp, "the methods that read it are in another file"),
            (Python, "an attribute is written into the instance, not declared on the type"),
        ],
    },
    ManyCollaborators {
        slug: "many-collaborators",
        category: Coupling,
        analysis: Syntax,
        metric: "other types one type has to know about",
        fixes: [ExtractClass, InvertDependency],
        confidence: Heuristic,
        severity: Warning,
        why: "each one is a reason for this type to change, and a test of it has to \
              stand up all of them first",
        not_applicable: [(Go, "there are no types that hold behaviour to collect them")],
    },
    OverLogging {
        slug: "over-logging",
        category: Noise,
        analysis: Syntax,
        metric: "log calls against the statements of the body they are in",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "the body says more about what it is doing than it does, and the reader \
              has to separate the work from the commentary on every line",
    },
    ManyLocals {
        slug: "many-locals",
        category: Size,
        analysis: Syntax,
        metric: "distinct names one function body introduces",
        fixes: [ExtractMethod, ExtractVariable],
        confidence: Heuristic,
        severity: Info,
        why: "every one of them is a thing the reader has to keep in mind to the end \
              of the body, and there is a number past which nobody does",
    },
    WideTry {
        slug: "wide-try",
        category: Size,
        analysis: Syntax,
        metric: "statements inside one guarded block",
        fixes: [ExtractMethod, DecomposeConditional],
        confidence: Heuristic,
        severity: Warning,
        why: "the handler is written for one of them and catches all of them, so a \
              failure from any other line arrives somewhere that cannot answer it",
        not_applicable: [(Go, "failure is a returned value, so no block is guarded")],
    },
    EmptySubclass {
        slug: "empty-subclass",
        category: Abstraction,
        analysis: Syntax,
        metric: "a type that inherits and declares nothing of its own",
        fixes: [CollapseHierarchy, InlineClass],
        confidence: Verified,
        severity: Info,
        why: "it is a second name for the thing it came from, and a reader has to \
              open it to find that out",
        not_applicable: [(Go, "there is no inheritance for a type to add nothing to")],
    },
    StaticOnlyClass {
        slug: "static-only-class",
        category: Abstraction,
        analysis: Syntax,
        metric: "a type whose every method belongs to the type rather than to an \
                 instance",
        fixes: [InlineClass, MoveMethod],
        confidence: Heuristic,
        severity: Info,
        why: "nothing is ever built from it, so the type is a namespace wearing a \
              class's syntax and nothing can be substituted for it in a test",
        not_applicable: [(Go, "there are no classes to hold the methods")],
    },
    WorkingConstructor {
        slug: "working-constructor",
        category: Abstraction,
        analysis: Syntax,
        metric: "calls made while an object is being built, beyond recording what it \
                 was given",
        fixes: [ExtractMethod, SeparateQueryFromModifier],
        confidence: Heuristic,
        severity: Warning,
        why: "the object cannot exist without whatever that work needs, so every test \
              of it has to stand the work up first",
        not_applicable: [(Go, "a value is built by a literal, which can only assign")],
    },
    OversizedModule {
        slug: "oversized-module",
        category: Size,
        analysis: Syntax,
        metric: "statements in one file, across every function in it",
        fixes: [ExtractSharedModule, ExtractClass],
        confidence: Heuristic,
        severity: Info,
        why: "a file this long is several files that have not been separated yet, and \
              every one of them is edited, reviewed and merged as though it were one",
    },
    DiscardedExpression {
        slug: "discarded-expression",
        category: Waste,
        analysis: Syntax,
        metric: "a statement that computes a value and does nothing with it",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "either the line was meant to do something and does not, or it does \
              nothing and should not be here, and a reader cannot tell which",
        not_applicable: [
            (Go, "the grammar allows only a call or a receive as a statement, so the compiler rejects this first"),
            (Java, "the grammar allows only a call, an assignment or an increment as a statement"),
        ],
    },
    FloatEquality {
        slug: "float-equality",
        category: Safety,
        analysis: Syntax,
        metric: "an equality test against a literal with a fractional part",
        fixes: [IntroduceAssertion],
        confidence: Verified,
        severity: Warning,
        why: "a binary float almost never lands on the decimal a person wrote, so the \
              branch that depends on it is decided by rounding rather than by the value",
    },
    DeepRelativeImport {
        slug: "deep-relative-import",
        category: Coupling,
        analysis: Semantics,
        metric: "levels a relative import climbs before it descends",
        fixes: [ExtractSharedModule, InvertDependency],
        confidence: Heuristic,
        severity: Info,
        why: "a path that climbs this far is reaching across the shape of the project \
              rather than through it, and it breaks the moment either end is moved",
        not_applicable: [
            (Go, "an import is a module path, so there is nothing relative to climb"),
            (Java, "an import is a package name, and packages do not nest by path"),
        ],
    },
    MixedReturn {
        slug: "mixed-return",
        category: Safety,
        analysis: Syntax,
        metric: "one function answering with a value on one path and with nothing on \
                 another",
        fixes: [SeparateQueryFromModifier, DecomposeConditional],
        confidence: Verified,
        severity: Warning,
        why: "a caller cannot write one line that handles both, so every call site \
              carries a check for the path the author happened to think of",
        not_applicable: [
            (Go, "a naked return beside a named result is the language's own idiom"),
            (Java, "the compiler refuses a body that answers two shapes"),
            (Cpp, "the compiler refuses a body that answers two shapes"),
        ],
    },
    ExitFromFinally {
        slug: "exit-from-finally",
        category: Safety,
        analysis: Syntax,
        metric: "a return, break, continue or raise inside the clause that runs on the \
                 way out",
        fixes: [DecomposeConditional],
        confidence: Verified,
        severity: Error,
        why: "it replaces whatever the block was leaving with, so a failure raised in \
              the body disappears and nothing anywhere records that it happened",
        not_applicable: [
            (Go, "cleanup is a deferred call, which cannot replace the return"),
            (Cpp, "there is no finally clause"),
        ],
    },
    CatchAllHandler {
        slug: "catch-all-handler",
        category: Safety,
        analysis: Syntax,
        metric: "a handler naming the root of the error hierarchy, or naming nothing at \
                 all",
        fixes: [DecomposeConditional],
        confidence: Verified,
        severity: Warning,
        why: "it takes the failures it was written for and every other one too, \
              including the ones that mean the program is already wrong",
        not_applicable: [(Go, "errors are values, so there is no handler to widen")],
    },
    GenericThrow {
        slug: "generic-throw",
        category: Safety,
        analysis: Syntax,
        metric: "raising the root error type rather than one that names the failure",
        fixes: [ReplaceDataValueWithObject],
        confidence: Verified,
        severity: Info,
        why: "the only way to react to it is to catch everything, so a caller that \
              wanted to handle this one failure cannot",
        not_applicable: [(Go, "errors are values, and the language raises nothing")],
    },
    LibraryExit {
        slug: "library-exit",
        category: Safety,
        analysis: Syntax,
        metric: "a call that ends the process, written outside the entry point",
        fixes: [SeparateQueryFromModifier],
        confidence: Heuristic,
        severity: Warning,
        why: "it takes the decision away from every caller above it, and a test that \
              reaches the line takes the runner down with it",
    },
    BooleanLiteralArgument {
        slug: "boolean-literal-argument",
        category: Safety,
        analysis: Syntax,
        metric: "a bare true or false among a call's arguments",
        fixes: [ReplaceParameterWithExplicitMethods],
        confidence: Heuristic,
        severity: Info,
        why: "nothing at the call site says what it turns on, so the reader has to \
              open the callee to find out what they just asked for",
    },
    UnusedImport {
        slug: "unused-import",
        category: Waste,
        analysis: Semantics,
        metric: "a name imported into a file and never read there",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "it makes the file look dependent on something it does not use, and the \
              dependency survives every search for who needs that module",
        not_applicable: [
            (Go, "the compiler refuses a file with one"),
            (Cpp, "an include may be there for a header that needs it"),
        ],
    },
    MutableModuleState {
        slug: "mutable-module-state",
        category: Coupling,
        analysis: Syntax,
        metric: "a binding at file level that anything in the file can write to",
        fixes: [EncapsulateField, ExtractClass],
        confidence: Heuristic,
        severity: Warning,
        why: "every function that reads it takes an argument nobody passed, so two \
              calls with the same arguments can answer differently and no signature \
              says why",
    },
    AmbientDependency {
        slug: "ambient-dependency",
        category: Coupling,
        analysis: Syntax,
        metric: "a body reaching for the clock, the environment or a random source",
        fixes: [IntroduceParameterObject, InvertDependency],
        confidence: Heuristic,
        severity: Info,
        why: "the answer depends on something no caller can set, so the only way to \
              test the branch that reads it is to change the machine",
    },
    SelfBuiltCollaborators {
        slug: "self-built-collaborators",
        category: Coupling,
        analysis: Syntax,
        metric: "a field filled with something the object constructed itself",
        fixes: [InvertDependency, IntroduceParameterObject],
        confidence: Verified,
        severity: Warning,
        why: "nothing can be put in its place, so every test of the owner stands up \
              the real collaborator and everything behind it",
        not_applicable: [(Go, "a value is built by a literal, which names no type to \
                              stand in for")],
    },
    ImportTimeWork {
        slug: "import-time-work",
        category: Coupling,
        analysis: Syntax,
        metric: "a call that runs the moment the file is loaded",
        fixes: [ExtractMethod, SeparateQueryFromModifier],
        confidence: Verified,
        severity: Warning,
        why: "it happens before any caller has asked for anything, in whatever order \
              the imports happen to resolve, and nothing can stop it",
        not_applicable: [
            (Go, "a file holds declarations, and only `init` runs on load"),
            (Java, "a file holds declarations, and only a static block runs on load"),
            (Cpp, "a translation unit holds declarations"),
        ],
    },
    ReachesIntoPrivate {
        slug: "reaches-into-private",
        category: Coupling,
        analysis: Syntax,
        metric: "reading a member another type marked as its own business",
        fixes: [EncapsulateDelegation, MoveMethod],
        confidence: Verified,
        severity: Warning,
        why: "the other type is free to change it and has said so, so this line \
              breaks on a change nobody could have known would reach it",
        off_by_default: true,
        not_applicable: [
            (Go, "the package boundary is the language's, and the compiler holds it"),
            (Java, "the compiler refuses it"),
            (Cpp, "the compiler refuses it"),
        ],
    },
    WildcardImport {
        slug: "wildcard-import",
        category: Coupling,
        analysis: Syntax,
        metric: "an import that brings in every name a module offers",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "nothing in the file says which names came from where, so a reader has \
              to open the other module to find out and a new export there can shadow \
              a local one silently",
    },
    RethrowOnlyCatch {
        slug: "rethrow-only-catch",
        category: Waste,
        analysis: Syntax,
        metric: "a handler whose only statement raises again",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "it reads as a decision about the failure and makes none, while costing \
              the stack trace in the languages that rebuild one on the way out",
        not_applicable: [(Go, "errors are values, so there is no handler to empty")],
    },
    PlaceholderBody {
        slug: "placeholder-body",
        category: Waste,
        analysis: Syntax,
        metric: "a body whose only statement raises to say the work is still to come",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Warning,
        why: "it is a callable promise with nothing behind it, and every caller that \
              trusts the signature finds that out at runtime",
        not_applicable: [(Go, "failure is a returned value, so no statement is a refusal")],
    },
    PassthroughFunction {
        slug: "passthrough-function",
        category: Waste,
        analysis: Syntax,
        metric: "a function whose whole body hands its own arguments to one other call",
        fixes: [InlineDelegation],
        confidence: Heuristic,
        severity: Info,
        why: "it adds a name and a stack frame between a caller and the thing it \
              wanted, and the indirection buys nothing while it forwards unchanged",
    },
    DebugOutput {
        slug: "debug-output",
        category: Waste,
        analysis: Syntax,
        metric: "a write to the terminal outside a test, of the kind left behind after \
                 debugging",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "it was written to be read once by the person debugging, and it is still \
              printing for everybody else",
        off_by_default: true,
    },
    DuplicateImport {
        slug: "duplicate-import",
        category: Waste,
        analysis: Semantics,
        metric: "the same module imported by more than one statement in a file",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "two statements bringing in one module means a reader has to check both \
              to know what this file takes from it, and a merge put at least one there \
              without anybody looking",
    },
    WideFanOut {
        slug: "wide-fan-out",
        category: Coupling,
        analysis: Modules,
        metric: "modules a single file imports",
        fixes: [ExtractSharedModule, InvertDependency],
        confidence: Heuristic,
        severity: Info,
        why: "a file that reaches into this many places cannot be read, moved or \
              tested without most of the project coming with it",
    },
    WideFanIn {
        slug: "wide-fan-in",
        category: Coupling,
        analysis: Modules,
        metric: "modules that import a single file",
        fixes: [ExtractInterface],
        confidence: Heuristic,
        severity: Info,
        why: "every change here reaches this many places, so the module is load \
              bearing whether or not anybody decided it should be",
    },
    BannerComment {
        slug: "banner-comment",
        category: Noise,
        analysis: Semantics,
        metric: "a comment that is a divider or a one-word heading",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "a table of contents for a file short enough not to need one, and it goes \
              stale the moment a section moves",
        autofix: Deletion,
    },
    ChatResidue {
        slug: "chat-residue",
        category: Noise,
        analysis: Semantics,
        metric: "a comment addressed to a person rather than about the code",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "it is the residue of a conversation that ended before the commit, and it \
              tells the next reader nothing about the code it sits on",
        autofix: Deletion,
    },
    ApologyComment {
        slug: "apology-comment",
        category: Noise,
        analysis: Semantics,
        metric: "a comment apologising for the code under it",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "somebody already knew this was wrong and left it, so the comment records \
              the decision without the reason or the ticket that would let it be undone",
    },
    EmojiInSource {
        slug: "emoji-in-source",
        category: Noise,
        analysis: Semantics,
        metric: "an emoji in a comment outside a test",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "it survives grep, diffs and terminals badly, and it is decoration in a \
              place where everything else has to earn its width",
        off_by_default: true,
    },
    ExampleValue {
        slug: "example-value",
        category: Noise,
        analysis: Semantics,
        metric: "a placeholder like example.com or YOUR_API_KEY outside a test",
        fixes: [ReplaceMagicNumberWithConstant],
        confidence: Heuristic,
        severity: Warning,
        why: "a stand-in that reads as real is the kind of value that ships, and the \
              first person to notice is whoever it was sent to",
    },
    UnjustifiedSuppression {
        slug: "unjustified-suppression",
        category: Safety,
        analysis: Syntax,
        metric: "an rqx-ignore comment with no reason, or one whose date has passed",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "the waiver is refused and the finding still stands, because a waiver \
              nobody has to justify is how a rule gets switched off one line at a time",
    },
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

    // ---- Performance ----------------------------------------------------------
    ScanInLoop {
        slug: "scan-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "a whole-collection search, sort or copy inside a loop",
        fixes: [ReplaceLoopWithLookup],
        confidence: Heuristic,
        severity: Warning,
        why: "the inner scan runs once per pass of the outer loop, so the work grows with \
              the product of the two sizes and a list that was fine at a hundred items \
              stalls at ten thousand",
    },
    DeepLoopNesting {
        slug: "deep-loop-nesting",
        category: Performance,
        analysis: Syntax,
        metric: "loop nesting depth, counting callbacks and comprehensions as loops",
        fixes: [ReplaceLoopWithLookup, ReplaceAlgorithm],
        confidence: Heuristic,
        severity: Warning,
        why: "each level multiplies the passes of the one inside it, so the innermost body \
              runs a number of times no test on small data will ever show",
    },
    QueryInLoop {
        slug: "query-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "a call on a database, HTTP or queue client inside a loop",
        fixes: [BatchTheWork],
        confidence: Heuristic,
        severity: Warning,
        why: "every pass pays a round trip, so listing a hundred rows makes a hundred and \
              one requests and the page waits for the sum of them",
    },
    AwaitInLoop {
        slug: "await-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "an await inside a loop body, outside any nested function",
        fixes: [BatchTheWork, AwaitConcurrently],
        confidence: Heuristic,
        severity: Warning,
        why: "each pass waits for the one before it to finish, so work that could overlap \
              runs end to end and the loop takes as long as all of its waits added together",
        not_applicable: [
            (Go, "a goroutine blocks by design, so there is no await here to serialise"),
            (Java, "Java has no await expression: a blocking call is a call, and futures compose by method"),
        ],
    },
    InvariantInLoop {
        slug: "invariant-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "a regular expression compiled from a literal inside a loop",
        fixes: [HoistInvariant],
        confidence: Verified,
        severity: Warning,
        why: "the same pattern is parsed and compiled on every pass when once would do, and \
              compiling is the expensive half of matching",
    },
    AccumulateByCopy {
        slug: "accumulate-by-copy",
        category: Performance,
        analysis: Semantics,
        metric: "an accumulator rebuilt by copying itself on every pass of a loop",
        fixes: [AccumulateInPlace],
        confidence: Heuristic,
        severity: Warning,
        why: "each step copies everything gathered so far, so building a result of n items \
              touches half of n squared of them",
    },
    BranchingRecursion {
        slug: "branching-recursion",
        category: Performance,
        analysis: Syntax,
        metric: "a function calling itself two or more times with no memo in sight",
        fixes: [Memoize, ReplaceAlgorithm],
        confidence: Heuristic,
        severity: Warning,
        why: "two calls per level is an exponential number of calls, most of them \
              recomputing an answer some other branch already found",
    },
    BlockingCallInAsync {
        slug: "blocking-call-in-async",
        category: Performance,
        analysis: Semantics,
        metric: "a synchronous file, process or sleep call inside an async function",
        fixes: [ReplaceBlockingCall],
        confidence: Heuristic,
        severity: Warning,
        why: "an async function promises not to hold the thread and this call holds it, so \
              everything else waiting on that loop queues behind one file read",
        not_applicable: [
            (Go, "blocking a goroutine is the intended model, not a mistake to report"),
            (Java, "Java has no async function marker, so there is no promise here to break"),
            (Cpp, "a coroutine is decided by co_return in its body rather than by a marker on \
                   its signature, so there is nothing to test a call against"),
        ],
    },
}

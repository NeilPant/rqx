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
        why: "a long body usually does several things, and a reader has to hold all of them in \
              mind to change any one",
    },
    CognitiveComplexity {
        slug: "cognitive-complexity",
        category: Size,
        analysis: Syntax,
        metric: "SonarSource cognitive complexity",
        fixes: [DecomposeConditional, ExtractMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "each nested decision point multiplies the paths a reader has to trace, and paths \
              that go untraced are where regressions pass review",
    },
    DeepNesting {
        slug: "deep-nesting",
        category: Size,
        analysis: Syntax,
        metric: "maximum block nesting depth",
        fixes: [ReplaceNestedConditionalWithGuardClauses],
        confidence: Heuristic,
        severity: Warning,
        why: "at this depth the conditions that led to the innermost code are off screen, so its \
              preconditions are hard to see",
    },
    LongParameterList {
        slug: "long-parameter-list",
        category: Size,
        analysis: Syntax,
        metric: "parameter count",
        fixes: [IntroduceParameterObject, PassWholeObject],
        confidence: Heuristic,
        severity: Warning,
        why: "callers have to remember a long argument order, and adjacent parameters of the same \
              type allow an undetected transposition at every call site",
    },
    LargeClass {
        slug: "large-class",
        category: Size,
        analysis: Syntax,
        metric: "member count and body statement count",
        fixes: [ExtractClass, ExtractSubclass],
        confidence: Heuristic,
        severity: Warning,
        why: "a class this size usually has more than one reason to change, so unrelated changes \
              land in the same file and produce the same merge conflicts",
    },
    DataClump {
        slug: "data-clump",
        category: Size,
        analysis: Semantics,
        metric: "the same group of three or more parameter names recurring across signatures",
        fixes: [IntroduceParameterObject, ExtractClass],
        confidence: Heuristic,
        severity: Info,
        why: "these values are passed together in several signatures, which indicates a concept \
              the code has not named, and each new use has to rediscover it",
    },
    PrimitiveObsession {
        slug: "primitive-obsession",
        category: Size,
        analysis: Semantics,
        metric: "a type alias that is a union of three or more string literals",
        fixes: [ReplaceTypeCodeWithClass, ReplaceDataValueWithObject],
        confidence: Low,
        severity: Info,
        why: "a raw string or number carries no rules of its own, so each place that uses it \
              repeats the same validation and one of them can be wrong",
        off_by_default: true,
        not_applicable: [
            (Go, "Go has no union types, so the string-literal alias this measures cannot be written"),
            (Java, "Java has no union types, so the string-literal alias this measures cannot be written"),
            (Cpp, "C++ has no union types, so the string-literal alias this measures cannot be written"),
            (JavaScript, "JavaScript has no type aliases, so the string-literal alias this measures cannot be written"),
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
        why: "the same set of cases is written out in several places, so adding a case requires \
              finding all of them and the compiler does not report a missed one",
    },
    TemporaryField {
        slug: "temporary-field",
        category: Abstraction,
        analysis: Semantics,
        metric: "instance field written and read by exactly one method, never in the constructor",
        fixes: [ExtractClass, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "the field is empty for most of the object's lifetime, so every other method has to \
              know it must not read it",
    },
    RefusedBequest {
        slug: "refused-bequest",
        category: Abstraction,
        analysis: Semantics,
        metric: "override whose body only throws or is empty",
        fixes: [PushDownMethod, ExtractInterface, CollapseHierarchy],
        confidence: Verified,
        severity: Warning,
        why: "the subclass inherits an interface it cannot honour, so a caller holding the base \
              type cannot tell which methods work",
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
        why: "the base cannot be deployed, tested or understood without its subclasses, which \
              removes the benefit of having a base class",
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
        why: "a static member cannot be overridden, so behaviour that belongs to the instance \
              cannot vary in subclasses or be replaced by a test double",
        not_applicable: [
            (Go, "Go has no methods on the type itself, only on values of it, so the static-on-own-type shape cannot be written"),
            (JavaScript, "a parameter declares no type, so nothing says it is an instance of its own class"),
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
        why: "the cost of the problems in this file is paid on every change, and this file \
              changes more often than any other, so cleaning it up has the largest effect",
    },
    SingleAuthorFile {
        slug: "single-author-file",
        category: Change,
        analysis: Syntax,
        metric: "a file with its own history and a single author",
        fixes: [ShareOwnership],
        confidence: Verified,
        severity: Info,
        why: "no one else has read the file closely enough to change it, so its knowledge is lost \
              when its author leaves",
    },
    ShotgunSurgery {
        slug: "shotgun-surgery",
        category: Change,
        analysis: Syntax,
        metric: "files that git history shows almost always change together",
        fixes: [ExtractClass, MoveMethod],
        confidence: Heuristic,
        severity: Info,
        why: "one decision is spread across files that must be edited together, and nothing in \
              the code says so, so an edit to only some of them breaks the behaviour",
    },
    DivergentChange {
        slug: "divergent-change",
        category: Change,
        analysis: Syntax,
        metric: "a file with many commits and no single frequent co-change partner",
        fixes: [ExtractClass],
        confidence: Low,
        severity: Info,
        why: "the file is edited for many unrelated reasons, which indicates that more than one \
              responsibility lives in it",
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
        why: "each new case needs a new subclass in both hierarchies, and adding only one leaves \
              the two hierarchies out of step",
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
        why: "a fix applied to one copy leaves the others unfixed, and nothing in the code links \
              the edited copy to the others",
    },
    ReimplementedExisting {
        slug: "reimplemented-existing",
        category: Waste,
        analysis: Syntax,
        metric: "added function whose normalised body matches a function that exists at the base revision",
        fixes: [ReplaceAlgorithm, ExtractSharedModule],
        confidence: Heuristic,
        severity: Warning,
        why: "a second copy of an existing function has to be changed together with the original, \
              and nothing in the code connects the two",
    },
    UnusedExport {
        slug: "unused-export",
        category: Waste,
        analysis: Modules,
        metric: "exported symbol with no importer in the resolved module graph",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "an export that nothing imports still has to be read, type-checked and kept working \
              during every refactoring that touches it",
        short: "an export nothing imports still has to be read and kept working",
    },
    UnusedFile {
        slug: "unused-file",
        category: Waste,
        analysis: Modules,
        metric: "module reachable from no entry point, with no importer",
        fixes: [DeleteFile],
        confidence: Verified,
        severity: Warning,
        why: "nothing in the build reaches this file, so the effort of maintaining it has no \
              effect on the program",
    },
    UnusedParameter {
        slug: "unused-parameter",
        category: Waste,
        analysis: Semantics,
        metric: "declared parameter with no resolved reference in its own scope",
        fixes: [RemoveParameter],
        confidence: Verified,
        severity: Warning,
        why: "every caller passes a value that is never read, so the parameter list misdescribes \
              the function",
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
        why: "the value is computed and discarded, which either wastes the work or hides a \
              missing use",
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
        why: "a private member can be used only from inside its class, so one that nothing in the \
              class references cannot be reached",
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
        why: "this code cannot run, so whatever it was written to do is not happening",
        short: "this code cannot run, so whatever it was written to do is not happening",
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
        why: "readers cannot tell whether the code still matters, so it is left in place, and \
              version control already preserves it",
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
        why: "the class costs a file, an import and a name, and does not do enough to justify \
              them",
    },
    DataClass {
        slug: "data-class",
        category: Waste,
        analysis: Semantics,
        metric: "class whose members are all fields or trivial accessors",
        fixes: [MoveMethod, EncapsulateField],
        confidence: Heuristic,
        severity: Info,
        why: "the behaviour that belongs with this data is implemented elsewhere, so the rules \
              about the data are enforced everywhere except where the data is defined",
    },
    SpeculativeGenerality {
        slug: "speculative-generality",
        category: Waste,
        analysis: Modules,
        metric: "abstract class with no subclass anywhere in the project",
        fixes: [CollapseHierarchy, InlineClass],
        confidence: Verified,
        severity: Info,
        why: "it was built for a case that has not occurred, and until it does it is an extra \
              indirection every reader has to follow. Interfaces are not reported: TypeScript \
              interfaces are structural, so the absence of implementers says nothing about \
              whether one is speculative",
        not_applicable: [(JavaScript, "JavaScript has no abstract keyword, so no class declares itself waiting for a subclass")],
    },
    EmptyCatch {
        slug: "empty-catch",
        category: Waste,
        analysis: Semantics,
        metric: "catch clause with an empty body, a warning when it names a type narrower \
                 than the root of the error hierarchy",
        fixes: [HandleTheFailure],
        confidence: Verified,
        severity: Error,
        why: "the failure is swallowed silently, so it first appears as wrong data elsewhere, \
              with nothing in the logs to connect the two",
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
        why: "the number encodes a decision that is not recorded, so a reader cannot tell whether \
              changing it is safe or what else depends on the same value",
    },

    ImpreciseType {
        slug: "imprecise-type",
        category: Safety,
        analysis: Semantics,
        metric: "an `any` annotation, a non-null assertion, or a cast that is not `as const`",
        fixes: [NarrowTheType, IntroduceAssertion],
        confidence: Verified,
        severity: Warning,
        why: "each of these types turns off type checking for the value, so the guarantees the \
              rest of the file relies on do not hold here",
        not_applicable: [(JavaScript, "JavaScript declares no types, so there is no annotation or cast to be imprecise")],
    },

    MixedLanguageFile {
        slug: "mixed-language-file",
        category: Waste,
        analysis: Semantics,
        metric: "SQL or HTML markup inside a template literal",
        fixes: [ExtractSharedModule],
        confidence: Heuristic,
        severity: Info,
        why: "a second language in the file gets none of the tooling the first language has, so \
              nothing checks it until it fails at run time",
    },
    BoundaryArithmetic {
        slug: "boundary-arithmetic",
        category: Safety,
        analysis: Semantics,
        metric: "the same subject adjusted by one at more sites than the threshold",
        fixes: [ExtractVariable, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "each place that recomputes the boundary can compute it wrongly, and off-by-one \
              errors are found by readers, not by types",
    },
    MixedConventions {
        slug: "mixed-conventions",
        category: Naming,
        analysis: Semantics,
        metric: "camelCase and snake_case names declared in one file",
        fixes: [RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "a reader who has to guess which convention a name follows cannot skim, and the \
              mixture spreads each time a neighbouring line is copied",
    },
    SlowTest {
        slug: "slow-test",
        category: Tests,
        analysis: Semantics,
        metric: "a sleep or timeout inside a test body above the configured pause",
        fixes: [ReplaceAlgorithm, InvertDependency],
        confidence: Verified,
        severity: Warning,
        why: "a suite that is slow enough to be skipped stops being run, and a test that is not \
              run provides no protection",
    },

    DistantDeclaration {
        slug: "distant-declaration",
        category: Waste,
        analysis: Semantics,
        metric: "lines between a local's declaration and the line that first reads it",
        fixes: [InlineVariable],
        confidence: Heuristic,
        severity: Info,
        why: "a reader has to keep the name in mind while reading everything in between, without \
              knowing whether it will matter",
        short: "a reader has to keep the name in mind through everything in between",
    },

    ObscuredExpression {
        slug: "obscured-expression",
        category: Size,
        analysis: Semantics,
        metric: "operations packed into one returned expression beyond the limit",
        fixes: [ExtractVariable, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "no part of the expression is named, so a reader has to re-derive each intermediate \
              step the author already worked out",
    },
    BuriedConfiguration {
        slug: "buried-configuration",
        category: Waste,
        analysis: Semantics,
        metric: "a URL or absolute path written below the configured block depth",
        fixes: [ReplaceMagicNumberWithConstant, ExtractVariable],
        confidence: Heuristic,
        severity: Info,
        why: "changing the program's target requires finding this line first, and nothing about \
              its position indicates that it is a setting",
    },

    TemporalCoupling {
        slug: "temporal-coupling",
        category: Abstraction,
        analysis: Semantics,
        metric: "a field no constructor sets that two or more methods test before acting",
        fixes: [IntroduceAssertion, ExtractClass],
        confidence: Heuristic,
        severity: Warning,
        why: "the order in which the methods must be called is required but not written down, so \
              it is learned by calling them in the wrong order",
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
        why: "no module in the cycle can be understood, tested or loaded on its own, and the \
              initialisation order that works today is accidental",
    },
    LayerViolation {
        slug: "layer-violation",
        category: Coupling,
        analysis: Modules,
        metric: "an import pointing from an inner layer to an outer one",
        fixes: [InvertDependency],
        confidence: Verified,
        severity: Error,
        why: "a lower layer depends on a layer above it, so what the lower layer was separated \
              from cannot be replaced without changing it",
    },
    ForbiddenImport {
        slug: "forbidden-import",
        category: Coupling,
        analysis: Modules,
        metric: "an import the project's own rules say this subtree may not make",
        fixes: [InvertDependency],
        confidence: Verified,
        severity: Error,
        why: "the project declared this boundary and the import crosses it, which is a kind of \
              architectural drift a reader cannot infer from the code",
    },
    MessageChain {
        slug: "message-chain",
        category: Coupling,
        analysis: Semantics,
        metric: "chained member access depth beyond the configured limit",
        fixes: [EncapsulateDelegation, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "the line depends on the shape of every object along the chain, so a change to any \
              of them breaks a caller that does not mention it",
    },
    FeatureEnvy {
        slug: "feature-envy",
        category: Coupling,
        analysis: Semantics,
        metric: "method touching another object's members more than its own",
        fixes: [MoveMethod, ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "the method belongs on the other class, and while it stays here both classes have to \
              change together to keep it working",
    },
    MiddleMan {
        slug: "middle-man",
        category: Coupling,
        analysis: Semantics,
        metric: "class where most methods do nothing but delegate",
        fixes: [InlineDelegation, InlineClass],
        confidence: Heuristic,
        severity: Info,
        why: "every call pays for a hop that adds nothing, and every new method on the delegate \
              needs a matching method here before callers can reach it",
    },

    JournalComment {
        slug: "journal-comment",
        category: Waste,
        analysis: Semantics,
        metric: "a comment carrying an author, a date or a change log",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "version control already records who changed the code and when, and unlike the \
              comment it is not left behind by later edits",
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
        why: "it takes space and attention to repeat what the name already says, and it has to be \
              kept accurate when the name changes",
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
        why: "the name does not say what the function does, so each call site requires reading \
              the definition",
    },
    QueryWithSideEffect {
        slug: "query-with-side-effect",
        category: Safety,
        analysis: Semantics,
        metric: "a get, is, has, should or can function that assigns to a member",
        fixes: [SeparateQueryFromModifier, RenameMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "callers assume a query has no effect, so they call it more than once, reorder \
              calls, and are surprised when the answer changes",
    },

    MisleadingPatternName {
        slug: "misleading-pattern-name",
        category: Naming,
        analysis: Semantics,
        metric: "a class named Factory or Builder with no method that makes or builds",
        fixes: [RenameMethod, ExtractClass],
        confidence: Heuristic,
        severity: Info,
        why: "a pattern name describes an interface, and a reader who trusts it looks for members \
              that do not exist",
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
        why: "a reader who has seen `fetchUser` cannot tell whether `getUser` is the same \
              operation or a different one, so both have to be read before either is used",
        short: "one idea has several names, so a reader cannot tell whether they are one operation",
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
        why: "neither class can be read, tested or moved without the other, so the pair behaves \
              as one class written in two places",
    },
    AlternativeClasses {
        slug: "alternative-classes",
        category: Abstraction,
        analysis: Semantics,
        metric: "two classes with the same method count and arities and no shared name",
        fixes: [RenameMethod, ExtractSuperclass, MoveMethod],
        confidence: Low,
        severity: Info,
        why: "callers have to learn both interfaces to do one job, and a fix applied to one class \
              leaves the other unfixed",
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
        why: "every public member is a commitment to keep working, so the surface determines what \
              the type costs to change",
    },
    ManyExits {
        slug: "many-exits",
        category: Size,
        analysis: Syntax,
        metric: "count of returns, throws and raises in one body, past the guard clauses",
        fixes: [ExtractMethod, DecomposeConditional],
        confidence: Heuristic,
        severity: Warning,
        why: "a reader has to find every exit before they can say what the function returns, and \
              exits deep in the body are the easiest to miss",
    },
    WideSwitch {
        slug: "wide-switch",
        category: Size,
        analysis: Syntax,
        metric: "count of arms in one switch or match",
        fixes: [ReplaceConditionalWithPolymorphism, ExtractMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "past a certain width the arms are no longer a table a reader scans, and become a \
              list the reader has to search",
    },
    CallbackNesting {
        slug: "callback-nesting",
        category: Size,
        analysis: Syntax,
        metric: "depth of function literals declared inside other function literals",
        fixes: [ExtractMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "the steps run in the reverse of the order in which they are written, so following \
              the sequence means reading from the inside out",
    },
    AssignmentInCondition {
        slug: "assignment-in-condition",
        category: Safety,
        analysis: Syntax,
        metric: "an assignment written as the whole condition of an `if`",
        fixes: [ExtractVariable],
        confidence: Verified,
        severity: Error,
        why: "it reads as a comparison and behaves as an assignment, so an accidental assignment \
              is easy to miss in review",
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
        why: "the same rule is written twice in two languages, so a change is likely to be made \
              in only one and the two diverge",
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
        why: "the branch above already exits, so the code after the `else` is indented one level \
              deeper than its logic requires",
    },
    CollapsibleIf {
        slug: "collapsible-if",
        category: Flow,
        analysis: Syntax,
        metric: "an `if` that is the only statement inside another, neither with an else",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Warning,
        why: "this is one condition written as two, so a reader has to follow a branch that never \
              branches",
    },
    BooleanFromBranch {
        slug: "boolean-from-branch",
        category: Flow,
        analysis: Syntax,
        metric: "a branch whose arms return opposite boolean literals",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Warning,
        why: "the condition is already the result, so branching on it to produce the same boolean \
              takes four lines to say what one says",
    },
    LiteralComparison {
        slug: "literal-comparison",
        category: Flow,
        analysis: Syntax,
        metric: "an equality comparison against a boolean literal",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Info,
        why: "comparing a boolean to a boolean literal asks a question the value already answers, \
              and suggests the value might be something other than a boolean",
    },
    SelfComparison {
        slug: "self-comparison",
        category: Flow,
        analysis: Syntax,
        metric: "an equality test whose operands are written identically and call nothing",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Error,
        why: "the result is decided before the comparison runs, so either it does nothing or one \
              side was meant to be a different expression",
    },
    IdenticalBranches {
        slug: "identical-branches",
        category: Flow,
        analysis: Syntax,
        metric: "two neighbouring arms of a branch whose bodies are written identically, a \
                 warning unless one is the final `else`",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Error,
        why: "the decision changes nothing, so either it is not a decision or one branch was \
              meant to differ",
    },
    EmptyBlock {
        slug: "empty-block",
        category: Flow,
        analysis: Syntax,
        metric: "a branch or loop body with no statements in it",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "a decision was written and no action was taken, which reads as unfinished work or \
              an accidentally inverted condition",
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
        metric: "identifier matching a fixed list of non-descriptive names",
        fixes: [RenameVariable],
        confidence: Low,
        severity: Info,
        why: "a name that describes nothing requires each reader to work out the value's purpose \
              from its uses",
        short: "a name that describes nothing makes every reader work out the purpose from its uses",
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
        why: "a one-letter name is readable while its whole lifetime fits on screen, and stops \
              being readable when it does not",
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
        why: "the type is already in the declaration and checked by the compiler, so the prefix \
              is a second copy that nothing keeps accurate",
        off_by_default: true,
    },

    // ---- Safety ---------------------------------------------------------------
    ReturnOfFreshLocal {
        slug: "return-of-fresh-local",
        category: Waste,
        analysis: Syntax,
        metric: "a local variable assigned on one line and returned on the next",
        fixes: [InlineVariable],
        confidence: Verified,
        severity: Info,
        why: "the name is read once, by the line below it, and says nothing the expression it \
              holds does not already say",
    },
    DuplicateCondition {
        slug: "duplicate-condition",
        category: Flow,
        analysis: Syntax,
        metric: "the same condition tested twice in one chain of branches",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Error,
        why: "the second branch cannot be reached, so whatever it was written to do has never run",
    },
    LoopConcatenation {
        slug: "loop-concatenation",
        category: Performance,
        analysis: Syntax,
        metric: "text built one piece at a time inside a loop",
        fixes: [AccumulateInPlace],
        confidence: Heuristic,
        severity: Warning,
        why: "each pass copies everything written so far, so the work grows with the square of \
              the number of pieces",
    },
    AbstractNothing {
        slug: "abstract-nothing",
        category: Abstraction,
        analysis: Syntax,
        metric: "a type declared abstract with nothing left for anyone to fill in",
        fixes: [CollapseHierarchy, InlineClass],
        confidence: Verified,
        severity: Info,
        why: "the `abstract` keyword implies that subclasses have work to do, but there is none, \
              so the only effect is to prevent construction",
        not_applicable: [
            (Go, "there is no inheritance and no abstract keyword"),
            (Cpp, "abstractness is a member with no body, which the type never declares"),
            (Python, "abstractness is a base class it inherits, not a word on the type"),
            (JavaScript, "JavaScript has no abstract keyword to leave a member for a subclass"),
        ],
    },
    SilentFallthrough {
        slug: "silent-fallthrough",
        category: Flow,
        analysis: Syntax,
        metric: "a switch arm that does work and then continues into the next one",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Error,
        why: "the arm below also runs, and nothing here says whether that was intended",
        short: "the arm below also runs, and nothing says whether that was meant",
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
        why: "two conditions folded into one expression have to be unfolded by the reader before \
              either can be checked",
        short: "two conditions in one expression have to be unfolded before either can be checked",
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
        why: "the language defines which operator binds tighter but the reader has to remember \
              it, and parentheses remove the question at no cost",
    },
    NegatedComparison {
        slug: "negated-comparison",
        category: Flow,
        analysis: Syntax,
        metric: "a negation wrapped around an equality test",
        fixes: [ConsolidateConditional],
        confidence: Verified,
        severity: Info,
        why: "the language already has an operator for this comparison, and the shorter form can \
              be read without inverting anything",
    },
    NoDefaultBranch {
        slug: "no-default-branch",
        category: Flow,
        analysis: Syntax,
        metric: "a switch with enough arms to count as one and no arm for the values \
                 it does not name",
        fixes: [IntroduceAssertion, ReplaceConditionalWithPolymorphism],
        confidence: Heuristic,
        severity: Warning,
        why: "a value the switch does not anticipate passes through it unhandled, and nothing in \
              the code says whether that is intended",
    },
    LoopRunsOnce {
        slug: "loop-runs-once",
        category: Flow,
        analysis: Syntax,
        metric: "a loop whose body always leaves before a second iteration",
        fixes: [RemoveDeadCode, ReplaceNestedConditionalWithGuardClauses],
        confidence: Verified,
        severity: Warning,
        why: "it is written as repetition and runs once, so either the exit is misplaced or the \
              loop is a lookup written as a loop",
    },
    RedundantJump {
        slug: "redundant-jump",
        category: Flow,
        analysis: Syntax,
        metric: "a continue or break as the last statement of a loop body",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "control reaches that point anyway, so the keyword makes a reader look for a \
              condition that does not exist",
        autofix: Deletion,
    },
    TinySwitch {
        slug: "tiny-switch",
        category: Flow,
        analysis: Syntax,
        metric: "arms in a switch, below the count that justifies a switch",
        fixes: [ConsolidateConditional],
        confidence: Heuristic,
        severity: Info,
        why: "a switch is a table a reader scans, and a switch with two arms is an `if` written \
              in four more lines",
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
        why: "a caller receives no result and cannot distinguish it from work that succeeded, and \
              a reader cannot tell whether the method is a stub or a decision",
    },
    ClosureOverLoopVariable {
        slug: "closure-over-loop-variable",
        category: Safety,
        analysis: Syntax,
        metric: "a function written inside a loop that reads the name the loop moves",
        fixes: [ExtractMethod, AddParameter],
        confidence: Heuristic,
        severity: Warning,
        why: "the closure reads whichever value the loop variable holds when it runs, so every \
              closure sees the last element instead of its own",
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
        why: "the iteration and the mutation disagree about what remains, so elements are skipped \
              or visited twice depending on where the change lands",
    },
    SequentialAwait {
        slug: "sequential-await",
        category: Performance,
        analysis: Syntax,
        metric: "two waits in a row where the second does not read the first's answer",
        fixes: [AwaitConcurrently],
        confidence: Heuristic,
        severity: Warning,
        why: "nothing orders them, so the call takes the sum of two waits where it could take the \
              longer one",
        short: "nothing orders the two waits, so the call takes their sum",
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
        fixes: [ReplaceExitWithError],
        confidence: Heuristic,
        severity: Warning,
        why: "the language may compile the assertion out, so the check a reader relies on may not \
              run in production",
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
        why: "it fails for reasons unrelated to the code under test, and it is slow enough that \
              it tends to be skipped",
        off_by_default: true,
    },
    SleepInProduction {
        slug: "sleep-in-production",
        category: Safety,
        analysis: Syntax,
        metric: "a call that stops the thread for a fixed time, outside a test",
        fixes: [ReplaceBlockingCall],
        confidence: Heuristic,
        severity: Warning,
        why: "it waits for a fixed length of time instead of for the condition it needs, so the \
              delay is either too short to work or longer than necessary",
    },
    UnlabelledAssertions {
        slug: "unlabelled-assertions",
        category: Tests,
        analysis: Syntax,
        metric: "a run of checks in one test with nothing said about any of them",
        fixes: [ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "a failing run prints two values, and the reader has to count lines to find which \
              assertion they came from",
        off_by_default: true,
    },
    SelfAssertingTest {
        slug: "self-asserting-test",
        category: Tests,
        analysis: Syntax,
        metric: "a check with the same expression on both sides",
        fixes: [IntroduceAssertion],
        confidence: Heuristic,
        severity: Error,
        why: "it holds the code to whatever the code already does, so it keeps passing through \
              every change, including a change that breaks the behaviour",
    },
    UnexplainedExpectedValue {
        slug: "unexplained-expected-value",
        category: Tests,
        analysis: Syntax,
        metric: "a number a check expects with nothing saying where it came from",
        fixes: [ReplaceMagicNumberWithConstant],
        confidence: Heuristic,
        severity: Info,
        why: "when it changes, nobody can tell whether the code or the number is wrong, so the \
              test tends to be edited until it passes",
        off_by_default: true,
    },
    GlobalAccessor {
        slug: "global-accessor",
        category: Coupling,
        analysis: Syntax,
        metric: "a function whose whole body hands back something the file keeps at \
                 module level",
        fixes: [EncapsulateField, InvertDependency],
        confidence: Verified,
        severity: Info,
        why: "it is a shared value behind a function call, and every caller still reads the same \
              single instance",
    },
    BarrelModule {
        slug: "barrel-module",
        category: Waste,
        analysis: Semantics,
        metric: "a file whose every import comes straight back out of it",
        fixes: [InlineDelegation, RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "it adds a name to every path through the project and answers no question of its \
              own, while hiding from tools which file a name comes from",
        not_applicable: [
            (Go, "a package is the unit of import, and it has no index file"),
            (Java, "a class is imported by its own name"),
            (Cpp, "a header is included by its own path"),
            (Python, "a package re-exports through `__all__`, which names rather than forwards"),
        ],
    },
    PassthroughOverride {
        slug: "passthrough-override",
        category: Waste,
        analysis: Syntax,
        metric: "a member that overrides something and then calls it unchanged",
        fixes: [RemoveDeadCode, CollapseHierarchy],
        confidence: Verified,
        severity: Info,
        why: "it changes nothing and costs a reader a hop, and it suggests the type has its own \
              behaviour for the member when it does not",
        not_applicable: [
            (Go, "there is no inheritance to override"),
            (Cpp, "a call to the base is written with the type's own name, which one \
                   file cannot resolve"),
        ],
    },
    DuplicateFunction {
        slug: "duplicate-function",
        category: Waste,
        analysis: Syntax,
        metric: "two functions in one file written the same way",
        fixes: [ExtractMethod, RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "one was copied from the other, and the next change to either has to find both or \
              leave them inconsistent",
    },
    ShadowedHandler {
        slug: "shadowed-handler",
        category: Safety,
        analysis: Syntax,
        metric: "a handler written before one that takes only failures it already takes",
        fixes: [NarrowTheHandler],
        confidence: Verified,
        severity: Error,
        why: "the handler below can never run, so whatever it was written to do about that \
              failure has never been done",
        not_applicable: [
            (Go, "errors are values, so there is no handler to order"),
            (Java, "the compiler rejects a handler that an earlier one already covers"),
            (TypeScript, "the language allows one handler per block"),
            (JavaScript, "the language allows one handler per block"),
        ],
    },
    NestedThen {
        slug: "nested-then",
        category: Flow,
        analysis: Syntax,
        metric: "a callback that starts another wait inside itself",
        fixes: [ExtractMethod, ReplaceAlgorithm],
        confidence: Verified,
        severity: Warning,
        why: "the order is real but is not written as an order, so a reader follows it by tracing \
              which callback holds which",
        not_applicable: [
            (Go, "waiting is a channel receive, not a callback"),
            (Python, "waiting is written as a statement"),
            (Cpp, "waiting is a call that returns"),
        ],
    },
    EmptyTest {
        slug: "empty-test",
        category: Tests,
        analysis: Syntax,
        metric: "a test with an empty body",
        fixes: [RemoveDeadCode, IntroduceAssertion],
        confidence: Verified,
        severity: Warning,
        why: "it passes on every run and checks nothing, while counting towards the test total",
    },
    SplitCohesion {
        slug: "split-cohesion",
        category: Abstraction,
        analysis: Syntax,
        metric: "groups of methods inside one type that share no state with each other",
        fixes: [ExtractClass, MoveMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "each group of fields is a type that has not been separated, so a change to one is \
              read against the fields of all of them",
        not_applicable: [
            (Go, "methods are attached to a type from outside it"),
            (Python, "an attribute is written into the instance, not declared on the type"),
        ],
    },
    MethodIgnoresInstance {
        slug: "method-ignores-instance",
        category: Abstraction,
        analysis: Syntax,
        metric: "a method that never mentions the instance it hangs off",
        fixes: [MoveMethod],
        confidence: Heuristic,
        severity: Info,
        why: "it needs nothing the type holds, so the type acts as a namespace for it and every \
              caller has to construct an instance to reach it",
        off_by_default: true,
        not_applicable: [(Go, "a method names its receiver, and one that does not is a function")],
    },
    DroppedPromise {
        slug: "dropped-promise",
        category: Safety,
        analysis: Syntax,
        metric: "an asynchronous call whose result is discarded",
        fixes: [HandleTheFailure],
        confidence: Heuristic,
        severity: Error,
        why: "nothing waits for it and nothing observes its failure, so a failure surfaces \
              elsewhere with no link to what started it",
        not_applicable: [
            (Go, "work started in the background is a goroutine, which the language marks"),
            (Java, "a future is a value, and dropping it is dropping a value"),
            (Cpp, "a future is a value, and dropping it is dropping a value"),
        ],
    },
    EagerTest {
        slug: "eager-test",
        category: Tests,
        analysis: Syntax,
        metric: "distinct units one test exercises",
        fixes: [ExtractMethod],
        confidence: Heuristic,
        severity: Info,
        why: "it fails if any of the assertions fail and the run reports only that the test \
              failed, so the test name does not identify the cause",
        off_by_default: true,
    },
    StringShapeAssertion {
        slug: "string-shape-assertion",
        category: Tests,
        analysis: Syntax,
        metric: "a check against a serialised shape written out as a string",
        fixes: [IntroduceAssertion],
        confidence: Heuristic,
        severity: Info,
        why: "it checks how the result prints and not what it is, so a change in spacing fails a \
              test about behaviour",
    },
    DeadStore {
        slug: "dead-store",
        category: Waste,
        analysis: Syntax,
        metric: "a value assigned twice with no read of the first",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "the first write has no effect, so either a line that should read it is missing or \
              the line that wrote it is unnecessary",
    },
    NeverReassigned {
        slug: "never-reassigned",
        category: Waste,
        analysis: Syntax,
        metric: "a name declared as changeable that never changes",
        fixes: [DeclareImmutable],
        confidence: Heuristic,
        severity: Info,
        why: "a reader has to allow for the possibility that the variable changes, and it never \
              does, so the declaration draws attention it does not need",
        not_applicable: [
            (Python, "the language has no word for a name that cannot change"),
            (Go, "a short declaration says nothing about whether the name moves"),
        ],
    },
    AssignOnceField {
        slug: "assign-once-field",
        category: Abstraction,
        analysis: Syntax,
        metric: "a changeable field only the constructor ever writes",
        fixes: [DeclareImmutable],
        confidence: Heuristic,
        severity: Info,
        why: "nothing after construction changes it, and every reader has to verify that for \
              themselves before relying on it",
        off_by_default: true,
        not_applicable: [
            (Python, "an attribute is written into the instance, not declared on the type"),
            (Go, "a struct field is written by whatever holds the struct"),
        ],
    },
    UnusedFixture {
        slug: "unused-fixture",
        category: Tests,
        analysis: Syntax,
        metric: "a value a setup hook prepares that no test reads",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "every case in the file pays for it and none uses it, which adds time to every run \
              and leaves a reader wondering what it is for",
    },
    UncheckedStub {
        slug: "unchecked-stub",
        category: Tests,
        analysis: Syntax,
        metric: "a test double that is never checked for the calls it received",
        fixes: [IntroduceAssertion],
        confidence: Heuristic,
        severity: Info,
        why: "it let the test run and checked nothing, so the call it replaces could stop \
              happening and the test would still pass",
    },
    BranchingTest {
        slug: "branching-test",
        category: Tests,
        analysis: Syntax,
        metric: "a decision inside a test body",
        fixes: [ExtractMethod, DecomposeConditional],
        confidence: Verified,
        severity: Info,
        why: "a test that branches has paths of its own and nothing says which one ran, so a \
              passing result no longer shows that the case was checked",
    },
    DuplicateAssertion {
        slug: "duplicate-assertion",
        category: Tests,
        analysis: Syntax,
        metric: "the same assertion written twice in one test",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "the second assertion cannot fail while the first passes, so it costs the reader a \
              check they have already read",
    },
    HandRolledThrows {
        slug: "hand-rolled-throws",
        category: Tests,
        analysis: Syntax,
        metric: "a test asserting a failure with a handler of its own",
        fixes: [IntroduceAssertion],
        confidence: Heuristic,
        severity: Warning,
        why: "when the call stops failing, the handler never runs and the test still passes, \
              which is the outcome the test was written to catch",
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
        why: "it was added to observe the test once, and it now prints on every run",
    },
    NegatedName {
        slug: "negated-name",
        category: Naming,
        analysis: Syntax,
        metric: "a name that states what a thing is not",
        fixes: [RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "every test of the name that is itself negated has to be inverted twice by the \
              reader before the condition has a meaning",
        short: "a condition on a negated name has to be inverted twice before it has a meaning",
    },
    ShadowsBuiltin {
        slug: "shadows-builtin",
        category: Naming,
        analysis: Syntax,
        metric: "a declared name the language or its standard library already owns",
        fixes: [RenameVariable],
        confidence: Verified,
        severity: Warning,
        why: "the original is unreachable for the rest of the scope, and a later line that \
              expects it fails in a place that names neither",
    },
    CommentPerLine {
        slug: "comment-per-line",
        category: Noise,
        analysis: Syntax,
        metric: "a run of statements each carrying a comment of its own",
        fixes: [RemoveDeadCode, RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "the code is being read aloud and not explained, so the reader has two things to \
              keep in step and only one of them is checked",
    },
    ExplanationLongerThanCode {
        slug: "explanation-longer-than-code",
        category: Noise,
        analysis: Syntax,
        metric: "lines of comment above a function against statements inside it",
        fixes: [ExtractMethod, RenameMethod],
        confidence: Heuristic,
        severity: Info,
        why: "a body that needs more prose than code has names that do not carry its meaning, and \
              the prose is what becomes outdated",
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
        why: "it repeats the signature, so it costs a reader a second pass and becomes wrong when \
              the signature changes",
        short: "it repeats the signature, so it becomes wrong when the signature changes",
    },
    GuardCascade {
        slug: "guard-cascade",
        category: Safety,
        analysis: Syntax,
        metric: "checks at the top of a body that all answer the same way",
        fixes: [IntroduceAssertion, IntroduceParameterObject],
        confidence: Heuristic,
        severity: Info,
        why: "the body has not started and the reader has already read several decisions, none of \
              which says what the function is for",
        short: "the reader meets several decisions before the body says what the function is for",
    },
    BlanketTry {
        slug: "blanket-try",
        category: Safety,
        analysis: Syntax,
        metric: "a guarded block holding nearly every statement of its function",
        fixes: [ExtractMethod, NarrowTheHandler],
        confidence: Heuristic,
        severity: Warning,
        why: "the handler wraps the whole body instead of answering one failure, so a mistake \
              anywhere inside arrives as the failure the handler was written for",
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
        why: "it is the number of tests needed to cover the body, and past a point those tests \
              are not written, so most paths have never run",
    },
    ManyResults {
        slug: "many-results",
        category: Size,
        analysis: Syntax,
        metric: "values handed back by one return",
        fixes: [IntroduceParameterObject, ExtractClass],
        confidence: Heuristic,
        severity: Info,
        why: "the caller has to know the order to read them, and nothing at either end says what \
              each one is",
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
        fixes: [ReplaceNestedConditionalWithGuardClauses],
        confidence: Verified,
        severity: Error,
        why: "nothing can stop the recursion, so the first call exhausts the stack",
    },
    BooleanNotAQuestion {
        slug: "boolean-not-a-question",
        category: Naming,
        analysis: Syntax,
        metric: "a boolean-valued member whose name is not phrased as a question",
        fixes: [RenameMethod],
        confidence: Heuristic,
        severity: Info,
        why: "a reader has to open the definition to learn which way round the answer runs, and a \
              name phrased as a question says it in one word",
        not_applicable: [(JavaScript, "a function declares no return type, so nothing says it answers true or false")],
    },
    UnnamedTest {
        slug: "unnamed-test",
        category: Tests,
        analysis: Syntax,
        metric: "a test whose name says nothing about what it checks",
        fixes: [RenameMethod],
        confidence: Verified,
        severity: Info,
        why: "a failing run reports the test name and nothing else, so the name is all the reader \
              has before opening the file",
    },
    NameRepeatsType {
        slug: "name-repeats-type",
        category: Naming,
        analysis: Syntax,
        metric: "a name ending in the word for what it holds",
        fixes: [RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "the adjacent declaration already says what it is, so the word is read twice and \
              becomes wrong when the type changes",
    },
    PluralMismatch {
        slug: "plural-mismatch",
        category: Naming,
        analysis: Syntax,
        metric: "a name whose number disagrees with what it was given",
        fixes: [RenameVariable],
        confidence: Heuristic,
        severity: Info,
        why: "a reader relies on the name to say whether there is one value or many, and here it \
              says the opposite",
    },
    SingleMethodField {
        slug: "single-method-field",
        category: Abstraction,
        analysis: Syntax,
        metric: "a field only one method of its type ever reads",
        fixes: [ExtractClass, MoveMethod],
        confidence: Heuristic,
        severity: Info,
        why: "it is that method's local variable kept where every other method can reach it, so \
              every method has to be read before it can be changed",
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
        metric: "other types one type references",
        fixes: [ExtractClass, InvertDependency],
        confidence: Heuristic,
        severity: Warning,
        why: "each collaborator is a reason for this type to change, and a test of it has to set \
              up all of them first",
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
        why: "the body says more about what it is doing than it does, and the reader has to \
              separate the work from the commentary on every line",
    },
    DeepInheritance {
        slug: "deep-inheritance",
        category: Abstraction,
        analysis: Modules,
        metric: "types above one type in its own hierarchy",
        fixes: [ReplaceInheritanceWithComposition, CollapseHierarchy],
        confidence: Verified,
        severity: Warning,
        why: "answering what one method does requires reading every type above it, and a change \
              to any of them reaches every type below",
        not_applicable: [(Go, "there is no inheritance to climb")],
    },
    StaleTodo {
        slug: "stale-todo",
        category: Change,
        analysis: Syntax,
        metric: "days since the line holding a note about work still to do was last \
                 written",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "a note that has not been touched for this long is not a plan, and every reader \
              since has had to decide whether it still applies",
    },
    OrphanTodo {
        slug: "orphan-todo",
        category: Change,
        analysis: Syntax,
        metric: "a note about work still to do with nobody attached to it",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Info,
        why: "there is no ticket to close and no one to ask, so it stays until someone deletes it \
              without knowing what it was for",
    },
    IntegerDivision {
        slug: "integer-division",
        category: Safety,
        analysis: Syntax,
        metric: "one whole number divided by another",
        fixes: [ReplaceAlgorithm],
        confidence: Heuristic,
        severity: Warning,
        why: "the remainder is discarded and nothing on the line says that was intended, so a \
              reader has to know the operand types to know whether the result is right",
    },
    DiscardedError {
        slug: "discarded-error",
        category: Safety,
        analysis: Syntax,
        metric: "a call whose answer is assigned to a name that means \"ignore this\"",
        fixes: [HandleTheFailure],
        confidence: Verified,
        severity: Warning,
        why: "the code receives the result and ignores it, and nothing records what happens when \
              the call fails",
    },
    ManyLocals {
        slug: "many-locals",
        category: Size,
        analysis: Syntax,
        metric: "distinct names one function body introduces",
        fixes: [ExtractMethod, InlineVariable],
        confidence: Heuristic,
        severity: Info,
        why: "each local variable is something the reader has to keep in mind to the end of the \
              body, and there is a number past which readers cannot",
    },
    WideTry {
        slug: "wide-try",
        category: Size,
        analysis: Syntax,
        metric: "statements inside one guarded block",
        fixes: [ExtractMethod, NarrowTheHandler],
        confidence: Heuristic,
        severity: Warning,
        why: "the handler is written for one of the calls and catches failures from all of them, \
              so a failure from any other line reaches a handler that cannot answer it",
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
        why: "it is a second name for the class it extends, and a reader has to open it to find \
              that out",
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
        why: "nothing is constructed from it, so the type is a namespace written as a class, and \
              nothing can be substituted for it in a test",
        not_applicable: [(Go, "there are no classes to hold the methods")],
    },
    WorkingConstructor {
        slug: "working-constructor",
        category: Abstraction,
        analysis: Syntax,
        metric: "calls made while an object is being built, beyond recording what it \
                 was given",
        fixes: [ExtractMethod],
        confidence: Heuristic,
        severity: Warning,
        why: "the object cannot exist without whatever that work needs, so every test of it has \
              to set the work up first",
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
        why: "a file this long is several files that have not been separated, and all of them are \
              edited, reviewed and merged as one",
    },
    DiscardedExpression {
        slug: "discarded-expression",
        category: Waste,
        analysis: Syntax,
        metric: "a statement that computes a value and does nothing with it",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "either the line was meant to do something and does not, or it does nothing and \
              should be removed, and a reader cannot tell which",
        short: "either the line was meant to do something and does not, or it should go",
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
        fixes: [ReplaceAlgorithm],
        confidence: Verified,
        severity: Warning,
        why: "a binary float rarely equals the decimal a person wrote, so a branch that depends \
              on it is decided by rounding and not by the value",
    },
    DeepRelativeImport {
        slug: "deep-relative-import",
        category: Coupling,
        analysis: Semantics,
        metric: "levels a relative import climbs before it descends",
        fixes: [ExtractSharedModule, InvertDependency],
        confidence: Heuristic,
        severity: Info,
        why: "a path that climbs this far reaches across the project's structure instead of \
              through it, and it breaks when either end moves",
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
        fixes: [ExtractMethod, ReplaceDataValueWithObject],
        confidence: Verified,
        severity: Warning,
        why: "a caller cannot write one line that handles both kinds of result, so every call \
              site carries a check for the case the author happened to consider",
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
        metric: "a return, or a break or continue leaving the clause that runs on the way \
                 out, or a throw from it outside Python",
        fixes: [HandleTheFailure],
        confidence: Verified,
        severity: Error,
        why: "it replaces whatever the block was leaving with, so a failure raised in the body \
              disappears and nothing records that it happened",
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
        fixes: [NarrowTheHandler],
        confidence: Verified,
        severity: Warning,
        why: "it catches the failures it was written for and every other failure too, including \
              failures that mean the program is already in an invalid state",
        short: "it catches the failures it was written for and every other one too",
        not_applicable: [
            (Go, "errors are values, so there is no handler to widen"),
            (JavaScript, "a catch clause cannot name a type in JavaScript, so none can name the root of the hierarchy"),
        ],
    },
    GenericThrow {
        slug: "generic-throw",
        category: Safety,
        analysis: Syntax,
        metric: "raising the root error type rather than one that names the failure",
        fixes: [ReplaceDataValueWithObject],
        confidence: Verified,
        severity: Info,
        why: "the only way to react to it is to catch every failure, so a caller that wants to \
              handle this one cannot",
        not_applicable: [(Go, "errors are values, and the language raises nothing")],
    },
    LibraryExit {
        slug: "library-exit",
        category: Safety,
        analysis: Syntax,
        metric: "a call that ends the process, written outside the entry point",
        fixes: [ReplaceExitWithError],
        confidence: Heuristic,
        severity: Warning,
        why: "it takes the decision away from every caller above it, and a test that reaches the \
              line terminates the test runner",
    },
    BooleanLiteralArgument {
        slug: "boolean-literal-argument",
        category: Safety,
        analysis: Syntax,
        metric: "a bare true or false among a call's arguments",
        fixes: [ReplaceParameterWithExplicitMethods],
        confidence: Heuristic,
        severity: Info,
        why: "nothing at the call site says what the argument turns on, so the reader has to open \
              the callee to find out",
    },
    UnusedImport {
        slug: "unused-import",
        category: Waste,
        analysis: Semantics,
        metric: "a name imported into a file and never read there",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "it makes the file appear to depend on something it does not use, and the dependency \
              survives every search for who needs that module",
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
        why: "every function that reads it takes an argument that no caller passed, so two calls \
              with the same arguments can return different results and no signature says why",
    },
    AmbientDependency {
        slug: "ambient-dependency",
        category: Coupling,
        analysis: Syntax,
        metric: "a body reaching for the clock, the environment or a random source",
        fixes: [AddParameter, InvertDependency],
        confidence: Heuristic,
        severity: Info,
        why: "the result depends on something no caller can set, so the only way to test the \
              branch that reads it is to change the machine",
    },
    SelfBuiltCollaborators {
        slug: "self-built-collaborators",
        category: Coupling,
        analysis: Syntax,
        metric: "a field filled with something the object constructed itself",
        fixes: [InvertDependency, AddParameter],
        confidence: Verified,
        severity: Warning,
        why: "nothing can be substituted for them, so every test of the owner has to set up the \
              real collaborator and everything behind it",
        not_applicable: [(Go, "a value is built by a literal, which names no type to \
                              stand in for")],
    },
    ImportTimeWork {
        slug: "import-time-work",
        category: Coupling,
        analysis: Syntax,
        metric: "a call that runs the moment the file is loaded",
        fixes: [ExtractMethod],
        confidence: Verified,
        severity: Warning,
        why: "it runs before any caller has asked for anything, in whatever order the imports \
              resolve, and nothing can prevent it",
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
        metric: "reading a member another type marks as private",
        fixes: [EncapsulateDelegation, MoveMethod],
        confidence: Verified,
        severity: Warning,
        why: "the other type is free to change the member and has declared it private, so this \
              line breaks on a change nobody could have known would reach it",
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
        why: "nothing in the file says which names come from where, so a reader has to open the \
              other module to find out, and a new export there can shadow a local name without \
              any warning",
    },
    RethrowOnlyCatch {
        slug: "rethrow-only-catch",
        category: Waste,
        analysis: Syntax,
        metric: "a handler whose only statement raises again",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "it reads as a decision about the failure and makes none, and in languages that \
              rebuild the stack trace on rethrow it loses the original",
        not_applicable: [(Go, "errors are values, so there is no handler to empty")],
    },
    PlaceholderBody {
        slug: "placeholder-body",
        category: Waste,
        analysis: Syntax,
        metric: "a body whose only statement raises to signal that the implementation is not written",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Warning,
        why: "it is a callable promise with nothing behind it, and every caller that trusts the \
              signature finds out at run time",
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
        why: "it adds a name and a stack frame between a caller and the function it wanted, and \
              the indirection has no effect because it forwards unchanged",
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
        why: "it was written to be read once by the person debugging, and it still prints for \
              everyone else",
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
        why: "two statements importing one module mean a reader has to check both to know what \
              the file takes from it, and a merge can introduce one without anyone reviewing it",
        short: "two imports of one module mean a reader has to check both",
    },
    WideFanOut {
        slug: "wide-fan-out",
        category: Coupling,
        analysis: Modules,
        metric: "modules a single file imports",
        fixes: [ExtractSharedModule, InvertDependency],
        confidence: Heuristic,
        severity: Info,
        why: "a file that reaches into this many places cannot be read, moved or tested without \
              most of the project coming with it",
        short: "a file that reaches into this many places drags most of the project with it",
    },
    WideFanIn {
        slug: "wide-fan-in",
        category: Coupling,
        analysis: Modules,
        metric: "modules that import a single file",
        fixes: [ExtractInterface],
        confidence: Heuristic,
        severity: Info,
        why: "every change here reaches this many places, so the module is heavily depended on \
              whether or not that was intended",
    },
    BannerComment {
        slug: "banner-comment",
        category: Noise,
        analysis: Semantics,
        metric: "a comment that is a divider or a one-word heading",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "it is a table of contents in a file short enough not to need one, and it becomes \
              outdated when a section moves",
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
        why: "it is left over from a conversation that ended before the commit, and it tells the \
              next reader nothing about the code it sits on",
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
        why: "someone knew this was wrong and left it, so the comment records the decision \
              without the reason or the ticket that would allow it to be reversed",
    },
    EmojiInSource {
        slug: "emoji-in-source",
        category: Noise,
        analysis: Semantics,
        metric: "an emoji in a comment outside a test",
        fixes: [RemoveDeadCode],
        confidence: Heuristic,
        severity: Info,
        why: "emoji display inconsistently in grep results, diffs and terminals, and they add \
              decoration to source code",
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
        why: "a placeholder that looks real is a value that can ship, and the first person to \
              notice is whoever receives it",
    },
    UnjustifiedSuppression {
        slug: "unjustified-suppression",
        category: Safety,
        analysis: Syntax,
        metric: "an rqx-ignore comment with no reason, or one whose date has passed",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "the waiver is refused and the finding still counts, because a waiver that needs no \
              justification allows a rule to be switched off one line at a time",
    },
    SuppressedDiagnostic {
        slug: "suppressed-diagnostic",
        category: Safety,
        analysis: Semantics,
        metric: "ts-ignore, ts-nocheck, eslint-disable, or a cast through any",
        fixes: [RemoveDeadCode],
        confidence: Verified,
        severity: Warning,
        why: "a check that would have caught the next mistake here has been turned off, and the \
              comment that turns it off outlives the reason for it",
    },
    FlagArgument {
        slug: "flag-argument",
        category: Safety,
        analysis: Semantics,
        metric: "parameter declared with a boolean type",
        fixes: [ReplaceParameterWithExplicitMethods],
        confidence: Heuristic,
        severity: Info,
        why: "the function does two things and the caller selects which, so `f(x, true)` tells a \
              reader nothing about what will happen",
        not_applicable: [(JavaScript, "a parameter declares no type, so nothing says it is a boolean")],
    },
    OutputParameter {
        slug: "output-parameter",
        category: Safety,
        analysis: Semantics,
        metric: "parameter reassigned or mutated inside the function body",
        fixes: [RemoveAssignmentsToParameters, SeparateQueryFromModifier],
        confidence: Verified,
        severity: Warning,
        why: "the caller's value changes without the call site saying so, which is hard to find \
              when it turns out to be wrong",
    },
    ComplexCondition {
        slug: "complex-condition",
        category: Safety,
        analysis: Semantics,
        metric: "boolean operands in a single condition beyond the configured limit",
        fixes: [ConsolidateConditional, ExtractVariable],
        confidence: Heuristic,
        severity: Info,
        why: "readers cannot hold this many clauses at once, so the branch is taken on trust and \
              a bug in it survives review",
    },
    NegativeConditional {
        slug: "negative-conditional",
        category: Safety,
        analysis: Semantics,
        metric: "negated condition that has an else branch",
        fixes: [DecomposeConditional],
        confidence: Heuristic,
        severity: Info,
        why: "the reader has to invert the condition mentally to follow the `else`, and inverting \
              a compound condition by hand is a source of off-by-one logic errors",
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
        why: "a skipped test reports success while the behaviour it names goes unchecked",
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
        why: "this silently disables every other test in the file, so the suite passes while \
              almost nothing runs",
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
        why: "it passes as long as nothing throws, so it reports success for behaviour it never \
              checked",
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
        why: "the inner scan runs once per pass of the outer loop, so the work grows with the \
              product of the two sizes and a list that is fast at a hundred items can stall at \
              ten thousand",
    },
    DeepLoopNesting {
        slug: "deep-loop-nesting",
        category: Performance,
        analysis: Syntax,
        metric: "loop nesting depth, counting callbacks and comprehensions as loops",
        fixes: [ReplaceLoopWithLookup, ReplaceAlgorithm],
        confidence: Heuristic,
        severity: Warning,
        why: "each level multiplies the passes of the one inside it, so the innermost body runs a \
              number of times that tests on small data do not show",
    },
    QueryInLoop {
        slug: "query-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "a call on a database, HTTP or queue client inside a loop",
        fixes: [BatchTheWork],
        confidence: Heuristic,
        severity: Warning,
        why: "every pass pays a round trip, so listing a hundred rows makes a hundred and one \
              requests and the page waits for the sum of them",
    },
    AwaitInLoop {
        slug: "await-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "an await inside a loop body, outside any nested function",
        fixes: [BatchTheWork, AwaitConcurrently],
        confidence: Heuristic,
        severity: Warning,
        why: "each pass waits for the previous one to finish, so work that could overlap runs end \
              to end and the loop takes as long as all of its waits added together",
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
        confidence: Verified,
        severity: Warning,
        why: "each step copies everything gathered so far, so building a result of n items \
              touches about half of n squared items",
    },
    BranchingRecursion {
        slug: "branching-recursion",
        category: Performance,
        analysis: Syntax,
        metric: "a function calling itself two or more times with no memo in sight",
        fixes: [Memoize, ReplaceAlgorithm],
        confidence: Heuristic,
        severity: Warning,
        why: "two calls per level is an exponential number of calls, and most of them recompute \
              an answer another branch already found",
    },
    BlockingCallInAsync {
        slug: "blocking-call-in-async",
        category: Performance,
        analysis: Semantics,
        metric: "a synchronous file, process or sleep call inside an async function",
        fixes: [ReplaceBlockingCall],
        confidence: Heuristic,
        severity: Warning,
        why: "an async function is expected not to hold the thread and this call holds it, so \
              everything else waiting on that event loop queues behind one blocking call",
        not_applicable: [
            (Go, "blocking a goroutine is the intended model, not a mistake to report"),
            (Java, "Java has no async function marker, so there is no promise here to break"),
            (Cpp, "a coroutine is decided by co_return in its body rather than by a marker on \
                   its signature, so there is nothing to test a call against"),
        ],
    },
    LinearOpInLoop {
        slug: "linear-op-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "a library call that walks or shifts its whole collection, on a collection of a \
                 known type, inside a loop that scan-in-loop does not report",
        fixes: [ReplaceLoopWithLookup],
        confidence: Heuristic,
        severity: Warning,
        why: "the call costs time proportional to the size of the collection on every pass, so \
              the loop grows with the product of its own length and the collection's",
    },
    SortInLoop {
        slug: "sort-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "a sort of a collection that the loop does not change, inside a loop that \
                 scan-in-loop does not report",
        fixes: [HoistInvariant],
        confidence: Heuristic,
        severity: Warning,
        why: "every pass after the first sorts input that is already sorted, at a cost of n log n \
              each time",
    },
    AllocationInLoop {
        slug: "allocation-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "three or more allocations, or one fixed allocation of a thousand elements or \
                 more, in each pass of an innermost loop",
        fixes: [HoistInvariant],
        confidence: Heuristic,
        severity: Info,
        why: "each pass pays for memory it could have reused, which adds allocator and \
              collection work to every iteration",
        off_by_default: true,
    },
    MissingCapacity {
        slug: "missing-capacity",
        category: Performance,
        analysis: Semantics,
        metric: "an empty collection appended to in a loop over a collection whose size is known \
                 before the loop starts",
        fixes: [PresizeCollection],
        confidence: Heuristic,
        severity: Info,
        why: "the collection grows by reallocating and copying its contents as it fills, work \
              that a capacity set from the loop's size avoids",
        short: "the collection reallocates and copies its contents as it fills",
        off_by_default: true,
        not_applicable: [
            (JavaScript, "arrays grow without a capacity to set"),
            (TypeScript, "arrays grow without a capacity to set"),
            (Python, "lists grow without a capacity to set"),
        ],
    },
    LockInLoop {
        slug: "lock-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "a lock taken inside a loop on a lock the loop does not change",
        fixes: [HoistInvariant],
        confidence: Heuristic,
        severity: Warning,
        why: "each pass pays to acquire and release the same lock, and other threads can only \
              interleave at those points instead of waiting once for the whole loop",
    },
    UnboundedCache {
        slug: "unbounded-cache",
        category: Performance,
        analysis: Semantics,
        metric: "a shared collection that functions add keyed entries to and nothing removes \
                 from or measures, or an lru_cache with no maximum size",
        fixes: [BoundTheCache],
        confidence: Heuristic,
        severity: Warning,
        why: "it holds an entry for every distinct key the program has ever been given, so \
              memory grows with the variety of the input for as long as the process runs",
    },
    DeferInLoop {
        slug: "defer-in-loop",
        category: Performance,
        analysis: Syntax,
        metric: "a defer statement with a loop between it and its enclosing function",
        fixes: [ExtractMethod],
        confidence: Verified,
        severity: Warning,
        why: "a deferred call runs when the function returns, not when the pass ends, so every \
              pass holds what it opened until the whole loop is done",
        not_applicable: [
            (JavaScript, "JavaScript has no defer statement; cleanup is written in a finally block"),
            (TypeScript, "TypeScript has no defer statement; cleanup is written in a finally block"),
            (Python, "a with block releases what it opens when the block ends, inside the pass"),
            (Java, "try-with-resources releases what it opens when its block ends, inside the pass"),
            (Cpp, "a destructor runs when its scope ends, which is inside the pass"),
        ],
    },
    BoxingInLoop {
        slug: "boxing-in-loop",
        category: Performance,
        analysis: Syntax,
        metric: "a local declared as Integer, Long, Double or another boxed number and changed \
                 with an arithmetic update inside a loop",
        fixes: [UsePrimitiveType],
        confidence: Verified,
        severity: Warning,
        why: "each update unboxes the value and boxes the result as a new object, which the \
              primitive type does in place",
        not_applicable: [
            (JavaScript, "numbers have one representation; there is no boxed form to avoid"),
            (TypeScript, "numbers have one representation; there is no boxed form to avoid"),
            (Python, "every int is an object; there is no primitive form to use instead"),
            (Go, "numeric values are stored directly; nothing is boxed"),
            (Cpp, "numeric values are stored directly; nothing is boxed"),
        ],
    },
    CopyInRangeFor {
        slug: "copy-in-range-for",
        category: Performance,
        analysis: Syntax,
        metric: "a range-for variable declared by value with a string or container type, \
                 that the loop body does not change",
        fixes: [TakeByReference],
        confidence: Verified,
        severity: Warning,
        why: "each pass copies the element into the variable, allocating and copying its \
              contents, when a reference to it would read the same value",
        short: "each pass copies the element where a reference would read the same value",
        not_applicable: [
            (JavaScript, "the loop variable refers to the element; nothing is copied"),
            (TypeScript, "the loop variable refers to the element; nothing is copied"),
            (Python, "the loop variable is a name for the element; nothing is copied"),
            (Go, "range always copies the element and offers no reference form to choose instead"),
            (Java, "the loop variable refers to the element; nothing is copied"),
        ],
    },
    LargeByValue {
        slug: "large-by-value",
        category: Performance,
        analysis: Syntax,
        metric: "a string or container parameter taken by value that the function neither \
                 changes nor passes on with std::move",
        fixes: [TakeByReference],
        confidence: Heuristic,
        severity: Warning,
        why: "every call copies the whole argument for a function that only reads it, and the \
              cost grows with the size of what callers pass",
        not_applicable: [
            (JavaScript, "arguments are references to the value; nothing is copied"),
            (TypeScript, "arguments are references to the value; nothing is copied"),
            (Python, "arguments are references to the value; nothing is copied"),
            (Go, "a slice, map or string argument copies only its small header"),
            (Java, "arguments are references to the object; nothing is copied"),
        ],
    },
    UnboundedConcurrency {
        slug: "unbounded-concurrency",
        category: Performance,
        analysis: Semantics,
        metric: "work that leaves the process started once per element of a collection of any \
                 size, in a function that mentions no limit",
        fixes: [BoundConcurrency],
        confidence: Heuristic,
        severity: Warning,
        why: "the number of requests in flight equals the size of the collection, so a large \
              input exhausts connections, memory or the other side's rate limit",
    },
    QuadraticViaCall {
        slug: "quadratic-via-call",
        category: Performance,
        analysis: Semantics,
        metric: "a loop over a collection around a call whose cost grows with a collection the function was given",
        fixes: [ReplaceLoopWithLookup, ReplaceAlgorithm],
        confidence: Heuristic,
        severity: Warning,
        why: "the callee does work in proportion to a collection on every pass of the loop, so the \
              total grows with the product of the two sizes, and with the square of the size when \
              they are one collection, although the caller shows only one loop",
    },
    DeepRecursion {
        slug: "deep-recursion",
        category: Performance,
        analysis: Semantics,
        metric: "a function that calls itself once, on an argument one smaller than its own",
        fixes: [ReplaceAlgorithm],
        confidence: Verified,
        severity: Warning,
        why: "every level keeps its stack frame until the last one returns, so an input larger \
              than the stack allows ends in a stack overflow that tests on small inputs never \
              reach",
        not_applicable: [
            (Go, "a goroutine's stack grows on demand up to a limit set by the runtime, so depth \
                  is bounded by memory and not by a fixed stack size"),
        ],
    },
    RepeatedComputationInLoop {
        slug: "repeated-computation-in-loop",
        category: Performance,
        analysis: Semantics,
        metric: "a call that costs at least linear time, in a loop that changes nothing the call reads",
        fixes: [HoistInvariant],
        confidence: Heuristic,
        severity: Warning,
        why: "the call returns the same answer on every pass, so the loop pays its cost once per \
              pass where once in total would do",
    },
    PairScanCouldHash {
        slug: "pair-scan-could-hash",
        category: Performance,
        analysis: Semantics,
        metric: "two nested loops over different collections, comparing a field of each element with ==",
        fixes: [ReplaceLoopWithLookup],
        confidence: Heuristic,
        severity: Warning,
        why: "every element of one collection is compared with every element of the other to \
              find its match, which a map keyed on the compared field answers in constant time \
              per element",
        off_by_default: true,
    },
}

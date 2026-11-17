// Nothing here carries a severity: an advisory is not a defect, and the health score
// cannot see it.

advisories! {
    CommandFits {
        slug: "command-fits",
        pattern: Command,
        stance: Applicable,
        analysis: Semantics,
        signature: "a switch or chain whose every branch does nothing but call one function of the same shape",
        techniques: [ReplaceConditionalWithPolymorphism, ReplaceParameterWithExplicitMethods],
        confidence: Heuristic,
        why: "the branches already agree on a shape, so the switch is a lookup table \
              written as control flow, and nothing can queue, log or undo what it picks",
    },
    FacadeFits {
        slug: "facade-fits",
        pattern: Facade,
        stance: Applicable,
        analysis: Modules,
        signature: "three or more first-party files importing the same third-party package directly",
        techniques: [ExtractSharedModule, EncapsulateDelegation],
        confidence: Heuristic,
        why: "every one of those files is written against someone else's API, so the \
              blast radius of that dependency changing is the number of files, not one",
    },
    MediatorFits {
        slug: "mediator-fits",
        pattern: Mediator,
        stance: Applicable,
        analysis: Modules,
        signature: "three or more modules that all reach each other, directly or through the ones between",
        techniques: [ExtractClass, InvertDependency],
        confidence: Heuristic,
        why: "no member of the group can be read, tested or moved without the others, \
              because each one already knows about all of them",
    },
    StrategyFits {
        slug: "strategy-fits",
        pattern: Strategy,
        stance: Applicable,
        analysis: Semantics,
        signature: "the same value switched on in more than one place, where no branch reassigns it",
        techniques: [ReplaceConditionalWithPolymorphism, ReplaceTypeCodeWithPolymorphicState],
        confidence: Heuristic,
        why: "the cases are spelled out in several places, so a new one is found by \
              searching rather than by the compiler refusing to build",
    },
    ChainOfResponsibilityFits {
        slug: "chain-of-responsibility-fits",
        pattern: ChainOfResponsibility,
        stance: Applicable,
        analysis: Semantics,
        signature: "an if/else chain of three or more branches where every branch tests something different",
        techniques: [ReplaceConditionalWithPolymorphism, ExtractClass],
        confidence: Heuristic,
        why: "no single value decides which branch runs, so the order of the tests is \
              the logic, and adding a case means working out where in the chain it goes",
    },
    StateFits {
        slug: "state-fits",
        pattern: State,
        stance: Applicable,
        analysis: Semantics,
        signature: "the same value switched on in more than one place, where a branch reassigns it",
        techniques: [ReplaceTypeCodeWithPolymorphicState, ReplaceConditionalWithPolymorphism],
        confidence: Heuristic,
        why: "the value being switched on is also the value the branches change, so the \
              transitions are scattered across every place that reads it",
    },

    DecoratorFound {
        slug: "decorator-found",
        pattern: Decorator,
        stance: Recognized,
        analysis: Semantics,
        signature: "a class that implements an interface, holds one of the same interface, forwards to it and adds behaviour of its own",
        techniques: [],
        confidence: Verified,
        why: "the class can stand in for what it wraps, so callers of the interface get \
              the extra behaviour without knowing it is there",
        not_applicable: [
            (Go, "a type here declares no interface, so nothing says what it can stand in for"),
            (JavaScript, "JavaScript declares no types, so nothing says what this class can stand in for"),
        ],
    },
    DecoratorNotSubstitutable {
        slug: "decorator-not-substitutable",
        pattern: Decorator,
        stance: Incomplete,
        analysis: Semantics,
        signature: "a class forwarding most of its methods to something it holds while declaring no interface of its own",
        techniques: [ExtractInterface, ReplaceInheritanceWithComposition],
        confidence: Heuristic,
        why: "it wraps the thing but cannot be passed where the thing is expected, so \
              every caller has to know which of the two it is holding",
        not_applicable: [
            (Go, "a type here declares no interface, so nothing says what it can stand in for"),
            (Python, "nothing in the language declares an interface, so wrapping without one is ordinary composition"),
            (JavaScript, "nothing in the language declares an interface, so wrapping without one is ordinary composition"),
        ],
    },
    ProxyFound {
        slug: "proxy-found",
        pattern: Proxy,
        stance: Recognized,
        analysis: Semantics,
        signature: "a class implementing an interface it also holds, where every method forwards and none adds anything",
        techniques: [],
        confidence: Heuristic,
        why: "it controls reaching the thing it stands for without changing what the \
              thing does, which is the whole of what a proxy is",
        not_applicable: [
            (Go, "a type here declares no interface, so nothing says what it can stand in for"),
            (JavaScript, "JavaScript declares no types, so nothing says what this class can stand in for"),
        ],
    },
    AdapterFound {
        slug: "adapter-found",
        pattern: Adapter,
        stance: Recognized,
        analysis: Semantics,
        signature: "a class implementing one interface while forwarding most of its methods to a field of an unrelated type",
        techniques: [],
        confidence: Heuristic,
        why: "it makes something written against one interface usable where another is \
              expected, and the translation lives in one place",
        not_applicable: [
            (Go, "a type here declares no interface, so nothing says what it can stand in for"),
            (JavaScript, "JavaScript declares no types, so nothing says what this class can stand in for"),
        ],
    },
    CompositeFound {
        slug: "composite-found",
        pattern: Composite,
        stance: Recognized,
        analysis: Semantics,
        signature: "a class holding a collection whose element type is the class itself or something it implements",
        techniques: [],
        confidence: Heuristic,
        why: "one of these and a tree of them answer to the same interface, so callers \
              never branch on which they were handed",
        not_applicable: [(JavaScript, "a field declares no type, so nothing says what a class holds")],
    },
    BuilderFound {
        slug: "builder-found",
        pattern: Builder,
        stance: Recognized,
        analysis: Semantics,
        signature: "three or more methods returning the class itself, and one returning something else",
        techniques: [],
        confidence: Verified,
        why: "the object is assembled a step at a time and handed over once, so no \
              caller ever holds a half-built one",
        not_applicable: [(JavaScript, "a method declares no return type, so nothing says it hands back itself or a copy")],
    },
    BuilderWithoutBuild {
        slug: "builder-without-build",
        pattern: Builder,
        stance: Incomplete,
        analysis: Semantics,
        signature: "three or more methods returning the class itself and none returning anything else",
        techniques: [SeparateQueryFromModifier],
        confidence: Heuristic,
        why: "the chain never ends in the thing being built, so callers reach past it \
              for the state it was supposed to hand over",
        not_applicable: [(JavaScript, "a method declares no return type, so nothing says it hands back itself or a copy")],
    },
    PrototypeFound {
        slug: "prototype-found",
        pattern: Prototype,
        stance: Recognized,
        analysis: Semantics,
        signature: "a method named for copying that returns the class it is declared on",
        techniques: [],
        confidence: Heuristic,
        why: "a new one is made from an existing one, so the caller never has to know \
              which concrete class it is holding",
        not_applicable: [(JavaScript, "a method declares no return type, so nothing says it hands back itself or a copy")],
    },
    SingletonFound {
        slug: "singleton-found",
        pattern: Singleton,
        stance: Recognized,
        analysis: Semantics,
        signature: "a non-public constructor beside a static field of the class's own type",
        techniques: [],
        confidence: Verified,
        why: "one instance exists and the constructor enforces it, which is also why \
              nothing that depends on it can be tested against a different one",
        not_applicable: [
            (Go, "there are no constructors to make private; a single instance is a package-level variable"),
            (Python, "nothing in the language stops a second call to the constructor"),
            (JavaScript, "a field declares no type, so nothing says what a class holds"),
        ],
    },
    SingletonEscapable {
        slug: "singleton-escapable",
        pattern: Singleton,
        stance: Incomplete,
        analysis: Semantics,
        signature: "a static field of the class's own type beside a constructor anything can still call",
        techniques: [HideMethod],
        confidence: Heuristic,
        why: "the shared instance is a convention rather than a guarantee, so some code \
              is using the one and some is making its own",
        not_applicable: [
            (Go, "there are no constructors to make private; a single instance is a package-level variable"),
            (Python, "nothing in the language stops a second call to the constructor"),
            (JavaScript, "a field declares no type, so nothing says what a class holds"),
        ],
    },
    FlyweightFound {
        slug: "flyweight-found",
        pattern: Flyweight,
        stance: Recognized,
        analysis: Semantics,
        signature: "a static collection of the class's own type beside a static method that returns one",
        techniques: [],
        confidence: Heuristic,
        why: "instances are handed out from a pool rather than made, so the same value \
              is one object however many places hold it",
        not_applicable: [
            (Go, "a type here has no static members, so a shared pool is a package variable rather than part of the type"),
            (Python, "a class attribute and an instance default are written the same way, so nothing says the pool is shared"),
            (JavaScript, "a field declares no type, so nothing says what a class holds"),
        ],
    },
    IteratorFound {
        slug: "iterator-found",
        pattern: Iterator,
        stance: Recognized,
        analysis: Semantics,
        signature: "a class declaring the language's own iteration members beside the collection it walks",
        techniques: [],
        confidence: Verified,
        why: "callers walk it with the language's own loop and never learn how it is \
              stored, so the storage can change without them",
        not_applicable: [
            (Go, "there is no iteration protocol to implement, so a method named Next is a convention rather than a promise"),
            (JavaScript, "a field declares no type, so nothing says what a class holds"),
        ],
    },
    ObserverFound {
        slug: "observer-found",
        pattern: Observer,
        stance: Recognized,
        analysis: Semantics,
        signature: "a collection of listeners, a method that adds to it, and a method that tells them all",
        techniques: [],
        confidence: Heuristic,
        why: "the thing that changes does not know who cares, so a new dependant is a \
              registration rather than an edit",
        not_applicable: [(JavaScript, "a field declares no type, so nothing says what a class holds")],
    },
    ObserverWithoutRemoval {
        slug: "observer-without-removal",
        pattern: Observer,
        stance: Incomplete,
        analysis: Semantics,
        signature: "a collection of listeners with a way to subscribe and none to stop",
        techniques: [AddParameter, RemoveSetter],
        confidence: Heuristic,
        why: "nothing that subscribes can ever be released, so the collection and \
              everything it points at outlive the objects that registered",
        not_applicable: [(JavaScript, "a field declares no type, so nothing says what a class holds")],
    },
    MementoFound {
        slug: "memento-found",
        pattern: Memento,
        stance: Recognized,
        analysis: Semantics,
        signature: "a method that hands out a snapshot type paired with one that takes the same type back",
        techniques: [],
        confidence: Low,
        why: "state is captured and put back without the caller learning what is in it, \
              so the object keeps its own invariants",
        off_by_default: true,
        not_applicable: [(JavaScript, "a method declares no return type, so nothing says it hands back itself or a copy")],
    },
    TemplateMethodFound {
        slug: "template-method-found",
        pattern: TemplateMethod,
        stance: Recognized,
        analysis: Semantics,
        signature: "a method with a body that calls two or more members the same class leaves abstract",
        techniques: [],
        confidence: Verified,
        why: "the order of the steps is fixed in one place and only the steps vary, so \
              a subclass cannot get the sequence wrong",
        not_applicable: [
            (Go, "there are no abstract members for a base to leave unimplemented"),
            (JavaScript, "JavaScript has no abstract keyword to leave a member for a subclass"),
        ],
    },

    FactoryMethodFound {
        slug: "factory-method-found",
        pattern: FactoryMethod,
        stance: Recognized,
        analysis: Modules,
        signature: "a method returning a type other classes implement, on a class two or more others extend",
        techniques: [],
        confidence: Heuristic,
        why: "which concrete class is made is decided by the subclass rather than by \
              the caller, so a new one is a new subclass and no edit anywhere else",
        not_applicable: [
            (Go, "the language has no inheritance, so nothing can override the decision"),
            (JavaScript, "JavaScript has no abstract keyword to leave a member for a subclass"),
        ],
    },
    AbstractFactoryFound {
        slug: "abstract-factory-found",
        pattern: AbstractFactory,
        stance: Recognized,
        analysis: Modules,
        signature: "a class whose every method returns an abstraction, implemented by two or more others",
        techniques: [],
        confidence: Heuristic,
        why: "the whole family is chosen once, so nothing downstream can mix a part \
              from one family with a part from another",
        not_applicable: [
            (Go, "the language has no inheritance, so nothing can implement the family twice"),
            (JavaScript, "JavaScript has no abstract keyword to leave a member for a subclass"),
        ],
    },
    BridgeFits {
        slug: "bridge-fits",
        pattern: Bridge,
        stance: Applicable,
        analysis: Modules,
        signature: "four or more subclasses whose names cover two independent axes exactly",
        techniques: [ReplaceInheritanceWithComposition, ExtractInterface],
        confidence: Heuristic,
        why: "the two things varying are multiplied into subclasses, so adding one \
              value to either axis means writing a class for every value of the other",
        not_applicable: [(Go, "the language has no inheritance, so there is no hierarchy to split in two")],
    },
    InterpreterFound {
        slug: "interpreter-found",
        pattern: Interpreter,
        stance: Recognized,
        analysis: Modules,
        signature: "three or more subclasses of one base that each answer the same evaluating method",
        techniques: [],
        confidence: Low,
        why: "each rule of the grammar is a class that knows how to evaluate itself, \
              so the tree runs by asking its own nodes",
        off_by_default: true,
        not_applicable: [(Go, "the language has no inheritance, so a grammar has no base to hang its rules from")],
    },
    VisitorFound {
        slug: "visitor-found",
        pattern: Visitor,
        stance: Recognized,
        analysis: Modules,
        signature: "a class declaring three or more visit methods over one hierarchy",
        techniques: [],
        confidence: Verified,
        why: "an operation over the whole hierarchy lives in one class, so adding one \
              touches nothing the hierarchy already does",
    },
}

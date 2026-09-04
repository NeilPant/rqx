use std::fmt;

use crate::advice::AdviceId;
use crate::coverage::{Coverage, Tally};

/// The book's own three groupings. Kept because they are the ones a reader already
/// knows, the same reason `Category` groups the rules.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PatternFamily {
    Creational,
    Structural,
    Behavioural,
}

impl PatternFamily {
    pub const ALL: [Self; 3] = [Self::Creational, Self::Structural, Self::Behavioural];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Creational => "creational",
            Self::Structural => "structural",
            Self::Behavioural => "behavioural",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Creational => "Creational",
            Self::Structural => "Structural",
            Self::Behavioural => "Behavioural",
        }
    }

    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|family| family.slug() == slug)
    }

    pub fn patterns(self) -> impl Iterator<Item = Pattern> {
        Pattern::ALL
            .iter()
            .copied()
            .filter(move |pattern| pattern.family() == self)
    }
}

/// The twenty-three patterns of *Design Patterns*, all of them, each with what Rqx
/// does about it. The catalogue is the claim, and holding
/// it as data is what makes the claim checkable rather than a sentence in a README.
///
/// `intent` is the book's own one-line Intent, so a reader can tell whether the
/// pattern is the one they meant without leaving the terminal.
macro_rules! patterns {
    ($(
        $variant:ident {
            name: $name:literal,
            slug: $slug:literal,
            family: $family:ident,
            section: $section:literal,
            intent: $intent:literal,
            coverage: $coverage:expr
        }
    ),+ $(,)?) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Pattern { $($variant),+ }

        impl Pattern {
            pub const ALL: &'static [Pattern] = &[$(Pattern::$variant),+];

            pub const fn name(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }

            /// Lower-kebab, so `rqx explain chain-of-responsibility` matches how every
            /// other name in the tool is typed.
            pub const fn slug(self) -> &'static str {
                match self { $(Self::$variant => $slug),+ }
            }

            pub const fn family(self) -> PatternFamily {
                match self { $(Self::$variant => PatternFamily::$family),+ }
            }

            /// The section it occupies in the book, so a citation can be looked up.
            pub const fn section(self) -> &'static str {
                match self { $(Self::$variant => $section),+ }
            }

            pub const fn intent(self) -> &'static str {
                match self { $(Self::$variant => $intent),+ }
            }

            pub const fn coverage(self) -> Coverage<AdviceId> {
                match self { $(Self::$variant => $coverage),+ }
            }
        }
    };
}

patterns! {
    AbstractFactory {
        name: "Abstract Factory",
        slug: "abstract-factory",
        family: Creational,
        section: "3.1",
        intent: "make families of related objects without naming their concrete classes",
        coverage: Coverage::Implemented(&[AdviceId::AbstractFactoryFound])
    },
    Builder {
        name: "Builder",
        slug: "builder",
        family: Creational,
        section: "3.2",
        intent: "separate constructing an object from what it ends up being",
        coverage: Coverage::Implemented(&[AdviceId::BuilderFound, AdviceId::BuilderWithoutBuild])
    },
    FactoryMethod {
        name: "Factory Method",
        slug: "factory-method",
        family: Creational,
        section: "3.3",
        intent: "let a subclass decide which class to instantiate",
        coverage: Coverage::Implemented(&[AdviceId::FactoryMethodFound])
    },
    Prototype {
        name: "Prototype",
        slug: "prototype",
        family: Creational,
        section: "3.4",
        intent: "make new objects by copying an existing one",
        coverage: Coverage::Implemented(&[AdviceId::PrototypeFound])
    },
    Singleton {
        name: "Singleton",
        slug: "singleton",
        family: Creational,
        section: "3.5",
        intent: "one instance, reachable from everywhere",
        coverage: Coverage::Implemented(&[AdviceId::SingletonFound, AdviceId::SingletonEscapable])
    },

    Adapter {
        name: "Adapter",
        slug: "adapter",
        family: Structural,
        section: "4.1",
        intent: "make one interface look like the one a caller expects",
        coverage: Coverage::Implemented(&[AdviceId::AdapterFound])
    },
    Bridge {
        name: "Bridge",
        slug: "bridge",
        family: Structural,
        section: "4.2",
        intent: "let an abstraction and its implementation vary independently",
        coverage: Coverage::Implemented(&[AdviceId::BridgeFits])
    },
    Composite {
        name: "Composite",
        slug: "composite",
        family: Structural,
        section: "4.3",
        intent: "treat one object and a tree of them the same way",
        coverage: Coverage::Implemented(&[AdviceId::CompositeFound])
    },
    Decorator {
        name: "Decorator",
        slug: "decorator",
        family: Structural,
        section: "4.4",
        intent: "add behaviour to one object without touching its class",
        coverage: Coverage::Implemented(&[AdviceId::DecoratorFound, AdviceId::DecoratorNotSubstitutable])
    },
    Facade {
        name: "Facade",
        slug: "facade",
        family: Structural,
        section: "4.5",
        intent: "one entrance to a subsystem, so callers do not learn its parts",
        coverage: Coverage::Implemented(&[AdviceId::FacadeFits])
    },
    Flyweight {
        name: "Flyweight",
        slug: "flyweight",
        family: Structural,
        section: "4.6",
        intent: "share one object between many uses instead of copying it",
        coverage: Coverage::Implemented(&[AdviceId::FlyweightFound])
    },
    Proxy {
        name: "Proxy",
        slug: "proxy",
        family: Structural,
        section: "4.7",
        intent: "stand in for another object and control reaching it",
        coverage: Coverage::Implemented(&[AdviceId::ProxyFound])
    },

    ChainOfResponsibility {
        name: "Chain of Responsibility",
        slug: "chain-of-responsibility",
        family: Behavioural,
        section: "5.1",
        intent: "pass a request along a line of handlers until one takes it",
        coverage: Coverage::Implemented(&[AdviceId::ChainOfResponsibilityFits])
    },
    Command {
        name: "Command",
        slug: "command",
        family: Behavioural,
        section: "5.2",
        intent: "make a request an object, so it can be queued, logged or undone",
        coverage: Coverage::Implemented(&[AdviceId::CommandFits])
    },
    Interpreter {
        name: "Interpreter",
        slug: "interpreter",
        family: Behavioural,
        section: "5.3",
        intent: "give a grammar a class per rule and evaluate over the tree",
        coverage: Coverage::Implemented(&[AdviceId::InterpreterFound])
    },
    Iterator {
        name: "Iterator",
        slug: "iterator",
        family: Behavioural,
        section: "5.4",
        intent: "walk a collection without exposing how it is stored",
        coverage: Coverage::Implemented(&[AdviceId::IteratorFound])
    },
    Mediator {
        name: "Mediator",
        slug: "mediator",
        family: Behavioural,
        section: "5.5",
        intent: "stop a set of objects referring to each other directly",
        coverage: Coverage::Implemented(&[AdviceId::MediatorFits])
    },
    Memento {
        name: "Memento",
        slug: "memento",
        family: Behavioural,
        section: "5.6",
        intent: "capture an object's state so it can be put back later",
        coverage: Coverage::Implemented(&[AdviceId::MementoFound])
    },
    Observer {
        name: "Observer",
        slug: "observer",
        family: Behavioural,
        section: "5.7",
        intent: "tell every dependant when one object changes",
        coverage: Coverage::Implemented(&[AdviceId::ObserverFound, AdviceId::ObserverWithoutRemoval])
    },
    State {
        name: "State",
        slug: "state",
        family: Behavioural,
        section: "5.8",
        intent: "let an object change class as its state changes",
        coverage: Coverage::Implemented(&[AdviceId::StateFits])
    },
    Strategy {
        name: "Strategy",
        slug: "strategy",
        family: Behavioural,
        section: "5.9",
        intent: "make a family of algorithms interchangeable behind one interface",
        coverage: Coverage::Implemented(&[AdviceId::StrategyFits])
    },
    TemplateMethod {
        name: "Template Method",
        slug: "template-method",
        family: Behavioural,
        section: "5.10",
        intent: "fix the shape of an algorithm and let subclasses fill in the steps",
        coverage: Coverage::Implemented(&[AdviceId::TemplateMethodFound])
    },
    Visitor {
        name: "Visitor",
        slug: "visitor",
        family: Behavioural,
        section: "5.11",
        intent: "add an operation over a hierarchy without changing its classes",
        coverage: Coverage::Implemented(&[AdviceId::VisitorFound])
    },
}

impl Pattern {
    pub fn tally() -> Tally {
        Tally::of(Self::ALL.iter().map(|pattern| pattern.coverage()))
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|pattern| pattern.slug() == name)
    }

    pub fn advisories(self) -> impl Iterator<Item = AdviceId> {
        AdviceId::ALL
            .iter()
            .copied()
            .filter(move |advisory| advisory.spec().pattern == self)
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn the_catalogue_is_complete() {
        assert_eq!(Pattern::ALL.len(), 23, "the book names 23 patterns");
        for (family, expected) in [
            (PatternFamily::Creational, 5),
            (PatternFamily::Structural, 7),
            (PatternFamily::Behavioural, 11),
        ] {
            assert_eq!(family.patterns().count(), expected, "{}", family.title());
        }
    }

    #[test]
    fn every_entry_is_accounted_for() {
        for pattern in Pattern::ALL.iter().copied() {
            let coverage = pattern.coverage();
            match coverage.reason() {
                Some(reason) => {
                    assert!(!reason.is_empty(), "{pattern} declines without saying why");
                }
                None => assert!(!coverage.rules().is_empty(), "{pattern} names no advisory"),
            }
            assert!(!pattern.intent().is_empty(), "{pattern} has no intent");
            assert!(!pattern.section().is_empty(), "{pattern} cites no section");
        }
        assert_eq!(Pattern::tally().total(), 23);
    }

    #[test]
    fn names_and_slugs_are_unique_and_round_trip() {
        let mut seen = BTreeSet::new();
        for pattern in Pattern::ALL.iter().copied() {
            assert!(seen.insert(pattern.name()), "duplicate pattern {pattern}");
            assert_eq!(Pattern::parse(pattern.slug()), Some(pattern));
        }
    }

    /// A pattern claiming an advisory that does not cite it back, or an advisory
    /// citing a pattern whose coverage does not list it, is the drift this pairing
    /// exists to catch.
    #[test]
    fn coverage_and_the_advisories_agree_about_each_other() {
        for pattern in Pattern::ALL.iter().copied() {
            let claimed: BTreeSet<AdviceId> = pattern.coverage().rules().iter().copied().collect();
            let citing: BTreeSet<AdviceId> = pattern.advisories().collect();
            assert_eq!(claimed, citing, "{pattern}");
        }
    }
}

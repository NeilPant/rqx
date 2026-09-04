//! The static shapes that decide each pattern. `crate::rules` turns facts into
//! findings; this turns the same facts into advice, and the two never meet — an
//! advisory cannot become a finding, so nothing here can reach the health score.

use std::sync::Arc;

use crate::advice::{Advice, AdviceId, Signal};
use crate::config::RqxConfig;
use crate::facts::{ClassReport, Field, Method, Patterns};
use crate::finding::SourceRange;
use crate::line_index::LineIndex;
use crate::span::Span;

pub struct AdviceBuilder<'a> {
    pub file: Arc<str>,
    pub index: &'a LineIndex<'a>,
    pub config: &'a RqxConfig,
    pub language: crate::language::Language,
}

impl AdviceBuilder<'_> {
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
        out: &mut Vec<Advice>,
        advisory: AdviceId,
        span: Span,
        signal: Signal,
        note: String,
    ) {
        // A declared exemption is enforced here rather than trusted to each
        // signature, so an advisory that cannot apply to a language cannot fire there
        // however its shape happens to read.
        if !self.config.patterns.enabled(advisory) || !advisory.applies_to(self.language) {
            return;
        }
        out.push(Advice {
            advisory,
            location: self.range(span),
            signal,
            note,
            related: Vec::new(),
        });
    }
}

pub fn from_patterns(patterns: &Patterns, builder: &AdviceBuilder<'_>, out: &mut Vec<Advice>) {
    for chain in &patterns.handler_chains {
        builder.emit(
            out,
            AdviceId::ChainOfResponsibilityFits,
            chain.span,
            Signal::Chain {
                topics: chain.topics.join(", "),
                branches: chain.topics.len() as u32,
            },
            format!(
                "give each test and its body a handler that either answers or passes the \
                 request on, so adding a {} case is one new handler rather than an edit \
                 in the middle of the chain",
                chain.topics.len() + 1
            ),
        );
    }

    for discriminant in &patterns.discriminants {
        // A dispatch is decided first: a switch whose branches all call one function
        // of the same shape is already a table, and telling its author to reach for
        // Strategy would describe the same move less precisely.
        let (advisory, note) = if discriminant.uniform_dispatch {
            (
                AdviceId::CommandFits,
                format!(
                    "give each case an object with the call it already makes, and look \
                     it up by `{}` instead of branching on it",
                    discriminant.name
                ),
            )
        } else if discriminant.assigns_subject {
            (
                AdviceId::StateFits,
                format!(
                    "give `{}` a type per state and let each one return the next, so a \
                     transition is a return value rather than an assignment in {}",
                    discriminant.name,
                    crate::finding::plural(discriminant.sites, "place", "places"),
                ),
            )
        } else {
            (
                AdviceId::StrategyFits,
                format!(
                    "give `{}` a type per case behind one interface, so adding a case is \
                     one new class rather than {} edits",
                    discriminant.name, discriminant.sites,
                ),
            )
        };
        builder.emit(
            out,
            advisory,
            discriminant.span,
            Signal::Discriminant {
                subject: discriminant.name.clone(),
                sites: discriminant.sites,
                branches: discriminant.branches,
            },
            note,
        );
    }
}

/// Words a language community uses for the same move. Name lists are the weakest thing
/// here, so every advisory that leans on one is only ever as strong as `Heuristic`.
const COPY_VERBS: [&str; 5] = ["clone", "copy", "duplicate", "dup", "replicate"];
const SUBSCRIBE_VERBS: [&str; 6] = ["add", "subscribe", "register", "on", "attach", "listen"];
const UNSUBSCRIBE_VERBS: [&str; 6] = [
    "remove",
    "unsubscribe",
    "deregister",
    "off",
    "detach",
    "unregister",
];
const NOTIFY_VERBS: [&str; 6] = ["notify", "emit", "publish", "fire", "dispatch", "broadcast"];
const LISTENER_WORDS: [&str; 6] = [
    "listener",
    "observer",
    "subscriber",
    "handler",
    "callback",
    "watcher",
];
const SAVE_VERBS: [&str; 4] = ["save", "snapshot", "capture", "memento"];
const RESTORE_VERBS: [&str; 4] = ["restore", "undo", "revert", "rollback"];

/// `addListener` starts with `add`; `address` does not. The case of the character
/// after the verb is the only thing separating the two, so the name is matched
/// case-insensitively and then read in its original case.
fn starts_with_verb(name: &str, verbs: &[&str]) -> bool {
    verbs.iter().any(|verb| {
        name.get(..verb.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(verb))
            && name[verb.len()..]
                .chars()
                .next()
                .is_none_or(|next| !next.is_ascii_lowercase())
    })
}

fn mentions(text: &str, words: &[&str]) -> bool {
    let lower = text.to_ascii_lowercase();
    words.iter().any(|word| lower.contains(word))
}

/// Whether a written type names this class. Types are compared as text because nothing
/// here resolves them, so `List<Handler>` and `Handler` both name `Handler`.
fn names(type_name: Option<&String>, wanted: &str) -> bool {
    type_name.is_some_and(|text| {
        text == wanted
            || text
                .split(['<', '>', '[', ']', ',', ' ', '*', '&'])
                .any(|part| part == wanted)
    })
}

fn holds(report: &ClassReport, wanted: &str) -> bool {
    report
        .held
        .iter()
        .any(|field| names(field.type_name.as_ref(), wanted))
}

fn behaviour(report: &ClassReport) -> impl Iterator<Item = &Method> {
    report
        .declared
        .iter()
        .filter(|method| !method.is_constructor)
}

pub fn from_classes(classes: &[ClassReport], builder: &AdviceBuilder<'_>, out: &mut Vec<Advice>) {
    for report in classes {
        wrapping(report, builder, out);
        composing(report, builder, out);
        building(report, builder, out);
        instancing(report, builder, out);
        listening(report, builder, out);
        remembering(report, builder, out);
        templating(report, builder, out);
    }
}

fn shape(report: &ClassReport, detail: String) -> Signal {
    Signal::Shape {
        class: report.name.clone(),
        detail,
    }
}

/// Decorator, Proxy and Adapter are one shape read three ways: a class that forwards
/// to something it holds. What it holds, and whether it can stand in for it, is the
/// whole of the difference.
fn wrapping(report: &ClassReport, builder: &AdviceBuilder<'_>, out: &mut Vec<Advice>) {
    if report.delegating == 0 {
        return;
    }
    if let Some(supertype) = report
        .supertypes
        .iter()
        .find(|supertype| holds(report, supertype))
    {
        let adds = report.real_methods > report.delegating;
        let advisory = if adds {
            AdviceId::DecoratorFound
        } else {
            AdviceId::ProxyFound
        };
        builder.emit(
            out,
            advisory,
            report.span,
            shape(
                report,
                format!(
                    "implements `{supertype}`, holds one, and forwards {} of {}",
                    report.delegating, report.real_methods
                ),
            ),
            if adds {
                format!("`{}` can stand in for `{supertype}`", report.name)
            } else {
                format!("`{}` adds nothing to `{supertype}`", report.name)
            },
        );
        return;
    }

    // Every method forwarding and none adding is what makes it a wrapper rather than
    // a class that happens to hold something. Anything looser catches ordinary
    // composition, which is most classes.
    if report.delegating < 2 || report.delegating != report.real_methods {
        return;
    }
    // One object, not many: a class forwarding into a collection it owns is holding
    // its own state, not standing in for something else.
    let Some(held) = report
        .held
        .iter()
        .find(|field| field.type_name.is_some() && field.element_type.is_none())
    else {
        return;
    };
    let wrapped = held.type_name.clone().unwrap_or_default();
    if report.supertypes.is_empty() {
        builder.emit(
            out,
            AdviceId::DecoratorNotSubstitutable,
            report.span,
            shape(
                report,
                format!("wraps a `{wrapped}` and implements nothing"),
            ),
            format!(
                "give `{}` the interface `{wrapped}` answers to, or callers have to \
                 know which of the two they are holding",
                report.name
            ),
        );
        return;
    }
    builder.emit(
        out,
        AdviceId::AdapterFound,
        report.span,
        shape(
            report,
            format!(
                "implements `{}` and forwards to a `{wrapped}`",
                report.supertypes.join("`, `")
            ),
        ),
        format!(
            "`{}` translates one interface into another; keep the translation here \
             rather than at its call sites",
            report.name
        ),
    );
}

fn composing(report: &ClassReport, builder: &AdviceBuilder<'_>, out: &mut Vec<Advice>) {
    let own: Vec<&str> = std::iter::once(report.name.as_str())
        .chain(report.supertypes.iter().map(String::as_str))
        .collect();
    // A static collection of the same type is a pool, not a tree of children.
    let Some(field) = report.held.iter().find(|field| {
        !field.is_static
            && own
                .iter()
                .any(|name| names(field.element_type.as_ref(), name))
    }) else {
        return;
    };
    builder.emit(
        out,
        AdviceId::CompositeFound,
        report.span,
        shape(
            report,
            format!("holds `{}`, a collection of its own kind", field.name),
        ),
        format!(
            "one `{}` and a tree of them answer the same way",
            report.name
        ),
    );
}

fn building(report: &ClassReport, builder: &AdviceBuilder<'_>, out: &mut Vec<Advice>) {
    let fluent: Vec<&Method> = behaviour(report)
        .filter(|method| names(method.returns.as_ref(), &report.name))
        .collect();
    if fluent.len() < 3 {
        return;
    }
    let terminal = behaviour(report)
        .find(|method| method.returns.is_some() && !names(method.returns.as_ref(), &report.name));
    let detail = format!("has {} steps returning itself", fluent.len());
    match terminal {
        Some(build) => builder.emit(
            out,
            AdviceId::BuilderFound,
            report.span,
            shape(report, format!("{detail} and `{}` to finish", build.name)),
            format!("`{}` is assembled a step at a time", report.name),
        ),
        None => builder.emit(
            out,
            AdviceId::BuilderWithoutBuild,
            report.span,
            shape(report, format!("{detail} and no step that finishes")),
            format!(
                "give `{}` one method that returns what it built, so callers stop \
                 reaching for its state",
                report.name
            ),
        ),
    }
}

/// Prototype, Singleton and Flyweight all turn on how an instance is come by.
fn instancing(report: &ClassReport, builder: &AdviceBuilder<'_>, out: &mut Vec<Advice>) {
    if let Some(copy) = behaviour(report).find(|method| {
        starts_with_verb(&method.name, &COPY_VERBS) && names(method.returns.as_ref(), &report.name)
    }) {
        builder.emit(
            out,
            AdviceId::PrototypeFound,
            report.span,
            shape(report, format!("copies itself through `{}`", copy.name)),
            format!("a new `{}` is made from an existing one", report.name),
        );
    }

    // One of itself, not many: a static collection of the same type is a pool.
    let own_static = |field: &&Field| {
        field.is_static
            && field.element_type.is_none()
            && names(field.type_name.as_ref(), &report.name)
    };
    if let Some(instance) = report.held.iter().find(own_static) {
        let open = report
            .declared
            .iter()
            .find(|method| method.is_constructor && method.is_public);
        let detail = format!("keeps one `{}` in `{}`", report.name, instance.name);
        match open {
            Some(_) => builder.emit(
                out,
                AdviceId::SingletonEscapable,
                report.span,
                shape(report, format!("{detail} and its constructor is public")),
                format!(
                    "make `{}`'s constructor private, or stop calling the shared \
                     instance the only one",
                    report.name
                ),
            ),
            None => builder.emit(
                out,
                AdviceId::SingletonFound,
                report.span,
                shape(report, detail),
                format!(
                    "everything that reaches `{}` shares one instance, and nothing \
                     depending on it can be tested against another",
                    report.name
                ),
            ),
        }
    }

    let pooled = report
        .held
        .iter()
        .any(|field| field.is_static && names(field.element_type.as_ref(), &report.name));
    if pooled
        && let Some(lookup) = behaviour(report)
            .find(|method| method.is_static && names(method.returns.as_ref(), &report.name))
    {
        builder.emit(
            out,
            AdviceId::FlyweightFound,
            report.span,
            shape(
                report,
                format!("hands out shared instances from `{}`", lookup.name),
            ),
            format!(
                "the same `{}` is one object wherever it is held",
                report.name
            ),
        );
    }
}

fn listening(report: &ClassReport, builder: &AdviceBuilder<'_>, out: &mut Vec<Advice>) {
    let Some(listeners) = report.held.iter().find(|field| {
        field.element_type.is_some()
            && (mentions(&field.name, &LISTENER_WORDS)
                || field
                    .element_type
                    .as_deref()
                    .is_some_and(|inner| mentions(inner, &LISTENER_WORDS)))
    }) else {
        return;
    };
    let subscribe =
        behaviour(report).find(|method| starts_with_verb(&method.name, &SUBSCRIBE_VERBS));
    let notify = behaviour(report).any(|method| starts_with_verb(&method.name, &NOTIFY_VERBS));
    let unsubscribe =
        behaviour(report).any(|method| starts_with_verb(&method.name, &UNSUBSCRIBE_VERBS));
    let Some(subscribe) = subscribe else { return };
    if !notify {
        return;
    }
    let detail = format!("keeps its listeners in `{}`", listeners.name);
    if unsubscribe {
        builder.emit(
            out,
            AdviceId::ObserverFound,
            report.span,
            shape(report, detail),
            format!(
                "`{}` tells its dependants without knowing them",
                report.name
            ),
        );
    } else {
        builder.emit(
            out,
            AdviceId::ObserverWithoutRemoval,
            report.span,
            shape(
                report,
                format!("{detail} and only `{}` to join", subscribe.name),
            ),
            format!(
                "give `{}` a way to unsubscribe, or every listener it takes lives as \
                 long as it does",
                report.name
            ),
        );
    }
}

fn remembering(report: &ClassReport, builder: &AdviceBuilder<'_>, out: &mut Vec<Advice>) {
    let Some(save) = behaviour(report)
        .find(|method| starts_with_verb(&method.name, &SAVE_VERBS) && method.returns.is_some())
    else {
        return;
    };
    let Some(kept) = save.returns.as_deref() else {
        return;
    };
    if behaviour(report)
        .any(|method| starts_with_verb(&method.name, &RESTORE_VERBS) && method.arity >= 1)
    {
        builder.emit(
            out,
            AdviceId::MementoFound,
            report.span,
            shape(
                report,
                format!("hands out a `{kept}` through `{}`", save.name),
            ),
            format!(
                "`{}` puts its own state back without exposing it",
                report.name
            ),
        );
    }
}

fn templating(report: &ClassReport, builder: &AdviceBuilder<'_>, out: &mut Vec<Advice>) {
    let abstract_members: Vec<&str> = report
        .declared
        .iter()
        .filter(|method| method.is_abstract)
        .map(|method| method.name.as_str())
        .collect();
    if abstract_members.len() < 2 {
        return;
    }
    let Some(template) = report.declared.iter().find(|method| {
        !method.is_abstract
            && method
                .calls_own
                .iter()
                .filter(|call| abstract_members.contains(&call.as_str()))
                .count()
                >= 2
    }) else {
        return;
    };
    builder.emit(
        out,
        AdviceId::TemplateMethodFound,
        report.span,
        shape(
            report,
            format!("fixes the order of its steps in `{}`", template.name),
        ),
        format!(
            "a subclass of `{}` fills in the steps and cannot change their order",
            report.name
        ),
    );
}

/// Iteration is the one pattern the language itself defines, so what counts as
/// implementing it is per-language data rather than a shape this can reason about.
pub fn iterating(
    classes: &[ClassReport],
    members: &[&str],
    builder: &AdviceBuilder<'_>,
    out: &mut Vec<Advice>,
) {
    if members.is_empty() {
        return;
    }
    for report in classes {
        let declares = |wanted: &&str| report.declared.iter().any(|method| method.name == **wanted);
        if !members.iter().all(declares) {
            continue;
        }
        // Without something to walk, the members are a coincidence of naming.
        if !report.held.iter().any(|field| field.element_type.is_some()) {
            continue;
        }
        builder.emit(
            out,
            AdviceId::IteratorFound,
            report.span,
            shape(report, format!("declares {}", members.join(", "))),
            format!(
                "callers walk `{}` with the language's own loop and never learn how it \
                 stores what it holds",
                report.name
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::facts::Discriminant;

    use super::*;

    fn advise(assigns_subject: bool) -> Vec<Advice> {
        let source = "let kind = 1;\n";
        let index = LineIndex::new(source);
        let config = RqxConfig::default();
        let builder = AdviceBuilder {
            file: Arc::from("shapes.ts"),
            index: &index,
            config: &config,
            language: crate::language::Language::TypeScript,
        };
        let patterns = Patterns {
            discriminants: vec![Discriminant {
                name: "kind".to_owned(),
                sites: 3,
                branches: 4,
                assigns_subject,
                uniform_dispatch: false,
                span: Span::new(0, 3),
            }],
            ..Patterns::default()
        };
        let mut out = Vec::new();
        from_patterns(&patterns, &builder, &mut out);
        out
    }

    /// The same switch is one pattern or the other, never both — a reader told to
    /// apply Strategy and State to one value has been told nothing.
    #[test]
    fn a_branch_writing_back_to_the_subject_is_a_state_and_not_a_strategy() {
        let states = advise(true);
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].advisory, AdviceId::StateFits);

        let strategies = advise(false);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].advisory, AdviceId::StrategyFits);
    }

    #[test]
    fn a_disabled_pattern_emits_nothing() {
        let source = "let kind = 1;\n";
        let index = LineIndex::new(source);
        let mut config = RqxConfig::default();
        config.patterns.disabled.insert("strategy".to_owned());
        let builder = AdviceBuilder {
            file: Arc::from("shapes.ts"),
            index: &index,
            config: &config,
            language: crate::language::Language::TypeScript,
        };
        let patterns = Patterns {
            discriminants: vec![Discriminant {
                name: "kind".to_owned(),
                sites: 3,
                branches: 4,
                assigns_subject: false,
                uniform_dispatch: false,
                span: Span::new(0, 3),
            }],
            ..Patterns::default()
        };
        let mut out = Vec::new();
        from_patterns(&patterns, &builder, &mut out);
        assert!(out.is_empty());
    }
}

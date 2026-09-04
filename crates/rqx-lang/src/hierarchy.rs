//! The inheritance tree across every scanned file. `ModuleFacts::heritage` pairs a
//! child name with a parent name inside one file; the patterns built on inheritance
//! need to ask what a type's siblings are, and in a language of one class per file
//! that question cannot be answered without joining the files first.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rqx_core::facts::{Method, ModuleFacts, TypeFacts};
use rqx_core::span::Span;

use crate::vocabulary::identifier_words;

/// Words a subclass of a grammar node answers to. Recognising an interpreter is the
/// weakest claim here and rests on this list alone.
const EVAL_NAMES: [&str; 5] = ["eval", "evaluate", "interpret", "execute", "apply"];

pub struct Declared<'a> {
    pub path: &'a Path,
    pub facts: &'a TypeFacts,
}

impl Declared<'_> {
    pub fn behaviour(&self) -> impl Iterator<Item = &Method> {
        self.facts
            .methods
            .iter()
            .filter(|method| !method.is_constructor)
    }

    fn declares(&self, wanted: &str) -> bool {
        self.behaviour().any(|method| method.name == wanted)
    }
}

pub struct Hierarchy<'a> {
    types: BTreeMap<&'a str, Declared<'a>>,
    children: BTreeMap<&'a str, Vec<&'a str>>,
}

impl<'a> Hierarchy<'a> {
    /// A name declared in two files is dropped rather than guessed at: `Handler` in
    /// two packages is two types, and merging them would invent a hierarchy.
    pub fn of(modules: &'a BTreeMap<PathBuf, &'a ModuleFacts>) -> Self {
        let mut seen: BTreeMap<&str, Option<Declared<'a>>> = BTreeMap::new();
        for (path, facts) in modules {
            for declared in &facts.types {
                seen.entry(declared.name.as_str())
                    .and_modify(|slot| *slot = None)
                    .or_insert(Some(Declared {
                        path: path.as_path(),
                        facts: declared,
                    }));
            }
        }
        let types: BTreeMap<&str, Declared<'a>> = seen
            .into_iter()
            .filter_map(|(name, slot)| slot.map(|declared| (name, declared)))
            .collect();

        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (name, declared) in &types {
            for parent in &declared.facts.supertypes {
                children.entry(parent.as_str()).or_default().push(name);
            }
        }
        for kin in children.values_mut() {
            kin.sort_unstable();
            kin.dedup();
        }
        Self { types, children }
    }

    pub fn get(&self, name: &str) -> Option<&Declared<'a>> {
        self.types.get(name)
    }

    pub fn children_of(&self, name: &str) -> &[&'a str] {
        self.children.get(name).map_or(&[], Vec::as_slice)
    }

    pub fn is_abstraction(&self, name: &str) -> bool {
        self.children.contains_key(name)
    }

    /// Every type in the tree, in a stable order so two runs report the same thing.
    pub fn declared(&self) -> impl Iterator<Item = (&&'a str, &Declared<'a>)> {
        self.types.iter()
    }
}

/// What a hierarchy says about one type, ready to be turned into advice. Kept as data
/// so the shapes can be tested without a scan and rendered without a tree.
pub struct Finding<'a> {
    pub kind: Shape,
    pub path: &'a Path,
    pub name: &'a str,
    pub span: Span,
    pub detail: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    FactoryMethod,
    AbstractFactory,
    Bridge,
    Interpreter,
    Visitor,
}

pub fn shapes<'a>(tree: &'a Hierarchy<'a>) -> Vec<Finding<'a>> {
    let mut found = Vec::new();
    for (name, declared) in tree.declared() {
        let kin = tree.children_of(name);
        factory(tree, name, declared, kin, &mut found);
        bridge(name, declared, kin, &mut found);
        interpreter(tree, name, declared, kin, &mut found);
        visitor(name, declared, &mut found);
    }
    found.sort_by(|left, right| {
        left.path
            .cmp(right.path)
            .then(left.span.start.cmp(&right.span.start))
            .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
    });
    found
}

fn factory<'a>(
    tree: &Hierarchy<'a>,
    name: &'a str,
    declared: &Declared<'a>,
    kin: &[&'a str],
    found: &mut Vec<Finding<'a>>,
) {
    if kin.len() < 2 {
        return;
    }
    let making: Vec<&Method> = declared
        .behaviour()
        .filter(|method| {
            method
                .returns
                .as_deref()
                .is_some_and(|returned| tree.is_abstraction(returned) && returned != name)
        })
        .collect();
    if making.is_empty() {
        return;
    }
    // Every method handing back an abstraction makes the type a factory for a family,
    // rather than one method on a class that also does other things.
    let all_abstract = making.len() == declared.behaviour().count() && making.len() >= 2;
    let (kind, detail) = if all_abstract {
        (
            Shape::AbstractFactory,
            format!(
                "every method returns an abstraction, and {} implement it",
                plural(kin.len(), "type", "types")
            ),
        )
    } else {
        (
            Shape::FactoryMethod,
            format!(
                "`{}` returns `{}`, and {} decide which",
                making[0].name,
                making[0].returns.clone().unwrap_or_default(),
                plural(kin.len(), "subclass", "subclasses")
            ),
        )
    };
    found.push(Finding {
        kind,
        path: declared.path,
        name,
        span: declared.facts.span,
        detail,
    });
}

/// Two axes crammed into one hierarchy show up in the names: `RedCircle`, `RedSquare`,
/// `BlueCircle`, `BlueSquare` is a grid, and a grid grows by multiplication.
fn bridge<'a>(
    name: &'a str,
    declared: &Declared<'a>,
    kin: &[&'a str],
    found: &mut Vec<Finding<'a>>,
) {
    if kin.len() < 4 {
        return;
    }
    let mut heads = Vec::new();
    let mut tails = Vec::new();
    for child in kin {
        let words = identifier_words(child);
        let [head, .., tail] = words.as_slice() else {
            return;
        };
        heads.push(head.clone());
        tails.push(tail.clone());
    }
    heads.sort();
    heads.dedup();
    tails.sort();
    tails.dedup();
    if heads.len() < 2 || tails.len() < 2 || heads.len() * tails.len() != kin.len() {
        return;
    }
    found.push(Finding {
        kind: Shape::Bridge,
        path: declared.path,
        name,
        span: declared.facts.span,
        detail: format!(
            "{} subclasses covering {} \u{d7} {}",
            kin.len(),
            heads.len(),
            tails.len()
        ),
    });
}

fn interpreter<'a>(
    tree: &Hierarchy<'a>,
    name: &'a str,
    declared: &Declared<'a>,
    kin: &[&'a str],
    found: &mut Vec<Finding<'a>>,
) {
    if kin.len() < 3 {
        return;
    }
    let Some(shared) = EVAL_NAMES.into_iter().find(|wanted| {
        kin.iter()
            .all(|child| tree.get(child).is_some_and(|kid| kid.declares(wanted)))
    }) else {
        return;
    };
    found.push(Finding {
        kind: Shape::Interpreter,
        path: declared.path,
        name,
        span: declared.facts.span,
        detail: format!("{} subclasses that each answer `{shared}`", kin.len()),
    });
}

fn visitor<'a>(name: &'a str, declared: &Declared<'a>, found: &mut Vec<Finding<'a>>) {
    let visits: Vec<&Method> = declared
        .behaviour()
        .filter(|method| method.name.len() > 5 && method.name.to_ascii_lowercase().starts_with("visit"))
        .collect();
    if visits.len() < 3 {
        return;
    }
    found.push(Finding {
        kind: Shape::Visitor,
        path: declared.path,
        name,
        span: declared.facts.span,
        detail: format!("{} visit methods over one hierarchy", visits.len()),
    });
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(name: &str, returns: Option<&str>) -> Method {
        Method {
            name: name.to_owned(),
            returns: returns.map(ToOwned::to_owned),
            arity: 0,
            is_static: false,
            is_constructor: false,
            is_abstract: false,
            is_public: true,
            calls_own: Vec::new(),
            span: Span::new(0, 1),
        }
    }

    fn facts(name: &str, supertypes: &[&str], methods: Vec<Method>) -> TypeFacts {
        TypeFacts {
            name: name.to_owned(),
            span: Span::new(0, 1),
            supertypes: supertypes.iter().map(|s| (*s).to_owned()).collect(),
            methods,
        }
    }

    fn tree(types: Vec<TypeFacts>) -> ModuleFacts {
        ModuleFacts {
            types,
            ..ModuleFacts::default()
        }
    }

    /// Four subclasses over two independent axes is a grid; four that only vary one
    /// way is an ordinary hierarchy, and calling it a Bridge would be noise.
    #[test]
    fn a_cross_product_of_names_is_a_bridge_and_a_flat_list_is_not() {
        let grid = tree(vec![
            facts("Shape", &[], Vec::new()),
            facts("RedCircle", &["Shape"], Vec::new()),
            facts("RedSquare", &["Shape"], Vec::new()),
            facts("BlueCircle", &["Shape"], Vec::new()),
            facts("BlueSquare", &["Shape"], Vec::new()),
        ]);
        let modules = BTreeMap::from([(PathBuf::from("a.ts"), &grid)]);
        let hierarchy = Hierarchy::of(&modules);
        let found = shapes(&hierarchy);
        assert!(found.iter().any(|entry| entry.kind == Shape::Bridge));

        let flat = tree(vec![
            facts("Shape", &[], Vec::new()),
            facts("RedCircle", &["Shape"], Vec::new()),
            facts("BlueCircle", &["Shape"], Vec::new()),
            facts("GreenCircle", &["Shape"], Vec::new()),
            facts("WhiteCircle", &["Shape"], Vec::new()),
        ]);
        let modules = BTreeMap::from([(PathBuf::from("a.ts"), &flat)]);
        let hierarchy = Hierarchy::of(&modules);
        let found = shapes(&hierarchy);
        assert!(!found.iter().any(|entry| entry.kind == Shape::Bridge));
    }

    /// A name declared twice is two types, and joining them would invent a hierarchy
    /// out of a coincidence of naming.
    #[test]
    fn a_name_declared_in_two_files_is_left_out() {
        let left = tree(vec![facts("Handler", &["Base"], Vec::new())]);
        let right = tree(vec![facts("Handler", &["Other"], Vec::new())]);
        let modules = BTreeMap::from([
            (PathBuf::from("a.ts"), &left),
            (PathBuf::from("b.ts"), &right),
        ]);
        let hierarchy = Hierarchy::of(&modules);
        assert!(hierarchy.get("Handler").is_none());
    }

    #[test]
    fn a_type_whose_every_method_hands_back_an_abstraction_is_an_abstract_factory() {
        let facts = tree(vec![
            facts(
                "Kit",
                &[],
                vec![method("button", Some("Button")), method("panel", Some("Panel"))],
            ),
            facts("DarkKit", &["Kit"], Vec::new()),
            facts("LightKit", &["Kit"], Vec::new()),
            facts("Button", &[], Vec::new()),
            facts("FlatButton", &["Button"], Vec::new()),
            facts("Panel", &[], Vec::new()),
            facts("FlatPanel", &["Panel"], Vec::new()),
        ]);
        let modules = BTreeMap::from([(PathBuf::from("a.ts"), &facts)]);
        let hierarchy = Hierarchy::of(&modules);
        let found = shapes(&hierarchy);
        assert!(
            found
                .iter()
                .any(|entry| entry.kind == Shape::AbstractFactory && entry.name == "Kit")
        );
    }
}

//! What library calls cost. A table says, for a receiver type and a method, or for a
//! free function, how its time and extra space grow with the size it works on. The
//! built-in tables ship with the binary; a project adds its own under
//! `performance.costs`, and its entries are tried first.
//!
//! A table never guesses a type. It answers for a type the source stated, and for a
//! method name that means the same thing on every type that has one, which the caller
//! reports as less sure.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, LazyLock};

use rqx_core::Language;
use rqx_core::config::{CostEntry, Performance};
use rqx_core::cost::Order;

use super::types::canonical_head;

/// Bumped when the meaning of an entry changes without its text changing.
const FORMAT: u32 = 1;

/// The receiver a method entry stands for on any type that has the method.
const ANY_RECEIVER: &str = "*";

const BUILT_IN: [(&[Language], &str); 6] = [
    (
        &[Language::JavaScript, Language::TypeScript],
        include_str!("../../data/cost/javascript.yaml"),
    ),
    (
        &[Language::TypeScript],
        include_str!("../../data/cost/typescript.yaml"),
    ),
    (
        &[Language::Python],
        include_str!("../../data/cost/python.yaml"),
    ),
    (&[Language::Java], include_str!("../../data/cost/java.yaml")),
    (&[Language::Go], include_str!("../../data/cost/go.yaml")),
    (&[Language::Cpp], include_str!("../../data/cost/cpp.yaml")),
];

/// Which argument an entry's size is taken from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeOf {
    /// What the method is called on.
    Receiver,
    /// The collection the argument at this position is, or is a range over.
    Arg(u8),
}

/// A condition on the arguments of a call, written under `when`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Condition {
    /// The argument is a literal, optionally this exact one.
    Literal(u8, Option<String>),
    Absent(u8),
    Present(u8),
    /// A condition the table could not read. It never holds, so a mistake in an entry
    /// switches the entry off rather than charging for calls it was not written for.
    Never,
}

impl Condition {
    fn parse(key: &str, value: &str) -> Self {
        let Some(position) = key
            .strip_prefix("arg")
            .and_then(|digits| digits.parse::<u8>().ok())
        else {
            return Self::Never;
        };
        let mut words = value.split_whitespace();
        match (words.next(), words.next(), words.next()) {
            (Some("literal"), None, None) => Self::Literal(position, None),
            (Some("literal"), Some(exact), None) => Self::Literal(position, Some(exact.to_owned())),
            (Some("absent"), None, None) => Self::Absent(position),
            (Some("present"), None, None) => Self::Present(position),
            _ => Self::Never,
        }
    }

    fn holds(&self, call: &CallFacts<'_>) -> bool {
        let at = |position: &u8| call.literals.get(usize::from(*position));
        match self {
            Self::Literal(position, exact) => at(position).is_some_and(|literal| {
                literal.is_some_and(|text| {
                    exact
                        .as_deref()
                        .is_none_or(|wanted| text.trim_matches(['"', '\'']) == wanted)
                })
            }),
            Self::Absent(position) => at(position).is_none(),
            Self::Present(position) => at(position).is_some(),
            Self::Never => false,
        }
    }
}

/// What a call's arguments look like, for the conditions of an entry: one slot per
/// argument, holding the literal's text where the argument is a literal.
pub struct CallFacts<'a> {
    pub literals: &'a [Option<&'a str>],
}

impl CallFacts<'_> {
    pub const NONE: CallFacts<'static> = CallFacts { literals: &[] };
}

/// One line of a table, ready to match.
#[derive(Debug)]
pub struct Entry {
    pub id: String,
    /// Time, in `n` for the size `size_of` names and `m` for the first argument's.
    pub cost: Order,
    pub space: Order,
    pub pure: bool,
    pub size_of: SizeOf,
    when: Vec<Condition>,
}

impl Entry {
    fn from(source: &CostEntry) -> Option<Self> {
        let size_of = match source.size_of.as_deref() {
            Some(text) => text
                .strip_prefix("arg")
                .and_then(|rest| rest.split('.').next())
                .and_then(|digits| digits.parse().ok())
                .map(SizeOf::Arg)
                .unwrap_or(SizeOf::Receiver),
            None if source.function.is_some() => SizeOf::Arg(0),
            None => SizeOf::Receiver,
        };
        Some(Self {
            id: source.id.clone(),
            cost: source.cost.parse().ok()?,
            space: source
                .space
                .as_deref()
                .map_or(Some(Order::one()), |space| space.parse().ok())?,
            pure: source.pure,
            size_of,
            when: source
                .when
                .iter()
                .map(|(key, value)| Condition::parse(key, value))
                .collect(),
        })
    }

    /// Whether the call takes time proportional to the size it works on, or more.
    pub fn at_least_linear(&self) -> bool {
        self.cost.rank() >= Order::var(0).rank()
    }

    /// Whether the call works in place or answers a question, rather than making a copy
    /// as large as its input.
    pub fn makes_no_copy(&self) -> bool {
        self.space.is_one()
    }

    /// Whether the call takes `n log n`: a sort.
    pub fn sorts(&self) -> bool {
        self.cost.rank() == Order::var(0).mul(&Order::log_var(0)).rank()
    }

    fn applies(&self, call: &CallFacts<'_>) -> bool {
        self.when.iter().all(|condition| condition.holds(call))
    }
}

/// The entries of one language, found by what they are called on.
#[derive(Default)]
struct Index {
    methods: HashMap<(String, String), Vec<u16>>,
    types: HashSet<String>,
    any_receiver: HashMap<String, Vec<u16>>,
    functions: HashMap<String, Vec<u16>>,
}

/// A function written with its namespace spells the same name however the language
/// separates one: `std::find`, `std.find` and a bare `find` under `using namespace`.
fn function_key(name: &str) -> String {
    let dotted = name.replace("::", ".");
    dotted.strip_prefix("std.").unwrap_or(&dotted).to_owned()
}

pub struct CostTables {
    entries: Vec<Entry>,
    languages: HashMap<Language, Index>,
    version: u64,
}

static BUILT_IN_TABLES: LazyLock<Arc<CostTables>> = LazyLock::new(|| {
    Arc::new(CostTables::build(&[]).expect("the built-in cost tables are well formed"))
});

impl CostTables {
    /// The tables that ship with rqx.
    pub fn built_in() -> Arc<Self> {
        Arc::clone(&BUILT_IN_TABLES)
    }

    /// The built-in tables with a project's own entries tried first. A project with no
    /// entries shares the built-in tables rather than building its own.
    pub fn for_performance(performance: &Performance) -> Arc<Self> {
        if performance.costs.is_empty() {
            return Self::built_in();
        }
        match Self::build(&performance.costs) {
            Ok(tables) => Arc::new(tables),
            Err(_) => Self::built_in(),
        }
    }

    fn build(user: &[CostEntry]) -> Result<Self, String> {
        let mut parsed: Vec<(Vec<Language>, CostEntry)> = user
            .iter()
            .map(|entry| (Language::ALL.to_vec(), entry.clone()))
            .collect();
        for (languages, text) in BUILT_IN {
            let entries: Vec<CostEntry> =
                serde_saphyr::from_str(text).map_err(|error| error.to_string())?;
            parsed.extend(entries.into_iter().map(|entry| (languages.to_vec(), entry)));
        }

        let mut hasher = DefaultHasher::new();
        FORMAT.hash(&mut hasher);
        BUILT_IN.iter().for_each(|(_, text)| text.hash(&mut hasher));
        user.hash(&mut hasher);

        let mut entries = Vec::new();
        let mut languages: HashMap<Language, Index> = HashMap::new();
        for (applies_to, source) in &parsed {
            let entry = Entry::from(source)
                .ok_or_else(|| format!("`{}` does not read as a cost", source.id))?;
            let position = u16::try_from(entries.len()).map_err(|error| error.to_string())?;
            entries.push(entry);
            for language in applies_to {
                languages
                    .entry(*language)
                    .or_default()
                    .register(*language, source, position);
            }
        }
        Ok(Self {
            entries,
            languages,
            version: hasher.finish(),
        })
    }

    /// A number that changes when any entry does, which is what a cached analysis has
    /// to be keyed on: it holds indexes into these entries.
    pub const fn version(&self) -> u64 {
        self.version
    }

    pub fn entry(&self, index: u16) -> Option<&Entry> {
        self.entries.get(usize::from(index))
    }

    /// Whether the tables cost anything called on a type of this head, which says it is
    /// a library type and not one of the project's.
    pub fn knows_type(&self, language: Language, head: &str) -> bool {
        self.languages
            .get(&language)
            .is_some_and(|index| index.types.contains(head))
    }

    /// The entry for `method` called on a value whose type is `head`.
    pub fn method(
        &self,
        language: Language,
        head: &str,
        method: &str,
        call: &CallFacts<'_>,
    ) -> Option<(u16, &Entry)> {
        let found = self
            .languages
            .get(&language)?
            .methods
            .get(&(head.to_owned(), method.to_owned()))?;
        self.first_applying(found, call)
    }

    /// The entry for `method` on a receiver of unknown type, when the name means one
    /// thing wherever it is found. Several candidates that would answer differently
    /// are no answer.
    pub fn any_method(
        &self,
        language: Language,
        method: &str,
        call: &CallFacts<'_>,
    ) -> Option<(u16, &Entry)> {
        let found = self.languages.get(&language)?.any_receiver.get(method)?;
        let mut applying = found
            .iter()
            .filter_map(|index| self.entry(*index).map(|entry| (*index, entry)))
            .filter(|(_, entry)| entry.applies(call));
        let first = applying.next()?;
        applying
            .all(|(_, other)| other.cost == first.1.cost && other.space == first.1.space)
            .then_some(first)
    }

    /// The entry for a free function, by the name it is called with.
    pub fn function(
        &self,
        language: Language,
        name: &str,
        call: &CallFacts<'_>,
    ) -> Option<(u16, &Entry)> {
        let found = self
            .languages
            .get(&language)?
            .functions
            .get(&function_key(name))?;
        self.first_applying(found, call)
    }

    fn first_applying(&self, found: &[u16], call: &CallFacts<'_>) -> Option<(u16, &Entry)> {
        found
            .iter()
            .filter_map(|index| self.entry(*index).map(|entry| (*index, entry)))
            .find(|(_, entry)| entry.applies(call))
    }
}

impl Index {
    fn register(&mut self, language: Language, source: &CostEntry, position: u16) {
        if let Some(function) = &source.function {
            self.functions
                .entry(function_key(function))
                .or_default()
                .push(position);
        }
        let Some(method) = &source.method else {
            return;
        };
        for receiver in &source.receiver {
            if receiver == ANY_RECEIVER {
                self.any_receiver
                    .entry(method.clone())
                    .or_default()
                    .push(position);
            } else if let Some(head) = canonical_head(language, receiver) {
                self.types.insert(head.clone());
                self.methods
                    .entry((head, method.clone()))
                    .or_default()
                    .push(position);
            }
        }
    }
}

#[cfg(test)]
mod tests;

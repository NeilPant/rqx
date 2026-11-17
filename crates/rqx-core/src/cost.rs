//! What a function costs, as the data the cost analysis passes between its stages: the
//! per-file extraction caches a [`FileCost`], and a later stage composes those across
//! files into a [`Summary`] per function. The types carry no tree-sitter node and no
//! I/O, so they serialise into the scan cache and cross crate boundaries freely.
//!
//! Everything that is empty or at its default is left out of the serialised form,
//! because most functions are O(1) and the cache holds one entry per file.

mod order;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use order::{Factor, Named, Order, ParseOrderError, SizeId, Term, standard_name};

/// Where the size a loop runs over comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SizeSrc {
    /// The parameter at this position.
    Param(u8),
    Local,
    Field,
    Global,
    Unknown,
}

/// A quantity a cost grows with: the length of a collection, or an integer argument.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizeVar {
    pub name: String,
    pub src: SizeSrc,
}

/// How many times a loop runs. The index is into [`FunctionCost::vars`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bound {
    Const,
    Size(u16),
    Log(u16),
    Unknown,
}

/// The size of what is passed to a callee, as far as the caller knows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArgSize {
    Param(u8),
    Local(u16),
    #[default]
    Const,
    Unknown,
}

impl ArgSize {
    pub const fn is_const(&self) -> bool {
        matches!(self, Self::Const)
    }
}

/// The cost of a piece of a function body, before the sizes are known. A function's
/// time and space are each one of these trees; composition turns them into an [`Order`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cost {
    #[default]
    One,
    Loop {
        bound: Bound,
        body: Box<Cost>,
        line: u32,
    },
    /// Statements one after the other: the most expensive one.
    Seq(Vec<Cost>),
    /// Alternative paths: the most expensive one.
    Branch(Vec<Cost>),
    /// A call, by index into [`FunctionCost::calls`].
    Call(u16),
    /// A call into a library function whose cost a table knows. In a function's time
    /// this is the entry's time and in its space the entry's space, with the entry's
    /// `n` being `size` and its `m` being `arg`.
    Lib {
        entry: u16,
        size: ArgSize,
        line: u32,
        /// The size of the first argument, which the entries written in `m` use.
        #[serde(default, skip_serializing_if = "ArgSize::is_const")]
        arg: ArgSize,
    },
}

impl Cost {
    pub const fn is_one(&self) -> bool {
        matches!(self, Self::One)
    }
}

/// How sure the analysis is of a cost. Ordered from least to most: a composed cost is
/// only as sure as its least sure part.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum Conf {
    Unknown,
    Heuristic,
    #[default]
    Verified,
}

impl Conf {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Heuristic => "heuristic",
            Self::Verified => "verified",
        }
    }

    pub fn weaker(self, other: Self) -> Self {
        self.min(other)
    }
}

impl std::str::FromStr for Conf {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        [Self::Verified, Self::Heuristic, Self::Unknown]
            .into_iter()
            .find(|conf| conf.slug() == text)
            .ok_or_else(|| format!("`{text}` is not a confidence: verified, heuristic or unknown"))
    }
}

/// Who a call goes to, as far as the file can tell.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalleeRef {
    /// A function or method of the same file, by name.
    Local(String),
    Method {
        recv_type: Option<String>,
        name: String,
    },
    /// Something imported; `req` indexes the file's import requests.
    Imported {
        local: String,
        member: Option<String>,
        req: u16,
    },
    #[default]
    Unresolved,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    pub callee: CalleeRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<ArgSize>,
    pub line: u32,
    /// How many loops enclose the call, inside this function.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub in_loop: u8,
}

/// How a recursive call's argument compares with the parameter it came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shrink {
    /// One smaller: `n - 1`.
    Dec1,
    /// Half as large: `n / 2`.
    Half,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recurrence {
    /// Self-calls in the body.
    pub calls: u8,
    pub shrink: Shrink,
    /// The position of the parameter that shrinks, which is the one the recursion's
    /// depth is measured in.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub param: u8,
    /// Each call copies its argument, as `xs[1:]` does.
    #[serde(default, skip_serializing_if = "is_false")]
    pub copies_arg: bool,
    /// The body reads a cache before recursing, so repeated arguments cost once.
    #[serde(default, skip_serializing_if = "is_false")]
    pub memo: bool,
}

/// Signals that are not an order of growth but still say a function is costly.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Indicators {
    /// Calls that leave the process, weighted by the loops around them. A single call
    /// outside every loop weighs one, which is also what no call weighs, so
    /// `round_trip` tells the two apart.
    #[serde(default, skip_serializing_if = "Cost::is_one")]
    pub round_trips: Cost,
    /// The body makes a call that leaves the process, however many times.
    #[serde(default, skip_serializing_if = "is_false")]
    pub round_trip: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub allocs_per_iter: u8,
    #[serde(default, skip_serializing_if = "is_false")]
    pub blocking_async: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locks_in_loop: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub unbounded_fanout: bool,
    /// Consecutive awaits that do not depend on one another.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub seq_awaits: u8,
    /// Module-level collections that are only ever added to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub static_growth: Vec<String>,
}

impl Indicators {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    /// The declared type's head, such as `Map` or `list`, when the source states one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_head: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCost {
    pub name: Option<String>,
    /// The name qualified by its class or module, which identifies the function in a
    /// file.
    pub qual: String,
    /// Byte offset of the function, which orders functions within a file.
    pub start: u32,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<SizeVar>,
    #[serde(default, skip_serializing_if = "Cost::is_one")]
    pub time: Cost,
    #[serde(default, skip_serializing_if = "Cost::is_one")]
    pub space: Cost,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<CallSite>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rec: Option<Recurrence>,
    #[serde(default, skip_serializing_if = "Indicators::is_empty")]
    pub ind: Indicators,
    /// How sure the function's own cost is, before its callees are folded in. A loop
    /// whose bound is not known, or a cost taken from a method name alone, makes it
    /// less than verified.
    #[serde(default, skip_serializing_if = "is_verified")]
    pub conf: Conf,
    /// The name under which another file imports this function.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_as: Option<String>,
}

/// The cost facts of one file. Holds only the functions that are not plainly O(1), or
/// that make calls, or carry indicators, so a file of small helpers stores nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCost {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<FunctionCost>,
}

impl FileCost {
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

/// One step of the path that makes a function's cost what it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceStep {
    pub file: String,
    pub line: u32,
    pub what: String,
    pub order: Order,
}

/// A function's cost once its callees are folded in.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub time: Order,
    pub space: Order,
    pub io: Order,
    pub stack: Order,
    pub conf: Conf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<TraceStep>,
}

/// The summaries of one scan, by file and by the byte offset the function starts at.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summaries {
    /// Whether the scan read the whole project. A call into a file the scan did not read
    /// is costed as constant, so a rule that concludes something from a cost that crosses
    /// files has nothing to stand on when this is false.
    pub complete: bool,
    functions: BTreeMap<String, BTreeMap<u32, Summary>>,
}

impl Summaries {
    pub fn new(complete: bool) -> Self {
        Self {
            complete,
            functions: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, file: &str, start: u32, summary: Summary) {
        self.functions
            .entry(file.to_owned())
            .or_default()
            .insert(start, summary);
    }

    pub fn get(&self, file: &str, start: u32) -> Option<&Summary> {
        self.functions.get(file)?.get(&start)
    }

    pub fn len(&self) -> usize {
        self.functions.values().map(BTreeMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }

    /// Every summary, by file and then by position in it.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u32, &Summary)> {
        self.functions.iter().flat_map(|(file, functions)| {
            functions
                .iter()
                .map(move |(start, summary)| (file.as_str(), *start, summary))
        })
    }
}

fn is_verified(conf: &Conf) -> bool {
    *conf == Conf::Verified
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_function_with_nothing_to_say_serialises_to_its_identity() {
        let plain = FunctionCost {
            name: Some("add".to_owned()),
            qual: "add".to_owned(),
            start: 10,
            line: 2,
            ..FunctionCost::default()
        };
        assert_eq!(
            serde_json::to_string(&plain).unwrap(),
            r#"{"name":"add","qual":"add","start":10,"line":2}"#
        );
    }

    #[test]
    fn a_function_cost_survives_the_cache_round_trip() {
        let cost = FunctionCost {
            name: Some("find".to_owned()),
            qual: "Index.find".to_owned(),
            start: 40,
            line: 5,
            params: vec![Param {
                name: "items".to_owned(),
                type_head: Some("list".to_owned()),
            }],
            vars: vec![SizeVar {
                name: "items".to_owned(),
                src: SizeSrc::Param(0),
            }],
            time: Cost::Loop {
                bound: Bound::Size(0),
                body: Box::new(Cost::Seq(vec![Cost::One, Cost::Call(0)])),
                line: 6,
            },
            space: Cost::One,
            calls: vec![CallSite {
                callee: CalleeRef::Method {
                    recv_type: None,
                    name: "has".to_owned(),
                },
                args: vec![ArgSize::Param(0), ArgSize::Const],
                line: 7,
                in_loop: 1,
            }],
            rec: Some(Recurrence {
                calls: 2,
                shrink: Shrink::Dec1,
                param: 1,
                copies_arg: false,
                memo: true,
            }),
            ind: Indicators {
                round_trip: true,
                seq_awaits: 2,
                static_growth: vec!["REGISTRY".to_owned()],
                ..Indicators::default()
            },
            conf: Conf::Heuristic,
            exported_as: Some("find".to_owned()),
        };
        let file = FileCost {
            functions: vec![cost],
        };
        let json = serde_json::to_string(&file).unwrap();
        assert_eq!(serde_json::from_str::<FileCost>(&json).unwrap(), file);
    }

    #[test]
    fn a_library_call_leaves_out_the_argument_size_it_does_not_use() {
        let uses_one = Cost::Lib {
            entry: 3,
            size: ArgSize::Param(0),
            line: 8,
            arg: ArgSize::Const,
        };
        let json = serde_json::to_string(&uses_one).unwrap();
        assert!(!json.contains("arg"), "{json}");
        assert_eq!(serde_json::from_str::<Cost>(&json).unwrap(), uses_one);

        let uses_both = Cost::Lib {
            entry: 3,
            size: ArgSize::Param(0),
            line: 8,
            arg: ArgSize::Param(1),
        };
        let json = serde_json::to_string(&uses_both).unwrap();
        assert_eq!(serde_json::from_str::<Cost>(&json).unwrap(), uses_both);
    }

    #[test]
    fn a_file_without_cost_facts_reads_as_empty() {
        let file: FileCost = serde_json::from_str("{}").unwrap();
        assert!(file.is_empty());
        assert_eq!(serde_json::to_string(&file).unwrap(), "{}");
    }

    #[test]
    fn a_composed_cost_is_only_as_sure_as_its_least_sure_part() {
        assert_eq!(Conf::Verified.weaker(Conf::Heuristic), Conf::Heuristic);
        assert_eq!(Conf::Heuristic.weaker(Conf::Unknown), Conf::Unknown);
        assert_eq!(Conf::default(), Conf::Verified);
        assert_eq!("heuristic".parse(), Ok(Conf::Heuristic));
        assert!("certain".parse::<Conf>().is_err());
    }

    #[test]
    fn summaries_are_found_by_file_and_start_and_listed_in_order() {
        let mut found = Summaries::new(true);
        let quadratic = Summary {
            time: "O(n²)".parse().unwrap(),
            ..Summary::default()
        };
        found.insert("b.ts", 5, Summary::default());
        found.insert("a.ts", 90, quadratic.clone());
        found.insert("a.ts", 12, Summary::default());

        assert_eq!(found.get("a.ts", 90), Some(&quadratic));
        assert_eq!(found.get("a.ts", 13), None);
        assert_eq!(found.len(), 3);
        let order: Vec<(&str, u32)> = found.iter().map(|(file, at, _)| (file, at)).collect();
        assert_eq!(order, [("a.ts", 12), ("a.ts", 90), ("b.ts", 5)]);
        assert!(found.complete);
        assert!(!Summaries::default().complete);
    }

    #[test]
    fn a_summary_carries_its_orders_and_trace() {
        let summary = Summary {
            time: "O(n²)".parse().unwrap(),
            conf: Conf::Heuristic,
            trace: vec![TraceStep {
                file: "a.ts".to_owned(),
                line: 90,
                what: "loop".to_owned(),
                order: "O(n)".parse().unwrap(),
            }],
            ..Summary::default()
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert_eq!(serde_json::from_str::<Summary>(&json).unwrap(), summary);
    }
}

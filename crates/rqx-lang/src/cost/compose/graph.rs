//! The call graph: a node for every function the analysis recorded, and for each of its
//! calls the functions it may reach. Everything is ordered by file and by position in
//! it, so the graph is the same however the files arrived.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use rqx_core::cost::{CallSite, CalleeRef, FunctionCost, SizeId, SizeSrc, SizeVar};
use rqx_core::facts::{ImportRequest, ModuleFacts};

use super::Unit;
use crate::cost::tables::CostTables;
use crate::module_graph::ResolvedRequests;

pub type NodeId = usize;

/// A call that could reach more functions than this is taken to reach none: a name that
/// common says nothing about which function was meant.
const MOST_CANDIDATES: usize = 4;

/// How many files a name is followed through: a barrel that re-exports from a barrel
/// that re-exports from a module is three.
const REEXPORT_DEPTH: u8 = 3;

pub struct Node<'a> {
    pub unit: usize,
    pub function: &'a FunctionCost,
    /// The function's size variables, and the one its recursion is measured in where
    /// the function does not measure that parameter itself.
    pub vars: Cow<'a, [SizeVar]>,
}

/// The functions a call may reach.
#[derive(Default)]
pub struct Callees {
    pub nodes: Vec<NodeId>,
    /// Which of them is called was settled by a name, not by what is known of the callee.
    pub guessed: bool,
}

pub struct Graph<'a> {
    units: Vec<&'a Unit<'a>>,
    pub nodes: Vec<Node<'a>>,
    /// Per unit, the functions defined in it by the name they are declared with.
    in_unit: Vec<HashMap<&'a str, Vec<NodeId>>>,
    /// Per unit, the functions other files can import, by the name they import them as.
    exported: Vec<HashMap<&'a str, Vec<NodeId>>>,
    by_name: HashMap<&'a str, Vec<NodeId>>,
    by_qualified_name: HashMap<&'a str, Vec<NodeId>>,
    /// The units each import request of each unit names.
    imports: HashMap<(usize, usize), Vec<usize>>,
    tables: &'a CostTables,
    /// For each node, what each of its calls may reach.
    pub calls: Vec<Vec<Callees>>,
}

impl<'a> Graph<'a> {
    /// `units` must already be in the order their names sort in.
    pub fn new(
        units: Vec<&'a Unit<'a>>,
        requests: &ResolvedRequests,
        tables: &'a CostTables,
    ) -> Self {
        let mut graph = Self {
            in_unit: vec![HashMap::new(); units.len()],
            exported: vec![HashMap::new(); units.len()],
            imports: imports_by_unit(&units, requests),
            units,
            nodes: Vec::new(),
            by_name: HashMap::new(),
            by_qualified_name: HashMap::new(),
            tables,
            calls: Vec::new(),
        };
        graph.add_nodes();
        graph.calls = (0..graph.nodes.len())
            .map(|node| {
                graph.nodes[node]
                    .function
                    .calls
                    .iter()
                    .map(|call| graph.resolve(node, call))
                    .collect()
            })
            .collect();
        graph
    }

    fn add_nodes(&mut self) {
        for unit in 0..self.units.len() {
            let found: &'a Unit<'a> = self.units[unit];
            let mut functions: Vec<&'a FunctionCost> = found.cost.functions.iter().collect();
            functions.sort_by_key(|function| function.start);
            for function in functions {
                let id = self.nodes.len();
                self.nodes.push(Node {
                    unit,
                    function,
                    vars: Cow::Borrowed(&function.vars),
                });
                self.by_qualified_name
                    .entry(&function.qual)
                    .or_default()
                    .push(id);
                if let Some(name) = function.name.as_deref() {
                    self.in_unit[unit].entry(name).or_default().push(id);
                    self.by_name.entry(name).or_default().push(id);
                }
                if let Some(name) = function.exported_as.as_deref() {
                    self.exported[unit].entry(name).or_default().push(id);
                }
            }
        }
    }

    pub fn file_of(&self, node: NodeId) -> &'a str {
        self.units[self.nodes[node].unit].name
    }

    pub fn entry_id(&self, entry: u16) -> String {
        self.tables
            .entry(entry)
            .map_or_else(String::new, |found| found.id.clone())
    }

    /// The variable a function's recursion is measured in: the one for the parameter that
    /// shrinks, added to the function's variables when the body never measures it.
    pub fn depth_variable(&mut self, node: NodeId, param: u8) -> SizeId {
        let found = self.nodes[node]
            .vars
            .iter()
            .position(|var| var.src == SizeSrc::Param(param));
        let position = found.unwrap_or_else(|| {
            let function = self.nodes[node].function;
            let name = function
                .params
                .get(usize::from(param))
                .map_or_else(|| "n".to_owned(), |param| param.name.clone());
            let src = if usize::from(param) < function.params.len() {
                SizeSrc::Param(param)
            } else {
                SizeSrc::Unknown
            };
            let vars = self.nodes[node].vars.to_mut();
            vars.push(SizeVar { name, src });
            vars.len() - 1
        });
        SizeId::try_from(position).unwrap_or(SizeId::MAX)
    }

    fn resolve(&self, caller: NodeId, call: &CallSite) -> Callees {
        let node = &self.nodes[caller];
        let (candidates, guessed) = match &call.callee {
            CalleeRef::Local(name) => (self.local(node, name), false),
            CalleeRef::Method {
                recv_type: Some(class),
                name,
            } => {
                let own = self
                    .by_qualified_name
                    .get(format!("{class}.{name}").as_str())
                    .cloned()
                    .unwrap_or_default();
                if own.is_empty() {
                    (self.named(name), true)
                } else {
                    (own, false)
                }
            }
            CalleeRef::Method {
                recv_type: None,
                name,
            } => (self.named(name), true),
            CalleeRef::Imported {
                member: Some(member),
                req,
                ..
            } => (self.imported(node.unit, usize::from(*req), member), false),
            CalleeRef::Imported { member: None, .. } | CalleeRef::Unresolved => {
                return Callees::default();
            }
        };
        self.choose(candidates, call.args.len(), guessed)
    }

    /// A call by a bare name: a function of the file, or a method of the class the
    /// caller is in.
    fn local(&self, caller: &Node<'a>, name: &str) -> Vec<NodeId> {
        let Some(defined) = self.in_unit[caller.unit].get(name) else {
            return Vec::new();
        };
        let qualified = |node: &NodeId| self.nodes[*node].function.qual.as_str();
        let plain: Vec<NodeId> = defined
            .iter()
            .copied()
            .filter(|node| qualified(node) == name)
            .collect();
        if !plain.is_empty() {
            return plain;
        }
        if let Some((class, _)) = caller.function.qual.rsplit_once('.') {
            let own = format!("{class}.{name}");
            let of_class: Vec<NodeId> = defined
                .iter()
                .copied()
                .filter(|node| qualified(node) == own)
                .collect();
            if !of_class.is_empty() {
                return of_class;
            }
        }
        defined.clone()
    }

    fn named(&self, name: &str) -> Vec<NodeId> {
        self.by_name.get(name).cloned().unwrap_or_default()
    }

    fn imported(&self, unit: usize, request: usize, member: &str) -> Vec<NodeId> {
        self.imports
            .get(&(unit, request))
            .into_iter()
            .flatten()
            .flat_map(|target| self.exported_by(*target, member, 0))
            .collect()
    }

    /// The functions `member` names among what a file exports, following the file's
    /// re-exports when it defines no such function itself.
    fn exported_by(&self, unit: usize, member: &str, depth: u8) -> Vec<NodeId> {
        if let Some(own) = self.exported[unit].get(member) {
            return own.clone();
        }
        if member == "default" {
            let mut offered = self.exported[unit].values().flatten();
            if let (Some(only), None) = (offered.next(), offered.next()) {
                return vec![*only];
            }
        }
        if depth >= REEXPORT_DEPTH {
            return Vec::new();
        }
        let facts = self.units[unit].facts;
        let alias = renamed_re_export(facts);
        for (position, request) in facts.imports.iter().enumerate() {
            let Some(targets) = self.imports.get(&(unit, position)) else {
                continue;
            };
            for name in followed_names(request, member, alias) {
                let found: Vec<NodeId> = targets
                    .iter()
                    .flat_map(|target| self.exported_by(*target, name, depth + 1))
                    .collect();
                if !found.is_empty() {
                    return found;
                }
            }
        }
        Vec::new()
    }

    /// Narrows what a call may reach to what fits the call, and gives up on a call
    /// that fits too many functions to be any of them.
    fn choose(&self, mut candidates: Vec<NodeId>, arguments: usize, guessed: bool) -> Callees {
        let fitting: Vec<NodeId> = candidates
            .iter()
            .copied()
            .filter(|node| self.nodes[*node].function.params.len() == arguments)
            .collect();
        if !fitting.is_empty() {
            candidates = fitting;
        }
        candidates.sort_unstable();
        candidates.dedup();
        if candidates.is_empty() || candidates.len() > MOST_CANDIDATES {
            return Callees::default();
        }
        Callees {
            guessed: guessed || candidates.len() > 1,
            nodes: candidates,
        }
    }
}

fn imports_by_unit(
    units: &[&Unit<'_>],
    requests: &ResolvedRequests,
) -> HashMap<(usize, usize), Vec<usize>> {
    let unit_of: HashMap<&Path, usize> = units
        .iter()
        .enumerate()
        .map(|(position, unit)| (unit.path, position))
        .collect();
    requests
        .iter()
        .filter_map(|((importer, request), targets)| {
            let importer = *unit_of.get(importer.as_path())?;
            let targets: Vec<usize> = targets
                .iter()
                .filter_map(|target| unit_of.get(target.as_path()).copied())
                .collect();
            (!targets.is_empty()).then_some(((importer, *request), targets))
        })
        .collect()
}

/// The one name a file re-exports under another, as `export { other as renamed } from`
/// does. A request lists the names it takes and the file lists the names it exports,
/// and nothing pairs them, so a rename can only be followed when it is the only one.
fn renamed_re_export(facts: &ModuleFacts) -> Option<&str> {
    let exported = |name: &str| facts.exports.iter().any(|export| export.name == name);
    let mut unexplained = facts
        .imports
        .iter()
        .filter(|request| request.re_export)
        .flat_map(|request| request.names.iter())
        .map(String::as_str)
        .filter(|name| !exported(name));
    match (unexplained.next(), unexplained.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

/// The names to look for in the file a request reaches when something is wanted under
/// the name `member`: the name it was imported under before it was exported again, the
/// name itself through a re-export, and the renamed one.
fn followed_names<'r>(
    request: &'r ImportRequest,
    member: &'r str,
    alias: Option<&'r str>,
) -> Vec<&'r str> {
    let mut names = Vec::new();
    if let Some(at) = request.locals.iter().position(|local| local == member)
        && let Some(original) = request.names.get(at)
    {
        names.push(original.as_str());
    }
    if request.re_export {
        if request.takes_everything || request.names.iter().any(|name| name == member) {
            names.push(member);
        }
        names.extend(alias.filter(|name| request.names.iter().any(|taken| taken == name)));
    }
    names.dedup();
    names
}

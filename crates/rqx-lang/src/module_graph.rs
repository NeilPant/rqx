use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use rqx_core::facts::{ExportedSymbol, ModuleFacts};
use rqx_core::span::Span;

/// How one language turns a written import into a file on disk, and which files the
/// outside world reaches directly. Everything else about the graph — cycles, dead
/// exports, blast radius — is the same question in every language.
pub trait Resolver: Sync {
    /// Empty when the specifier names something outside the project, which carries
    /// no information about dead code inside it. More than one file when the language
    /// imports a unit larger than a file: Go imports a package, which is a directory.
    fn resolve(&self, importer: &Path, specifier: &str) -> Vec<PathBuf>;

    /// Modules the outside world enters through. Their exports cannot be judged by
    /// who imports them inside the project.
    fn entrypoints(
        &self,
        root: &Path,
        configured: &[String],
        known: &HashSet<PathBuf>,
    ) -> HashSet<PathBuf>;
}

pub struct ModuleEntry {
    pub path: PathBuf,
    pub language: rqx_core::Language,
    pub facts: ModuleFacts,
}

#[derive(Debug)]
pub struct DeadExport {
    pub path: PathBuf,
    pub name: String,
    pub kind: &'static str,
    pub span: Span,
}

#[derive(Debug)]
pub struct DeadFile {
    pub path: PathBuf,
    pub exports: u32,
}

#[derive(Clone, Debug)]
pub struct External {
    pub specifier: String,
    pub importers: Vec<PathBuf>,
    /// Where the first importer names it, so advice can point at a real line.
    pub path: PathBuf,
    pub span: Span,
}

#[derive(Debug)]
pub struct Cycle {
    pub members: Vec<PathBuf>,
}

#[derive(Debug, Default)]
pub struct GraphReport {
    pub dead_exports: Vec<DeadExport>,
    pub external: Vec<External>,
    pub dead_files: Vec<DeadFile>,
    pub cycles: Vec<Cycle>,
    /// How many modules import each file: the blast radius of changing it.
    pub importers: BTreeMap<PathBuf, u32>,
    /// importer -> imported, retained so a diff can find a change's dependents.
    pub edges: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    /// Where each edge is written, so a rule about a forbidden import can point at
    /// the line rather than at the file. The first request wins where a module is
    /// imported twice; requests arrive in source order.
    pub edge_spans: BTreeMap<(PathBuf, PathBuf), Span>,
    /// Interfaces and abstract classes nothing in the project implements.
    pub unimplemented: Vec<DeadExport>,
    /// Two hierarchies whose subclasses are named for the same set of things.
    pub parallel: Vec<Parallel>,
}

#[derive(Debug)]
pub struct Parallel {
    pub path: PathBuf,
    pub parent: String,
    pub twin: String,
    pub shared: Vec<String>,
    pub span: Span,
}

/// The leading word of a subclass name is what the subclass is *for*: `DogFeeder`
/// and `DogFood` are both about dogs. Two hierarchies whose subclasses are named for
/// the same set of things move together, which is what the smell is about.
fn leading_word(name: &str) -> String {
    let mut word = String::new();
    for (index, character) in name.char_indices() {
        if index > 0 && character.is_uppercase() {
            break;
        }
        word.push(character);
    }
    word
}

/// Subclass leading-word to where that subclass is declared.
type Branches<'a> = BTreeMap<String, (&'a PathBuf, Span)>;

fn parallel_hierarchies(by_path: &BTreeMap<&PathBuf, &ModuleFacts>) -> Vec<Parallel> {
    let mut children: BTreeMap<&str, Branches<'_>> = BTreeMap::new();
    for (path, facts) in by_path {
        for entry in &facts.heritage {
            children
                .entry(entry.parent.as_str())
                .or_default()
                .insert(leading_word(&entry.child), (*path, entry.span));
        }
    }

    let mut found = Vec::new();
    let hierarchies: Vec<(&&str, &Branches<'_>)> = children
        .iter()
        .filter(|(_, kids)| kids.len() >= 2)
        .collect();
    for (index, (parent, kids)) in hierarchies.iter().enumerate() {
        for (twin, others) in &hierarchies[index + 1..] {
            let shared: Vec<String> = kids
                .keys()
                .filter(|word| others.contains_key(*word))
                .cloned()
                .collect();
            if shared.len() < 2 || shared.len() != kids.len() || shared.len() != others.len() {
                continue;
            }
            let (path, span) = kids.values().next().expect("a hierarchy has children");
            found.push(Parallel {
                path: (*path).clone(),
                parent: (**parent).to_owned(),
                twin: (**twin).to_owned(),
                shared,
                span: *span,
            });
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path).then(a.span.start.cmp(&b.span.start)));
    found
}

struct Edges {
    /// importer -> imported modules
    out: HashMap<PathBuf, BTreeSet<PathBuf>>,
    spans: HashMap<(PathBuf, PathBuf), Span>,
    /// module -> names other modules take from it
    used_names: HashMap<PathBuf, HashSet<String>>,
    /// modules imported wholesale, whose export list cannot be narrowed
    opaque: HashSet<PathBuf>,
    imported_at_all: HashSet<PathBuf>,
    /// Specifiers that resolve to nothing, which is every third-party package. The
    /// graph has no use for them, but "eleven of your files reach into `stripe`
    /// directly" is a fact only this loop is in a position to notice.
    external: BTreeMap<String, Vec<(PathBuf, Span)>>,
}

fn resolve_edges(
    modules: &[ModuleEntry],
    resolver: &dyn Resolver,
    is_vendor: fn(&str) -> bool,
) -> Edges {
    let known: HashSet<&PathBuf> = modules.iter().map(|module| &module.path).collect();
    let mut edges = Edges {
        out: HashMap::new(),
        spans: HashMap::new(),
        used_names: HashMap::new(),
        opaque: HashSet::new(),
        imported_at_all: HashSet::new(),
        external: BTreeMap::new(),
    };

    // The same file is the target of many imports, and canonicalising is a syscall on
    // the one thread the whole parallel walk funnels into.
    let mut resolved: HashMap<PathBuf, Option<PathBuf>> = HashMap::new();
    for module in modules {
        let Some(directory) = module.path.parent() else {
            continue;
        };
        for request in &module.facts.imports {
            let targets = resolver.resolve(directory, &request.specifier);
            // A relative specifier that resolves to nothing is a broken import, not a
            // dependency; only a bare package name is worth counting.
            // Java synthesises a request per referenced type to resolve it against
            // the package, and marks those with an empty span. They are not written
            // dependencies, so counting them would suggest wrapping `String`.
            let written = request.span.end > request.span.start;
            if written
                && targets.is_empty()
                && !request.specifier.starts_with(['.', '/'])
                && is_vendor(&request.specifier)
            {
                edges
                    .external
                    .entry(request.specifier.clone())
                    .or_default()
                    .push((module.path.clone(), request.span));
            }
            for target in targets {
                let canonical = resolved
                    .entry(target)
                    .or_insert_with_key(|target| target.canonicalize().ok());
                let Some(target) = canonical.clone() else {
                    continue;
                };
                if !known.contains(&target) || target == module.path {
                    continue;
                }
                edges
                    .out
                    .entry(module.path.clone())
                    .or_default()
                    .insert(target.clone());
                if written {
                    edges
                        .spans
                        .entry((module.path.clone(), target.clone()))
                        .or_insert(request.span);
                }
                edges.imported_at_all.insert(target.clone());
                if request.takes_everything {
                    edges.opaque.insert(target.clone());
                }
                edges
                    .used_names
                    .entry(target)
                    .or_default()
                    .extend(request.names.iter().cloned());
            }
        }
    }
    edges
}

/// Tarjan's strongly connected components: every component of more than one module
/// is an import cycle, and reporting components rather than individual back-edges
/// means one finding per cycle instead of one per edge in it.
fn strongly_connected(
    nodes: &[PathBuf],
    out: &HashMap<PathBuf, BTreeSet<PathBuf>>,
) -> Vec<Vec<PathBuf>> {
    let index_of: HashMap<&PathBuf, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, path)| (path, index))
        .collect();
    let count = nodes.len();
    // Resolved once. The traversal visits a node once per edge out of it, and looking
    // its neighbours up by path each time hashes and compares a `PathBuf` per edge.
    let adjacency: Vec<Vec<usize>> = nodes
        .iter()
        .map(|path| {
            out.get(path)
                .map(|targets| {
                    targets
                        .iter()
                        .filter_map(|t| index_of.get(t).copied())
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    let mut index = vec![usize::MAX; count];
    let mut low = vec![0usize; count];
    let mut on_stack = vec![false; count];
    let mut stack: Vec<usize> = Vec::new();
    let mut next = 0usize;
    let mut components = Vec::new();

    for root in 0..count {
        if index[root] != usize::MAX {
            continue;
        }
        // Explicit stack: a deep import chain would blow the call stack on a
        // recursive Tarjan, and import chains in real monorepos get very deep.
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some((node, child)) = work.pop() {
            if child == 0 {
                index[node] = next;
                low[node] = next;
                next += 1;
                stack.push(node);
                on_stack[node] = true;
            }
            let neighbours = &adjacency[node];

            if child < neighbours.len() {
                work.push((node, child + 1));
                let target = neighbours[child];
                if index[target] == usize::MAX {
                    work.push((target, 0));
                } else if on_stack[target] {
                    low[node] = low[node].min(index[target]);
                }
                continue;
            }

            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == index[node] {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component.push(nodes[member].clone());
                    if member == node {
                        break;
                    }
                }
                if component.len() > 1 {
                    component.sort();
                    components.push(component);
                }
            }
        }
    }
    components.sort();
    components
}

pub fn analyze(
    modules: Vec<ModuleEntry>,
    root: &Path,
    configured: &[String],
    resolver: &dyn Resolver,
    is_vendor: fn(&str) -> bool,
) -> GraphReport {
    let known: HashSet<PathBuf> = modules.iter().map(|module| module.path.clone()).collect();
    let entries = resolver.entrypoints(root, configured, &known);
    let edges = resolve_edges(&modules, resolver, is_vendor);

    let mut report = GraphReport::default();
    let by_path: BTreeMap<&PathBuf, &ModuleFacts> = modules
        .iter()
        .map(|module| (&module.path, &module.facts))
        .collect();

    for (path, facts) in &by_path {
        if entries.contains(*path) || facts.exports.is_empty() {
            continue;
        }
        let used = edges.used_names.get(*path);
        let opaque = edges.opaque.contains(*path);
        let dead: Vec<&ExportedSymbol> = facts
            .exports
            .iter()
            .filter(|export| !opaque && !used.is_some_and(|names| names.contains(&export.name)))
            .collect();

        if dead.len() == facts.exports.len() && !edges.imported_at_all.contains(*path) {
            // Every export dead and nobody importing the file at all: one finding
            // about the file beats one per export saying the same thing.
            report.dead_files.push(DeadFile {
                path: (*path).clone(),
                exports: facts.exports.len() as u32,
            });
            continue;
        }
        for export in dead {
            report.dead_exports.push(DeadExport {
                path: (*path).clone(),
                name: export.name.clone(),
                kind: export.kind.label(),
                span: export.span,
            });
        }
    }

    for targets in edges.out.values() {
        for target in targets {
            *report.importers.entry(target.clone()).or_insert(0) += 1;
        }
    }

    // An abstraction is speculative only if *nothing anywhere* implements it, so the
    // question can only be settled once every file has been read.
    let implemented: HashSet<&str> = by_path
        .values()
        .flat_map(|facts| facts.implemented.iter().map(String::as_str))
        .collect();
    for (path, facts) in &by_path {
        for abstraction in &facts.abstractions {
            if !implemented.contains(abstraction.name.as_str()) {
                report.unimplemented.push(DeadExport {
                    path: (*path).clone(),
                    name: abstraction.name.clone(),
                    kind: abstraction.kind.label(),
                    span: abstraction.span,
                });
            }
        }
    }
    report
        .unimplemented
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.span.start.cmp(&b.span.start)));
    report.parallel = parallel_hierarchies(&by_path);
    report.external = edges
        .external
        .iter()
        .filter_map(|(specifier, sites)| {
            // The walk hands modules over as its workers finish, so "the first file
            // that imported this" depends on the thread count unless it is ordered.
            let mut sites = sites.clone();
            sites
                .sort_by(|left, right| left.0.cmp(&right.0).then(left.1.start.cmp(&right.1.start)));
            let (path, span) = sites.first()?;
            let mut importers: Vec<PathBuf> = sites.iter().map(|(path, _)| path.clone()).collect();
            importers.dedup();
            Some(External {
                specifier: specifier.clone(),
                importers,
                path: path.clone(),
                span: *span,
            })
        })
        .collect();

    let nodes: Vec<PathBuf> = modules.into_iter().map(|module| module.path).collect();
    report.cycles = strongly_connected(&nodes, &edges.out)
        .into_iter()
        .map(|members| Cycle { members })
        .collect();
    // Moved rather than deep-copied: this used to clone every path and every target set
    // in the graph, and then drop the originals unread.
    report.edge_spans = edges.spans.into_iter().collect();
    report.edges = edges.out.into_iter().collect();

    report
        .dead_exports
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.span.start.cmp(&b.span.start)));
    report.dead_files.sort_by(|a, b| a.path.cmp(&b.path));
    report
}

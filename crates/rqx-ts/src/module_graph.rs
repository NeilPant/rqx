use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use oxc_resolver::{
    ResolveOptions, Resolver, TsconfigDiscovery, TsconfigOptions, TsconfigReferences,
};

use crate::semantics::{ExportedSymbol, ModuleFacts};

pub struct ModuleEntry {
    pub path: PathBuf,
    pub facts: ModuleFacts,
}

#[derive(Debug)]
pub struct DeadExport {
    pub path: PathBuf,
    pub name: String,
    pub kind: &'static str,
    pub span: oxc::span::Span,
}

#[derive(Debug)]
pub struct DeadFile {
    pub path: PathBuf,
    pub exports: u32,
}

#[derive(Debug)]
pub struct Cycle {
    pub members: Vec<PathBuf>,
}

#[derive(Debug, Default)]
pub struct GraphReport {
    pub dead_exports: Vec<DeadExport>,
    pub dead_files: Vec<DeadFile>,
    pub cycles: Vec<Cycle>,
    /// How many modules import each file: the blast radius of changing it.
    pub importers: BTreeMap<PathBuf, u32>,
    /// importer -> imported, retained so a diff can find a change's dependents.
    pub edges: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    /// Interfaces and abstract classes nothing in the project implements.
    pub unimplemented: Vec<DeadExport>,
}

fn build_resolver(root: &Path) -> Resolver {
    let tsconfig = root.join("tsconfig.json");
    let options = ResolveOptions {
        extensions: [".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"]
            .map(str::to_owned)
            .to_vec(),
        main_fields: ["module", "main"].map(str::to_owned).to_vec(),
        condition_names: ["import", "require", "default"].map(str::to_owned).to_vec(),
        tsconfig: tsconfig.exists().then_some(TsconfigDiscovery::Manual(TsconfigOptions {
            config_file: tsconfig,
            references: TsconfigReferences::Auto,
        })),
        ..ResolveOptions::default()
    };
    Resolver::new(options)
}

/// Packages resolve into `node_modules`, which is not part of the project and is
/// never scanned, so an edge into one carries no information about dead code.
fn is_project_file(path: &Path) -> bool {
    !path.components().any(|part| part.as_os_str() == "node_modules")
}

/// A workspace has one `package.json` per package, each declaring its own entry
/// points, so reading only the root manifest calls every package's public surface
/// dead.
fn package_manifests(root: &Path, depth: u32, found: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let manifest = root.join("package.json");
    if manifest.is_file() {
        found.push(manifest);
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() && !name.starts_with('.') && name != "node_modules" {
            package_manifests(&path, depth - 1, found);
        }
    }
}

fn read_package_entrypoints(manifest: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(manifest) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for key in ["main", "module", "types", "typings", "browser"] {
        if let Some(value) = json.get(key).and_then(serde_json::Value::as_str) {
            found.push(value.to_owned());
        }
    }
    let mut walk = |value: Option<&serde_json::Value>| {
        let mut stack = vec![];
        if let Some(value) = value {
            stack.push(value);
        }
        while let Some(node) = stack.pop() {
            match node {
                serde_json::Value::String(text) => found.push(text.clone()),
                serde_json::Value::Object(map) => stack.extend(map.values()),
                serde_json::Value::Array(items) => stack.extend(items),
                _ => {}
            }
        }
    };
    walk(json.get("exports"));
    walk(json.get("bin"));
    found
}

/// An entry point is a module the outside world reaches directly, so its exports
/// cannot be judged by who imports them inside the project.
fn entrypoints(root: &Path, configured: &[String], known: &HashSet<PathBuf>) -> HashSet<PathBuf> {
    let mut manifests = Vec::new();
    package_manifests(root, 5, &mut manifests);

    let mut declared: Vec<(PathBuf, String)> = Vec::new();
    for manifest in &manifests {
        let base = manifest.parent().unwrap_or(root).to_path_buf();
        declared.extend(
            read_package_entrypoints(manifest)
                .into_iter()
                .map(|entry| (base.clone(), entry)),
        );
    }
    declared.extend(configured.iter().map(|entry| (root.to_path_buf(), entry.clone())));

    let mut entries = HashSet::new();
    for (base, candidate) in declared {
        let root = base.as_path();
        let trimmed = candidate.trim_start_matches("./");
        let direct = root.join(trimmed);
        if let Ok(canonical) = direct.canonicalize()
            && known.contains(&canonical)
        {
            entries.insert(canonical);
            continue;
        }
        // package.json points at build output; the source beside it is the real entry.
        for extension in ["ts", "tsx", "mts", "cts"] {
            let source = direct.with_extension(extension);
            if let Ok(canonical) = source.canonicalize()
                && known.contains(&canonical)
            {
                entries.insert(canonical);
            }
        }
        let rewritten = trimmed
            .replacen("dist/", "src/", 1)
            .replacen("build/", "src/", 1)
            .replacen("lib/", "src/", 1);
        for extension in ["ts", "tsx", "mts", "cts"] {
            let source = root.join(&rewritten).with_extension(extension);
            if let Ok(canonical) = source.canonicalize()
                && known.contains(&canonical)
            {
                entries.insert(canonical);
            }
        }
    }
    entries
}

struct Edges {
    /// importer -> imported modules
    out: HashMap<PathBuf, BTreeSet<PathBuf>>,
    /// module -> names other modules take from it
    used_names: HashMap<PathBuf, HashSet<String>>,
    /// modules imported wholesale, whose export list cannot be narrowed
    opaque: HashSet<PathBuf>,
    imported_at_all: HashSet<PathBuf>,
}

fn resolve_edges(modules: &[ModuleEntry], root: &Path) -> Edges {
    let resolver = build_resolver(root);
    let known: HashSet<&PathBuf> = modules.iter().map(|module| &module.path).collect();
    let mut edges = Edges {
        out: HashMap::new(),
        used_names: HashMap::new(),
        opaque: HashSet::new(),
        imported_at_all: HashSet::new(),
    };

    for module in modules {
        let Some(directory) = module.path.parent() else {
            continue;
        };
        for request in &module.facts.imports {
            let Ok(resolution) = resolver.resolve(directory, &request.specifier) else {
                continue;
            };
            let Ok(target) = resolution.full_path().canonicalize() else {
                continue;
            };
            if !is_project_file(&target) || !known.contains(&target) {
                continue;
            }
            edges.out.entry(module.path.clone()).or_default().insert(target.clone());
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
    edges
}

/// Tarjan's strongly connected components: every component of more than one module
/// is an import cycle, and reporting components rather than individual back-edges
/// means one finding per cycle instead of one per edge in it.
fn strongly_connected(nodes: &[PathBuf], out: &HashMap<PathBuf, BTreeSet<PathBuf>>) -> Vec<Vec<PathBuf>> {
    let index_of: HashMap<&PathBuf, usize> =
        nodes.iter().enumerate().map(|(index, path)| (path, index)).collect();
    let count = nodes.len();
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
            let neighbours: Vec<usize> = out
                .get(&nodes[node])
                .map(|targets| targets.iter().filter_map(|t| index_of.get(t).copied()).collect())
                .unwrap_or_default();

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

pub fn analyze(modules: Vec<ModuleEntry>, root: &Path, configured: &[String]) -> GraphReport {
    let known: HashSet<PathBuf> = modules.iter().map(|module| module.path.clone()).collect();
    let entries = entrypoints(root, configured, &known);
    let edges = resolve_edges(&modules, root);

    let mut report = GraphReport::default();
    let by_path: BTreeMap<&PathBuf, &ModuleFacts> =
        modules.iter().map(|module| (&module.path, &module.facts)).collect();

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

    report.edges = edges.out.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
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
    report.unimplemented.sort_by(|a, b| a.path.cmp(&b.path).then(a.span.start.cmp(&b.span.start)));

    let nodes: Vec<PathBuf> = modules.into_iter().map(|module| module.path).collect();
    report.cycles = strongly_connected(&nodes, &edges.out)
        .into_iter()
        .map(|members| Cycle { members })
        .collect();

    report.dead_exports.sort_by(|a, b| a.path.cmp(&b.path).then(a.span.start.cmp(&b.span.start)));
    report.dead_files.sort_by(|a, b| a.path.cmp(&b.path));
    report
}

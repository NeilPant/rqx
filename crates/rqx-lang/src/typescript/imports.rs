//! Turning a TypeScript import specifier into a file. Only project-internal
//! resolution is implemented: an edge into `node_modules` says nothing about dead
//! code inside the project, so those specifiers resolve to nothing on purpose.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::module_graph::Resolver;

const EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];
const SOURCE_EXTENSIONS: [&str; 4] = ["ts", "tsx", "mts", "cts"];

pub struct TypeScriptResolver {
    /// `tsconfig.json` path aliases, longest prefix first so `@app/ui/*` wins over
    /// `@app/*`.
    aliases: Vec<(String, Vec<String>)>,
    base: PathBuf,
}

impl TypeScriptResolver {
    pub fn new(root: &Path) -> Self {
        let mut aliases = read_tsconfig_paths(&root.join("tsconfig.json"));
        aliases.sort_by_key(|(pattern, _)| std::cmp::Reverse(pattern.len()));
        Self {
            aliases,
            base: root.to_path_buf(),
        }
    }

    /// `./x` resolves to `x.ts`, then to `x/index.ts`: the extension is optional in
    /// the source and the directory form is the package convention.
    fn as_file(candidate: &Path) -> Option<PathBuf> {
        if candidate.is_file() {
            return Some(candidate.to_path_buf());
        }
        for extension in EXTENSIONS {
            let with_extension = append_extension(candidate, extension);
            if with_extension.is_file() {
                return Some(with_extension);
            }
        }
        for extension in EXTENSIONS {
            let index = candidate.join(format!("index.{extension}"));
            if index.is_file() {
                return Some(index);
            }
        }
        None
    }

    fn through_aliases(&self, specifier: &str) -> Option<PathBuf> {
        for (pattern, targets) in &self.aliases {
            let Some(stem) = pattern.strip_suffix('*') else {
                if pattern != specifier {
                    continue;
                }
                for target in targets {
                    if let Some(found) = Self::as_file(&self.base.join(target)) {
                        return Some(found);
                    }
                }
                continue;
            };
            let Some(rest) = specifier.strip_prefix(stem) else {
                continue;
            };
            for target in targets {
                let filled = target.replace('*', rest);
                if let Some(found) = Self::as_file(&self.base.join(filled)) {
                    return Some(found);
                }
            }
        }
        None
    }
}

/// `Path::with_extension` replaces what follows the last dot, which turns
/// `./order.service` into `./order.ts`.
fn append_extension(path: &Path, extension: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(".");
    name.push(extension);
    path.with_file_name(name)
}

fn read_tsconfig_paths(config: &Path) -> Vec<(String, Vec<String>)> {
    let Ok(text) = std::fs::read_to_string(config) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&strip_comments(&text)) else {
        return Vec::new();
    };
    let options = json.get("compilerOptions");
    let base = options
        .and_then(|options| options.get("baseUrl"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(".");
    let Some(paths) = options
        .and_then(|options| options.get("paths"))
        .and_then(|p| p.as_object())
    else {
        return Vec::new();
    };
    paths
        .iter()
        .map(|(pattern, targets)| {
            let targets = targets
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(|target| format!("{base}/{}", target.trim_start_matches("./")))
                        .collect()
                })
                .unwrap_or_default();
            (pattern.clone(), targets)
        })
        .collect()
}

/// `tsconfig.json` is JSON with comments, which `serde_json` will not read.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if in_string {
            out.push(character);
            if character == '\\' {
                if let Some(escaped) = characters.next() {
                    out.push(escaped);
                }
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => {
                in_string = true;
                out.push(character);
            }
            '/' if characters.peek() == Some(&'/') => {
                for skipped in characters.by_ref() {
                    if skipped == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if characters.peek() == Some(&'*') => {
                characters.next();
                let mut previous = ' ';
                for skipped in characters.by_ref() {
                    if previous == '*' && skipped == '/' {
                        break;
                    }
                    previous = skipped;
                }
            }
            _ => out.push(character),
        }
    }
    out
}

/// A workspace has one `package.json` per package, each declaring its own entry
/// points, so reading only the root manifest calls every package's public surface
/// dead.
///
/// Derived from the modules the scan already found rather than by walking the
/// tree. A manifest that governs anything sits in a directory holding code, or an
/// ancestor of one — so the analysed paths name every candidate there is, and
/// there are tens of them. The walk this replaced recursed five levels from the
/// root skipping only dotfiles and `node_modules`, which on a Rust checkout meant
/// `stat`ing all two hundred thousand entries under `target/debug/deps` and cost
/// ninety-three percent of the scan.
fn package_manifests(root: &Path, known: &HashSet<PathBuf>) -> Vec<PathBuf> {
    let mut directories: BTreeSet<&Path> = BTreeSet::new();
    for module in known {
        let mut at = module.parent();
        while let Some(directory) = at {
            if !directories.insert(directory) {
                // An ancestor already walked to the root, and so would this one.
                break;
            }
            if directory == root {
                break;
            }
            at = directory.parent();
        }
    }
    directories.insert(root);
    directories
        .into_iter()
        .map(|directory| directory.join("package.json"))
        .filter(|manifest| manifest.is_file())
        .collect()
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

impl Resolver for TypeScriptResolver {
    fn resolve(&self, importer: &Path, specifier: &str) -> Vec<PathBuf> {
        let found = if specifier.starts_with('.') {
            TypeScriptResolver::as_file(&importer.join(specifier))
        } else {
            self.through_aliases(specifier)
        };
        found.into_iter().collect()
    }

    fn entrypoints(
        &self,
        root: &Path,
        configured: &[String],
        known: &HashSet<PathBuf>,
    ) -> HashSet<PathBuf> {
        let manifests = package_manifests(root, known);

        let mut declared: Vec<(PathBuf, String)> = Vec::new();
        for manifest in &manifests {
            let base = manifest.parent().unwrap_or(root).to_path_buf();
            declared.extend(
                read_package_entrypoints(manifest)
                    .into_iter()
                    .map(|entry| (base.clone(), entry)),
            );
        }
        declared.extend(
            configured
                .iter()
                .map(|entry| (root.to_path_buf(), entry.clone())),
        );

        let mut entries = HashSet::new();
        for (base, candidate) in declared {
            let base = base.as_path();
            let trimmed = candidate.trim_start_matches("./");
            let direct = base.join(trimmed);
            if let Ok(canonical) = direct.canonicalize()
                && known.contains(&canonical)
            {
                entries.insert(canonical);
                continue;
            }
            // package.json points at build output; the source beside it is the real
            // entry.
            for extension in SOURCE_EXTENSIONS {
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
            for extension in SOURCE_EXTENSIONS {
                let source = base.join(&rewritten).with_extension(extension);
                if let Ok(canonical) = source.canonicalize()
                    && known.contains(&canonical)
                {
                    entries.insert(canonical);
                }
            }
        }
        entries
    }
}

/// `node:`-prefixed or one of the bare builtins. Everything else came from a registry
/// and could be swapped for something else.
pub fn is_vendor(specifier: &str) -> bool {
    const BUILTIN: [&str; 14] = [
        "assert",
        "buffer",
        "child_process",
        "crypto",
        "events",
        "fs",
        "http",
        "https",
        "os",
        "path",
        "stream",
        "url",
        "util",
        "zlib",
    ];
    !specifier.starts_with("node:") && !BUILTIN.contains(&specifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_are_stripped_but_strings_keep_their_slashes() {
        let text = r#"{ // head
            "compilerOptions": { "baseUrl": "./src", /* block */ "paths": { "@a/*": ["a/*"] } }
        }"#;
        let stripped = strip_comments(text);
        assert!(!stripped.contains("head"));
        assert!(!stripped.contains("block"));
        assert!(stripped.contains("./src"));
    }

    #[test]
    fn an_extension_is_appended_rather_than_replacing_a_dotted_name() {
        let path = Path::new("/tmp/order.service");
        assert_eq!(
            append_extension(path, "ts"),
            Path::new("/tmp/order.service.ts")
        );
    }
}

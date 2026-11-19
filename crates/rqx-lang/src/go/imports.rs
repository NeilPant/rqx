//! Turning a Go import path into a directory of files. Go imports a package, not a
//! file, so a specifier resolves to every file in that package's directory — the
//! graph edge is to the directory's contents.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::module_graph::Resolver;

pub struct GoResolver {
    /// The module path declared in `go.mod`, which is the prefix every internal
    /// import carries.
    module: Option<String>,
    root: PathBuf,
}

impl GoResolver {
    pub fn new(root: &Path) -> Self {
        Self {
            module: module_path(&root.join("go.mod")),
            root: root.to_path_buf(),
        }
    }

    /// A Go package is a directory of files compiled together, so importing it
    /// reaches all of them; picking one would call the rest dead.
    fn package_files(directory: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "go"))
            .collect();
        files.sort();
        files
    }
}

fn module_path(manifest: &Path) -> Option<String> {
    let text = std::fs::read_to_string(manifest).ok()?;
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("module "))
        .map(|rest| rest.trim().to_owned())
}

/// Whether the module a Go file belongs to declares Go 1.22 or later, from which a
/// `for` loop gives each iteration its own copy of the loop variables, so a closure
/// that captures one no longer sees the value the loop stopped on. Read from the
/// nearest `go.mod` above the file.
pub fn loop_variables_per_iteration(file: &Path) -> bool {
    file.ancestors()
        .skip(1)
        .find_map(|directory| std::fs::read_to_string(directory.join("go.mod")).ok())
        .and_then(|manifest| declared_go_version(&manifest))
        .is_some_and(|version| version >= (1, 22))
}

/// The `go` directive of a `go.mod`: `go 1.22`, `go 1.22.3`.
fn declared_go_version(manifest: &str) -> Option<(u32, u32)> {
    let version = manifest
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("go "))?
        .trim();
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts
        .next()
        .and_then(|minor| {
            let digits: String = minor.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .unwrap_or(0);
    Some((major, minor))
}

impl Resolver for GoResolver {
    fn resolve(&self, _importer: &Path, specifier: &str) -> Vec<PathBuf> {
        let relative = match &self.module {
            Some(module) if specifier == module => "",
            Some(module) => match specifier.strip_prefix(module) {
                Some(rest) => rest.trim_start_matches('/'),
                None => return Vec::new(),
            },
            // Without a `go.mod` the only internal imports that can be recognised are
            // ones written relative to the scanned root.
            None => specifier,
        };
        let directory = if relative.is_empty() {
            self.root.clone()
        } else {
            self.root.join(relative)
        };
        if !directory.is_dir() {
            return Vec::new();
        }
        Self::package_files(&directory)
    }

    fn entrypoints(
        &self,
        root: &Path,
        configured: &[String],
        known: &HashSet<PathBuf>,
    ) -> HashSet<PathBuf> {
        let mut entries = HashSet::new();
        for path in known {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned());
            // A test file is run by `go test`, which is outside the project.
            if name
                .as_deref()
                .is_some_and(|name| name.ends_with("_test.go"))
            {
                entries.insert(path.clone());
                continue;
            }
            // `package main` is what Go calls an executable: nothing imports it.
            if std::fs::read_to_string(path).is_ok_and(|text| {
                text.lines()
                    .map(str::trim)
                    .any(|line| line == "package main")
            }) {
                entries.insert(path.clone());
            }
        }
        // A library's exported names are called by its users, outside the project.
        entries.extend(crate::library::go_public(root, known));
        for entry in configured {
            if let Ok(canonical) = rqx_core::paths::canonicalize(&root.join(entry))
                && known.contains(&canonical)
            {
                entries.insert(canonical);
            }
        }
        entries
    }
}

/// Go's standard library is reached by a bare name; everything else is fetched from a
/// domain, so the first segment carrying a dot is the whole test.
pub fn is_vendor(specifier: &str) -> bool {
    specifier
        .split('/')
        .next()
        .is_some_and(|root| root.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::declared_go_version;

    #[test]
    fn the_go_directive_names_the_language_version() {
        assert_eq!(declared_go_version("module m\n\ngo 1.22\n"), Some((1, 22)));
        assert_eq!(
            declared_go_version("module m\ngo 1.23.4\ntoolchain go1.24.0\n"),
            Some((1, 23))
        );
        assert_eq!(declared_go_version("module m\ngo 1.15\n"), Some((1, 15)));
        assert_eq!(declared_go_version("module m\n"), None);
    }
}

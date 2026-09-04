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
        for entry in configured {
            if let Ok(canonical) = root.join(entry).canonicalize()
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

//! Turning `#include "header.hpp"` into a file. An angle-bracket include names the
//! standard library or a dependency, neither of which is part of the project, so only
//! quoted includes resolve.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::module_graph::{Resolver, WalkedFiles, join_specifier};

pub struct CppResolver {
    /// Directories a quoted include is tried against after the including file's own,
    /// which is the convention every build system configures.
    roots: Vec<PathBuf>,
}

impl CppResolver {
    pub fn new(root: &Path) -> Self {
        let mut roots = Vec::new();
        for layout in ["include", "src", "lib"] {
            let candidate = root.join(layout);
            if candidate.is_dir() {
                roots.push(candidate);
            }
        }
        roots.push(root.to_path_buf());
        Self { roots }
    }
}

impl Resolver for CppResolver {
    fn resolve(&self, importer: &Path, specifier: &str) -> Vec<PathBuf> {
        self.resolve_within(importer, specifier, &WalkedFiles::unknown())
    }

    fn resolve_within(
        &self,
        importer: &Path,
        specifier: &str,
        files: &WalkedFiles<'_>,
    ) -> Vec<PathBuf> {
        let direct = join_specifier(importer, specifier);
        if files.is_file(&direct) {
            return vec![direct];
        }
        self.roots
            .iter()
            .map(|base| base.join(specifier))
            .find(|path| files.is_file(path))
            .into_iter()
            .collect()
    }

    fn entrypoints(
        &self,
        root: &Path,
        configured: &[String],
        known: &HashSet<PathBuf>,
    ) -> HashSet<PathBuf> {
        let mut entries = HashSet::new();
        for path in known {
            // Only a header is reached through an include; a `.cpp` is compiled
            // directly, so nothing including it says nothing about whether it is dead.
            let is_header = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| rqx_core::Language::Cpp.is_header(extension));
            if !is_header {
                entries.insert(path.clone());
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            // `main` is the only entry point the language defines; a GoogleTest file
            // is reached by the test runner instead.
            if text.contains("int main(") || text.contains("TEST(") || text.contains("TEST_F(") {
                entries.insert(path.clone());
            }
        }
        // A library's installed headers are included by its users, outside the project.
        entries.extend(crate::library::cpp_public(root, known));
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

/// A standard header is a bare word: `<string>`, `<vector>`. A library brings its own
/// directory or its own extension, `<boost/asio.hpp>`.
pub fn is_vendor(specifier: &str) -> bool {
    specifier.contains('/') || specifier.contains('.')
}

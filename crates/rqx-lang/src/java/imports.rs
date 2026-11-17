//! Turning a Java import into a file. A package maps to a directory under a source
//! root, which is the only thing standing in for a classpath here — reading a build
//! file would tie Rqx to Maven or Gradle, and neither is required to be present.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::module_graph::{Resolver, WalkedFiles};

pub struct JavaResolver {
    roots: Vec<PathBuf>,
}

impl JavaResolver {
    pub fn new(root: &Path) -> Self {
        let mut roots = Vec::new();
        for layout in [
            "src/main/java",
            "src/test/java",
            "src/main/kotlin",
            "src",
            "java",
        ] {
            let candidate = root.join(layout);
            if candidate.is_dir() {
                roots.push(candidate);
            }
        }
        roots.push(root.to_path_buf());
        Self { roots }
    }
}

impl Resolver for JavaResolver {
    fn resolve(&self, importer: &Path, specifier: &str) -> Vec<PathBuf> {
        self.resolve_within(importer, specifier, &WalkedFiles::unknown())
    }

    fn resolve_within(
        &self,
        _importer: &Path,
        specifier: &str,
        files: &WalkedFiles<'_>,
    ) -> Vec<PathBuf> {
        // A static import names a member of a type, so the type is one segment up.
        let candidates = [specifier.to_owned(), {
            let mut parts: Vec<&str> = specifier.split('.').collect();
            parts.pop();
            parts.join(".")
        }];
        for candidate in candidates {
            let relative = candidate.replace('.', "/");
            for base in &self.roots {
                let file = base.join(format!("{relative}.java"));
                if files.is_file(&file) {
                    return vec![file];
                }
            }
        }
        Vec::new()
    }

    fn entrypoints(
        &self,
        root: &Path,
        configured: &[String],
        known: &HashSet<PathBuf>,
    ) -> HashSet<PathBuf> {
        let mut entries = HashSet::new();
        for path in known {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            // `main` is the one entry point the language itself defines, and a test
            // class is reached by the runner rather than by an import.
            if text.contains("static void main") || text.contains("@Test") {
                entries.insert(path.clone());
            }
        }
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

/// The packages shipped with the platform. Anything else is a dependency a build file
/// names and a project could change.
pub fn is_vendor(specifier: &str) -> bool {
    !["java.", "javax.", "jdk.", "sun.", "com.sun."]
        .iter()
        .any(|prefix| specifier.starts_with(prefix))
}

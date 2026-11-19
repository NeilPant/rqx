//! Turning a Python import into a file. Only modules inside the project resolve: a
//! standard-library or site-packages import says nothing about dead code here.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::module_graph::{Resolver, WalkedFiles};

pub struct PythonResolver {
    /// Directories an absolute import is resolved against: the project root, and any
    /// `src`-style layout beside it.
    roots: Vec<PathBuf>,
}

impl PythonResolver {
    pub fn new(root: &Path) -> Self {
        let mut roots = vec![root.to_path_buf()];
        for layout in ["src", "lib"] {
            let candidate = root.join(layout);
            if candidate.is_dir() {
                roots.push(candidate);
            }
        }
        Self { roots }
    }

    /// A dotted name is a directory path, ending either at a module file or at the
    /// `__init__.py` that makes a directory a package.
    fn module_file(files: &WalkedFiles<'_>, base: &Path, dotted: &str) -> Option<PathBuf> {
        let mut path = base.to_path_buf();
        for segment in dotted.split('.').filter(|segment| !segment.is_empty()) {
            path.push(segment);
        }
        [
            path.with_extension("py"),
            path.with_extension("pyi"),
            path.join("__init__.py"),
        ]
        .into_iter()
        .find(|candidate| files.is_file(candidate))
    }

    fn module_of(
        &self,
        files: &WalkedFiles<'_>,
        importer: &Path,
        specifier: &str,
    ) -> Option<PathBuf> {
        let dots = specifier.chars().take_while(|c| *c == '.').count();
        if dots > 0 {
            let base = Self::relative_base(importer, dots)?;
            return Self::module_file(files, &base, specifier.trim_start_matches('.'));
        }
        self.roots
            .iter()
            .find_map(|base| Self::module_file(files, base, specifier))
    }

    /// `.` is the importing module's own package, `..` its parent, and so on: one
    /// level up per leading dot beyond the first.
    fn relative_base(importer: &Path, dots: usize) -> Option<PathBuf> {
        let mut base = importer.to_path_buf();
        for _ in 1..dots {
            base = base.parent()?.to_path_buf();
        }
        Some(base)
    }
}

impl Resolver for PythonResolver {
    fn resolve(&self, importer: &Path, specifier: &str) -> Vec<PathBuf> {
        self.resolve_within(importer, specifier, &WalkedFiles::unknown())
    }

    fn resolve_within(
        &self,
        importer: &Path,
        specifier: &str,
        files: &WalkedFiles<'_>,
    ) -> Vec<PathBuf> {
        self.module_of(files, importer, specifier)
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
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned());

            let name = name.unwrap_or_default();
            if matches!(
                name.as_str(),
                "__main__.py" | "main.py" | "conftest.py" | "setup.py"
            ) {
                entries.insert(path.clone());
                continue;
            }
            // A test module is run by the test runner, which is outside the project:
            // nothing imports it, and that is not a finding about it.
            if name.starts_with("test_") || name.ends_with("_test.py") {
                entries.insert(path.clone());
                continue;
            }
            // A module guarded by `if __name__ == "__main__"` is run directly, which
            // is Python's only in-source declaration that a file is an entry point.
            if std::fs::read_to_string(path)
                .is_ok_and(|text| text.contains("__name__") && text.contains("__main__"))
            {
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
        entries.extend(crate::library::python_public(root, known));
        for script in project_scripts(&root.join("pyproject.toml")) {
            // `package.module:function` — only the module half names a file.
            let module = script.split(':').next().unwrap_or(&script);
            if let Some(found) = self.module_of(&WalkedFiles::new(known), root, module)
                && let Ok(canonical) = rqx_core::paths::canonicalize(&found)
                && known.contains(&canonical)
            {
                entries.insert(canonical);
            }
        }
        entries
    }
}

/// `[project.scripts]` entries in `pyproject.toml`, read with a line scanner rather
/// than a TOML parser: only the values matter and they are always `name = "target"`.
fn project_scripts(manifest: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(manifest) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = matches!(line, "[project.scripts]" | "[tool.poetry.scripts]");
            continue;
        }
        if inside && let Some((_, value)) = line.split_once('=') {
            found.push(value.trim().trim_matches(['"', '\'']).to_owned());
        }
    }
    found
}

/// The standard library is reached by its top-level name, so only that segment
/// decides; anything else was installed.
pub fn is_vendor(specifier: &str) -> bool {
    const STDLIB: [&str; 30] = [
        "abc",
        "argparse",
        "asyncio",
        "collections",
        "contextlib",
        "copy",
        "csv",
        "dataclasses",
        "datetime",
        "enum",
        "functools",
        "hashlib",
        "io",
        "itertools",
        "json",
        "logging",
        "math",
        "os",
        "pathlib",
        "random",
        "re",
        "shutil",
        "socket",
        "subprocess",
        "sys",
        "tempfile",
        "threading",
        "time",
        "typing",
        "unittest",
    ];
    let root = specifier.split('.').next().unwrap_or(specifier);
    !STDLIB.contains(&root)
}

//! Which files make up a library's public surface. Code outside the project imports a
//! library, so an export nothing in the project reads may still be its interface; the
//! module graph treats these files like entry points and judges only the rest.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

/// Directories whose files a library's users do not import, whatever the language.
const NOT_PUBLISHED: [&str; 6] = [
    "test", "tests", "testing", "testdata", "examples", "example",
];

/// Go refuses an import of a package under `internal/` from outside the module.
const GO_INTERNAL: &str = "internal";

/// The directories between `root` and `path`, as written.
fn directories(root: &Path, path: &Path) -> Vec<String> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut parts: Vec<String> = relative
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    parts.pop();
    parts
}

fn published(root: &Path, path: &Path) -> bool {
    !directories(root, path)
        .iter()
        .any(|directory| NOT_PUBLISHED.contains(&directory.as_str()))
}

fn canonical_root(root: &Path) -> PathBuf {
    rqx_core::paths::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

fn is_package_main(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| {
        text.lines()
            .map(str::trim)
            .any(|line| line == "package main")
    })
}

/// A Go module whose root package is not `main` is published to be imported: every
/// package outside `internal/` is part of what its users can call. A module whose
/// root holds no Go files counts as a library when it builds no executable.
pub fn go_public(root: &Path, known: &HashSet<PathBuf>) -> Vec<PathBuf> {
    let root = canonical_root(root);
    if !root.join("go.mod").is_file() {
        return Vec::new();
    }
    let is_source = |path: &&PathBuf| {
        path.extension().is_some_and(|extension| extension == "go")
            && !path.to_string_lossy().ends_with("_test.go")
    };
    let at_root: Vec<&PathBuf> = known
        .iter()
        .filter(is_source)
        .filter(|path| path.parent() == Some(root.as_path()))
        .collect();
    let library = if at_root.is_empty() {
        !known
            .iter()
            .filter(is_source)
            .any(|path| is_package_main(path))
    } else {
        !at_root.iter().any(|path| is_package_main(path))
    };
    if !library {
        return Vec::new();
    }
    known
        .iter()
        .filter(is_source)
        .filter(|path| {
            published(&root, path)
                && !directories(&root, path)
                    .iter()
                    .any(|directory| directory == GO_INTERNAL)
        })
        .cloned()
        .collect()
}

/// The `package.json` that describes the scanned code: the root's own, or the nearest
/// one above it when a package's source folder is scanned on its own.
pub fn package_manifest(root: &Path) -> Option<PathBuf> {
    canonical_root(root)
        .ancestors()
        .map(|directory| directory.join("package.json"))
        .find(|manifest| manifest.is_file())
}

/// A Python tree is a library when it is packaged or made of packages, and nothing at
/// its root says it is an application. Its public modules are those no underscore
/// marks private, outside its tests.
pub fn python_public(root: &Path, known: &HashSet<PathBuf>) -> Vec<PathBuf> {
    let root = canonical_root(root);
    let application = [
        "manage.py",
        "wsgi.py",
        "asgi.py",
        "app.py",
        "main.py",
        "__main__.py",
    ]
    .iter()
    .any(|marker| root.join(marker).is_file());
    let packaged = ["pyproject.toml", "setup.py", "setup.cfg"]
        .iter()
        .any(|marker| root.join(marker).is_file());
    let has_packages = known
        .iter()
        .any(|path| path.file_name().is_some_and(|name| name == "__init__.py"));
    if application || !(packaged || has_packages) {
        return Vec::new();
    }
    known
        .iter()
        .filter(|path| published(&root, path))
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let private_module = name.starts_with('_') && name != "__init__.py";
            let private_package = directories(&root, path)
                .iter()
                .any(|directory| directory.starts_with('_'));
            !private_module && !private_package && !name.starts_with("test_")
        })
        .cloned()
        .collect()
}

/// A header under `include/<library>/` is installed for other projects to include as
/// `<library/header.h>`, which is how a C or C++ library publishes its interface. A
/// flat `include/` is a project keeping its own headers apart from its sources.
pub fn cpp_public(root: &Path, known: &HashSet<PathBuf>) -> Vec<PathBuf> {
    let root = canonical_root(root);
    known
        .iter()
        .filter(|path| {
            let directories = directories(&root, path);
            let below_include = directories
                .iter()
                .position(|directory| directory == "include")
                .is_some_and(|at| at + 1 < directories.len());
            below_include && published(&root, path)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_under_tests_or_examples_is_not_published() {
        let root = Path::new("/r");
        assert!(published(root, Path::new("/r/pkg/a.go")));
        assert!(!published(root, Path::new("/r/examples/a.go")));
        assert!(!published(root, Path::new("/r/pkg/testdata/x/a.go")));
        assert!(!published(root, Path::new("/r/tests/a.py")));
        assert!(published(root, Path::new("/r/a.go")));
    }
}

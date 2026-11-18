//! The precision corpus on disk: `fixtures/complexity/<language>/*` and the cross-file
//! projects under `fixtures/complexity/projects/<name>/`, every annotated function in
//! them, and the id each is known by in the baseline.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::annotation::{self, Annotation};

pub const LANGUAGES: [&str; 6] = ["typescript", "javascript", "python", "go", "java", "cpp"];

/// Which of the two sets a function scores in: the ones written for the corpus, or the
/// ones copied from public projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Set {
    Written,
    Real,
}

pub struct Function {
    pub set: Set,
    pub language: &'static str,
    /// Path under the corpus root, such as `python/loops.py`.
    pub file: String,
    /// What the function is analysed together with: its own file, or the project it is
    /// part of, whose files call one another.
    pub group: String,
    pub name: Option<String>,
    pub annotation: Annotation,
}

impl Function {
    /// `python/loops.py:has_duplicate`, or the line of the declaration when the name
    /// cannot be read. A second function of the same name in a file gets `#2`.
    fn id_before_disambiguation(&self) -> String {
        match &self.name {
            Some(name) => format!("{}:{name}", self.file),
            None => format!("{}:line{}", self.file, self.annotation.target_line),
        }
    }
}

pub struct Corpus {
    pub functions: Vec<Function>,
    pub problems: Vec<String>,
}

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/complexity")
}

impl Corpus {
    pub fn load(root: &Path) -> Self {
        let mut corpus = Self {
            functions: Vec::new(),
            problems: Vec::new(),
        };
        for (set, base) in [
            (Set::Written, root.to_path_buf()),
            (Set::Real, root.join(REAL)),
        ] {
            for language in LANGUAGES {
                for path in files_in(&base.join(language)) {
                    let group = relative_to(root, &path);
                    corpus.read(set, root, language, &path, &group, true);
                }
            }
        }
        for project in directories_in(&root.join(PROJECTS)) {
            let group = relative_to(root, &project);
            for path in files_under(&project) {
                let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                match language_of(extension) {
                    // A barrel that only re-exports has nothing to annotate.
                    Some(language) => {
                        corpus.read(Set::Written, root, language, &path, &group, false);
                    }
                    None => corpus.problems.push(format!(
                        "{}: no language for `.{extension}`",
                        relative_to(root, &path)
                    )),
                }
            }
        }
        corpus
    }

    fn read(
        &mut self,
        set: Set,
        root: &Path,
        language: &'static str,
        path: &Path,
        group: &str,
        must_annotate: bool,
    ) {
        let relative = relative_to(root, path);
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let Some(marker) = annotation::comment_marker(extension) else {
            self.problems
                .push(format!("{relative}: no comment style for `.{extension}`"));
            return;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            self.problems.push(format!("{relative}: cannot be read"));
            return;
        };
        let (annotations, problems) = annotation::parse(marker, &text);
        self.problems
            .extend(problems.iter().map(|p| format!("{relative}: {p}")));
        if must_annotate && annotations.is_empty() && problems.is_empty() {
            self.problems
                .push(format!("{relative}: holds no `@complexity` annotation"));
        }
        let lines: Vec<&str> = text.lines().collect();
        for annotation in annotations {
            let declaration = lines.get(annotation.target_line - 1).copied().unwrap_or("");
            self.functions.push(Function {
                set,
                language,
                file: relative.clone(),
                group: group.to_owned(),
                name: annotation::function_name(declaration),
                annotation,
            });
        }
    }

    /// Each function's id, unique across the corpus.
    pub fn ids(&self) -> Vec<String> {
        let mut seen: BTreeMap<String, u32> = BTreeMap::new();
        self.functions
            .iter()
            .map(|function| {
                let base = function.id_before_disambiguation();
                let count = seen.entry(base.clone()).or_default();
                *count += 1;
                if *count == 1 {
                    base
                } else {
                    format!("{base}#{count}")
                }
            })
            .collect()
    }

    pub fn count_in(&self, set: Set, language: &str) -> usize {
        self.functions
            .iter()
            .filter(|function| function.set == set && function.language == language)
            .count()
    }

    pub fn count_of(&self, set: Set) -> usize {
        self.functions
            .iter()
            .filter(|function| function.set == set)
            .count()
    }
}

pub const PROJECTS: &str = "projects";

/// Where the functions copied from public projects are, one directory per language.
pub const REAL: &str = "real";

/// The files a group is analysed from: the one file, or everything in the project.
pub fn files_of(root: &Path, group: &str) -> Vec<PathBuf> {
    let path = root.join(group);
    if path.is_dir() {
        files_under(&path)
            .into_iter()
            .filter(|file| {
                file.extension()
                    .and_then(|extension| extension.to_str())
                    .and_then(language_of)
                    .is_some()
            })
            .collect()
    } else {
        vec![path]
    }
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// The corpus language a source file belongs to, from its extension.
pub fn language_of(extension: &str) -> Option<&'static str> {
    match extension {
        "ts" => Some("typescript"),
        "js" => Some("javascript"),
        "py" => Some("python"),
        "go" => Some("go"),
        "java" => Some("java"),
        "cpp" | "hpp" => Some("cpp"),
        _ => None,
    }
}

fn directories_in(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    found.sort();
    found
}

/// Every file below a directory, in a fixed order.
fn files_under(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return found;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// Files directly inside a corpus language directory, in a fixed order.
fn files_in(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rqx-corpus-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        root
    }

    #[test]
    fn functions_are_read_in_language_then_file_order_with_unique_ids() {
        let root = scratch(
            "ids",
            &[
                (
                    "python/b.py",
                    "# @complexity time=O(1) space=O(1) conf=verified\ndef same(x):\n    return x\n\n# @complexity time=O(1) space=O(1) conf=verified\ndef same(x, y):\n    return y\n",
                ),
                (
                    "typescript/a.ts",
                    "// @complexity time=O(n) space=O(1) conf=verified\nexport function total(xs: number[]) { return 0; }\n",
                ),
            ],
        );
        let corpus = Corpus::load(&root);
        assert!(corpus.problems.is_empty(), "{:?}", corpus.problems);
        assert_eq!(
            corpus.ids(),
            [
                "typescript/a.ts:total",
                "python/b.py:same",
                "python/b.py:same#2"
            ]
        );
        assert_eq!(corpus.count_in(Set::Written, "python"), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_file_with_no_annotation_and_a_malformed_one_are_both_problems() {
        let root = scratch(
            "problems",
            &[
                ("go/empty.go", "package corpus\n"),
                ("go/bad.go", "// @complexity time=O(n)\nfunc F() {}\n"),
                ("java/Thing.txt", "x"),
            ],
        );
        let corpus = Corpus::load(&root);
        assert_eq!(corpus.problems.len(), 3, "{:?}", corpus.problems);
        assert!(
            corpus
                .problems
                .iter()
                .any(|p| p.contains("go/bad.go: line 1"))
        );
        assert!(
            corpus
                .problems
                .iter()
                .any(|p| p.contains("go/empty.go: holds no"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

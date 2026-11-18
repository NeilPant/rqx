//! The check one file's verification cannot make: whether an edit removes, or changes
//! the signature of, something another file uses. Every file the scan read is
//! searched, and so is every file of the same language beside the edited one, which
//! covers a Go file a build tag or a `_windows.go` suffix keeps out of this build.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use rqx_core::Language;
use rqx_lang::outline::Outline;
use rqx_lang::rewrite::Withheld;

use super::{FileFix, Withholding};

/// The fixes that pass, in their original order. The rest move to `withheld` with the
/// name and the file that still uses it.
pub(super) fn guard(
    root: &Path,
    analyzed: &BTreeSet<String>,
    fixes: Vec<FileFix>,
    withheld: &mut Vec<Withholding>,
) -> Vec<FileFix> {
    if fixes.is_empty() {
        return fixes;
    }
    let surfaces: Vec<Result<Surface, Withheld>> = fixes.iter().map(Surface::of).collect();
    let words: Vec<[HashSet<String>; 2]> = fixes
        .iter()
        .map(|fix| [identifiers(&fix.before), identifiers(&fix.after)])
        .collect();
    let mut corpus = Corpus::new(root, analyzed);
    let mut refused: Vec<Option<Withheld>> = vec![None; fixes.len()];

    // A withheld file stays as it was, so a name its edit would have stopped using
    // is still used. Withholding only grows, so this settles.
    loop {
        let on_disk: HashMap<PathBuf, &HashSet<String>> = fixes
            .iter()
            .zip(&refused)
            .zip(&words)
            .map(|((fix, refusal), [before, after])| {
                let words = if refusal.is_some() { before } else { after };
                (root.join(&fix.file), words)
            })
            .collect();
        let mut changed = false;
        for (index, fix) in fixes.iter().enumerate() {
            if refused[index].is_some() {
                continue;
            }
            let refusal = match &surfaces[index] {
                Err(reason) => Some(reason.clone()),
                Ok(surface) => surface.refusal(root, fix, &mut corpus, &on_disk),
            };
            if refusal.is_some() {
                refused[index] = refusal;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut kept = Vec::new();
    for (fix, refusal) in fixes.into_iter().zip(refused) {
        match refusal {
            None => kept.push(fix),
            Some(reason) => withheld.push(Withholding {
                file: fix.file,
                edits: fix.applied.len(),
                reason,
            }),
        }
    }
    kept
}

/// What one fix takes away from the rest of the project.
struct Surface {
    language: Language,
    removed: Vec<String>,
    resigned: Vec<String>,
}

impl Surface {
    fn of(fix: &FileFix) -> Result<Self, Withheld> {
        let language = Language::of_path(Path::new(&fix.file))
            .ok_or_else(|| Withheld::Unanalysable("no language for this file".to_owned()))?;
        let markup = rqx_lang::holds_markup(&fix.file);
        let outline = |source: &str| {
            Outline::of(language, markup, source)
                .map_err(|error| Withheld::Unanalysable(error.to_string()))
        };
        let before = outline(&fix.before)?;
        let after = outline(&fix.after)?;
        Ok(Self {
            language,
            removed: before.removed(&after),
            resigned: before.changed_signatures(&after),
        })
    }

    fn refusal(
        &self,
        root: &Path,
        fix: &FileFix,
        corpus: &mut Corpus,
        on_disk: &HashMap<PathBuf, &HashSet<String>>,
    ) -> Option<Withheld> {
        // Every function in a Go package is callable from every file in it, and the
        // build tags decide which of those files exist, so no caller list is complete.
        if self.language == Language::Go && !self.resigned.is_empty() {
            return Some(Withheld::ChangesSignature(self.resigned.clone()));
        }
        let path = root.join(&fix.file);
        let names: BTreeSet<&str> = self
            .removed
            .iter()
            .chain(&self.resigned)
            .map(String::as_str)
            .collect();
        if names.is_empty() {
            return None;
        }
        for other in corpus.neighbours(&path, self.language) {
            let uses = match on_disk.get(&other) {
                Some(words) => words,
                None => corpus.identifiers_of(&other),
            };
            if let Some(name) = names.iter().find(|name| uses.contains(**name)) {
                return Some(Withheld::StillReferenced {
                    name: (*name).to_owned(),
                    by: display(root, &other),
                });
            }
        }
        None
    }
}

/// The files a removed name could be read from, read once each.
struct Corpus {
    analyzed: Vec<PathBuf>,
    identifiers: HashMap<PathBuf, HashSet<String>>,
}

impl Corpus {
    fn new(root: &Path, analyzed: &BTreeSet<String>) -> Self {
        Self {
            analyzed: analyzed.iter().map(|file| root.join(file)).collect(),
            identifiers: HashMap::new(),
        }
    }

    /// Every analysed file, and every file of `language` in the directory that holds
    /// `path`, except `path` itself.
    fn neighbours(&self, path: &Path, language: Language) -> Vec<PathBuf> {
        let mut files: BTreeSet<PathBuf> = self.analyzed.iter().cloned().collect();
        if let Some(directory) = path.parent()
            && let Ok(entries) = std::fs::read_dir(directory)
        {
            files.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|sibling| sibling.is_file())
                    .filter(|sibling| Language::of_path(sibling) == Some(language)),
            );
        }
        files.remove(path);
        files.into_iter().collect()
    }

    /// A file that cannot be read has nothing in it to search, and a file the scan
    /// read a moment ago is not expected to vanish.
    fn identifiers_of(&mut self, path: &Path) -> &HashSet<String> {
        self.identifiers
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                std::fs::read_to_string(path)
                    .map(|text| identifiers(&text))
                    .unwrap_or_default()
            })
    }
}

/// Every word that could be an identifier, comments and strings included: a name in a
/// string may be looked up by reflection, and a false match only withholds a fix.
fn identifiers(text: &str) -> HashSet<String> {
    text.split(|character: char| {
        !(character.is_alphanumeric() || character == '_' || character == '$')
    })
    .filter(|word| !word.is_empty())
    .map(str::to_owned)
    .collect()
}

fn display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rqx-package-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temp project");
        for (path, contents) in files {
            std::fs::write(root.join(path), contents).expect("a file");
        }
        root
    }

    fn fix_of(file: &str, before: &str, after: &str) -> FileFix {
        FileFix {
            file: file.to_owned(),
            before: before.to_owned(),
            after: after.to_owned(),
            applied: Vec::new(),
            dropped: Vec::new(),
            resolved: Vec::new(),
            findings_after: Vec::new(),
        }
    }

    fn guarded(root: &Path, analyzed: &[&str], fixes: Vec<FileFix>) -> (Vec<String>, Vec<String>) {
        let analyzed = analyzed.iter().map(|file| (*file).to_owned()).collect();
        let mut withheld = Vec::new();
        let kept = guard(root, &analyzed, fixes, &mut withheld);
        (
            kept.into_iter().map(|fix| fix.file).collect(),
            withheld
                .into_iter()
                .map(|held| held.reason.to_string())
                .collect(),
        )
    }

    /// cobra's `preExecHookFn`: declared in a build-tagged file, read by another. The
    /// scan names only the edited file, so the reader is found as a sibling.
    #[test]
    fn a_name_a_sibling_file_reads_is_not_removed() {
        let declared = "//go:build windows\n\npackage cobra\n\nvar preExecHookFn = func() {}\n";
        let root = project(
            "sibling",
            &[
                ("command_win.go", declared),
                (
                    "command.go",
                    "package cobra\n\nfunc run() {\n\tif preExecHookFn != nil {\n\t\tpreExecHookFn()\n\t}\n}\n",
                ),
            ],
        );
        let (kept, withheld) = guarded(
            &root,
            &["command_win.go"],
            vec![fix_of(
                "command_win.go",
                declared,
                "//go:build windows\n\npackage cobra\n",
            )],
        );
        assert!(kept.is_empty());
        assert_eq!(
            withheld,
            ["removes `preExecHookFn`, which command.go still uses"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_go_signature_change_is_withheld() {
        let before = "package cobra\n\nfunc check(args []string, last string) (string, error) {\n\treturn last, nil\n}\n";
        let after =
            "package cobra\n\nfunc check(args []string, last string) () {\n\treturn last, nil\n}\n";
        let root = project("signature", &[("completions.go", before)]);
        let (kept, withheld) = guarded(
            &root,
            &["completions.go"],
            vec![fix_of("completions.go", before, after)],
        );
        assert!(kept.is_empty());
        assert!(withheld[0].contains("signature of `check`"), "{withheld:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_edit_inside_a_function_passes() {
        let before = "package cobra\n\nfunc run() int {\n\treturn 1\n\tprintln(\"dead\")\n}\n";
        let after = "package cobra\n\nfunc run() int {\n\treturn 1\n}\n";
        let root = project(
            "inside",
            &[
                ("run.go", before),
                ("other.go", "package cobra\n\nvar _ = run\n"),
            ],
        );
        let (kept, withheld) = guarded(
            &root,
            &["run.go", "other.go"],
            vec![fix_of("run.go", before, after)],
        );
        assert_eq!(kept, ["run.go"], "{withheld:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A withheld file stays as it was, so a name its edit would have stopped
    /// reading is still read, and the edit removing that name is withheld too.
    #[test]
    fn withholding_one_file_withholds_the_removal_it_still_reads() {
        let declarer = "package p\n\nvar hook = 1\n";
        let reader =
            "package p\n\nfunc a(x int) int {\n\treturn x\n}\n\nfunc b() int {\n\treturn hook\n}\n";
        // Stops reading `hook`, and changes `a`'s signature, which is refused.
        let rewritten =
            "package p\n\nfunc a() int {\n\treturn 0\n}\n\nfunc b() int {\n\treturn 0\n}\n";
        let root = project(
            "settle",
            &[("declarer.go", declarer), ("reader.go", reader)],
        );
        let (kept, withheld) = guarded(
            &root,
            &["declarer.go", "reader.go"],
            vec![
                fix_of("declarer.go", declarer, "package p\n"),
                fix_of("reader.go", reader, rewritten),
            ],
        );
        assert!(kept.is_empty(), "{kept:?}");
        assert_eq!(withheld.len(), 2, "{withheld:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}

//! Files a project marks as generated or vendored in `.gitattributes`, the convention
//! GitHub's linguist reads. The patterns go through the `ignore` crate's gitignore
//! matcher, which shares the glob syntax, so the last line that matches a file decides.

use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The linguist attributes that take a file out of a project's own code.
const ATTRIBUTES: [&str; 2] = ["linguist-generated", "linguist-vendored"];

/// One `.gitattributes` file, read as one matcher per attribute.
struct Listing {
    /// Where the scanned directory sits below the file's directory, so a path relative
    /// to the scanned directory can be matched against the file's patterns.
    prefix: PathBuf,
    matchers: Vec<(&'static str, Gitignore)>,
}

/// The `.gitattributes` files that apply to a scan: the one at the scanned directory
/// and the one at the repository root above it. The nearer one wins where both say
/// something about a file, as in git.
pub struct Marked {
    listings: Vec<Listing>,
}

impl Marked {
    /// `None` when neither file marks anything, so a project without the attributes
    /// pays nothing per file.
    pub fn load(root: &Path) -> Option<Self> {
        let start = rqx_core::paths::canonicalize(root).ok()?;
        let repository = start
            .ancestors()
            .find(|directory| directory.join(".git").exists());
        let mut listings: Vec<Listing> = repository
            .filter(|repository| *repository != start)
            .and_then(|repository| {
                let prefix = start.strip_prefix(repository).ok()?.to_path_buf();
                Listing::read(repository, prefix)
            })
            .into_iter()
            .collect();
        listings.extend(Listing::read(&start, PathBuf::new()));
        (!listings.is_empty()).then_some(Self { listings })
    }

    /// The attribute that marks `relative`, a file path relative to the scanned
    /// directory, or `None` when it is the project's own.
    pub fn marking(&self, relative: &Path) -> Option<&'static str> {
        ATTRIBUTES.into_iter().find(|attribute| {
            let mut decided = Match::None;
            for listing in &self.listings {
                let path = listing.prefix.join(relative);
                for (name, matcher) in &listing.matchers {
                    if name == attribute {
                        let found = matcher.matched_path_or_any_parents(&path, false);
                        if !found.is_none() {
                            decided = found;
                        }
                    }
                }
            }
            decided.is_ignore()
        })
    }
}

impl Listing {
    fn read(directory: &Path, prefix: PathBuf) -> Option<Self> {
        let text = std::fs::read_to_string(directory.join(".gitattributes")).ok()?;
        let matchers: Vec<(&'static str, Gitignore)> = ATTRIBUTES
            .into_iter()
            .filter_map(|attribute| matcher(&text, attribute).map(|found| (attribute, found)))
            .collect();
        (!matchers.is_empty()).then_some(Self { prefix, matchers })
    }
}

/// A matcher that ignores the files `attribute` is set on and whitelists the ones it
/// is unset on, line by line, so the gitignore rule that the last match wins is the
/// gitattributes rule too. `None` when no line mentions the attribute.
fn matcher(text: &str, attribute: &str) -> Option<Gitignore> {
    let mut builder = GitignoreBuilder::new("");
    let mut any = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with("[attr]") {
            continue;
        }
        let mut words = line.split_whitespace();
        let Some(pattern) = words.next() else {
            continue;
        };
        // Negative patterns are forbidden in gitattributes, and a quoted one is rare
        // enough to leave unread rather than guess at.
        if pattern.starts_with('!') || pattern.starts_with('"') {
            continue;
        }
        let Some(set) = words.filter_map(|word| state(word, attribute)).next_back() else {
            continue;
        };
        let line = if set {
            pattern.to_owned()
        } else {
            format!("!{pattern}")
        };
        any |= builder.add_line(None, &line).is_ok();
    }
    any.then(|| builder.build().ok()).flatten()
}

/// Whether `word` sets (`attribute`, `attribute=true`) or unsets (`-attribute`,
/// `!attribute`, `attribute=false`) the attribute. Any other value says nothing.
fn state(word: &str, attribute: &str) -> Option<bool> {
    if let Some(name) = word.strip_prefix('-').or_else(|| word.strip_prefix('!')) {
        return (name == attribute).then_some(false);
    }
    match word.split_once('=') {
        None => (word == attribute).then_some(true),
        Some((name, value)) if name == attribute => match value {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked_by(attributes: &str) -> Marked {
        Marked {
            listings: vec![Listing {
                prefix: PathBuf::new(),
                matchers: ATTRIBUTES
                    .into_iter()
                    .filter_map(|attribute| matcher(attributes, attribute).map(|m| (attribute, m)))
                    .collect(),
            }],
        }
    }

    #[test]
    fn set_and_unset_forms_are_read() {
        assert_eq!(
            state("linguist-generated", "linguist-generated"),
            Some(true)
        );
        assert_eq!(
            state("linguist-generated=true", "linguist-generated"),
            Some(true)
        );
        assert_eq!(
            state("linguist-generated=false", "linguist-generated"),
            Some(false)
        );
        assert_eq!(
            state("-linguist-generated", "linguist-generated"),
            Some(false)
        );
        assert_eq!(
            state("!linguist-generated", "linguist-generated"),
            Some(false)
        );
        assert_eq!(
            state("linguist-generated=maybe", "linguist-generated"),
            None
        );
        assert_eq!(state("linguist-vendored", "linguist-generated"), None);
        assert_eq!(state("text", "linguist-generated"), None);
    }

    #[test]
    fn common_glob_forms_mark_the_files_under_them() {
        let marked = marked_by(
            "# vendored code\n\
             public/lib/** linguist-vendored\n\
             *.pb.ts linguist-generated=true\n\
             src/api/ linguist-generated\n\
             *.ts text eol=lf\n",
        );
        assert_eq!(
            marked.marking(Path::new("public/lib/chart/core.js")),
            Some("linguist-vendored")
        );
        assert_eq!(
            marked.marking(Path::new("deep/down/user.pb.ts")),
            Some("linguist-generated")
        );
        assert_eq!(
            marked.marking(Path::new("src/api/client.ts")),
            Some("linguist-generated")
        );
        assert_eq!(marked.marking(Path::new("src/app.ts")), None);
    }

    #[test]
    fn a_later_line_or_an_unset_form_brings_a_file_back() {
        let marked = marked_by(
            "vendor/** linguist-vendored\n\
             vendor/own/** -linguist-vendored\n\
             *.gen.ts linguist-generated\n\
             keep.gen.ts linguist-generated=false\n\
             old.js linguist-generated\n\
             old.js !linguist-generated\n",
        );
        assert_eq!(
            marked.marking(Path::new("vendor/x/a.js")),
            Some("linguist-vendored")
        );
        assert_eq!(marked.marking(Path::new("vendor/own/a.js")), None);
        assert_eq!(
            marked.marking(Path::new("src/a.gen.ts")),
            Some("linguist-generated")
        );
        assert_eq!(marked.marking(Path::new("src/keep.gen.ts")), None);
        assert_eq!(marked.marking(Path::new("old.js")), None);
    }

    #[test]
    fn a_file_with_no_linguist_lines_marks_nothing() {
        assert!(matcher("*.ts text eol=lf\n*.png binary\n", "linguist-generated").is_none());
    }

    #[test]
    fn the_repository_file_applies_below_and_the_nearer_file_wins() {
        let root = std::env::temp_dir().join(format!("rqx-gitattributes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("web/src")).unwrap();
        std::fs::write(
            root.join(".gitattributes"),
            "web/src/legacy/** linguist-vendored\nweb/src/kept/** linguist-vendored\n",
        )
        .unwrap();
        std::fs::write(
            root.join("web/.gitattributes"),
            "src/kept/** -linguist-vendored\n",
        )
        .unwrap();

        let marked = Marked::load(&root.join("web")).expect("attributes");
        let legacy = marked.marking(Path::new("src/legacy/a.js"));
        let kept = marked.marking(Path::new("src/kept/a.js"));
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(legacy, Some("linguist-vendored"));
        assert_eq!(kept, None);
    }
}

//! `--files-from`: a list of files a pipeline already knows about, in place of asking
//! git which ones changed.
//!
//! The list is plain text so anything can produce it: one path per line, or separated
//! by NUL when the input holds one, which is what `git diff -z --name-only` writes and
//! the only form that survives a file name with a newline in it.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};

pub struct Listed {
    /// The files that exist under the scan root, named the way a finding names them.
    pub files: BTreeSet<String>,
    /// What the list named that is not a file under the scan root. Kept so it can be
    /// said once, rather than once per path or not at all.
    pub missing: Vec<String>,
}

impl Listed {
    /// `-` is standard input, so a list can be piped in without a temporary file.
    pub fn read(source: &str, root: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        if source == "-" {
            std::io::stdin()
                .lock()
                .read_to_end(&mut bytes)
                .context("reading the file list from standard input")?;
        } else {
            bytes = std::fs::read(source).with_context(|| format!("reading {source}"))?;
        }
        Ok(Self::parse(&bytes, root))
    }

    pub fn parse(bytes: &[u8], root: &Path) -> Self {
        let separator = if bytes.contains(&0) { 0 } else { b'\n' };
        let canonical_root =
            rqx_core::paths::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let mut listed = Self {
            files: BTreeSet::new(),
            missing: Vec::new(),
        };
        for entry in bytes.split(|byte| *byte == separator) {
            let named = String::from_utf8_lossy(entry);
            let named = named.strip_suffix('\r').unwrap_or(&named);
            if named.is_empty() {
                continue;
            }
            match under(&canonical_root, root, named) {
                Some(relative) => {
                    listed.files.insert(relative);
                }
                None => listed.missing.push(named.to_owned()),
            }
        }
        listed
    }

    /// Words for stderr when the list named things that were left out, `None` when it
    /// named only files that exist.
    pub fn note_on_missing(&self) -> Option<String> {
        const SHOWN: usize = 5;
        if self.missing.is_empty() {
            return None;
        }
        let shown: Vec<&str> = self
            .missing
            .iter()
            .take(SHOWN)
            .map(String::as_str)
            .collect();
        let more = self.missing.len().saturating_sub(SHOWN);
        let tail = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        Some(format!(
            "--files-from: ignored {} that are not files under the scan root: {}{tail}",
            self.missing.len(),
            shown.join(", ")
        ))
    }
}

/// The path relative to the scan root, when it is a file inside it. A relative path is
/// read against the root, not against wherever the command happened to run.
fn under(canonical_root: &Path, root: &Path, named: &str) -> Option<String> {
    let found = rqx_core::paths::canonicalize(&root.join(named)).ok()?;
    if !found.is_file() {
        return None;
    }
    let relative = found.strip_prefix(canonical_root).ok()?;
    Some(rqx_core::paths::display_path(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("rqx-listed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("a directory");
        for file in ["src/a.ts", "src/b.ts", "weird\nname.ts"] {
            std::fs::write(root.join(file), "export {}\n").expect("a file");
        }
        root
    }

    #[test]
    fn lines_name_files_relative_to_the_root_or_absolute() {
        let root = project("lines");
        let absolute = root.join("src/b.ts");
        let text = format!("src/a.ts\r\n\n{}\n", absolute.display());
        let listed = Listed::parse(text.as_bytes(), &root);
        assert_eq!(
            listed.files.iter().map(String::as_str).collect::<Vec<_>>(),
            ["src/a.ts", "src/b.ts"]
        );
        assert!(listed.missing.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn nul_separated_input_keeps_a_newline_inside_a_name() {
        let root = project("nul");
        let listed = Listed::parse(b"src/a.ts\0weird\nname.ts\0", &root);
        assert!(listed.files.contains("weird\nname.ts"));
        assert_eq!(listed.files.len(), 2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn what_is_not_a_file_under_the_root_is_set_aside_and_counted_once() {
        let root = project("missing");
        let outside = std::env::temp_dir().display().to_string();
        let text = format!("src/a.ts\ngone.ts\nsrc\n{outside}\n");
        let listed = Listed::parse(text.as_bytes(), &root);
        assert_eq!(listed.files.len(), 1);
        assert_eq!(listed.missing, ["gone.ts", "src", outside.as_str()]);
        let note = listed.note_on_missing().expect("a note");
        assert!(note.starts_with("--files-from: ignored 3 that are not files"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_list_of_files_that_all_exist_has_nothing_to_say() {
        let root = project("quiet");
        assert!(
            Listed::parse(b"src/a.ts\n", &root)
                .note_on_missing()
                .is_none()
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

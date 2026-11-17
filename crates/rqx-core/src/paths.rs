//! The two path conversions every other module relies on: resolving a path to one
//! canonical form, and writing a path down as text.
//!
//! Both exist because the standard library's answers differ by platform. On Windows
//! `std::fs::canonicalize` returns a verbatim path (`\\?\C:\repo`) that never compares
//! equal to the `C:\repo` a working directory reports, and `Path::display` joins
//! components with `\`, which no glob written for a repository ever matches.

use std::io;
use std::path::{Component, Path, PathBuf};

/// The canonical form of a path that exists, without the `\\?\` prefix Windows adds.
/// Every comparison between two paths has to use this on both, so a path is never
/// canonical on one side and as-given on the other.
pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    dunce::canonicalize(path)
}

/// The canonical form when the path exists, else the path unchanged.
pub fn canonical_or_same(path: &Path) -> PathBuf {
    canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// A path as it appears in a finding, a report, a glob match or a cache key: its
/// components joined with `/` on every platform, so output and architecture globs do
/// not depend on where the scan ran.
pub fn display_path(path: &Path) -> String {
    let mut text = String::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => text.push_str(&prefix.as_os_str().to_string_lossy()),
            Component::RootDir => text.push('/'),
            Component::CurDir => push_part(&mut text, "."),
            Component::ParentDir => push_part(&mut text, ".."),
            Component::Normal(name) => push_part(&mut text, &name.to_string_lossy()),
        }
    }
    text
}

fn push_part(text: &mut String, part: &str) {
    let after_root_or_drive = text.ends_with('/') || text.ends_with(':');
    if !text.is_empty() && !after_root_or_drive {
        text.push('/');
    }
    text.push_str(part);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_canonical_path_has_no_verbatim_prefix() {
        let here = std::env::current_dir().expect("a working directory");
        let canonical = canonicalize(&here).expect("the working directory exists");
        assert!(!canonical.to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn a_path_that_does_not_exist_is_returned_as_given() {
        let missing = Path::new("no-such-path-here");
        assert_eq!(canonical_or_same(missing), missing);
    }

    #[test]
    fn components_are_joined_with_forward_slashes() {
        let nested = Path::new("domain").join("billing").join("invoice.ts");
        assert_eq!(display_path(&nested), "domain/billing/invoice.ts");
    }

    #[test]
    fn a_leading_current_directory_and_parent_steps_are_kept() {
        let path = Path::new(".").join("..").join("sibling").join("a.ts");
        assert_eq!(display_path(&path), "./../sibling/a.ts");
    }

    #[test]
    fn an_empty_path_is_empty() {
        assert_eq!(display_path(Path::new("")), "");
    }

    #[cfg(unix)]
    #[test]
    fn a_backslash_in_a_unix_file_name_is_part_of_the_name() {
        assert_eq!(display_path(Path::new("a\\b/c.ts")), "a\\b/c.ts");
    }

    #[cfg(unix)]
    #[test]
    fn an_absolute_path_keeps_its_root() {
        assert_eq!(display_path(Path::new("/tmp/x/a.ts")), "/tmp/x/a.ts");
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_path_uses_forward_slashes() {
        assert_eq!(
            display_path(Path::new(r"C:\repo\src\a.ts")),
            "C:/repo/src/a.ts"
        );
    }
}

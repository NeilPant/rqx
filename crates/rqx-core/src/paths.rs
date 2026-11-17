//! Resolving a path to one canonical form.
//!
//! On Windows `std::fs::canonicalize` returns a verbatim path (`\\?\C:\repo`) that never
//! compares equal to the `C:\repo` a working directory reports, so a path made relative
//! to either one keeps its prefix and reaches the output as `//?/C:/repo/...`.

use std::io;
use std::path::{Path, PathBuf};

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
}

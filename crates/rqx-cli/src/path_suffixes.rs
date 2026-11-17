//! Finding the walked file a recorded path names. A profile records the path the
//! runtime saw and rqx the path the scan walked, so the two agree on the end of a
//! path and not on its start: either one can be the longer.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Files by path, answering "which file does this path end, or end in?" without
/// comparing the path with every file.
pub(crate) struct PathSuffixes<'files, V> {
    files: &'files BTreeMap<Arc<str>, V>,
    /// A path component boundary of a file's path, to the files that end there:
    /// `src/a/b.ts` is listed under `a/b.ts` and under `b.ts`.
    ending: HashMap<&'files str, Vec<&'files Arc<str>>>,
}

impl<'files, V> PathSuffixes<'files, V> {
    pub(crate) fn of(files: &'files BTreeMap<Arc<str>, V>) -> Self {
        let mut ending: HashMap<&str, Vec<&Arc<str>>> = HashMap::new();
        for path in files.keys() {
            for suffix in suffixes_after_separator(path) {
                ending.entry(suffix).or_default().push(path);
            }
        }
        Self { files, ending }
    }

    /// The first file, in path order, that is `path`, ends in `/path`, or is a
    /// path-suffix of `path` after a `/`.
    pub(crate) fn find(&self, path: &str) -> Option<&'files V> {
        let exact = self.files.get_key_value(path);
        let shorter =
            suffixes_after_separator(path).filter_map(|rest| self.files.get_key_value(rest));
        let longer = self
            .ending
            .get(path)
            .into_iter()
            .flatten()
            .filter_map(|known| self.files.get_key_value(known.as_ref()));
        exact
            .into_iter()
            .chain(shorter)
            .chain(longer)
            .min_by(|(left, _), (right, _)| left.cmp(right))
            .map(|(_, value)| value)
    }
}

fn suffixes_after_separator(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/').map(move |(at, _)| &path[at + 1..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> BTreeMap<Arc<str>, usize> {
        paths
            .iter()
            .enumerate()
            .map(|(order, path)| (Arc::from(*path), order))
            .collect()
    }

    fn scanned_by_comparison<'a>(
        files: &'a BTreeMap<Arc<str>, usize>,
        path: &str,
    ) -> Option<&'a usize> {
        files
            .iter()
            .find(|(known, _)| {
                known.as_ref() == path
                    || path.ends_with(&format!("/{known}"))
                    || known.ends_with(&format!("/{path}"))
            })
            .map(|(_, value)| value)
    }

    #[test]
    fn it_answers_as_comparing_with_every_file_does() {
        let known = files(&[
            "a/main.py",
            "b/main.py",
            "src/app/util.ts",
            "util.ts",
            "lib/x/y/z.go",
            "z.go",
        ]);
        let index = PathSuffixes::of(&known);
        for path in [
            "a/main.py",
            "main.py",
            "/work/repo/b/main.py",
            "/work/repo/src/app/util.ts",
            "app/util.ts",
            "util.ts",
            "x/y/z.go",
            "/srv/lib/x/y/z.go",
            "z.go",
            "missing.rs",
            "in.py",
            "",
            "/",
        ] {
            assert_eq!(
                index.find(path),
                scanned_by_comparison(&known, path),
                "{path:?}"
            );
        }
    }
}

use std::path::{Path, PathBuf};

/// Canonicalised rather than joined lexically, because a symlink inside the project
/// that points outside it is only visible after the link resolves — and an agent that
/// can name a path can otherwise read `/etc` through a scan.
pub fn within(root: &Path, asked: Option<&str>) -> Result<PathBuf, String> {
    let Some(asked) = asked.map(str::trim).filter(|asked| !asked.is_empty()) else {
        return Ok(root.to_path_buf());
    };
    let real = root
        .join(asked)
        .canonicalize()
        .map_err(|_| outside(root, asked))?;
    if real.starts_with(root) {
        Ok(real)
    } else {
        Err(outside(root, asked))
    }
}

/// The same sentence whether the path escaped or never existed: telling the two apart
/// answers "does this file exist outside your root", which is the question containment
/// is there to refuse.
fn outside(root: &Path, asked: &str) -> String {
    format!(
        "`{asked}` is not inside `{}`. rqx only reads within the directory it was started on.",
        root.display()
    )
}

/// The scan a cursor came from travels with it, so one that outlives its scan is
/// refused rather than silently paging through a different set of findings.
pub struct Cursor {
    pub scan: u64,
    pub offset: usize,
}

impl Cursor {
    pub fn format(scan: u64, offset: usize) -> String {
        format!("{scan}:{offset}")
    }

    pub fn parse(text: &str, scan: u64) -> Result<Self, String> {
        let (left, right) = text.split_once(':').ok_or_else(|| malformed(text))?;
        let cursor = Self {
            scan: left.parse().map_err(|_| malformed(text))?,
            offset: right.parse().map_err(|_| malformed(text))?,
        };
        if cursor.scan != scan {
            return Err(format!(
                "cursor `{text}` is from scan {} and the project has been scanned again since \
                 (now {scan}). Ask again without a cursor.",
                cursor.scan
            ));
        }
        Ok(cursor)
    }
}

fn malformed(text: &str) -> String {
    format!("`{text}` is not a cursor. Pass back the `next_cursor` a previous answer gave you.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rqx mcp {name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("a sandbox");
        root.canonicalize().expect("a real path")
    }

    #[test]
    fn no_path_is_the_root_itself() {
        let root = sandbox("no-path");
        assert_eq!(within(&root, None).expect("the root"), root);
        assert_eq!(within(&root, Some("  ")).expect("the root"), root);
    }

    #[test]
    fn a_subdirectory_is_allowed() {
        let root = sandbox("subdirectory");
        assert_eq!(
            within(&root, Some("src")).expect("the subdirectory"),
            root.join("src")
        );
    }

    #[test]
    fn climbing_out_is_refused() {
        let root = sandbox("climbing");
        assert!(within(&root, Some("..")).is_err());
        assert!(within(&root, Some("../..")).is_err());
        assert!(within(&root, Some("src/../..")).is_err());
    }

    #[test]
    fn an_absolute_path_elsewhere_is_refused() {
        let root = sandbox("absolute");
        assert!(within(&root, Some("/etc")).is_err());
    }

    /// A lexical prefix check passes this one: the string starts inside the root and
    /// only the resolved link leaves it.
    #[test]
    #[cfg(unix)]
    fn a_symlink_out_of_the_root_is_refused() {
        let root = sandbox("symlink");
        std::os::unix::fs::symlink("/etc", root.join("escape")).expect("a link");
        assert!(within(&root, Some("escape")).is_err());
    }

    #[test]
    fn a_cursor_survives_its_own_scan_and_no_other() {
        let cursor = Cursor::parse(&Cursor::format(7, 40), 7).expect("a cursor");
        assert_eq!((cursor.scan, cursor.offset), (7, 40));
        assert!(Cursor::parse(&Cursor::format(6, 40), 7).is_err());
        assert!(Cursor::parse("40", 7).is_err());
        assert!(Cursor::parse("seven:40", 7).is_err());
    }
}

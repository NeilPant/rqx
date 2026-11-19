//! A temporary directory that removes itself. Shared by the integration tests, through
//! `tests/common`, and by the unit tests inside the binary crate.

use std::path::{Path, PathBuf};

/// A directory under the system temporary directory that is removed, with everything in
/// it, when the value is dropped, including when the test fails. It dereferences to its
/// path, so it is passed wherever a `&Path` is wanted.
pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// A new, empty directory whose name starts with `rqx-` and `label`. The rest of
    /// the name is unique to the process and the call, so two tests that use the same
    /// label do not share a directory.
    pub fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "rqx-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Self { path }
    }
}

impl std::ops::Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<std::ffi::OsStr> for Scratch {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.path.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

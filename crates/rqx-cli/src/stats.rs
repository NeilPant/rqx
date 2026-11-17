//! What a scan did, counted rather than timed.
//!
//! A time moves with the machine, so a regression in it needs a noisy benchmark to
//! find. A count does not: a warm run that reads every file again, or a scan that
//! walks the tree a third time, shows up as a different number on any machine.

use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanStats {
    pub cache_hits: u32,
    pub cache_misses: u32,
    pub files_read: u32,
    pub walks: u32,
    /// Every `git` the scan started, blames included.
    pub git_processes: u32,
    pub blames: u32,
}

/// Files opened for analysis. Counted from the worker threads, so it is an atomic
/// the scan owns and hands to them rather than a field the collector could bump.
#[derive(Default)]
pub struct FileReads(AtomicU32);

impl FileReads {
    pub fn note(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

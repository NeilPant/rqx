use serde::{Deserialize, Serialize};

/// A byte range in one source file. Byte offsets rather than line/column because a
/// parser reports them for free and every consumer that wants a position already has
/// the file's `LineIndex` to convert with.
#[derive(Copy, Clone, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(self) -> bool {
        self.end <= self.start
    }

    pub fn text(self, source: &str) -> &str {
        let start = (self.start as usize).min(source.len());
        let end = (self.end as usize).clamp(start, source.len());
        source.get(start..end).unwrap_or_default()
    }
}

impl From<(u32, u32)> for Span {
    fn from((start, end): (u32, u32)) -> Self {
        Self::new(start, end)
    }
}

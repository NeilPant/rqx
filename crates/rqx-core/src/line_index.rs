/// Where every line of a file begins, kept apart from the text so a caller that holds
/// the text itself can build this once and ask it many times.
pub struct LineStarts(Vec<u32>);

impl LineStarts {
    pub fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset as u32 + 1),
        );
        Self(starts)
    }

    /// `source` has to be the text these starts were built from.
    pub fn locate(&self, source: &str, offset: u32) -> (u32, u32) {
        let line = self.0.partition_point(|start| *start <= offset).max(1);
        let line_start = self.0[line - 1] as usize;
        // An offset pointing inside a multi-byte character would panic the slice, and
        // an editor's synthetic span is under nobody's control but the editor's.
        let mut end = (offset as usize).min(source.len());
        while end > line_start && !source.is_char_boundary(end) {
            end -= 1;
        }
        let column = source[line_start..end].chars().count() as u32 + 1;
        (line as u32, column)
    }

    /// Lines are 1-based, so line 0 does not exist; taking it as the first line beats
    /// subtracting into a wrapped index.
    pub fn line_start(&self, line: u32) -> u32 {
        self.0.get(line.max(1) as usize - 1).copied().unwrap_or(0)
    }

    fn next_start(&self, line: u32) -> Option<u32> {
        self.0.get(line as usize).copied()
    }

    pub fn line_count(&self) -> u32 {
        self.0.len() as u32
    }
}

/// Byte offset to line and column, and back to the text of a line. Every finding is
/// discovered at a byte offset and reported at a line, so this sits between the two.
pub struct LineIndex<'src> {
    source: &'src str,
    starts: LineStarts,
}

impl<'src> LineIndex<'src> {
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            starts: LineStarts::new(source),
        }
    }

    pub fn locate(&self, offset: u32) -> (u32, u32) {
        self.starts.locate(self.source, offset)
    }

    pub fn line_start(&self, line: u32) -> u32 {
        self.starts.line_start(line)
    }

    pub fn line_text(&self, line: u32) -> &'src str {
        let start = self.line_start(line) as usize;
        let end = self
            .starts
            .next_start(line)
            .map_or(self.source.len(), |next| next as usize);
        self.source[start..end].trim_end_matches(['\n', '\r'])
    }

    pub fn line_count(&self) -> u32 {
        self.starts.line_count()
    }

    pub fn code_lines(&self) -> u32 {
        self.source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_starts_place_every_offset_where_the_index_does() {
        let source = "let a = 1;\nlet é = \"日本\";\r\n\nend";
        let index = LineIndex::new(source);
        let starts = LineStarts::new(source);

        for offset in 0..=source.len() as u32 + 2 {
            assert_eq!(
                starts.locate(source, offset),
                index.locate(offset),
                "{offset}"
            );
        }
        assert_eq!(starts.line_count(), index.line_count());
    }
}

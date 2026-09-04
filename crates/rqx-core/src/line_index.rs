/// Byte offset to line and column, and back to the text of a line. Every finding is
/// discovered at a byte offset and reported at a line, so this sits between the two.
pub struct LineIndex<'src> {
    source: &'src str,
    starts: Vec<u32>,
}

impl<'src> LineIndex<'src> {
    pub fn new(source: &'src str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset as u32 + 1),
        );
        Self { source, starts }
    }

    pub fn locate(&self, offset: u32) -> (u32, u32) {
        let line = self.starts.partition_point(|start| *start <= offset).max(1);
        let line_start = self.starts[line - 1] as usize;
        // An offset pointing inside a multi-byte character would panic the slice, and
        // an editor's synthetic span is under nobody's control but the editor's.
        let mut end = (offset as usize).min(self.source.len());
        while end > line_start && !self.source.is_char_boundary(end) {
            end -= 1;
        }
        let column = self.source[line_start..end].chars().count() as u32 + 1;
        (line as u32, column)
    }

    /// Lines are 1-based, so line 0 does not exist; taking it as the first line beats
    /// subtracting into a wrapped index.
    pub fn line_start(&self, line: u32) -> u32 {
        self.starts
            .get(line.max(1) as usize - 1)
            .copied()
            .unwrap_or(0)
    }

    pub fn line_text(&self, line: u32) -> &'src str {
        let start = self.line_start(line) as usize;
        let end = self
            .starts
            .get(line as usize)
            .map_or(self.source.len(), |next| *next as usize);
        self.source[start..end].trim_end_matches(['\n', '\r'])
    }

    pub fn line_count(&self) -> u32 {
        self.starts.len() as u32
    }

    pub fn code_lines(&self) -> u32 {
        self.source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count() as u32
    }
}

//! The trimmed lines of the fixture files, which is what an expectation is keyed on.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub struct Sources {
    lines: BTreeMap<String, Vec<String>>,
}

impl Sources {
    /// A file that cannot be read has no lines, so an entry naming a deleted fixture
    /// is reported as matching nothing.
    pub fn read(root: &Path, files: BTreeSet<&str>) -> Self {
        Self::from_texts(files.into_iter().map(|file| {
            let bytes = fs::read(root.join(file)).unwrap_or_default();
            (file, String::from_utf8_lossy(&bytes).into_owned())
        }))
    }

    pub fn from_texts<'a>(files: impl IntoIterator<Item = (&'a str, String)>) -> Self {
        let lines = files
            .into_iter()
            .map(|(file, text)| {
                let trimmed = text.lines().map(|line| line.trim().to_owned()).collect();
                (file.to_owned(), trimmed)
            })
            .collect();
        Self { lines }
    }

    fn of(&self, file: &str) -> &[String] {
        self.lines.get(file).map_or(&[], Vec::as_slice)
    }

    pub fn text_at(&self, file: &str, line: usize) -> Option<&str> {
        let index = line.checked_sub(1)?;
        self.of(file).get(index).map(String::as_str)
    }

    /// Which of the file's lines with this line's text it is, counting from one.
    pub fn occurrence_of(&self, file: &str, line: usize) -> Option<usize> {
        let text = self.text_at(file, line)?;
        let earlier = &self.of(file)[..line - 1];
        Some(earlier.iter().filter(|other| *other == text).count() + 1)
    }

    /// The one-based line holding the given occurrence of the text.
    pub fn line_of(&self, file: &str, text: &str, occurrence: usize) -> Option<usize> {
        self.of(file)
            .iter()
            .enumerate()
            .filter(|(_, line)| *line == text)
            .nth(occurrence.checked_sub(1)?)
            .map(|(index, _)| index + 1)
    }
}

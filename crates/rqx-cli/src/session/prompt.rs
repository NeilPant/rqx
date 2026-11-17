//! The line being typed: what is in it, where the cursor is, and what was typed before.
//!
//! Pure state with no terminal in it, so every editing rule is a unit test.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Entries older than this fall off the front, so a long-lived history cannot grow
/// without bound in memory.
const KEPT: usize = 1000;

#[derive(Default)]
pub struct Prompt {
    text: Vec<char>,
    cursor: usize,
    history: History,
}

impl Prompt {
    pub fn with_history(entries: Vec<String>) -> Self {
        Self {
            history: History::new(entries),
            ..Self::default()
        }
    }

    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// The cursor, in characters from the start.
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Everything left of the cursor, which is what completion reads.
    pub fn before_cursor(&self) -> String {
        self.text[..self.cursor].iter().collect()
    }

    /// Applies an editing key. `false` means the key is not one of these, so the caller
    /// may give it another meaning.
    pub fn edit(&mut self, key: KeyEvent) -> bool {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('u') if control => self.delete_before_cursor(),
            KeyCode::Char('w') if control => self.delete_word_before_cursor(),
            KeyCode::Char('a') if control => self.cursor = 0,
            KeyCode::Char('e') if control => self.cursor = self.text.len(),
            KeyCode::Char(character) if !control && !alt => self.insert(character),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.text.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            _ => return false,
        }
        true
    }

    fn insert(&mut self, character: char) {
        self.text.insert(self.cursor, character);
        self.cursor += 1;
        self.history.stop_browsing();
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.text.remove(self.cursor);
            self.history.stop_browsing();
        }
    }

    fn delete(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
            self.history.stop_browsing();
        }
    }

    fn delete_before_cursor(&mut self) {
        self.text.drain(..self.cursor);
        self.cursor = 0;
        self.history.stop_browsing();
    }

    /// The word before the cursor and the spaces between it and the cursor.
    fn delete_word_before_cursor(&mut self) {
        let mut start = self.cursor;
        while start > 0 && self.text[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !self.text[start - 1].is_whitespace() {
            start -= 1;
        }
        self.text.drain(start..self.cursor);
        self.cursor = start;
        self.history.stop_browsing();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.history.stop_browsing();
    }

    /// Puts `text` in the line with the cursor at its end.
    pub fn set(&mut self, text: &str) {
        self.text = text.chars().collect();
        self.cursor = self.text.len();
    }

    /// Replaces the text left of the cursor from character `from`, and leaves the
    /// cursor after what went in. What completion does with an accepted suggestion.
    pub fn replace_before_cursor(&mut self, from: usize, with: &str) {
        let inserted: Vec<char> = with.chars().collect();
        let after = self.text.split_off(self.cursor);
        self.text.truncate(from);
        self.text.extend(&inserted);
        self.cursor = self.text.len();
        self.text.extend(after);
        self.history.stop_browsing();
    }

    /// Takes the line, remembers it, and leaves the prompt empty.
    pub fn submit(&mut self) -> String {
        let line = self.text();
        self.text.clear();
        self.cursor = 0;
        self.history.remember(&line);
        line
    }

    pub fn recall_older(&mut self) {
        let current = self.text();
        if let Some(entry) = self.history.older(&current) {
            self.set(&entry);
        }
    }

    pub fn recall_newer(&mut self) {
        if let Some(entry) = self.history.newer() {
            self.set(&entry);
        }
    }
}

/// What was submitted, oldest first, and where in it the arrow keys are.
#[derive(Default)]
struct History {
    entries: Vec<String>,
    /// The entry on show while browsing, as an index into `entries`.
    browsing: Option<usize>,
    /// What was being typed when browsing began, so coming back down returns to it.
    draft: String,
}

impl History {
    fn new(mut entries: Vec<String>) -> Self {
        let excess = entries.len().saturating_sub(KEPT);
        entries.drain(..excess);
        Self {
            entries,
            ..Self::default()
        }
    }

    fn remember(&mut self, line: &str) {
        self.stop_browsing();
        let line = line.trim();
        if line.is_empty() || self.entries.last().is_some_and(|last| last == line) {
            return;
        }
        self.entries.push(line.to_owned());
        if self.entries.len() > KEPT {
            self.entries.remove(0);
        }
    }

    fn stop_browsing(&mut self) {
        self.browsing = None;
    }

    fn older(&mut self, current: &str) -> Option<String> {
        let next = match self.browsing {
            None if self.entries.is_empty() => return None,
            None => {
                self.draft = current.to_owned();
                self.entries.len() - 1
            }
            Some(0) => 0,
            Some(at) => at - 1,
        };
        self.browsing = Some(next);
        Some(self.entries[next].clone())
    }

    fn newer(&mut self) -> Option<String> {
        let at = self.browsing?;
        if at + 1 < self.entries.len() {
            self.browsing = Some(at + 1);
            return Some(self.entries[at + 1].clone());
        }
        self.browsing = None;
        Some(std::mem::take(&mut self.draft))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn control(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }

    fn typed(prompt: &mut Prompt, text: &str) {
        for character in text.chars() {
            prompt.edit(key(KeyCode::Char(character)));
        }
    }

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "plan");
        prompt.edit(key(KeyCode::Home));
        typed(&mut prompt, "/");
        assert_eq!(prompt.text(), "/plan");
        assert_eq!(prompt.cursor(), 1);
    }

    #[test]
    fn the_arrows_home_and_end_move_within_the_line_and_stop_at_its_ends() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "abc");
        prompt.edit(key(KeyCode::Right));
        assert_eq!(prompt.cursor(), 3);
        prompt.edit(key(KeyCode::Left));
        prompt.edit(key(KeyCode::Left));
        assert_eq!(prompt.cursor(), 1);
        prompt.edit(key(KeyCode::Home));
        prompt.edit(key(KeyCode::Left));
        assert_eq!(prompt.cursor(), 0);
        prompt.edit(key(KeyCode::End));
        assert_eq!(prompt.cursor(), 3);
    }

    #[test]
    fn backspace_removes_before_the_cursor_and_delete_removes_at_it() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "abcd");
        prompt.edit(key(KeyCode::Left));
        prompt.edit(key(KeyCode::Backspace));
        assert_eq!(prompt.text(), "abd");
        prompt.edit(key(KeyCode::Delete));
        assert_eq!(prompt.text(), "ab");
        prompt.edit(key(KeyCode::Delete));
        assert_eq!(prompt.text(), "ab", "delete at the end does nothing");
        prompt.edit(key(KeyCode::Home));
        prompt.edit(key(KeyCode::Backspace));
        assert_eq!(prompt.text(), "ab", "backspace at the start does nothing");
    }

    #[test]
    fn ctrl_u_deletes_to_the_start_and_keeps_what_follows_the_cursor() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "/plan --budget 20");
        for _ in 0..3 {
            prompt.edit(key(KeyCode::Left));
        }
        prompt.edit(control('u'));
        assert_eq!(prompt.text(), " 20");
        assert_eq!(prompt.cursor(), 0);
    }

    #[test]
    fn ctrl_w_deletes_the_word_before_the_cursor_and_the_spaces_after_it() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "/plan --budget  ");
        prompt.edit(control('w'));
        assert_eq!(prompt.text(), "/plan ");
        prompt.edit(control('w'));
        assert_eq!(prompt.text(), "");
        prompt.edit(control('w'));
        assert_eq!(prompt.text(), "", "nothing left to delete");
    }

    #[test]
    fn ctrl_a_and_ctrl_e_are_home_and_end() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "abc");
        prompt.edit(control('a'));
        assert_eq!(prompt.cursor(), 0);
        prompt.edit(control('e'));
        assert_eq!(prompt.cursor(), 3);
    }

    #[test]
    fn a_key_that_is_not_editing_is_left_to_the_caller() {
        let mut prompt = Prompt::default();
        assert!(!prompt.edit(key(KeyCode::Enter)));
        assert!(!prompt.edit(key(KeyCode::Up)));
        assert!(!prompt.edit(control('d')));
    }

    #[test]
    fn wide_characters_are_one_step_each() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "\u{4e2d}\u{6587}");
        prompt.edit(key(KeyCode::Left));
        prompt.edit(key(KeyCode::Backspace));
        assert_eq!(prompt.text(), "\u{6587}");
    }

    #[test]
    fn replacing_before_the_cursor_keeps_what_follows_it() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "/pl --json");
        prompt.edit(control('a'));
        for _ in 0..3 {
            prompt.edit(key(KeyCode::Right));
        }
        prompt.replace_before_cursor(0, "/plan ");
        assert_eq!(prompt.text(), "/plan  --json");
        assert_eq!(prompt.cursor(), 6);
    }

    #[test]
    fn submitting_empties_the_line_and_remembers_it() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "/plan");
        assert_eq!(prompt.submit(), "/plan");
        assert_eq!(prompt.text(), "");
        prompt.recall_older();
        assert_eq!(prompt.text(), "/plan");
    }

    #[test]
    fn the_arrows_walk_the_history_and_come_back_to_the_draft() {
        let mut prompt = Prompt::with_history(vec!["/plan".into(), "/health".into()]);
        typed(&mut prompt, "half a thought");
        prompt.recall_older();
        assert_eq!(prompt.text(), "/health");
        prompt.recall_older();
        assert_eq!(prompt.text(), "/plan");
        prompt.recall_older();
        assert_eq!(prompt.text(), "/plan", "the oldest is the end of the line");
        prompt.recall_newer();
        assert_eq!(prompt.text(), "/health");
        prompt.recall_newer();
        assert_eq!(prompt.text(), "half a thought");
        prompt.recall_newer();
        assert_eq!(
            prompt.text(),
            "half a thought",
            "nothing newer than the draft"
        );
    }

    #[test]
    fn editing_a_recalled_line_leaves_the_history_alone() {
        let mut prompt = Prompt::with_history(vec!["/plan".into()]);
        prompt.recall_older();
        typed(&mut prompt, " --json");
        assert_eq!(prompt.submit(), "/plan --json");
        prompt.recall_older();
        assert_eq!(prompt.text(), "/plan --json");
        prompt.recall_older();
        assert_eq!(prompt.text(), "/plan");
    }

    #[test]
    fn a_repeat_or_an_empty_line_is_not_remembered_twice() {
        let mut prompt = Prompt::default();
        for line in ["/plan", "/plan", "  ", "/health"] {
            prompt.set(line);
            prompt.submit();
        }
        prompt.recall_older();
        assert_eq!(prompt.text(), "/health");
        prompt.recall_older();
        assert_eq!(prompt.text(), "/plan");
        prompt.recall_older();
        assert_eq!(prompt.text(), "/plan");
    }

    #[test]
    fn history_with_nothing_in_it_leaves_the_line_alone() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "abc");
        prompt.recall_older();
        prompt.recall_newer();
        assert_eq!(prompt.text(), "abc");
    }
}

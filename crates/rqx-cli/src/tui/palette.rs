//! One place to type what you want.
//!
//! Twenty-nine subcommands and nine tabs is more than anyone reads a menu for. The
//! palette is the answer to "which command do I want": type a few letters of it and
//! it is the first row.

use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::brand::{Ink, Palette};

use super::app::{Nav, Session, Tab, is_press, row_at, wheel};
use super::chrome::centred;

/// What choosing a row does.
#[derive(Clone, PartialEq, Eq)]
pub enum Choice {
    Go(Tab),
    Nav(Nav),
    /// Run an analysis that has no screen and show what it prints.
    Read(Page),
    /// Open one file's findings.
    File(String),
    /// Explain one rule, in the catalogue.
    Rule(String),
}

/// An analysis whose answer is a page of text rather than a table to move around in.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Page {
    Recommend,
    Suppressions,
    Patterns,
    Techniques,
    Levels,
}

impl Page {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Recommend => "recommend",
            Self::Suppressions => "suppressions",
            Self::Patterns => "patterns",
            Self::Techniques => "techniques",
            Self::Levels => "levels",
        }
    }

    const fn about(self) -> &'static str {
        match self {
            Self::Recommend => "what to do about this project, not what is wrong with it",
            Self::Suppressions => "every finding a comment waives, and why",
            Self::Patterns => "which design patterns are half applied, and where one fits",
            Self::Techniques => "every refactoring rqx prescribes",
            Self::Levels => "what each cleanliness level changes",
        }
    }
}

struct Entry {
    label: String,
    about: String,
    choice: Choice,
}

/// The lines above the first row inside the overlay: the top border, then the input,
/// then the rule under it.
const ABOVE: u16 = 3;
const HEIGHT: u16 = 16;
const WIDTH: u16 = 72;

#[derive(Default)]
pub struct CommandPalette {
    typed: String,
    shown: Vec<Entry>,
    state: ListState,
    at: Rect,
}

impl CommandPalette {
    pub fn open(&mut self, session: &Session) {
        self.typed.clear();
        self.rebuild(session);
    }

    /// Every place the palette can send you, before any narrowing. Rebuilt per
    /// keystroke rather than held, because the file list follows the last scan and a
    /// stale copy of it would offer files that are gone.
    fn every(session: &Session) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Tab::ALL
            .iter()
            .chain(std::iter::once(&Tab::Config))
            .map(|tab| Entry {
                label: tab.title().to_owned(),
                about: tab.promise().to_owned(),
                choice: Choice::Go(*tab),
            })
            .collect();

        entries.push(Entry {
            label: "scan".to_owned(),
            about: "analyse the whole project again".to_owned(),
            choice: Choice::Nav(Nav::Scan),
        });
        entries.push(Entry {
            label: "refresh".to_owned(),
            about: "re-analyse only what changed on disk".to_owned(),
            choice: Choice::Nav(Nav::Refresh),
        });
        entries.push(Entry {
            label: if session.watching {
                "unwatch".to_owned()
            } else {
                "watch".to_owned()
            },
            about: "re-analyse on every save".to_owned(),
            choice: Choice::Nav(Nav::Watch),
        });
        entries.push(Entry {
            label: "quit".to_owned(),
            about: "leave the interface".to_owned(),
            choice: Choice::Nav(Nav::Quit),
        });

        for page in [
            Page::Recommend,
            Page::Suppressions,
            Page::Patterns,
            Page::Techniques,
            Page::Levels,
        ] {
            entries.push(Entry {
                label: page.title().to_owned(),
                about: page.about().to_owned(),
                choice: Choice::Read(page),
            });
        }

        if let Some(outcome) = &session.outcome {
            for file in outcome
                .analyzed
                .iter()
                .filter(|file| session.in_scope(file))
            {
                entries.push(Entry {
                    label: file.clone(),
                    about: "open this file's findings".to_owned(),
                    choice: Choice::File(file.clone()),
                });
            }
        }
        for rule in rqx_core::rule::RuleId::ALL {
            let spec = rule.spec();
            entries.push(Entry {
                label: format!("explain {}", spec.slug),
                about: spec.why.to_owned(),
                choice: Choice::Rule(spec.slug.to_owned()),
            });
        }
        entries
    }

    fn rebuild(&mut self, session: &Session) {
        let typed = self.typed.to_lowercase();
        let mut scored: Vec<(usize, Entry)> = Self::every(session)
            .into_iter()
            .filter_map(|entry| score(&entry.label, &typed).map(|rank| (rank, entry)))
            .collect();
        // Rank first, then alphabetically, so the list does not reshuffle among equals
        // as you type and the row under the cursor stays the row you were aiming at.
        scored.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.label.cmp(&right.1.label)));
        self.shown = scored.into_iter().map(|(_, entry)| entry).collect();
        self.state.select((!self.shown.is_empty()).then_some(0));
    }

    fn step(&mut self, delta: isize) {
        if self.shown.is_empty() {
            return;
        }
        let current = self.state.selected().unwrap_or(0) as isize;
        self.state.select(Some(
            (current + delta).clamp(0, self.shown.len() as isize - 1) as usize,
        ));
    }

    fn chosen(&self) -> Option<Choice> {
        Some(self.shown.get(self.state.selected()?)?.choice.clone())
    }

    /// `None` means the palette stays open. `Some(None)` means it closes with nothing
    /// chosen; `Some(Some(choice))` means it closes and that happens.
    pub fn handle(&mut self, key: KeyEvent, session: &Session) -> Option<Option<Choice>> {
        match key.code {
            KeyCode::Esc => Some(None),
            KeyCode::Enter => Some(self.chosen()),
            KeyCode::Down => {
                self.step(1);
                None
            }
            KeyCode::Up => {
                self.step(-1);
                None
            }
            KeyCode::PageDown => {
                self.step(8);
                None
            }
            KeyCode::PageUp => {
                self.step(-8);
                None
            }
            KeyCode::Backspace => {
                self.typed.pop();
                self.rebuild(session);
                None
            }
            KeyCode::Char(character) => {
                self.typed.push(character);
                self.rebuild(session);
                None
            }
            _ => None,
        }
    }

    pub fn click(&mut self, event: MouseEvent) -> Option<Option<Choice>> {
        if let Some(rows) = wheel(event) {
            self.step(rows);
            return None;
        }
        if !is_press(event) {
            return None;
        }
        let Some(row) = row_at(self.at, ABOVE, self.state.offset(), event) else {
            // A click outside the overlay dismisses it, which is what a click outside
            // a dialog means everywhere else.
            return Some(None);
        };
        if row >= self.shown.len() {
            return None;
        }
        self.state.select(Some(row));
        Some(self.chosen())
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, palette: Palette) {
        let area = centred(frame.area(), WIDTH, HEIGHT);
        self.at = area;
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(palette.rule()),
            area,
        );

        let inner = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(2),
        };
        let [prompt, list] = ratatui::layout::Layout::vertical([
            ratatui::layout::Constraint::Length(2),
            ratatui::layout::Constraint::Min(0),
        ])
        .areas(inner);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("\u{203a} ", Style::new().fg(palette.color(Ink::Accent))),
                Span::styled(
                    self.typed.clone(),
                    Style::new().fg(palette.color(Ink::Text)),
                ),
                Span::styled("\u{2588}", Style::new().fg(palette.color(Ink::Accent))),
            ])),
            prompt,
        );

        if self.shown.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::styled("  nothing matches", palette.dim())),
                list,
            );
            return;
        }

        let items: Vec<ListItem<'static>> = self
            .shown
            .iter()
            .map(|entry| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:<28}", trim(&entry.label, 28)),
                        Style::new().fg(palette.color(Ink::Text)),
                    ),
                    Span::styled(trim(&entry.about, 38), palette.dim()),
                ]))
            })
            .collect();
        frame.render_stateful_widget(
            List::new(items).highlight_style(
                Style::new()
                    .fg(palette.color(Ink::Accent))
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            ),
            list,
            &mut self.state,
        );
    }
}

/// A subsequence match, ranked by how tightly the letters sit together and how early
/// they start. Not a full fuzzy scorer: the labels are short, and "cplx" reaching
/// Complexity is the whole requirement.
fn score(label: &str, typed: &str) -> Option<usize> {
    if typed.is_empty() {
        return Some(0);
    }
    let label = label.to_lowercase();
    if let Some(at) = label.find(typed) {
        return Some(at);
    }
    let mut rank = 0;
    let mut last = None;
    let mut haystack = label.char_indices();
    for wanted in typed.chars() {
        let at = haystack.find(|(_, character)| *character == wanted)?.0;
        rank += at - last.map_or(0, |previous| previous) + 1;
        last = Some(at);
    }
    Some(rank + label.len())
}

fn trim(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "\u{2026}"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_beats_a_scattered_match() {
        let prefix = score("complexity", "com").expect("a match");
        let scattered = score("cross-module", "com").expect("a match");
        assert!(prefix < scattered, "{prefix} !< {scattered}");
    }

    #[test]
    fn the_letters_of_a_word_reach_it_in_order() {
        assert!(score("complexity", "cplx").is_some());
        assert!(
            score("complexity", "xlpc").is_none(),
            "order does not matter"
        );
    }

    #[test]
    fn nothing_typed_matches_everything_equally() {
        assert_eq!(score("anything", ""), Some(0));
    }
}

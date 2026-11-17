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

use crate::session::fuzzy::score;

use super::app::{Nav, Page, Session, Tab, Task, is_press, row_at, wheel};
use super::chrome::centred;

struct Entry {
    label: String,
    about: String,
    nav: Nav,
    group: &'static str,
}

/// What the list holds. With nothing typed the palette is the menu — the groups in
/// order, each under its heading — and the moment anything is typed it flattens into
/// ranked matches, because a heading over one row is noise.
enum Row {
    Heading(&'static str),
    Entry(Entry),
}

/// The groups, in the order they are offered. Analyse first because it is what a
/// session starts with; Session last because it is how one ends.
const GROUPS: [&str; 5] = ["Analyse", "Browse", "Report", "Set up", "Session"];

/// What a review compares against unless you say otherwise.
const DEFAULT_BASE: &str = "origin/HEAD";

/// The lines above the first row inside the overlay: the top border, then the input,
/// then the rule under it.
const ABOVE: u16 = 3;
/// Tall enough to hold a group and its entries without scrolling, since with nothing
/// typed this is the menu; `centred` clamps it to the frame on a short terminal.
const HEIGHT: u16 = 26;
const WIDTH: u16 = 74;

#[derive(Default)]
pub struct CommandPalette {
    typed: String,
    shown: Vec<Row>,
    state: ListState,
    at: Rect,
    /// The question a chosen row still has to answer before it can run. Only a
    /// review has one: it needs a revision, and guessing one runs the wrong scan.
    asking: Option<Asking>,
}

struct Asking {
    what: &'static str,
    answer: String,
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
        let mut entries: Vec<Entry> = Vec::new();
        let mut add = |group, label: String, about: String, nav| {
            entries.push(Entry {
                label,
                about,
                nav,
                group,
            });
        };

        add(
            "Analyse",
            "scan".to_owned(),
            "analyse this project, every file".to_owned(),
            Nav::Scan,
        );
        add(
            "Analyse",
            "review a diff".to_owned(),
            "only what changed against a revision".to_owned(),
            Nav::Ask,
        );
        add(
            "Analyse",
            "refresh".to_owned(),
            "re-analyse only what changed on disk".to_owned(),
            Nav::Refresh,
        );
        add(
            "Analyse",
            if session.watching { "unwatch" } else { "watch" }.to_owned(),
            "re-analyse on every save".to_owned(),
            Nav::Watch,
        );

        for tab in Tab::ALL {
            add(
                "Browse",
                tab.title().to_owned(),
                tab.promise().to_owned(),
                Nav::To(tab),
            );
        }
        for page in Page::ALL {
            add(
                "Report",
                page.title().to_owned(),
                page.about().to_owned(),
                Nav::Read(page),
            );
        }
        for task in [Task::WriteConfig, Task::Baseline, Task::ClearCache] {
            add(
                "Set up",
                task.title().to_lowercase(),
                task.about().to_owned(),
                Nav::Do(task),
            );
        }
        add(
            "Session",
            "settings".to_owned(),
            Tab::Config.promise().to_owned(),
            Nav::To(Tab::Config),
        );
        add(
            "Session",
            "quit".to_owned(),
            "leave, and print what the session found".to_owned(),
            Nav::Quit,
        );

        // Files and rules are reachable but not listed: there are thousands of them
        // and a menu of thousands is a search box with extra steps. Typing finds them.
        if let Some(outcome) = &session.outcome {
            for file in outcome
                .analyzed
                .iter()
                .filter(|file| session.in_scope(file))
            {
                add(
                    "",
                    file.clone(),
                    "open this file's findings".to_owned(),
                    Nav::Inspect(file.clone()),
                );
            }
        }
        for rule in rqx_core::rule::RuleId::ALL {
            let spec = rule.spec();
            add(
                "",
                format!("explain {}", spec.slug),
                spec.why.to_owned(),
                Nav::Explain(spec.slug.to_owned()),
            );
        }
        entries
    }

    fn rebuild(&mut self, session: &Session) {
        let every = Self::every(session);
        self.shown = if self.typed.is_empty() {
            let mut rows = Vec::new();
            for group in GROUPS {
                rows.push(Row::Heading(group));
                rows.extend(
                    every
                        .iter()
                        .filter(|entry| entry.group == group)
                        .map(|entry| {
                            Row::Entry(Entry {
                                label: entry.label.clone(),
                                about: entry.about.clone(),
                                nav: entry.nav.clone(),
                                group: entry.group,
                            })
                        }),
                );
            }
            rows
        } else {
            let typed = self.typed.to_lowercase();
            let mut scored: Vec<(usize, Entry)> = every
                .into_iter()
                .filter_map(|entry| score(&entry.label, &typed).map(|rank| (rank, entry)))
                .collect();
            // Rank first, then alphabetically, so the list does not reshuffle among
            // equals as you type and the row under the cursor stays the one you were
            // aiming at.
            scored
                .sort_by(|left, right| left.0.cmp(&right.0).then(left.1.label.cmp(&right.1.label)));
            scored
                .into_iter()
                .map(|(_, entry)| Row::Entry(entry))
                .collect()
        };
        self.state.select(self.first_entry());
    }

    fn first_entry(&self) -> Option<usize> {
        self.shown
            .iter()
            .position(|row| matches!(row, Row::Entry(_)))
    }

    /// A heading is a label rather than a place to be, so a step passes over one.
    fn step(&mut self, delta: isize) {
        if self.shown.is_empty() {
            return;
        }
        let last = self.shown.len() as isize - 1;
        let mut at = (self.state.selected().unwrap_or(0) as isize + delta).clamp(0, last);
        let toward = if delta < 0 { -1 } else { 1 };
        while matches!(self.shown.get(at as usize), Some(Row::Heading(_))) {
            let next = at + toward;
            if next < 0 || next > last {
                break;
            }
            at = next;
        }
        if matches!(self.shown.get(at as usize), Some(Row::Entry(_))) {
            self.state.select(Some(at as usize));
        }
    }

    fn chosen(&self) -> Option<Nav> {
        match self.shown.get(self.state.selected()?)? {
            Row::Entry(entry) => Some(entry.nav.clone()),
            Row::Heading(_) => None,
        }
    }

    /// `None` means the palette stays open. `Some(None)` means it closes with nothing
    /// chosen; `Some(Some(choice))` means it closes and that happens.
    pub fn handle(&mut self, key: KeyEvent, session: &Session) -> Option<Option<Nav>> {
        if let Some(asking) = &mut self.asking {
            return match key.code {
                KeyCode::Esc => {
                    self.asking = None;
                    None
                }
                KeyCode::Enter if asking.answer.trim().is_empty() => None,
                KeyCode::Enter => {
                    let answer = asking.answer.trim().to_owned();
                    self.asking = None;
                    Some(Some(Nav::Review(answer)))
                }
                KeyCode::Backspace => {
                    asking.answer.pop();
                    None
                }
                KeyCode::Char(character) => {
                    asking.answer.push(character);
                    None
                }
                _ => None,
            };
        }
        match key.code {
            KeyCode::Esc => Some(None),
            KeyCode::Enter if self.chosen() == Some(Nav::Ask) => {
                self.asking = Some(Asking {
                    what: "review against",
                    answer: DEFAULT_BASE.to_owned(),
                });
                None
            }
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

    pub fn click(&mut self, event: MouseEvent) -> Option<Option<Nav>> {
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
        if !matches!(self.shown.get(row), Some(Row::Entry(_))) {
            return None;
        }
        self.state.select(Some(row));
        if self.chosen() == Some(Nav::Ask) {
            self.asking = Some(Asking {
                what: "review against",
                answer: DEFAULT_BASE.to_owned(),
            });
            return None;
        }
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

        let (lead, typed) = match &self.asking {
            Some(asking) => (format!("{} ", asking.what), asking.answer.clone()),
            None => ("\u{203a} ".to_owned(), self.typed.clone()),
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(lead, Style::new().fg(palette.color(Ink::Accent))),
                Span::styled(typed, Style::new().fg(palette.color(Ink::Text))),
                Span::styled("\u{2588}", Style::new().fg(palette.color(Ink::Accent))),
            ])),
            prompt,
        );

        if self.asking.is_some() {
            frame.render_widget(
                Paragraph::new(Line::styled(
                    "  enter to run, esc to go back",
                    palette.dim(),
                )),
                list,
            );
            return;
        }

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
            .map(|row| {
                let entry = match row {
                    Row::Heading(title) => {
                        return ListItem::new(Line::from(Span::styled(
                            (*title).to_owned(),
                            Style::new()
                                .fg(palette.color(Ink::Muted))
                                .add_modifier(Modifier::BOLD),
                        )));
                    }
                    Row::Entry(entry) => entry,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<26}", trim(&entry.label, 26)),
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

fn trim(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "\u{2026}"
}

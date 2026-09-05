use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::brand::Ink;
use crate::tui::app::{Nav, Screen, Session, Tab, global, is_press, row_at};

const DEFAULT_BASE: &str = "origin/HEAD";

/// Two words and a number each. The tab bar already reaches every view, so the menu
/// only carries what starts something, and the digits match the way the tabs work.
const CHOICES: [(&str, &str); 5] = [
    ("Scan", "this project, every file"),
    ("Review", "only what changed against a revision"),
    ("Rules", "the catalogue and what each one measures"),
    ("Config", "the settings this run will use"),
    ("Quit", ""),
];

#[derive(Default)]
pub struct Menu {
    cursor: usize,
    at: Rect,
    /// Non-empty while the base revision is being typed, which is the one menu entry
    /// that needs an answer before it can run.
    base: Option<String>,
}

impl Menu {
    fn choose(&mut self) -> Nav {
        match self.cursor {
            0 => Nav::Scan,
            1 => {
                self.base = Some(DEFAULT_BASE.to_owned());
                Nav::Redraw
            }
            2 => Nav::To(Tab::Rules),
            3 => Nav::To(Tab::Config),
            _ => Nav::Quit,
        }
    }
}

impl Screen for Menu {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        let [list, prompt] = Layout::vertical([
            Constraint::Length(CHOICES.len() as u16 + 1),
            Constraint::Min(0),
        ])
        .areas(area);
        self.at = list;

        let lines: Vec<Line<'static>> = CHOICES
            .iter()
            .enumerate()
            .map(|(index, (label, detail))| {
                let selected = index == self.cursor;
                let mark = if selected { "\u{25b8}" } else { " " };
                Line::from(vec![
                    Span::styled(
                        format!("  {mark} {} ", index + 1),
                        Style::new().fg(palette.color(Ink::Accent)),
                    ),
                    Span::styled(
                        format!("{label:<8}"),
                        if selected {
                            Style::new()
                                .fg(palette.color(Ink::Accent))
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::new().fg(palette.color(Ink::Text))
                        },
                    ),
                    Span::styled(*detail, palette.dim()),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), list);

        let mut trailer = Vec::new();
        if let Some(base) = &self.base {
            trailer.push(Line::from(vec![
                Span::styled("  review against ", palette.dim()),
                Span::styled(
                    format!("{base}\u{2588}"),
                    Style::new().fg(palette.color(Ink::Accent)),
                ),
            ]));
            trailer.push(Line::styled("  enter to run, esc to cancel", palette.dim()));
        } else if let Some(failure) = &session.failure {
            trailer.push(Line::styled(
                format!("  {failure}"),
                Style::new().fg(palette.color(Ink::Danger)),
            ));
        }
        frame.render_widget(Paragraph::new(trailer), prompt);
    }

    fn click(&mut self, event: MouseEvent, _session: &Session) -> Nav {
        // Nothing to scroll and nothing to type into: five entries, and a click on one
        // is choosing it.
        if !is_press(event) || self.base.is_some() {
            return Nav::Stay;
        }
        let Some(row) = row_at(self.at, 0, 0, event) else {
            return Nav::Stay;
        };
        if row >= CHOICES.len() {
            return Nav::Stay;
        }
        self.cursor = row;
        self.choose()
    }

    fn handle(&mut self, key: KeyEvent, _session: &Session) -> Nav {
        if let Some(base) = &mut self.base {
            return match key.code {
                KeyCode::Char(letter) => {
                    base.push(letter);
                    Nav::Redraw
                }
                KeyCode::Backspace => {
                    base.pop();
                    Nav::Redraw
                }
                KeyCode::Enter if base.trim().is_empty() => Nav::Stay,
                KeyCode::Enter => {
                    let base = self.base.take().unwrap_or_default();
                    Nav::Review(base.trim().to_owned())
                }
                KeyCode::Esc => {
                    self.base = None;
                    Nav::Redraw
                }
                _ => Nav::Stay,
            };
        }

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = (self.cursor + 1).min(CHOICES.len() - 1);
                Nav::Redraw
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
                Nav::Redraw
            }
            KeyCode::Char(digit @ '1'..='5') => {
                self.cursor = digit as usize - '1' as usize;
                self.choose()
            }
            KeyCode::Enter => self.choose(),
            // `esc` at the root has nowhere to go back to, so it leaves.
            _ => global(key).map_or(
                Nav::Stay,
                |nav| {
                    if nav == Nav::Back { Nav::Quit } else { nav }
                },
            ),
        }
    }

    fn capturing(&self) -> bool {
        self.base.is_some()
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("1-5", "choose"),
            ("↑↓", "move"),
            ("enter", "run"),
            ("?", "keys"),
            ("q", "quit"),
        ]
    }
}

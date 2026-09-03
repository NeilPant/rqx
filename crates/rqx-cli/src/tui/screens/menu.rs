use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::brand::Ink;
use crate::tui::app::{Nav, Screen, Session, Tab, global};

const CHOICES: [(&str, Nav); 6] = [
    ("Scan this project", Nav::Scan),
    ("Review changes against the base revision", Nav::Review),
    ("Rule catalogue", Nav::To(Tab::Rules)),
    ("Catalogue coverage", Nav::To(Tab::Coverage)),
    ("Effective configuration", Nav::To(Tab::Config)),
    ("Quit", Nav::Quit),
];

/// Opens on the scan, so the command this replaced is still one keystroke away.
#[derive(Default)]
pub struct Menu {
    cursor: usize,
}

impl Screen for Menu {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        let lines: Vec<Line<'static>> = CHOICES
            .iter()
            .enumerate()
            .map(|(index, (label, _))| {
                let selected = index == self.cursor;
                Line::from(vec![
                    Span::styled(
                        if selected { "  \u{25b8} " } else { "    " },
                        Style::new().fg(palette.color(Ink::Accent)),
                    ),
                    Span::styled(
                        *label,
                        if selected {
                            Style::new()
                                .fg(palette.color(Ink::Accent))
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::new().fg(palette.color(Ink::Text))
                        },
                    ),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn handle(&mut self, key: KeyEvent, _session: &Session) -> Nav {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = (self.cursor + 1).min(CHOICES.len() - 1);
                Nav::Redraw
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
                Nav::Redraw
            }
            KeyCode::Enter => CHOICES[self.cursor].1.clone(),
            // `esc` at the root has nowhere to go back to, so it leaves.
            _ => global(key).map_or(Nav::Stay, |nav| {
                if nav == Nav::Back { Nav::Quit } else { nav }
            }),
        }
    }

    fn animating(&self, session: &Session) -> bool {
        !session.motion.reduced()
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[("↑↓", "move"), ("enter", "choose"), ("?", "keys"), ("q", "quit")]
    }
}

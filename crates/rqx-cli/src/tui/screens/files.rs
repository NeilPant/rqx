use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState,
};

use rqx_core::Cleanliness;

use crate::brand::Ink;
use crate::tui::app::{Nav, Screen, Session, global};

/// One row per file, ordered worst-first, because the question this screen answers is
/// which file to open.
#[derive(Default)]
pub struct Files {
    rows: Vec<(String, Cleanliness, u32, u32, u32)>,
    state: TableState,
    ready: bool,
    by_name: bool,
}

const fn grade_ink(grade: Cleanliness) -> Ink {
    match grade {
        Cleanliness::Pristine | Cleanliness::Clean => Ink::Good,
        Cleanliness::Fair => Ink::Accent,
        Cleanliness::Poor => Ink::Warn,
        Cleanliness::Critical => Ink::Danger,
    }
}

impl Files {
    fn build(&mut self, session: &Session) {
        let Some(outcome) = &session.outcome else { return };
        self.rows = outcome
            .analyzed
            .iter()
            .map(|file| {
                let lines = outcome.lines_by_file.get(file.as_str()).copied().unwrap_or(0);
                let found = outcome.findings_by_file.get(file.as_str()).copied().unwrap_or(0);
                let impact = outcome.impact.get(file);
                (
                    file.clone(),
                    Cleanliness::of(outcome.counts.file_score(file, lines), found),
                    found,
                    lines,
                    impact.importers,
                )
            })
            .collect();
        if self.by_name {
            self.rows.sort_by(|left, right| left.0.cmp(&right.0));
        } else {
            self.rows.sort_by(|left, right| {
                left.1.cmp(&right.1).then(right.2.cmp(&left.2)).then(left.0.cmp(&right.0))
            });
        }
        self.state.select((!self.rows.is_empty()).then_some(0));
        self.ready = true;
    }
}

impl Screen for Files {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        if !self.ready {
            self.build(session);
        }
        if self.rows.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::styled("  nothing scanned yet", palette.dim())),
                area,
            );
            return;
        }

        let rows: Vec<Row<'static>> = self
            .rows
            .iter()
            .map(|(file, grade, found, lines, importers)| {
                Row::new(vec![
                    Span::styled(
                        format!(" {:9}", grade.label()),
                        Style::new().fg(palette.color(grade_ink(*grade))),
                    ),
                    Span::styled(file.clone(), Style::new().fg(palette.color(Ink::Text))),
                    Span::styled(found.to_string(), palette.dim()),
                    Span::styled(lines.to_string(), palette.dim()),
                    Span::styled(importers.to_string(), palette.dim()),
                ])
            })
            .collect();

        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(10),
                    Constraint::Min(20),
                    Constraint::Length(9),
                    Constraint::Length(7),
                    Constraint::Length(10),
                ],
            )
            .header(
                Row::new(vec!["  grade", "file", "findings", "lines", "importers"])
                    .style(palette.dim()),
            )
            .row_highlight_style(
                Style::new()
                    .fg(palette.color(Ink::Text))
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            )
            .block(Block::default().borders(Borders::TOP).border_style(palette.dim())),
            area,
            &mut self.state,
        );

        let mut scrollbar =
            ScrollbarState::new(self.rows.len()).position(self.state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(palette.dim()),
            area,
            &mut scrollbar,
        );
    }

    fn handle(&mut self, key: KeyEvent, session: &Session) -> Nav {
        let step = |state: &mut TableState, delta: isize, len: usize| {
            if len == 0 {
                return;
            }
            let current = state.selected().unwrap_or(0) as isize;
            state.select(Some((current + delta).clamp(0, len as isize - 1) as usize));
        };
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => step(&mut self.state, 1, self.rows.len()),
            KeyCode::Up | KeyCode::Char('k') => step(&mut self.state, -1, self.rows.len()),
            KeyCode::PageDown => step(&mut self.state, 12, self.rows.len()),
            KeyCode::PageUp => step(&mut self.state, -12, self.rows.len()),
            KeyCode::Char('n') => {
                self.by_name = !self.by_name;
                self.build(session);
            }
            _ => return global(key).unwrap_or(Nav::Stay),
        }
        Nav::Redraw
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[("↑↓", "move"), ("n", "sort by name"), ("tab", "next view"), ("esc", "back")]
    }
}

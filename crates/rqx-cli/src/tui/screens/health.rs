use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{
    Block, Borders, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState,
};

use crate::brand::{Ink, score_ink};
use crate::health::{self, By};
use crate::slop;
use crate::tui::app::{Nav, Screen, Session, global, is_press, row_at, wheel};

/// The block's top border, then the header.
const ABOVE: u16 = 2;

/// Two halves of the same question — what the score is made of, and which files read
/// as written rather than read — so one screen with a switch rather than two tabs.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Ledger {
    Debt,
    Slop,
}

pub struct Health {
    ledger: Ledger,
    by: By,
    rows: Vec<Line_>,
    state: TableState,
    ready: bool,
    at: Rect,
}

struct Line_ {
    name: String,
    cells: [String; 3],
    detail: String,
    ink: Ink,
    /// The file this row is about, when it is about one. A family is not a file.
    file: Option<String>,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            ledger: Ledger::Debt,
            by: By::Family,
            rows: Vec::new(),
            state: TableState::default(),
            ready: false,
            at: Rect::default(),
        }
    }
}

impl Health {
    const fn headings(&self) -> [&'static str; 4] {
        match self.ledger {
            Ledger::Debt => ["bucket", "found", "debt", "without"],
            Ledger::Slop => ["file", "score", "stmts", "signals"],
        }
    }

    fn build(&mut self, session: &Session) {
        let Some(outcome) = &session.outcome else {
            return;
        };
        let now = crate::report::Report::everything(outcome).health();
        self.rows = match self.ledger {
            Ledger::Debt => health::buckets(outcome, self.by, now)
                .into_iter()
                .map(|bucket| {
                    let gain = bucket.without.saturating_sub(now);
                    Line_ {
                        cells: [
                            bucket.findings.to_string(),
                            format!("{}%", bucket.share),
                            format!("{} (+{gain})", bucket.without),
                        ],
                        detail: bucket.worst.join(", "),
                        ink: if gain > 0 { Ink::Good } else { Ink::Text },
                        file: (self.by == By::File).then(|| bucket.name.clone()),
                        name: bucket.name,
                    }
                })
                .collect(),
            Ledger::Slop => slop::suspects(outcome)
                .into_iter()
                .map(|suspect| Line_ {
                    cells: [
                        suspect.score.to_string(),
                        suspect.statements.to_string(),
                        suspect.signals.len().to_string(),
                    ],
                    detail: suspect
                        .signals
                        .iter()
                        .take(3)
                        .map(|(rule, _)| rule.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    ink: score_ink(100 - suspect.score.min(100)),
                    file: Some(suspect.file.clone()),
                    name: suspect.file,
                })
                .collect(),
        };
        self.state.select((!self.rows.is_empty()).then_some(0));
        self.ready = true;
    }

    fn step(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let current = self.state.selected().unwrap_or(0) as isize;
        self.state.select(Some(
            (current + delta).clamp(0, self.rows.len() as isize - 1) as usize,
        ));
    }

    fn selected_file(&self) -> Option<String> {
        self.rows.get(self.state.selected()?)?.file.clone()
    }
}

impl Screen for Health {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        self.at = area;
        if !self.ready {
            self.build(session);
        }
        if session.outcome.is_none() {
            super::nothing_yet(frame, area, session, crate::tui::app::Tab::Health);
            return;
        }
        if self.rows.is_empty() {
            super::nothing_found(
                frame,
                area,
                session,
                "findings, so the score has nothing to explain",
            );
            return;
        }

        let rows: Vec<Row<'static>> = self
            .rows
            .iter()
            .map(|row| {
                let mut cells = vec![Span::styled(
                    format!(" {}", row.name),
                    Style::new().fg(palette.color(Ink::Text)),
                )];
                cells.extend(row.cells.iter().enumerate().map(|(at, cell)| {
                    let ink = if at == 0 { row.ink } else { Ink::Muted };
                    Span::styled(cell.clone(), Style::new().fg(palette.color(ink)))
                }));
                cells.push(Span::styled(row.detail.clone(), palette.dim()));
                Row::new(cells)
            })
            .collect();

        let [first, second, third, fourth] = self.headings();
        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(24),
                    Constraint::Length(8),
                    Constraint::Length(8),
                    Constraint::Length(10),
                    Constraint::Min(20),
                ],
            )
            .header(
                Row::new(vec![
                    format!(" {first}"),
                    second.to_owned(),
                    third.to_owned(),
                    fourth.to_owned(),
                    "worst of it".to_owned(),
                ])
                .style(palette.dim()),
            )
            .row_highlight_style(
                Style::new()
                    .fg(palette.color(Ink::Text))
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            )
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(palette.rule()),
            ),
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
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::PageDown => self.step(12),
            KeyCode::PageUp => self.step(-12),
            KeyCode::Char('s') => {
                self.ledger = match self.ledger {
                    Ledger::Debt => Ledger::Slop,
                    Ledger::Slop => Ledger::Debt,
                };
                self.build(session);
            }
            KeyCode::Char('b') => {
                self.by = match self.by {
                    By::Family => By::Rule,
                    By::Rule => By::Dir,
                    By::Dir => By::File,
                    By::File => By::Family,
                };
                self.ledger = Ledger::Debt;
                self.build(session);
            }
            KeyCode::Enter => {
                let Some(file) = self.selected_file() else {
                    return Nav::Stay;
                };
                return Nav::Inspect(file);
            }
            _ => return global(key).unwrap_or(Nav::Stay),
        }
        Nav::Redraw
    }

    fn click(&mut self, event: MouseEvent, _session: &Session) -> Nav {
        if let Some(rows) = wheel(event) {
            self.step(rows);
            return Nav::Redraw;
        }
        if !is_press(event) {
            return Nav::Stay;
        }
        let Some(row) = row_at(self.at, ABOVE, self.state.offset(), event) else {
            return Nav::Stay;
        };
        if row >= self.rows.len() {
            return Nav::Stay;
        }
        if self.state.selected() == Some(row) {
            return self.selected_file().map_or(Nav::Stay, Nav::Inspect);
        }
        self.state.select(Some(row));
        Nav::Redraw
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑↓", "move"),
            ("s", "slop"),
            ("b", "by rule, dir, file"),
            ("enter", "its findings"),
        ]
    }
}

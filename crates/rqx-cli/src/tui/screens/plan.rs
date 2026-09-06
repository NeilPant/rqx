use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState,
};

use crate::brand::{Ink, severity_ink};
use crate::tui::app::{Nav, Screen, Session, global, is_press, row_at, wheel};

/// The list's own top border is the only line above the first row.
const ABOVE: u16 = 1;

/// One ranked item, flattened out of the plan so the screen holds no borrow of the
/// outcome between frames.
struct Item {
    rule: String,
    file: String,
    line: u32,
    effort: u8,
    health_after: u32,
    technique: String,
    severity: rqx_core::finding::Severity,
}

/// Effort points to spend. The same range `rqx plan --budget` takes, because the
/// slider and the flag are the same question asked in two places.
const MIN_BUDGET: u32 = 2;
const MAX_BUDGET: u32 = 40;
const STEP: u32 = 2;

pub struct Plan {
    items: Vec<Item>,
    state: TableState,
    budget: u32,
    before: u32,
    after: u32,
    ready: bool,
    at: Rect,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            state: TableState::default(),
            budget: 12,
            before: 0,
            after: 0,
            ready: false,
            at: Rect::default(),
        }
    }
}

impl Plan {
    fn rank(&mut self, session: &Session) {
        let Some(outcome) = &session.outcome else {
            return;
        };
        let ranked = crate::plan::rank(outcome, Some(self.budget), 20);
        let (before, after) = crate::plan::simulated(outcome, &ranked);
        self.before = before;
        self.after = after;
        self.items = ranked
            .iter()
            .map(|item| {
                let spec = item.finding.rule.spec();
                Item {
                    rule: item.finding.slug().to_owned(),
                    file: item.finding.location.file.to_string(),
                    line: item.finding.location.line,
                    effort: item.effort,
                    health_after: item.health_after,
                    technique: spec
                        .fixes
                        .iter()
                        .map(|fix| fix.name())
                        .collect::<Vec<_>>()
                        .join(" / "),
                    severity: item.finding.severity,
                }
            })
            .collect();
        let selected = self
            .state
            .selected()
            .unwrap_or(0)
            .min(self.items.len().saturating_sub(1));
        self.state
            .select((!self.items.is_empty()).then_some(selected));
        self.ready = true;
    }

    fn step(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let current = self.state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, self.items.len() as isize - 1);
        self.state.select(Some(next as usize));
    }

    fn spend(&mut self, delta: i32, session: &Session) {
        let next = (self.budget as i32 + delta).clamp(MIN_BUDGET as i32, MAX_BUDGET as i32) as u32;
        if next != self.budget {
            self.budget = next;
            self.rank(session);
        }
    }
}

impl Screen for Plan {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        if !self.ready {
            self.rank(session);
        }
        if session.outcome.is_none() {
            super::nothing_yet(frame, area, session, crate::tui::app::Tab::Plan);
            return;
        }

        let [bar, list] = Layout::vertical([Constraint::Length(2), Constraint::Min(4)]).areas(area);
        self.at = list;

        let spent: u32 = self.items.iter().map(|item| u32::from(item.effort)).sum();
        let header = vec![
            Line::from(vec![
                Span::styled(
                    format!("  {} points", self.budget),
                    Style::new().fg(palette.color(Ink::Accent)),
                ),
                Span::styled(
                    format!("  {spent} spent on {} items", self.items.len()),
                    palette.dim(),
                ),
            ]),
            Line::from(vec![
                Span::styled("  health ", palette.dim()),
                Span::styled(
                    format!("{}", self.before),
                    Style::new().fg(palette.color(Ink::Text)),
                ),
                Span::styled(" \u{2192} ", palette.dim()),
                Span::styled(
                    format!("{}", self.after),
                    Style::new()
                        .fg(palette.color(crate::brand::score_ink(self.after)))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  once the plan is done", palette.dim()),
            ]),
        ];
        frame.render_widget(Paragraph::new(header), bar);

        let rows: Vec<Row<'static>> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                Row::new(vec![
                    Span::styled(format!(" {:>2} ", index + 1), palette.dim()),
                    Span::styled(
                        item.rule.clone(),
                        Style::new().fg(palette.color(severity_ink(item.severity))),
                    ),
                    Span::styled(format!("{}:{}", item.file, item.line), palette.dim()),
                    Span::styled(item.technique.clone(), palette.dim()),
                    Span::styled(
                        format!("{} pt", item.effort),
                        Style::new().fg(palette.color(Ink::Text)),
                    ),
                    Span::styled(
                        format!("{}", item.health_after),
                        Style::new().fg(palette.color(crate::brand::score_ink(item.health_after))),
                    ),
                ])
            })
            .collect();

        if rows.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::styled(
                    "  Nothing in this project can be ranked: every finding is unscored.",
                    palette.dim(),
                )),
                list,
            );
            return;
        }

        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(4),
                    Constraint::Length(24),
                    Constraint::Min(16),
                    Constraint::Length(22),
                    Constraint::Length(6),
                    Constraint::Length(4),
                ],
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
            list,
            &mut self.state,
        );

        let mut scrollbar =
            ScrollbarState::new(self.items.len()).position(self.state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(palette.dim()),
            list,
            &mut scrollbar,
        );
    }

    fn handle(&mut self, key: KeyEvent, session: &Session) -> Nav {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::Char('+') | KeyCode::Char('=') => self.spend(STEP as i32, session),
            KeyCode::Char('-') | KeyCode::Char('_') => self.spend(-(STEP as i32), session),
            KeyCode::Enter => {
                let Some(item) = self.items.get(self.state.selected().unwrap_or(0)) else {
                    return Nav::Stay;
                };
                return Nav::Inspect(item.file.clone());
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
        let Some(item) = self.items.get(row) else {
            return Nav::Stay;
        };
        if self.state.selected() == Some(row) {
            return Nav::Inspect(item.file.clone());
        }
        self.state.select(Some(row));
        Nav::Redraw
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("\u{2191}\u{2193}", "move"),
            ("+ -", "budget"),
            ("enter", "open \u{b7} its findings"),
            ("esc", "back"),
        ]
    }
}

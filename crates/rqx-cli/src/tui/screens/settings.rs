use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState,
};

use crate::brand::Ink;
use crate::tui::app::{Nav, Screen, Session, global, wheel};

/// What this run will actually do, and where each answer came from. The rows are the
/// ones `rqx config` prints, so the two surfaces cannot disagree about the settings.
#[derive(Default)]
pub struct Settings {
    state: TableState,
}

impl Screen for Settings {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        let settings = &session.config;
        let rows = crate::report::setting_rows(settings);

        let [head, body] =
            ratatui::layout::Layout::vertical([Constraint::Length(2), Constraint::Min(0)])
                .areas(area);
        let read = if settings.sources.is_empty() {
            "  no rqx.yaml found; `rqx init` writes one".to_owned()
        } else {
            format!(
                "  read {}",
                settings
                    .sources
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        frame.render_widget(
            Paragraph::new(vec![Line::styled(read, palette.dim()), Line::default()]),
            head,
        );

        let table: Vec<Row<'static>> = rows
            .iter()
            .map(|entry| {
                Row::new(vec![
                    Span::styled(
                        format!("  {}", entry.name),
                        Style::new().fg(palette.color(Ink::Text)),
                    ),
                    Span::styled(
                        entry.value.clone(),
                        Style::new().fg(palette.color(Ink::Accent)),
                    ),
                    Span::styled(entry.source(settings), palette.dim()),
                ])
            })
            .collect();
        let count = table.len();

        frame.render_stateful_widget(
            Table::new(
                table,
                [
                    Constraint::Length(26),
                    Constraint::Length(14),
                    Constraint::Min(8),
                ],
            )
            .header(Row::new(vec!["  setting", "value", "from"]).style(palette.dim()))
            .row_highlight_style(Style::new().add_modifier(ratatui::style::Modifier::REVERSED))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(palette.rule()),
            ),
            body,
            &mut self.state,
        );

        let mut scrollbar = ScrollbarState::new(count).position(self.state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(palette.dim()),
            body,
            &mut scrollbar,
        );
    }

    fn handle(&mut self, key: KeyEvent, session: &Session) -> Nav {
        let len = crate::report::setting_rows(&session.config).len();
        let mut step = |delta: isize| {
            if len == 0 {
                return;
            }
            let current = self.state.selected().unwrap_or(0) as isize;
            self.state
                .select(Some((current + delta).clamp(0, len as isize - 1) as usize));
        };
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => step(1),
            KeyCode::Up | KeyCode::Char('k') => step(-1),
            KeyCode::PageDown => step(12),
            KeyCode::PageUp => step(-12),
            _ => return global(key).unwrap_or(Nav::Stay),
        }
        Nav::Redraw
    }

    fn click(&mut self, event: MouseEvent, _session: &Session) -> Nav {
        let Some(rows) = wheel(event) else {
            return Nav::Stay;
        };
        let current = self.state.selected().unwrap_or(0) as isize;
        self.state
            .select(Some(current.saturating_add(rows).max(0) as usize));
        Nav::Redraw
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[("↑↓", "move"), ("esc", "back")]
    }
}

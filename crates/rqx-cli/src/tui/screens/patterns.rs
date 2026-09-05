use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use rqx_core::{Pattern, PatternFamily, Stance};

use crate::brand::Ink;
use crate::tui::app::{Nav, Screen, Session, Tab, global, wheel};

/// Recognised patterns collapse to a count because an inventory is a number; the two
/// stances that ask something of the reader get a line each.
#[derive(Default)]
pub struct Patterns {
    offset: usize,
}

impl Screen for Patterns {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let Some(outcome) = session.outcome.as_ref() else {
            super::nothing_yet(frame, area, session, Tab::Patterns);
            return;
        };
        let palette = session.palette;
        let mut lines: Vec<Line<'static>> = Vec::new();

        for family in PatternFamily::ALL {
            let mut rows: Vec<Line<'static>> = Vec::new();
            for pattern in family.patterns() {
                let mut seen: Vec<&rqx_core::Advice> = outcome
                    .advice
                    .iter()
                    .filter(|advice| {
                        advice.advisory.pattern() == pattern
                            && session.in_scope(&advice.location.file)
                    })
                    .collect();
                if seen.is_empty() {
                    continue;
                }
                seen.sort_by_key(|advice| advice.advisory.stance());
                rows.push(Line::from(vec![
                    Span::styled(
                        format!("  {:26}", pattern.name()),
                        Style::new().fg(palette.color(Ink::Text)),
                    ),
                    Span::styled(pattern.intent().to_owned(), palette.dim()),
                ]));
                let in_use = seen
                    .iter()
                    .filter(|advice| advice.advisory.stance() == Stance::Recognized)
                    .count();
                if in_use > 0 {
                    rows.push(Line::from(vec![
                        Span::styled(
                            "    \u{2713} ".to_owned(),
                            Style::new().fg(palette.color(Ink::Good)),
                        ),
                        Span::styled(
                            crate::report::plural(in_use as u32, "place", "places"),
                            palette.dim(),
                        ),
                    ]));
                }
                for advice in seen {
                    let stance = advice.advisory.stance();
                    if stance == Stance::Recognized {
                        continue;
                    }
                    let ink = if stance == Stance::Incomplete {
                        Ink::Warn
                    } else {
                        Ink::Accent
                    };
                    rows.push(Line::from(vec![
                        Span::styled(
                            "    \u{25cf} ".to_owned(),
                            Style::new().fg(palette.color(ink)),
                        ),
                        Span::styled(
                            advice.location.to_string(),
                            Style::new().fg(palette.color(Ink::Text)),
                        ),
                        Span::styled(format!("  {}", stance.label()), palette.dim()),
                    ]));
                    rows.push(Line::styled(
                        format!("        {}", advice.note),
                        palette.dim(),
                    ));
                }
                rows.push(Line::default());
            }
            if rows.is_empty() {
                continue;
            }
            lines.push(Line::styled(
                format!("  {}", family.title()),
                Style::new().fg(palette.color(Ink::Accent)).bold(),
            ));
            lines.push(Line::default());
            lines.extend(rows);
        }

        if lines.is_empty() {
            let known = Pattern::ALL.len();
            frame.render_widget(
                Paragraph::new(vec![
                    Line::default(),
                    Line::styled(
                        format!("  None of the {known} patterns showed up here."),
                        palette.dim(),
                    ),
                    Line::styled(
                        "  `rqx patterns --coverage` says what rqx looks for.".to_owned(),
                        palette.dim(),
                    ),
                ]),
                area,
            );
            return;
        }

        let height = area.height as usize;
        self.offset = self.offset.min(lines.len().saturating_sub(1));
        let shown: Vec<Line<'static>> = lines
            .iter()
            .skip(self.offset)
            .take(height)
            .cloned()
            .collect();
        frame.render_widget(Paragraph::new(shown), area);

        let mut scrollbar = ScrollbarState::new(lines.len()).position(self.offset);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(palette.dim()),
            area,
            &mut scrollbar,
        );
    }

    fn handle(&mut self, key: KeyEvent, _session: &Session) -> Nav {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.offset += 1,
            KeyCode::Up | KeyCode::Char('k') => self.offset = self.offset.saturating_sub(1),
            KeyCode::PageDown => self.offset += 12,
            KeyCode::PageUp => self.offset = self.offset.saturating_sub(12),
            _ => return global(key).unwrap_or(Nav::Stay),
        }
        Nav::Redraw
    }

    fn click(&mut self, event: MouseEvent, _session: &Session) -> Nav {
        let Some(rows) = wheel(event) else {
            return Nav::Stay;
        };
        self.offset = self.offset.saturating_add_signed(rows);
        Nav::Redraw
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑↓", "scroll"),
            ("tab", "next view"),
            ("esc", "back"),
            ("q", "quit"),
        ]
    }
}

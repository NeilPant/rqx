use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};

use rqx_core::rule::RuleId;

use crate::brand::Ink;
use crate::tui::app::{Nav, Screen, Session, global, is_press, row_at, wheel};

/// The list's own top border is the only line above the first row.
const ABOVE: u16 = 1;

/// The rule catalogue, browsable rather than printed. Nothing is written to stdout
/// after the terminal is handed back, which is what used to leave the menu on screen
/// underneath the listing.
#[derive(Default)]
pub struct Rules {
    order: Vec<RuleId>,
    state: ListState,
    filter: String,
    typing: bool,
    ready: bool,
    at: Rect,
}

impl Rules {
    fn reorder(&mut self) {
        let needle = self.filter.to_lowercase();
        self.order = RuleId::ALL
            .iter()
            .copied()
            .filter(|rule| {
                let spec = rule.spec();
                needle.is_empty()
                    || spec.slug.contains(&needle)
                    || spec.category.slug().contains(&needle)
                    || spec.why.to_lowercase().contains(&needle)
            })
            .collect();
        let selected = self
            .state
            .selected()
            .unwrap_or(0)
            .min(self.order.len().saturating_sub(1));
        self.state
            .select((!self.order.is_empty()).then_some(selected));
        self.ready = true;
    }

    /// Narrow to one rule and select it, which is what the palette's `explain` does.
    pub fn only(&mut self, slug: &str) {
        self.filter = slug.to_owned();
        self.reorder();
        self.state.select((!self.order.is_empty()).then_some(0));
    }

    fn step(&mut self, delta: isize) {
        if self.order.is_empty() {
            return;
        }
        let current = self.state.selected().unwrap_or(0) as isize;
        self.state.select(Some(
            (current + delta).clamp(0, self.order.len() as isize - 1) as usize,
        ));
    }
}

impl Screen for Rules {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        if !self.ready {
            self.reorder();
        }
        let [bar, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(42), Constraint::Min(20)]).areas(body);
        self.at = list;

        let mut header = vec![Span::styled(
            format!("  {} of {} rules", self.order.len(), RuleId::ALL.len()),
            Style::new().fg(palette.color(Ink::Text)),
        )];
        if self.typing || !self.filter.is_empty() {
            header.push(Span::styled(
                format!(
                    "  /{}{}",
                    self.filter,
                    if self.typing { "\u{2588}" } else { "" }
                ),
                Style::new().fg(palette.color(Ink::Accent)),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(header)), bar);

        let items: Vec<ListItem<'static>> = self
            .order
            .iter()
            .map(|rule| {
                let spec = rule.spec();
                let enabled = session.config.enabled_anywhere(*rule);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {} ", if enabled { "\u{25cf}" } else { "\u{25cb}" }),
                        Style::new().fg(palette.color(if enabled {
                            Ink::Good
                        } else {
                            Ink::Muted
                        })),
                    ),
                    Span::styled(
                        format!("{:26}", spec.slug),
                        Style::new().fg(palette.color(Ink::Text)),
                    ),
                    Span::styled(spec.category.slug().to_owned(), palette.dim()),
                ]))
            })
            .collect();

        frame.render_stateful_widget(
            List::new(items)
                .highlight_style(
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
            ScrollbarState::new(self.order.len()).position(self.state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(palette.dim()),
            list,
            &mut scrollbar,
        );

        let Some(rule) = self
            .state
            .selected()
            .and_then(|index| self.order.get(index))
        else {
            frame.render_widget(
                Paragraph::new(Line::styled("  no rule matches", palette.dim())),
                detail,
            );
            return;
        };
        let spec = rule.spec();
        let field = |label: &'static str, value: String| {
            Line::from(vec![
                Span::styled(format!("  {label:<12}"), palette.dim()),
                Span::styled(value, Style::new().fg(palette.color(Ink::Text))),
            ])
        };
        let lines = vec![
            Line::default(),
            Line::from(vec![Span::styled(
                format!("  {}", spec.slug),
                Style::new()
                    .fg(palette.color(Ink::Accent))
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::default(),
            field("Family", spec.category.title().to_owned()),
            field("Metric", spec.metric.to_owned()),
            field(
                "Fix",
                spec.fixes
                    .iter()
                    .map(|fix| fix.name())
                    .collect::<Vec<_>>()
                    .join(" / "),
            ),
            field(
                "Reported",
                format!("{} / {}", spec.severity.label(), spec.confidence.label()),
            ),
            field("Needs", format!("{:?}", spec.analysis).to_lowercase()),
            Line::default(),
            Line::styled(
                format!("  {}", spec.why),
                Style::new().fg(palette.color(Ink::Text)),
            ),
        ];
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(palette.rule()),
            ),
            detail,
        );
    }

    fn handle(&mut self, key: KeyEvent, _session: &Session) -> Nav {
        if self.typing {
            match key.code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.typing = false;
                }
                KeyCode::Enter => self.typing = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(character) => self.filter.push(character),
                _ => return Nav::Stay,
            }
            self.reorder();
            return Nav::Redraw;
        }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::PageDown => self.step(12),
            KeyCode::PageUp => self.step(-12),
            KeyCode::Char('/') => self.typing = true,
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
        if row >= self.order.len() {
            return Nav::Stay;
        }
        self.state.select(Some(row));
        Nav::Redraw
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[("↑↓", "move"), ("/", "search"), ("esc", "back")]
    }
}

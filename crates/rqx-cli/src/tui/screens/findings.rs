use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState, Wrap,
};

use rqx_core::LineIndex;
use rqx_core::finding::{Finding, Severity};

use crate::brand::{Ink, severity_ink};
use crate::preview::{self, Role};
use crate::report::FindingFilter;
use crate::tui::app::{Nav, Screen, Session, global};

#[derive(Copy, Clone, PartialEq, Eq)]
enum Group {
    Rank,
    File,
    Rule,
}

impl Group {
    const fn label(self) -> &'static str {
        match self {
            Self::Rank => "rank",
            Self::File => "file",
            Self::Rule => "rule",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Rank => Self::File,
            Self::File => Self::Rule,
            Self::Rule => Self::Rank,
        }
    }
}

#[derive(Default)]
pub struct Findings {
    order: Vec<usize>,
    state: TableState,
    filter: String,
    typing: bool,
    group: Option<Group>,
    floor: Option<Severity>,
    source: Option<(String, String)>,
    ready: bool,
}

impl Findings {
    fn reorder(&mut self, session: &Session) {
        let Some(outcome) = &session.outcome else {
            return;
        };
        let filter = FindingFilter::default().with_text(&self.filter);
        let mut order: Vec<usize> = (0..outcome.findings.len())
            .filter(|index| {
                let finding = &outcome.findings[*index];
                session.in_scope(&finding.location.file)
                    && filter.matches(finding)
                    && self.floor.is_none_or(|floor| finding.severity >= floor)
            })
            .collect();
        match self.group.unwrap_or(Group::Rank) {
            Group::Rank => {}
            Group::File => order.sort_by(|a, b| {
                let (left, right) = (&outcome.findings[*a], &outcome.findings[*b]);
                left.location
                    .file
                    .cmp(&right.location.file)
                    .then(left.location.line.cmp(&right.location.line))
            }),
            Group::Rule => order.sort_by(|a, b| {
                let (left, right) = (&outcome.findings[*a], &outcome.findings[*b]);
                left.rule
                    .slug()
                    .cmp(right.rule.slug())
                    .then(left.location.line.cmp(&right.location.line))
            }),
        }
        self.order = order;
        let selected = self
            .state
            .selected()
            .unwrap_or(0)
            .min(self.order.len().saturating_sub(1));
        self.state
            .select((!self.order.is_empty()).then_some(selected));
        self.source = None;
        self.ready = true;
    }

    fn selected<'a>(&self, session: &'a Session) -> Option<&'a Finding> {
        let outcome = session.outcome.as_ref()?;
        outcome
            .findings
            .get(*self.order.get(self.state.selected()?)?)
    }

    fn step(&mut self, delta: isize) {
        if self.order.is_empty() {
            return;
        }
        let current = self.state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, self.order.len() as isize - 1);
        self.state.select(Some(next as usize));
        self.source = None;
    }
}

impl Screen for Findings {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        if !self.ready {
            self.reorder(session);
        }
        let Some(outcome) = &session.outcome else {
            super::nothing_yet(frame, area, session, crate::tui::app::Tab::Findings);
            return;
        };

        let [bar, list, detail] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Percentage(42),
            Constraint::Min(6),
        ])
        .areas(area);

        let total = outcome
            .findings
            .iter()
            .filter(|finding| session.in_scope(&finding.location.file))
            .count();
        let mut header = vec![Span::styled(
            format!("  {} of {total} findings", self.order.len()),
            Style::new().fg(palette.color(Ink::Text)),
        )];
        if let Some(floor) = self.floor {
            header.push(Span::styled(
                format!("  \u{2265} {}", floor.label()),
                palette.dim(),
            ));
        }
        if let Some(group) = self.group {
            header.push(Span::styled(
                format!("  by {}", group.label()),
                palette.dim(),
            ));
        }
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

        let rows: Vec<Row<'static>> = self
            .order
            .iter()
            .map(|index| {
                let finding = &outcome.findings[*index];
                Row::new(vec![
                    Span::styled(
                        format!(" {} ", severity_mark(finding.severity)),
                        Style::new().fg(palette.color(severity_ink(finding.severity))),
                    ),
                    Span::styled(
                        finding.rule.slug().to_owned(),
                        Style::new().fg(palette.color(Ink::Accent)),
                    ),
                    Span::styled(finding.location.to_string(), palette.dim()),
                ])
            })
            .collect();

        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(3),
                    Constraint::Length(26),
                    Constraint::Min(10),
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
            ScrollbarState::new(self.order.len()).position(self.state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(palette.dim()),
            list,
            &mut scrollbar,
        );

        self.render_detail(frame, detail, session);
    }

    fn handle(&mut self, key: KeyEvent, session: &Session) -> Nav {
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
            self.reorder(session);
            return Nav::Redraw;
        }

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::PageDown => self.step(10),
            KeyCode::PageUp => self.step(-10),
            KeyCode::Home => self.state.select(Some(0)),
            KeyCode::End => self.state.select(Some(self.order.len().saturating_sub(1))),
            KeyCode::Char('/') => self.typing = true,
            KeyCode::Char('g') => {
                self.group = Some(self.group.unwrap_or(Group::Rank).next());
                self.reorder(session);
            }
            KeyCode::Char('s') => {
                self.floor = match self.floor {
                    None => Some(Severity::Warning),
                    Some(Severity::Warning) => Some(Severity::Error),
                    Some(_) => None,
                };
                self.reorder(session);
            }
            _ => return global(key).unwrap_or(Nav::Stay),
        }
        Nav::Redraw
    }

    fn capturing(&self) -> bool {
        self.typing
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑↓", "move"),
            ("/", "filter"),
            ("g", "group"),
            ("s", "severity"),
            ("tab", "next view"),
            ("esc", "back"),
        ]
    }
}

const fn severity_mark(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "\u{2716}",
        Severity::Warning => "\u{26a0}",
        Severity::Info => "\u{2139}",
    }
}

impl Findings {
    fn render_detail(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        let Some(finding) = self.selected(session) else {
            frame.render_widget(
                Paragraph::new(Line::styled("  no findings match", palette.dim())).block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_style(palette.rule()),
                ),
                area,
            );
            return;
        };

        let file = finding.location.file.to_string();
        if self.source.as_ref().is_none_or(|(path, _)| *path != file) {
            let root = session.outcome.as_ref().map(|outcome| outcome.root.clone());
            let text = root
                .map(|root| root.join(&file))
                .and_then(|path| std::fs::read_to_string(path).ok());
            self.source = text.map(|text| (file.clone(), text));
        }

        let mut lines = Vec::new();
        if let Some((_, text)) = &self.source {
            let index = LineIndex::new(text);
            if let Some(view) = preview::of(&index, &finding.location, 2) {
                let ink = severity_ink(finding.severity);
                for line in &view.lines {
                    let body = match (&line.mark, line.role) {
                        (Some(mark), _) => vec![
                            Span::styled(line.text[..mark.start].to_owned(), palette.dim()),
                            Span::styled(
                                line.text[mark.clone()].to_owned(),
                                Style::new()
                                    .bg(palette.color(ink))
                                    .fg(palette.color(Ink::Ground)),
                            ),
                            Span::styled(line.text[mark.end..].to_owned(), palette.dim()),
                        ],
                        (None, Role::Offence) => vec![Span::styled(
                            line.text.clone(),
                            Style::new().fg(palette.color(Ink::Text)),
                        )],
                        (None, Role::Context) => {
                            vec![Span::styled(line.text.clone(), palette.dim())]
                        }
                    };
                    let mut spans = vec![Span::styled(
                        format!("{:>6} \u{2502} ", line.number),
                        palette.dim(),
                    )];
                    spans.extend(body);
                    lines.push(Line::from(spans));
                }
            }
        }

        let spec = finding.rule.spec();
        lines.push(Line::default());
        lines.push(Line::styled(
            finding.evidence.to_string(),
            Style::new().fg(palette.color(Ink::Text)),
        ));
        lines.push(Line::from(vec![
            Span::styled("Why  ", palette.dim()),
            Span::styled(spec.why, Style::new().fg(palette.color(Ink::Text))),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Fix  ", palette.dim()),
            Span::styled(
                spec.fixes
                    .iter()
                    .map(|fix| fix.name())
                    .collect::<Vec<_>>()
                    .join(" / "),
                Style::new().fg(palette.color(Ink::Good)),
            ),
            Span::styled(format!("  \u{2014} {}", finding.fix_hint), palette.dim()),
        ]));

        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(palette.rule()),
            ),
            area,
        );
    }
}

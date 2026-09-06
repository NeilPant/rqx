use std::collections::{BTreeMap, BTreeSet};

use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, ListState, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::brand::Ink;
use crate::tui::app::{Nav, Screen, Session, global, is_press, row_at, wheel};

/// The block's top border is the only line above the first row.
const ABOVE: u16 = 1;

/// What imports what, with every cycle first. The rings and the edges are both on the
/// outcome already — this screen is the one place that reads them for a person rather
/// than for a file.
#[derive(Default)]
pub struct Graph {
    rows: Vec<Entry>,
    state: ListState,
    ready: bool,
    cycles_only: bool,
    at: Rect,
}

enum Entry {
    Heading(String),
    /// A member of a ring, or the target of an import. The string is the file to open.
    Node {
        text: String,
        file: String,
        in_cycle: bool,
    },
}

impl Graph {
    fn build(&mut self, session: &Session) {
        let Some(outcome) = &session.outcome else {
            return;
        };
        let in_cycle: BTreeSet<(&str, &str)> = outcome
            .cycles
            .iter()
            .flat_map(|ring| {
                ring.windows(2)
                    .map(|edge| (edge[0].as_str(), edge[1].as_str()))
            })
            .collect();

        let mut rows = Vec::new();
        if !outcome.cycles.is_empty() {
            rows.push(Entry::Heading(format!(
                "{} \u{b7} every module in one cannot be read on its own",
                crate::report::plural(outcome.cycles.len() as u32, "cycle", "cycles"),
            )));
            for ring in &outcome.cycles {
                for (at, member) in ring.iter().enumerate() {
                    // The ring closes on its first member, and printing it twice would
                    // read as two files with the same name.
                    if at + 1 == ring.len() {
                        continue;
                    }
                    rows.push(Entry::Node {
                        text: format!("{}{member}", if at == 0 { "  " } else { "  \u{2192} " }),
                        file: member.clone(),
                        in_cycle: true,
                    });
                }
            }
        }

        if !self.cycles_only {
            let mut by_importer: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for (importer, imported) in &outcome.import_edges {
                by_importer
                    .entry(importer.as_str())
                    .or_default()
                    .extend(imported.iter().map(String::as_str));
            }
            if !by_importer.is_empty() {
                let edges: usize = by_importer.values().map(Vec::len).sum();
                rows.push(Entry::Heading(format!(
                    "{edges} imports from {} modules",
                    by_importer.len()
                )));
                for (importer, imported) in by_importer {
                    rows.push(Entry::Node {
                        text: format!("  {importer}"),
                        file: importer.to_owned(),
                        in_cycle: false,
                    });
                    for target in imported {
                        rows.push(Entry::Node {
                            text: format!("    \u{2192} {target}"),
                            file: target.to_owned(),
                            in_cycle: in_cycle.contains(&(importer, target)),
                        });
                    }
                }
            }
        }

        self.rows = rows;
        self.state.select(self.first_node());
        self.ready = true;
    }

    fn first_node(&self) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, Entry::Node { .. }))
    }

    /// A heading is a label rather than a place to be, so a step passes over one.
    fn step(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        let mut at = (self.state.selected().unwrap_or(0) as isize + delta).clamp(0, last);
        let toward = if delta < 0 { -1 } else { 1 };
        while matches!(self.rows.get(at as usize), Some(Entry::Heading(_))) {
            let next = at + toward;
            if next < 0 || next > last {
                break;
            }
            at = next;
        }
        if matches!(self.rows.get(at as usize), Some(Entry::Node { .. })) {
            self.state.select(Some(at as usize));
        }
    }

    fn selected_file(&self) -> Option<String> {
        match self.rows.get(self.state.selected()?)? {
            Entry::Node { file, .. } => Some(file.clone()),
            Entry::Heading(_) => None,
        }
    }
}

impl Screen for Graph {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        self.at = area;
        if !self.ready {
            self.build(session);
        }
        if session.outcome.is_none() {
            super::nothing_yet(frame, area, session, crate::tui::app::Tab::Graph);
            return;
        }
        if self.rows.is_empty() {
            super::nothing_found(
                frame,
                area,
                session,
                "module in this project imports another",
            );
            return;
        }

        let items: Vec<ListItem<'static>> = self
            .rows
            .iter()
            .map(|row| match row {
                Entry::Heading(text) => ListItem::new(Line::from(Span::styled(
                    format!("  {text}"),
                    Style::new()
                        .fg(palette.color(Ink::Muted))
                        .add_modifier(Modifier::BOLD),
                ))),
                Entry::Node {
                    text,
                    in_cycle: hot,
                    ..
                } => ListItem::new(Line::from(Span::styled(
                    text.clone(),
                    Style::new().fg(palette.color(if *hot { Ink::Danger } else { Ink::Text })),
                ))),
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
            KeyCode::Char('m') => {
                self.cycles_only = !self.cycles_only;
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
        if !matches!(self.rows.get(row), Some(Entry::Node { .. })) {
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
            ("m", "mode \u{b7} cycles only"),
            ("enter", "open \u{b7} its findings"),
            ("esc", "back"),
        ]
    }
}

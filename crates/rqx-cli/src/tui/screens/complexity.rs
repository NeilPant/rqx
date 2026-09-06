use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{
    Block, Borders, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState,
};

use crate::brand::Ink;
use crate::complexity::{self, By, Sort, View};
use crate::perf;
use crate::tui::app::{Nav, Screen, Session, global, is_press, row_at, wheel};

/// Complexity and bottlenecks are the same table over two measurements — one row per
/// function, sorted by a number — so they are one screen with a switch rather than
/// two screens that would have to be kept looking alike.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Measure {
    Complexity,
    Bottlenecks,
}

/// The lines above the first row: the block's top border, then the header.
const ABOVE: u16 = 2;

pub struct Complexity {
    measure: Measure,
    sort: Sort,
    by: By,
    rows: Vec<Line_>,
    footer: String,
    state: TableState,
    ready: bool,
    at: Rect,
}

/// A row as it is shown, so the two measurements can share one table.
struct Line_ {
    name: String,
    at: String,
    cells: [String; 4],
    hot: bool,
}

impl Default for Complexity {
    fn default() -> Self {
        Self {
            measure: Measure::Complexity,
            sort: Sort::Complexity,
            by: By::Function,
            rows: Vec::new(),
            footer: String::new(),
            state: TableState::default(),
            ready: false,
            at: Rect::default(),
        }
    }
}

impl Complexity {
    fn headings(&self) -> [&'static str; 4] {
        match self.measure {
            Measure::Complexity => ["cplx", "stmts", "nest", "args"],
            Measure::Bottlenecks => ["order", "loops", "io", "reach"],
        }
    }

    fn build(&mut self, session: &Session) {
        let Some(outcome) = &session.outcome else {
            return;
        };
        let limits = &session.config.config.thresholds;
        self.rows = match self.measure {
            Measure::Complexity => {
                let measured = complexity::functions(outcome);
                let view = View {
                    by: self.by,
                    sort: self.sort,
                    top: usize::MAX,
                    min: 0,
                    histogram: false,
                };
                let threshold = limits.cognitive_complexity;
                let over = measured
                    .iter()
                    .filter(|row| row.complexity >= threshold)
                    .count();
                self.footer = format!(
                    "p50 {} \u{b7} p90 {} \u{b7} p99 {} \u{b7} {over} of {} over {threshold}",
                    complexity::percentile(&measured, 50),
                    complexity::percentile(&measured, 90),
                    complexity::percentile(&measured, 99),
                    measured.len(),
                );
                complexity::group(&measured, &view)
                    .into_iter()
                    .map(|row| Line_ {
                        hot: row.complexity >= threshold,
                        cells: [
                            row.complexity.to_string(),
                            row.statements.to_string(),
                            row.nesting.to_string(),
                            row.parameters.to_string(),
                        ],
                        name: row.name,
                        at: row.at,
                    })
                    .collect()
            }
            Measure::Bottlenecks => {
                let view = perf::View {
                    by: perf::By::Function,
                    sort: perf::Sort::Weight,
                    top: usize::MAX,
                    min_depth: 0,
                };
                let threshold = limits.loop_nesting;
                let ranked = perf::ranked(outcome, &view);
                self.footer = format!(
                    "{} worth a look \u{b7} loops nest past {threshold}",
                    ranked.len()
                );
                ranked
                    .into_iter()
                    .map(|row| Line_ {
                        hot: row.loops > threshold || row.self_calls >= 2,
                        cells: [
                            perf::order(row.loops, row.self_calls).to_owned(),
                            row.loops.to_string(),
                            row.io.to_string(),
                            format!("\u{d7}{:.1}", row.reach),
                        ],
                        name: row.name,
                        at: row.at,
                    })
                    .collect()
            }
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

    /// The file the selected row points at, so the mouse and `enter` can both open it
    /// in Findings. The location reads `file:line`, and only the file is a file.
    fn selected_file(&self) -> Option<String> {
        let at = &self.rows.get(self.state.selected()?)?.at;
        at.rsplit_once(':')
            .map(|(file, _)| file.to_owned())
            .filter(|file| !file.is_empty())
    }
}

impl Screen for Complexity {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        self.at = area;
        if !self.ready {
            self.build(session);
        }
        if session.outcome.is_none() {
            super::nothing_yet(frame, area, session, crate::tui::app::Tab::Complexity);
            return;
        }
        if self.rows.is_empty() {
            super::nothing_found(
                frame,
                area,
                session,
                "function in this project has a body to measure",
            );
            return;
        }

        let rows: Vec<Row<'static>> = self
            .rows
            .iter()
            .map(|row| {
                let ink = if row.hot { Ink::Warn } else { Ink::Text };
                let mut cells = vec![Span::styled(
                    format!(" {}", row.name),
                    Style::new().fg(palette.color(ink)),
                )];
                cells.extend(
                    row.cells
                        .iter()
                        .map(|cell| Span::styled(cell.clone(), palette.dim())),
                );
                cells.push(Span::styled(row.at.clone(), palette.dim()));
                Row::new(cells)
            })
            .collect();

        let [head_a, head_b, head_c, head_d] = self.headings();
        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Min(20),
                    Constraint::Length(7),
                    Constraint::Length(7),
                    Constraint::Length(6),
                    Constraint::Length(7),
                    Constraint::Length(30),
                ],
            )
            .header(
                Row::new(vec![
                    " function".to_owned(),
                    head_a.to_owned(),
                    head_b.to_owned(),
                    head_c.to_owned(),
                    head_d.to_owned(),
                    self.footer.clone(),
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
            KeyCode::Home => self.step(isize::MIN / 2),
            KeyCode::End => self.step(isize::MAX / 2),
            KeyCode::Char('m') => {
                self.measure = match self.measure {
                    Measure::Complexity => Measure::Bottlenecks,
                    Measure::Bottlenecks => Measure::Complexity,
                };
                self.build(session);
            }
            KeyCode::Char('g') => {
                self.by = match self.by {
                    By::Function => By::File,
                    By::File => By::Dir,
                    By::Dir => By::Function,
                };
                self.build(session);
            }
            KeyCode::Char('s') => {
                self.sort = match self.sort {
                    Sort::Complexity => Sort::Statements,
                    Sort::Statements => Sort::Nesting,
                    Sort::Nesting => Sort::Params,
                    Sort::Params => Sort::Complexity,
                };
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
        // A click on the row already under the cursor opens it, which is the second
        // click of a double-click without having to time one.
        if self.state.selected() == Some(row) {
            return self.selected_file().map_or(Nav::Stay, Nav::Inspect);
        }
        self.state.select(Some(row));
        Nav::Redraw
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑↓", "move"),
            ("m", "mode \u{b7} bottlenecks"),
            ("g", "group \u{b7} by file"),
            ("s", "sort \u{b7} by measure"),
            ("enter", "open \u{b7} its findings"),
        ]
    }
}

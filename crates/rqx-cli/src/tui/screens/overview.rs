use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, BarGroup, LineGauge, Paragraph};

use rqx_core::Cleanliness;
use rqx_core::finding::Severity;

use crate::brand::{Ink, Palette, grade_ink, score_ink};
use crate::report::plural;
use crate::tui::app::{Nav, Screen, Session, global, is_press, row_at};

const REVEAL: Duration = Duration::from_millis(600);

pub struct Overview {
    since: Instant,
    /// Where the last frame put "Start here" and what it listed, so clicking a name
    /// opens it. The rest of the screen is numbers, which nothing can be done to.
    start: Rect,
    listed: Vec<String>,
}

impl Default for Overview {
    fn default() -> Self {
        Self {
            since: Instant::now(),
            start: Rect::default(),
            listed: Vec::new(),
        }
    }
}

fn bars(palette: Palette, rows: &[(&'static str, u64, Ink)], grown: f64) -> Vec<Bar<'static>> {
    rows.iter()
        .map(|(label, value, ink)| {
            Bar::default()
                .value((*value as f64 * grown).round() as u64)
                .text_value(value.to_string())
                .label(Line::from(*label))
                .style(Style::new().fg(palette.color(*ink)))
        })
        .collect()
}

impl Screen for Overview {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        let Some(outcome) = &session.outcome else {
            super::nothing_yet(frame, area, session, crate::tui::app::Tab::Overview);
            return;
        };

        let [score, severity, grades, start] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Min(0),
        ])
        .areas(area);

        // Counted up rather than printed, so the band it lands in registers.
        let target = session.health().unwrap_or(100);
        let shown = session.motion.count_to(target, self.since, REVEAL);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("  Health  ", palette.dim()),
                    Span::styled(
                        format!("{shown}/100"),
                        Style::new()
                            .fg(palette.color(score_ink(shown)))
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::default(),
            ]),
            Rect { height: 2, ..score },
        );
        frame.render_widget(
            LineGauge::default()
                .ratio(f64::from(shown) / 100.0)
                .filled_style(Style::new().fg(palette.color(score_ink(shown))))
                .unfilled_style(palette.dim())
                .label(""),
            Rect {
                x: score.x + 2,
                y: score.y + 2,
                width: score.width.saturating_sub(4),
                height: 1,
            },
        );

        let grown = f64::from(session.motion.eased(self.since, REVEAL));
        let count = |level: Severity| {
            u64::from(outcome.counts.by_severity.get(&level).copied().unwrap_or(0))
        };
        frame.render_widget(
            BarChart::default()
                .bar_width(9)
                .bar_gap(2)
                .data(BarGroup::default().bars(&bars(
                    palette,
                    &[
                        ("error", count(Severity::Error), Ink::Danger),
                        ("warning", count(Severity::Warning), Ink::Warn),
                        ("info", count(Severity::Info), Ink::Accent),
                    ],
                    grown,
                ))),
            severity,
        );

        let mut tally: Vec<(&'static str, u64, Ink)> = Vec::new();
        for grade in Cleanliness::ALL.iter().rev() {
            let files = session
                .outcome
                .as_ref()
                .map(|outcome| {
                    outcome
                        .analyzed
                        .iter()
                        .filter(|file| session.in_scope(file))
                        .filter(|file| {
                            let lines = outcome
                                .lines_by_file
                                .get(file.as_str())
                                .copied()
                                .unwrap_or(0);
                            let found = outcome
                                .findings_by_file
                                .get(file.as_str())
                                .copied()
                                .unwrap_or(0);
                            Cleanliness::of(outcome.counts.file_score(file, lines), found) == *grade
                        })
                        .count() as u64
                })
                .unwrap_or(0);
            if files > 0 {
                tally.push((grade.label(), files, grade_ink(*grade)));
            }
        }
        if !tally.is_empty() {
            frame.render_widget(
                BarChart::default()
                    .bar_width(9)
                    .bar_gap(1)
                    .data(BarGroup::default().bars(&bars(palette, &tally, grown))),
                grades,
            );
        }

        let mut lines = Vec::new();
        if let Some(verdict) = &session.verdict {
            let (text, ink) = if verdict.passed() {
                ("  gate passed".to_owned(), Ink::Good)
            } else {
                (
                    format!("  gate failed: {}", verdict.failures.join("; ")),
                    Ink::Danger,
                )
            };
            lines.push(Line::styled(text, Style::new().fg(palette.color(ink))));
            lines.push(Line::default());
        }
        lines.push(Line::styled(
            "  Start here",
            Style::new()
                .fg(palette.color(Ink::Text))
                .add_modifier(Modifier::BOLD),
        ));
        // Ordered by grade then blast radius, the same way the report ranks it, so the
        // two never disagree about what to open first.
        let mut ranked: Vec<(&str, Cleanliness)> = outcome
            .findings_by_file
            .keys()
            .filter(|file| session.in_scope(file))
            .map(|file| {
                let lines = outcome.lines_by_file.get(file).copied().unwrap_or(0);
                let found = outcome.findings_by_file.get(file).copied().unwrap_or(0);
                (
                    file.as_ref(),
                    Cleanliness::of(outcome.counts.file_score(file, lines), found),
                )
            })
            .collect();
        ranked.sort_by(|left, right| {
            left.1.cmp(&right.1).then_with(|| {
                outcome
                    .impact
                    .get(right.0)
                    .multiplier()
                    .total_cmp(&outcome.impact.get(left.0).multiplier())
            })
        });
        self.listed.clear();
        for (file, grade) in ranked.into_iter().take(4) {
            let impact = outcome.impact.get(file);
            self.listed.push(file.to_owned());
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    {:9} ", grade.label()),
                    Style::new().fg(palette.color(grade_ink(grade))),
                ),
                Span::styled(file.to_owned(), Style::new().fg(palette.color(Ink::Text))),
                Span::styled(
                    format!(
                        "  {}, {}",
                        plural(impact.commits, "commit", "commits"),
                        plural(impact.importers, "importer", "importers")
                    ),
                    palette.dim(),
                ),
            ]));
        }
        // The heading sits at the top of `start`, so the names begin one line down.
        self.start = Rect {
            y: start
                .y
                .saturating_add(u16::try_from(lines.len() - self.listed.len()).unwrap_or(0)),
            ..start
        };
        frame.render_widget(Paragraph::new(lines), start);
    }

    fn click(&mut self, event: MouseEvent, _session: &Session) -> Nav {
        if !is_press(event) {
            return Nav::Stay;
        }
        let Some(row) = row_at(self.start, 0, 0, event) else {
            return Nav::Stay;
        };
        self.listed
            .get(row)
            .cloned()
            .map_or(Nav::Stay, Nav::Inspect)
    }

    fn handle(&mut self, key: KeyEvent, _session: &Session) -> Nav {
        global(key).unwrap_or(Nav::Stay)
    }

    fn animating(&self, session: &Session) -> Option<std::time::Duration> {
        (!session.motion.finished(self.since, REVEAL)).then_some(crate::tui::motion::FRAME)
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[("esc", "back")]
    }
}

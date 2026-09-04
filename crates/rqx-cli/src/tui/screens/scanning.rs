use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph, Sparkline};

use crate::brand::Ink;
use crate::progress::Phase;
use crate::tui::app::{Nav, Screen, Session, global};
use crate::tui::chrome::thousands;

/// Samples of findings-per-tick, kept only long enough to draw. The total file count
/// is unknown until the walk ends, so the rate is the honest thing to show.
#[derive(Default)]
pub struct Scanning {
    rate: Vec<u64>,
    last: u32,
}

impl Scanning {
    pub fn sample(&mut self, session: &Session) {
        let found = session.progress.findings;
        self.rate.push(u64::from(found.saturating_sub(self.last)));
        self.last = found;
        if self.rate.len() > 64 {
            self.rate.remove(0);
        }
    }
}

impl Screen for Scanning {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        let [status, gauge, spark, _] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .areas(area);

        if let Some(failure) = &session.failure {
            frame.render_widget(
                Paragraph::new(Line::styled(
                    format!("  scan failed: {failure}"),
                    Style::new().fg(palette.color(Ink::Danger)),
                )),
                status,
            );
            return;
        }

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("  {} ", session.motion.spinner()),
                        Style::new().fg(palette.color(Ink::Accent)),
                    ),
                    Span::styled(
                        format!("{} files", thousands(session.progress.files_scanned)),
                        Style::new().fg(palette.color(Ink::Text)),
                    ),
                    Span::styled(
                        format!("  {} findings", thousands(session.progress.findings)),
                        palette.dim(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        format!("  {}  ", session.progress.phase.label()),
                        palette.dim(),
                    ),
                    Span::styled(
                        session.progress.file.as_deref().unwrap_or_default().to_owned(),
                        palette.dim(),
                    ),
                ]),
            ]),
            status,
        );

        // The files are counted before the walk starts, so the bar fills against a real
        // total. It still sweeps for the phases after the walk, where there is no total
        // to divide and a percentage would be a claim nobody can check.
        let (ratio, label) = match session.progress.total {
            Some(total) if total > 0 && session.progress.phase == Phase::Scanning => {
                let done = f64::from(session.progress.files_scanned.min(total));
                let ratio = done / f64::from(total);
                (ratio, format!("{:.0}%", ratio * 100.0))
            }
            _ => {
                let phase = session.motion.phase(std::time::Duration::from_millis(1400));
                (f64::from((phase * 2.0 - 1.0).abs()), String::new())
            }
        };
        frame.render_widget(
            Gauge::default()
                .ratio(ratio.clamp(0.0, 1.0))
                .label(Span::styled(label, palette.dim()))
                .gauge_style(Style::new().fg(palette.color(Ink::Accent)))
                .use_unicode(true),
            gauge,
        );

        if self.rate.iter().any(|sample| *sample > 0) {
            frame.render_widget(
                Sparkline::default()
                    .data(&self.rate)
                    .style(Style::new().fg(palette.color(Ink::Muted))),
                spark,
            );
        }
    }

    fn handle(&mut self, key: KeyEvent, _session: &Session) -> Nav {
        match key.code {
            // Cancelling a scan means going back to the menu, not leaving the program.
            KeyCode::Esc => Nav::Back,
            _ => global(key).unwrap_or(Nav::Stay),
        }
    }

    fn animating(&self, session: &Session) -> Option<std::time::Duration> {
        session.scanning.then_some(crate::tui::motion::FRAME)
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[("esc", "cancel"), ("q", "quit")]
    }
}

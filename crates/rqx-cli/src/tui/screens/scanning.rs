use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, BarGroup, Gauge, Paragraph};

use crate::brand::Ink;
use crate::progress::Phase;
use crate::tui::app::{Nav, Screen, Session, global};
use crate::tui::chrome::thousands;

/// How often the trace takes a sample. On a clock rather than on a repaint: the old
/// sparkline sampled every `tick`, so its horizontal axis was frame timing and the
/// height of a bar meant "findings since the last redraw", which is not a rate.
const SAMPLE: Duration = Duration::from_millis(120);

/// How many files scroll past. Enough to see movement, few enough to read.
const TRACE: usize = 5;

pub struct Scanning {
    sampled_at: Instant,
    trace: Vec<String>,
    /// When each phase was first seen and last seen, so a finished one can say how
    /// long it took rather than only that it is done.
    entered: Vec<(Phase, Instant)>,
}

impl Default for Scanning {
    fn default() -> Self {
        Self {
            sampled_at: Instant::now(),
            trace: Vec::new(),
            entered: Vec::new(),
        }
    }
}

impl Scanning {
    pub fn sample(&mut self, session: &Session) {
        let now = Instant::now();
        let phase = session.progress.phase;
        if self.entered.last().is_none_or(|(last, _)| *last != phase) {
            self.entered.push((phase, now));
        }
        if now.duration_since(self.sampled_at) < SAMPLE {
            return;
        }
        self.sampled_at = now;
        if let Some(file) = &session.progress.file
            && self.trace.last().map(String::as_str) != Some(&**file)
        {
            self.trace.push(file.to_string());
            if self.trace.len() > TRACE {
                self.trace.remove(0);
            }
        }
    }

    /// How long a phase took, for the ones already behind us.
    fn took(&self, phase: Phase, now: Instant) -> Option<Duration> {
        let at = self.entered.iter().position(|(seen, _)| *seen == phase)?;
        let began = self.entered[at].1;
        let ended = self.entered.get(at + 1).map_or(now, |(_, when)| *when);
        Some(ended.duration_since(began))
    }

    fn reached(&self, phase: Phase) -> bool {
        self.entered.iter().any(|(seen, _)| *seen == phase)
    }
}

/// `counting → scanning → duplicates → …`, with what is done behind it and what is
/// left ahead. Every phase after the walk used to run with the screen still saying
/// "scanning" and the bar full, which on a large project is the part that looks like
/// a hang.
fn stepper(session: &Session, screen: &Scanning) -> Line<'static> {
    let palette = session.palette;
    let now = Instant::now();
    let here = session.progress.phase;
    let mut spans = vec![Span::raw("  ")];
    for (at, phase) in Phase::ALL.into_iter().enumerate() {
        if at > 0 {
            spans.push(Span::styled(" \u{203a} ", palette.dim()));
        }
        let current = phase == here;
        let done = screen.reached(phase) && !current;
        let style = if current {
            Style::new()
                .fg(palette.color(Ink::Accent))
                .add_modifier(Modifier::BOLD)
        } else if done {
            Style::new().fg(palette.color(Ink::Good))
        } else {
            palette.dim()
        };
        let mark = if current {
            format!("{} ", session.motion.spinner())
        } else if done {
            "\u{2713} ".to_owned()
        } else {
            "  ".to_owned()
        };
        spans.push(Span::styled(format!("{mark}{}", phase.short()), style));
        if done && let Some(took) = screen.took(phase, now) {
            spans.push(Span::styled(
                format!(" {}ms", took.as_millis()),
                palette.dim(),
            ));
        }
    }
    Line::from(spans)
}

/// Files done of the files there are, and beneath it the rate and what is left. The
/// counting phase has no total to be a proportion of, which is what the sweep is for.
fn progress(session: &Session, screen: &Scanning) -> (f64, String) {
    let seen = &session.progress;
    let counting = seen.phase == Phase::Counting;
    match seen.total.filter(|total| *total > 0 && !counting) {
        Some(total) => {
            let ratio = f64::from(seen.files_scanned.min(total)) / f64::from(total);
            (
                ratio,
                format!("{}/{}", thousands(seen.files_scanned), thousands(total)),
            )
        }
        // Nothing to be a proportion of yet, so it sweeps rather than claiming a
        // percentage it cannot know.
        None => {
            let _ = screen;
            let phase = session.motion.phase(Duration::from_millis(1400));
            (f64::from((phase * 2.0 - 1.0).abs()), String::new())
        }
    }
}

fn throughput(session: &Session) -> String {
    let seen = &session.progress;
    let seconds = seen.elapsed_ms as f64 / 1000.0;
    let mut said = vec![format!("{} findings", thousands(seen.findings))];
    if seen.code_lines > 0 {
        said.push(format!("{} lines", thousands(seen.code_lines as u32)));
    }
    if seconds > 0.2 {
        said.push(format!(
            "{} lines/s",
            thousands((seen.code_lines as f64 / seconds) as u32)
        ));
        // Only once there is a total to project against, and only while there is
        // something left: an estimate that counts down to nothing is noise.
        if let Some(total) = seen.total
            && seen.files_scanned > 0
            && total > seen.files_scanned
        {
            let each = seconds / f64::from(seen.files_scanned);
            let left = each * f64::from(total - seen.files_scanned);
            if left >= 1.0 {
                said.push(format!("~{left:.0}s left"));
            }
        }
    }
    if seen.skipped > 0 {
        said.push(format!("{} skipped", seen.skipped));
    }
    said.join(" \u{b7} ")
}

impl Screen for Scanning {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        if let Some(failure) = &session.failure {
            let lines = vec![
                Line::default(),
                Line::styled(
                    format!("  scan failed: {failure}"),
                    Style::new().fg(palette.color(Ink::Danger)),
                ),
                Line::default(),
                Line::styled("  press r to try again", palette.dim()),
            ];
            frame.render_widget(Paragraph::new(lines), area);
            return;
        }

        let [steps, gauge, rate, mosaic, files] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(8),
            Constraint::Min(0),
        ])
        .areas(area);

        frame.render_widget(Paragraph::new(stepper(session, self)), steps);

        let (ratio, counts) = progress(session, self);
        frame.render_widget(
            Gauge::default()
                .ratio(ratio)
                .label(counts)
                .use_unicode(true)
                .gauge_style(Style::new().fg(palette.color(Ink::Accent)))
                .style(palette.dim()),
            gauge,
        );

        frame.render_widget(
            Paragraph::new(Line::styled(
                format!("  {}", throughput(session)),
                palette.dim(),
            )),
            rate,
        );

        // The shape of the codebase appearing while the work happens. One bar on a
        // single-language project, which is honest: that is the shape.
        let seen: Vec<(&'static str, u64)> = rqx_core::Language::ALL
            .into_iter()
            .enumerate()
            .filter(|(at, _)| session.progress.by_language[*at] > 0)
            .map(|(at, language)| (language.slug(), u64::from(session.progress.by_language[at])))
            .collect();
        if !seen.is_empty() {
            let bars: Vec<Bar<'static>> = seen
                .iter()
                .map(|(name, count)| {
                    Bar::default()
                        .value(*count)
                        .label(Line::styled(*name, palette.dim()))
                        .style(Style::new().fg(palette.color(Ink::Accent)))
                })
                .collect();
            frame.render_widget(
                BarChart::default()
                    .data(BarGroup::default().bars(&bars))
                    .bar_width(11)
                    .bar_gap(2),
                mosaic,
            );
        }

        let trace: Vec<Line<'static>> = self
            .trace
            .iter()
            .map(|file| Line::styled(format!("  {file}"), palette.dim()))
            .collect();
        frame.render_widget(Paragraph::new(trace), files);
    }

    fn handle(&mut self, key: KeyEvent, _session: &Session) -> Nav {
        match key.code {
            // Cancelling a scan means going back to what was on screen, not leaving
            // the program.
            KeyCode::Esc => Nav::Back,
            _ => global(key).unwrap_or(Nav::Stay),
        }
    }

    fn animating(&self, _session: &Session) -> Option<Duration> {
        Some(crate::tui::motion::FRAME)
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[("esc", "leave it running"), ("q", "quit")]
    }
}

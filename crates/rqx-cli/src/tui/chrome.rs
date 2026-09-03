use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};

use crate::brand::{Ink, Palette};

use super::app::{Session, Tab};

/// Facts about this run rather than a slogan. The line that used to say "every
/// finding, a reason" said nothing about what the tool was about to do.
fn strip(session: &Session) -> Vec<Line<'static>> {
    let palette = session.palette;
    let dim = palette.dim();
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "rqx",
            Style::new().fg(palette.color(Ink::Text)).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {}", env!("CARGO_PKG_VERSION")), dim),
    ])];

    let threads = match session.config.scan.threads {
        0 => "auto".to_owned(),
        count => count.to_string(),
    };
    lines.push(Line::styled(
        format!(
            "{}/{} rules \u{b7} {} \u{b7} {threads} threads",
            session.rules_enabled(),
            rqx_core::rule::RuleId::ALL.len(),
            session.config_file.as_deref().unwrap_or("defaults"),
        ),
        dim,
    ));

    let mut place = session.root.clone();
    if let Some(branch) = &session.branch {
        place.push_str(&format!(" on {branch}"));
    }
    if let Some(base) = &session.base {
        place.push_str(&format!(" \u{b7} vs {base}"));
    }
    lines.push(Line::styled(place, dim));

    lines.push(match (&session.outcome, session.scanning) {
        (_, true) => Line::from(vec![
            Span::styled(format!("{} scanning ", session.motion.spinner()), dim),
            Span::styled(
                format!(
                    "{} files \u{b7} {} findings",
                    thousands(session.progress.files_scanned),
                    thousands(session.progress.findings)
                ),
                dim,
            ),
        ]),
        (Some(outcome), false) => {
            let score = session.health().unwrap_or(100);
            Line::from(vec![
                Span::styled(
                    format!(
                        "{} files \u{b7} {} lines \u{b7} {:.2}s \u{b7} health ",
                        thousands(outcome.files_scanned),
                        thousands(outcome.code_lines as u32),
                        outcome.elapsed_ms as f64 / 1000.0
                    ),
                    dim,
                ),
                Span::styled(
                    format!("{score}/100"),
                    Style::new().fg(palette.color(crate::brand::score_ink(score))),
                ),
            ])
        }
        (None, false) => Line::styled("not scanned yet", dim),
    });
    lines
}

/// Header, tab bar, body, footer. Every screen is handed the body and never has to
/// know what is around it.
pub struct Chrome<'a> {
    pub session: &'a Session,
    pub active: Option<Tab>,
}

impl Chrome<'_> {
    pub fn render(&self, frame: &mut Frame<'_>) -> Rect {
        let mascot = &self.session.mascot;
        let [head, tabs, body, _footer] = Layout::vertical([
            Constraint::Length(mascot.cell_height() + 1),
            Constraint::Length(u16::from(self.active.is_some())),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        self.render_head(frame, head);
        if let Some(active) = self.active {
            self.render_tabs(frame, tabs, active);
        }
        body
    }

    fn render_head(&self, frame: &mut Frame<'_>, area: Rect) {
        let session = self.session;
        let [art, text] =
            Layout::horizontal([Constraint::Length(session.mascot.width() + 3), Constraint::Min(0)])
                .areas(area);

        // The art wipes in a column at a time on the first frames, so the interface
        // arrives rather than appearing.
        let reveal = if session.motion.reduced() {
            usize::MAX
        } else {
            let progress = session.motion.elapsed().as_millis() as f32 / 250.0;
            ((progress * f32::from(session.mascot.width())).ceil() as usize).max(1)
        };
        frame.render_widget(
            Paragraph::new(session.mascot.render_frame(
                session.palette,
                session.mascot_frame(),
                session.signal(),
                reveal,
            )),
            art,
        );
        frame.render_widget(Paragraph::new(strip(session)), text);
    }

    fn render_tabs(&self, frame: &mut Frame<'_>, area: Rect, active: Tab) {
        let palette = self.session.palette;
        let titles: Vec<Line<'static>> = Tab::ALL
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let reachable = !tab.needs_outcome() || self.session.outcome.is_some();
                let ink = if reachable { Ink::Text } else { Ink::Muted };
                Line::from(vec![
                    Span::styled(format!("{} ", index + 1), palette.dim()),
                    Span::styled(tab.title(), Style::new().fg(palette.color(ink))),
                ])
            })
            .collect();
        frame.render_widget(
            Tabs::new(titles)
                .select(Tab::ALL.iter().position(|tab| *tab == active).unwrap_or(0))
                .highlight_style(
                    Style::new().fg(palette.color(Ink::Accent)).add_modifier(Modifier::BOLD),
                )
                .divider(Span::styled("\u{2502}", palette.dim())),
            area,
        );
    }

    pub fn render_footer(&self, frame: &mut Frame<'_>, keys: &[(&'static str, &'static str)]) {
        let palette = self.session.palette;
        let area = Rect {
            y: frame.area().height.saturating_sub(1),
            height: 1,
            ..frame.area()
        };
        let mut spans = Vec::new();
        for (key, meaning) in keys {
            spans.push(Span::styled(
                format!(" {key} "),
                Style::new().fg(palette.color(Ink::Accent)),
            ));
            spans.push(Span::styled(*meaning, palette.dim()));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

/// Built from the screens' own key tables, so a binding cannot exist without being
/// documented here.
pub fn help(frame: &mut Frame<'_>, palette: Palette, rows: &[(&'static str, &'static str)]) {
    let width = 54;
    let height = (rows.len() as u16 + 4).min(frame.area().height);
    let area = centred(frame.area(), width, height);
    frame.render_widget(Clear, area);

    let mut lines = vec![Line::default()];
    lines.extend(rows.iter().map(|(key, meaning)| {
        Line::from(vec![
            Span::styled(format!("  {key:<12}"), Style::new().fg(palette.color(Ink::Accent))),
            Span::styled(*meaning, Style::new().fg(palette.color(Ink::Text))),
        ])
    }));
    lines.push(Line::styled("  any key to close", palette.dim()));

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(palette.dim())
                .title(Span::styled(" keys ", Style::new().fg(palette.color(Ink::Accent)))),
        ),
        area,
    );
}

pub fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

pub fn thousands(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use rqx_core::config::RqxConfig;

    use crate::brand::ColorDepth;
    use crate::scan::ScanProgress;
    use crate::tui::motion::Motion;
    use crate::tui::sprite::Sprite;

    use super::*;

    fn session() -> Session {
        Session {
            palette: Palette::with_depth(ColorDepth::Monochrome),
            mascot: Sprite::parse(include_str!("../../assets/mascot.txt")),
            motion: Motion::new(true),
            root: "src".to_owned(),
            path: std::path::PathBuf::from("."),
            config: RqxConfig::default(),
            config_file: None,
            branch: Some("main".to_owned()),
            outcome: None,
            progress: ScanProgress::default(),
            scanning: false,
            failure: None,
            base: None,
        }
    }

    fn draw(session: &Session, active: Option<Tab>, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                Chrome { session, active }.render(frame);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The line that used to carry a slogan now carries what this run will do.
    #[test]
    fn the_header_reports_the_run_rather_than_a_tagline() {
        let screen = draw(&session(), None, 100, 20);
        assert!(screen.contains("rqx 0.1.0"), "{screen}");
        assert!(screen.contains("rules"), "no rule count:\n{screen}");
        assert!(screen.contains("defaults"), "no config source:\n{screen}");
        assert!(screen.contains("src on main"), "no target or branch:\n{screen}");
        assert!(screen.contains("not scanned yet"), "no scan state:\n{screen}");
        assert!(!screen.contains("every finding"), "the tagline is still here:\n{screen}");
    }

    #[test]
    fn the_tab_bar_appears_only_when_a_tab_is_open() {
        assert!(!draw(&session(), None, 100, 20).contains("Overview"));
        let tabbed = draw(&session(), Some(Tab::Rules), 100, 20);
        assert!(tabbed.contains("Overview") && tabbed.contains("Coverage"), "{tabbed}");
    }

    #[test]
    fn the_smallest_supported_terminal_still_renders_without_panicking() {
        let screen = draw(&session(), Some(Tab::Overview), 60, 20);
        assert!(screen.contains("rqx"), "{screen}");
    }
}

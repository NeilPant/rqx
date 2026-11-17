use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::brand::{Ink, Palette};

use super::app::{Session, Tab};
use super::{MIN_HEIGHT, MIN_WIDTH};

/// Where the frame put the things a click can land on. Rects live for one frame, so
/// what a screen drew has to be written down before the frame ends or there is
/// nothing to test a click against.
#[derive(Default)]
pub struct Hits {
    pub tabs: Vec<(Rect, Tab)>,
    pub keys: Vec<(Rect, &'static str)>,
}

impl Hits {
    pub fn at(&self, column: u16, row: u16) -> Option<Landing> {
        for (area, tab) in &self.tabs {
            if holds(*area, column, row) {
                return Some(Landing::Tab(*tab));
            }
        }
        for (area, key) in &self.keys {
            if holds(*area, column, row) {
                return Some(Landing::Key(key));
            }
        }
        None
    }
}

pub enum Landing {
    Tab(Tab),
    Key(&'static str),
}

/// Whether a point is inside a rect. Shared, because the app asks it of the body
/// and the hit table asks it of everything else.
pub const fn within(area: Rect, column: u16, row: u16) -> bool {
    holds(area, column, row)
}

const fn holds(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.x + area.width && row >= area.y && row < area.y + area.height
}

/// Facts about this run rather than a slogan. The line that used to say "every
/// finding, a reason" said nothing about what the tool was about to do.
fn strip(session: &Session) -> Vec<Line<'static>> {
    let palette = session.palette;
    let dim = palette.dim();
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "rqx",
            Style::new()
                .fg(palette.color(Ink::Text))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {}", env!("CARGO_PKG_VERSION")), dim),
    ])];

    let threads = match session.config.config.scan.threads {
        0 => "auto".to_owned(),
        count => count.to_string(),
    };
    lines.push(Line::styled(
        format!(
            "{}/{} rules \u{b7} {} \u{b7} {threads} threads",
            session.config.rules_enabled(),
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
            // The score leads because this line is the first thing narrowed when the
            // terminal is, and the score is the one fact in it worth keeping.
            Line::from(vec![
                Span::styled("health ", dim),
                Span::styled(
                    format!("{score}/100"),
                    Style::new().fg(palette.color(crate::brand::score_ink(score))),
                ),
                Span::styled(
                    format!(
                        " \u{b7} {} files \u{b7} {} lines \u{b7} {:.2}s",
                        thousands(outcome.files_scanned),
                        thousands(outcome.code_lines as u32),
                        outcome.elapsed_ms as f64 / 1000.0
                    ),
                    dim,
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
    pub fn render(&self, frame: &mut Frame<'_>) -> (Rect, Hits) {
        let mascot = &self.session.mascot;
        let [head, tabs, body, _footer] = Layout::vertical([
            Constraint::Length(mascot.cell_height() + 1),
            Constraint::Length(u16::from(self.active.is_some())),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        self.render_head(frame, head);
        let mut hits = Hits::default();
        if let Some(active) = self.active {
            hits.tabs = self.render_tabs(frame, tabs, active);
        }
        (body, hits)
    }

    fn render_head(&self, frame: &mut Frame<'_>, area: Rect) {
        let session = self.session;
        let [art, text] = Layout::horizontal([
            Constraint::Length(session.mascot.width() + 3),
            Constraint::Min(0),
        ])
        .areas(area);

        frame.render_widget(
            Paragraph::new(session.mascot.render_frame(
                session.palette,
                session.mascot_frame(),
                session.signal(),
                session.reveal(),
            )),
            art,
        );
        frame.render_widget(Paragraph::new(strip(session)), text);
    }

    /// Laid out by hand rather than with `Tabs`, because a click has to know which
    /// title it landed on and only the code that placed them knows where they are.
    fn render_tabs(&self, frame: &mut Frame<'_>, area: Rect, active: Tab) -> Vec<(Rect, Tab)> {
        let palette = self.session.palette;
        let mut spans = Vec::new();
        let mut hits = Vec::new();
        let mut x = area.x;
        for (index, tab) in Tab::ALL.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled("\u{2502}", palette.dim()));
                x += 1;
            }
            // Dim means "nothing scanned yet", not "cannot open": every tab opens.
            let filled = !tab.needs_outcome() || self.session.outcome.is_some();
            let style = if *tab == active {
                Style::new()
                    .fg(palette.color(Ink::Accent))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(palette.color(if filled { Ink::Text } else { Ink::Muted }))
            };
            let label = format!(" {} {} ", index + 1, tab.title());
            let width = label.chars().count() as u16;
            if x.saturating_add(width) > area.x.saturating_add(area.width) {
                break;
            }
            hits.push((
                Rect {
                    x,
                    y: area.y,
                    width,
                    height: 1,
                },
                *tab,
            ));
            x += width;
            spans.push(Span::styled(label, style));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        hits
    }

    /// The hints are the shortest list of what this screen does, so they are also
    /// the buttons: clicking one sends the key it names.
    pub fn render_footer(
        &self,
        frame: &mut Frame<'_>,
        keys: &[(&'static str, &'static str)],
    ) -> Vec<(Rect, &'static str)> {
        let palette = self.session.palette;
        let area = Rect {
            y: frame.area().height.saturating_sub(1),
            height: 1,
            ..frame.area()
        };
        // A command that wrote to disk says so here, on the one line that is on every
        // screen, until the next key — a message anywhere else is one the reader has
        // to already be looking at.
        if let Some(notice) = &self.session.notice {
            frame.render_widget(
                Paragraph::new(Line::styled(
                    format!(" {notice}"),
                    Style::new().fg(palette.color(Ink::Good)),
                )),
                area,
            );
            return Vec::new();
        }
        let mut spans = Vec::new();
        let mut hits = Vec::new();
        let mut x = area.x;
        for (key, meaning) in keys {
            // What this screen's version of the key acts on, which is what a reader
            // glancing at one line wants; the whole meaning is in the overlay. Without
            // this the hints for a screen with four keys run off the width.
            let meaning = meaning.rsplit(" \u{b7} ").next().unwrap_or(meaning);
            let label = format!(" {key} ");
            let width = (label.chars().count() + meaning.chars().count()) as u16;
            hits.push((
                Rect {
                    x,
                    y: area.y,
                    width,
                    height: 1,
                },
                *key,
            ));
            x = x.saturating_add(width);
            spans.push(Span::styled(
                label,
                Style::new().fg(palette.color(Ink::Accent)),
            ));
            spans.push(Span::styled(meaning, palette.dim()));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        hits
    }
}

/// What is drawn instead of the interface once the window is too small for it. A
/// layout squeezed past its minimum reads as a bug in the tool rather than as a
/// window that needs widening.
pub fn cramped(frame: &mut Frame<'_>, palette: Palette, width: u16, height: u16) {
    let lines = vec![
        Line::default(),
        Line::from(Span::styled(
            format!("  the interface needs {MIN_WIDTH}x{MIN_HEIGHT}"),
            Style::new().fg(palette.color(Ink::Text)),
        )),
        Line::from(Span::styled(
            format!("  this window is {width}x{height}"),
            palette.dim(),
        )),
    ];
    frame.render_widget(Clear, frame.area());
    frame.render_widget(Paragraph::new(lines), frame.area());
}

/// One key, one meaning, on every screen that has it. A screen may say what its own
/// version of a meaning acts on, after a `\u{b7}` — `mode \u{b7} slop` — but never give
/// a key a second meaning. `keys_mean_one_thing` holds every screen's table to this.
#[cfg(test)]
pub const VOCABULARY: &[(&str, &str)] = &[
    ("\u{2191}\u{2193}", "move"),
    ("enter", "open"),
    ("/", "search"),
    ("f", "filter"),
    (">", "severity floor"),
    ("g", "group"),
    ("s", "sort"),
    ("m", "mode"),
    ("a", "fix"),
    ("y", "write"),
    ("x", "waive"),
    ("esc", "back"),
];

/// Answered on every screen, so no screen lists them and the overlay adds them once.
/// Before this they were bound in `global` and documented nowhere.
pub const EVERYWHERE: &[(&str, &str)] = &[
    ("tab", "next view"),
    ("1-9", "jump to a view"),
    ("ctrl-p", "commands"),
    ("r", "scan"),
    ("w", "watch"),
    (",", "settings"),
    ("?", "keys"),
    ("q", "quit"),
];

/// Built from the screen's own key table and the ones every screen answers, so a
/// binding cannot exist without being documented here.
pub fn help(frame: &mut Frame<'_>, palette: Palette, rows: &[(&'static str, &'static str)]) {
    let rows: Vec<(&str, &str)> = rows
        .iter()
        .copied()
        .chain(
            EVERYWHERE
                .iter()
                .copied()
                .filter(|(key, _)| !rows.iter().any(|(had, _)| had == key)),
        )
        .collect();
    let width = 54;
    let height = (rows.len() as u16 + 4).min(frame.area().height);
    let area = centred(frame.area(), width, height);
    frame.render_widget(Clear, area);

    let mut lines = vec![Line::default()];
    lines.extend(rows.iter().map(|(key, meaning)| {
        Line::from(vec![
            Span::styled(
                format!("  {key:<12}"),
                Style::new().fg(palette.color(Ink::Accent)),
            ),
            Span::styled(*meaning, Style::new().fg(palette.color(Ink::Text))),
        ])
    }));
    lines.push(Line::styled("  any key to close", palette.dim()));

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(palette.rule())
                .title(Span::styled(
                    " keys ",
                    Style::new().fg(palette.color(Ink::Accent)),
                )),
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

pub use crate::session::text::thousands;

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::brand::ColorDepth;

    use super::*;

    fn session() -> Session {
        Session::probe(Palette::with_depth(ColorDepth::Monochrome))
    }

    fn draw(session: &Session, active: Option<Tab>, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                Chrome { session, active }.render(frame);
            })
            .unwrap();
        buffer_text(&terminal)
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
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
        assert!(
            screen.contains(&format!("rqx {}", env!("CARGO_PKG_VERSION"))),
            "{screen}"
        );
        assert!(screen.contains("rules"), "no rule count:\n{screen}");
        assert!(screen.contains("defaults"), "no config source:\n{screen}");
        assert!(
            screen.contains("src on main"),
            "no target or branch:\n{screen}"
        );
        assert!(
            screen.contains("not scanned yet"),
            "no scan state:\n{screen}"
        );
        assert!(
            !screen.contains("every finding"),
            "the tagline is still here:\n{screen}"
        );
    }

    #[test]
    fn the_tab_bar_appears_only_when_a_tab_is_open() {
        assert!(!draw(&session(), None, 100, 20).contains("Overview"));
        let tabbed = draw(&session(), Some(Tab::Rules), 100, 20);
        assert!(
            tabbed.contains("Overview") && tabbed.contains("Patterns"),
            "{tabbed}"
        );
    }

    #[test]
    fn the_smallest_supported_terminal_still_renders_without_panicking() {
        let screen = draw(&session(), Some(Tab::Overview), 60, 20);
        assert!(screen.contains("rqx"), "{screen}");
    }

    /// A rect written down during a frame is the only thing a click has to aim at, so
    /// what the tab bar reports has to be where the tab bar actually drew.
    #[test]
    fn a_click_lands_on_the_tab_it_is_over() {
        let session = session();
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
        let mut hits = Hits::default();
        terminal
            .draw(|frame| {
                let (_, drawn) = Chrome {
                    session: &session,
                    active: Some(Tab::Overview),
                }
                .render(frame);
                hits = drawn;
            })
            .unwrap();

        assert_eq!(
            hits.tabs.len(),
            Tab::ALL.len(),
            "not every tab is clickable"
        );
        let screen = buffer_text(&terminal);
        for (area, tab) in &hits.tabs {
            let row: String = (0..terminal.backend().buffer().area.width)
                .map(|x| terminal.backend().buffer()[(x, area.y)].symbol())
                .collect();
            let under: String = row
                .chars()
                .skip(usize::from(area.x))
                .take(usize::from(area.width))
                .collect();
            assert!(
                under.contains(tab.title()),
                "{tab:?} is reported at {area:?}, which holds {under:?}\n{screen}"
            );
            assert!(
                matches!(hits.at(area.x, area.y), Some(Landing::Tab(hit)) if hit == *tab),
                "a click at the start of {tab:?} does not reach it"
            );
        }
    }

    /// The hints are the buttons, so each one has to be found where it was drawn.
    #[test]
    fn a_click_on_a_key_hint_reaches_that_key() {
        let session = session();
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
        let keys: &[(&str, &str)] = &[("enter", "open"), ("r", "scan"), ("esc", "back")];
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                hits = Chrome {
                    session: &session,
                    active: Some(Tab::Overview),
                }
                .render_footer(frame, keys);
            })
            .unwrap();

        assert_eq!(hits.len(), keys.len());
        for ((area, named), (key, _)) in hits.iter().zip(keys) {
            assert_eq!(named, key);
            let under: String = (0..area.width)
                .map(|offset| terminal.backend().buffer()[(area.x + offset, area.y)].symbol())
                .collect();
            assert!(under.contains(key), "{key} is not at {area:?}: {under:?}");
        }
    }
}

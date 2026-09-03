pub mod brand;
pub mod browser;
pub mod dashboard;
pub mod mascot;
pub mod sprite;

use std::fmt;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{DefaultTerminal, Frame, TerminalOptions, Viewport};
use rqx_core::config::RqxConfig;

use brand::{Palette, TAGLINE};
use browser::Browser;
use dashboard::{Phase, View};
use mascot::{Mascot, Mood};
use sprite::Sprite;

use crate::scan::{self, ScanOutcome, ScanProgress};

const WORDMARK: &str = include_str!("../../assets/wordmark.txt");

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 20;
const INLINE_HEIGHT: u16 = 16;

/// Why the interface declined to start. Every variant is a condition under which
/// taking over the terminal would corrupt output or hang a pipeline, so each one
/// falls back to plain help rather than degrading the interface.
#[derive(Debug, PartialEq, Eq)]
pub enum Decline {
    NotATerminal,
    DumbTerminal,
    ContinuousIntegration,
    TooSmall { width: u16, height: u16 },
    SetupFailed(String),
}

impl fmt::Display for Decline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotATerminal => f.write_str("output is not a terminal"),
            Self::DumbTerminal => f.write_str("TERM does not support a full-screen interface"),
            Self::ContinuousIntegration => f.write_str("running under CI"),
            Self::TooSmall { width, height } => write!(
                f,
                "terminal is {width}x{height}; the interface needs at least {MIN_WIDTH}x{MIN_HEIGHT}"
            ),
            Self::SetupFailed(error) => write!(f, "terminal setup failed ({error})"),
        }
    }
}

/// Checked before any terminal state is touched: a TUI that starts inside a pipe
/// writes escape sequences into whatever is reading, and one that starts in CI
/// hangs the job waiting for a keypress nobody will send.
pub fn gate() -> Result<(), Decline> {
    if !std::io::stdout().is_terminal() {
        return Err(Decline::NotATerminal);
    }
    if std::env::var_os("CI").is_some() {
        return Err(Decline::ContinuousIntegration);
    }
    match std::env::var("TERM") {
        Err(_) => return Err(Decline::DumbTerminal),
        Ok(term) if term.is_empty() || term == "dumb" => return Err(Decline::DumbTerminal),
        Ok(_) => {}
    }
    let (width, height) = crossterm::terminal::size().map_err(|_| Decline::NotATerminal)?;
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        return Err(Decline::TooSmall { width, height });
    }
    Ok(())
}

/// The brand without a terminal to animate in: the same wordmark asset, rendered
/// as plain glyphs, so the declined path still identifies the tool.
pub fn static_header() -> String {
    let sprite = Sprite::parse(WORDMARK);
    let mut out = String::from("\n");
    for line in sprite.render(Palette::with_depth(brand::ColorDepth::Monochrome)) {
        out.push_str("  ");
        for span in &line.spans {
            out.push_str(span.content.as_ref());
        }
        out.push('\n');
    }
    out.push_str(&format!("  {TAGLINE}\n"));
    out
}

const SLEEP_AFTER: Duration = Duration::from_secs(60);
const PROGRESS_POLL: Duration = Duration::from_millis(80);

struct App {
    palette: Palette,
    wordmark: Sprite,
    rex: Mascot,
    phase: Phase,
    root: String,
    updates: Receiver<ScanProgress>,
    worker: Option<JoinHandle<anyhow::Result<ScanOutcome>>>,
    last_input: Instant,
    intent: Option<Intent>,
    needs_redraw: bool,
    path: PathBuf,
    config: RqxConfig,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Intent {
    Quit,
    Browse,
}

impl App {
    fn start(root: &Path, config: &RqxConfig) -> Self {
        let (sender, updates) = mpsc::sync_channel(1);
        let (path, settings) = (root.to_path_buf(), config.clone());
        let worker = thread::spawn(move || scan::scan_with_progress(&path, &settings, sender));

        let mut rex = Mascot::new();
        rex.set_mood(Mood::Walking);
        Self {
            palette: Palette::detect(),
            wordmark: Sprite::parse(WORDMARK),
            rex,
            phase: Phase::Scanning(ScanProgress::default()),
            root: display_root(root),
            updates,
            worker: Some(worker),
            last_input: Instant::now(),
            intent: None,
            needs_redraw: true,
            path: root.to_path_buf(),
            config: config.clone(),
        }
    }

    fn rescan(&mut self) {
        let (sender, updates) = mpsc::sync_channel(1);
        let (path, settings) = (self.path.clone(), self.config.clone());
        self.updates = updates;
        self.worker = Some(thread::spawn(move || {
            scan::scan_with_progress(&path, &settings, sender)
        }));
        self.phase = Phase::Scanning(ScanProgress::default());
        self.rex.set_mood(Mood::Walking);
    }

    fn browsable(&self) -> bool {
        matches!(&self.phase, Phase::Done(outcome) if outcome.counts.total > 0)
    }

    fn handle(&mut self, key: KeyEvent) {
        self.last_input = Instant::now();
        if self.rex.mood() == Mood::Sleeping {
            self.rex.set_mood(Mood::Idle);
        }
        if is_quit(key, matches!(self.phase, Phase::Scanning(_))) {
            self.intent = Some(Intent::Quit);
        } else if key.code == KeyCode::Enter && self.browsable() {
            self.intent = Some(Intent::Browse);
        } else if key.code == KeyCode::Char('r') && self.worker.is_none() {
            self.rescan();
        }
        self.needs_redraw = true;
    }

    /// Drains progress, adopts the finished scan, and advances the mascot. Returns
    /// whether anything visible changed, so the loop redraws only when it did.
    fn tick(&mut self) -> bool {
        let mut changed = self.rex.tick();

        while let Ok(progress) = self.updates.try_recv() {
            self.phase = Phase::Scanning(progress);
            changed = true;
        }

        if self.worker.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(worker) = self.worker.take()
        {
            self.phase = match worker.join() {
                Ok(Ok(outcome)) => {
                    // One beat when the hunt ends: a chomp per finding would strobe
                    // at twenty-six thousand files a second.
                    let clean = outcome.counts.health_score(outcome.code_lines) >= 90;
                    self.rex.set_mood(match (outcome.counts.total, clean) {
                        (0, _) | (_, true) => Mood::Roaring,
                        _ => Mood::Chomping,
                    });
                    Phase::Done(Box::new(outcome))
                }
                Ok(Err(error)) => {
                    self.rex.set_mood(Mood::Idle);
                    Phase::Failed(error.to_string())
                }
                Err(_) => {
                    self.rex.set_mood(Mood::Idle);
                    Phase::Failed("the scan thread panicked".to_owned())
                }
            };
            changed = true;
        }

        if self.worker.is_none()
            && self.rex.mood() == Mood::Idle
            && self.last_input.elapsed() > SLEEP_AFTER
        {
            self.rex.set_mood(Mood::Sleeping);
            changed = true;
        }

        changed
    }

    /// Once the scan is done the only thing on a clock is the mascot, so the loop
    /// sleeps until its next frame — seconds, during a held blink — rather than
    /// waking on a fixed tick to discover nothing changed. Input wakes `poll`
    /// immediately either way.
    fn poll_timeout(&self) -> Duration {
        let next_frame = self.rex.time_to_next_frame();
        if self.worker.is_some() { next_frame.min(PROGRESS_POLL) } else { next_frame }
    }

    fn render(&self, frame: &mut Frame<'_>) {
        dashboard::render(
            frame,
            &View {
                palette: self.palette,
                wordmark: &self.wordmark,
                rex: &self.rex,
                phase: &self.phase,
                root: &self.root,
            },
        );
    }
}

/// `esc` only quits mid-scan, where it means cancel; once results are on screen it
/// is the key for stepping back out of the browser, so quitting on it there would
/// throw away the scan the user is reading.
fn is_quit(key: KeyEvent, scanning: bool) -> bool {
    match key.code {
        KeyCode::Char('q') => true,
        KeyCode::Char('c') => key.modifiers.contains(KeyModifiers::CONTROL),
        KeyCode::Esc => scanning,
        _ => false,
    }
}

/// Scanning the working directory is the common case, and printing its absolute
/// path there is noise that pushes the useful part of the line off screen.
fn display_root(root: &Path) -> String {
    let absolute = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let relative = std::env::current_dir()
        .ok()
        .and_then(|cwd| absolute.strip_prefix(&cwd).ok().map(PathBuf::from));
    match relative {
        Some(path) if path.as_os_str().is_empty() => ".".to_owned(),
        Some(path) => path.display().to_string(),
        None => absolute.display().to_string(),
    }
}

/// `ratatui::init_with_options` panics when setup fails, and an inline viewport
/// fails whenever the terminal will not answer a cursor-position query — real
/// behaviour on some multiplexers and flaky links. A panic naming a file inside
/// ratatui is not an answer for that, so setup failure declines like any other
/// gate.
pub fn launch(path: &Path, config: &RqxConfig) -> Result<(), Decline> {
    let rows = crossterm::terminal::size().map_err(|_| Decline::NotATerminal)?.1;
    let height = INLINE_HEIGHT.min(rows.saturating_sub(1));
    let mut terminal =
        ratatui::try_init_with_options(TerminalOptions { viewport: Viewport::Inline(height) })
            .map_err(|error| Decline::SetupFailed(error.to_string()))?;
    let session = run_inline(&mut terminal, path, config);
    ratatui::restore();

    let (outcome, intent, palette) =
        session.map_err(|error| Decline::SetupFailed(error.to_string()))?;

    // Browsing takes the whole screen on its own terminal rather than switching the
    // live one's viewport, and does not return to the inline view: re-entering it
    // would scroll a second copy of the header into the user's history.
    if intent == Some(Intent::Browse)
        && let Some(outcome) = &outcome
    {
        ratatui::run(|terminal| run_browser(terminal, outcome, palette))
            .map_err(|error| Decline::SetupFailed(error.to_string()))?;
    }
    Ok(())
}

fn run_browser(
    terminal: &mut DefaultTerminal,
    outcome: &ScanOutcome,
    palette: Palette,
) -> Result<()> {
    let mut browser = Browser::new(outcome, palette);
    let mut needs_redraw = true;
    while !browser.should_quit {
        if needs_redraw {
            terminal.draw(|frame| browser.render(frame))?;
            needs_redraw = false;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                needs_redraw = browser.handle(key);
            }
            Event::Resize(..) => needs_redraw = true,
            _ => {}
        }
    }
    Ok(())
}

/// Repainting on every poll timeout costs a redraw many times a second for a screen
/// that has not changed, which is both wasted CPU and a source of flicker.
type Session = (Option<ScanOutcome>, Option<Intent>, Palette);

fn run_inline(terminal: &mut DefaultTerminal, path: &Path, config: &RqxConfig) -> Result<Session> {
    let mut app = App::start(path, config);
    while app.intent.is_none() {
        if app.needs_redraw {
            terminal.draw(|frame| app.render(frame))?;
            app.needs_redraw = false;
        }
        if event::poll(app.poll_timeout())? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.handle(key),
                Event::Resize(..) => app.needs_redraw = true,
                _ => {}
            }
        }
        app.needs_redraw |= app.tick();
    }

    // Best effort from here on: the scan succeeded and its result is already on
    // screen, so a terminal that will not take a parting line is not a reason to
    // fail the run. `insert_before` needs a cursor-position reply, which some
    // terminals never send.
    let _ = terminal.clear();
    if let Phase::Done(outcome) = &app.phase {
        let summary = dashboard::scrollback_summary(outcome, app.palette, &app.root);
        let _ = terminal.insert_before(summary.len() as u16, |buffer| {
            for (row, line) in summary.iter().enumerate() {
                buffer.set_line(0, row as u16, line, buffer.area.width);
            }
        });
    }

    let outcome = match app.phase {
        Phase::Done(outcome) => Some(*outcome),
        _ => None,
    };
    Ok((outcome, app.intent, app.palette))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn static_header_carries_the_wordmark_and_tagline() {
        let header = static_header();
        assert!(header.contains(TAGLINE));
        assert!(header.contains('\u{2588}'), "the wordmark should render as blocks");
    }

    #[test]
    fn q_and_ctrl_c_quit_from_anywhere() {
        for scanning in [true, false] {
            assert!(is_quit(press(KeyCode::Char('q'), KeyModifiers::NONE), scanning));
            assert!(is_quit(press(KeyCode::Char('c'), KeyModifiers::CONTROL), scanning));
        }
    }

    #[test]
    fn esc_cancels_a_scan_but_does_not_discard_a_finished_one() {
        assert!(is_quit(press(KeyCode::Esc, KeyModifiers::NONE), true));
        assert!(!is_quit(press(KeyCode::Esc, KeyModifiers::NONE), false));
    }

    #[test]
    fn a_plain_c_is_not_ctrl_c() {
        assert!(!is_quit(press(KeyCode::Char('c'), KeyModifiers::NONE), false));
    }

    #[test]
    fn the_working_directory_renders_as_a_dot_not_an_absolute_path() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(display_root(&cwd), ".");
        assert_eq!(display_root(Path::new(".")), ".");
    }
}

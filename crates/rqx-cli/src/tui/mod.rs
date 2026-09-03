pub mod app;
pub mod chrome;
pub mod motion;
pub mod screens;
pub mod sprite;

use std::fmt;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{DefaultTerminal, Frame};
use rqx_core::config::RqxConfig;

use crate::brand::Palette;
use crate::scan::{self, ScanOutcome, ScanProgress};

use app::{Nav, Screen, Session, Tab};
use chrome::Chrome;
use motion::Motion;
use sprite::Sprite;

const MASCOT: &str = include_str!("../../assets/mascot.txt");
const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 20;

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

/// The header as plain glyphs, for the path that could not have a terminal. It
/// carries the same facts the interface would have opened with.
pub fn static_header(root: &str, config: &RqxConfig, config_file: Option<&str>) -> String {
    let sprite = Sprite::parse(MASCOT);
    let rows = sprite.render_frame(
        Palette::with_depth(crate::brand::ColorDepth::Monochrome),
        0,
        crate::brand::Ink::Signal,
        usize::MAX,
    );
    let enabled = rqx_core::rule::RuleId::ALL
        .iter()
        .filter(|rule| config.rules.enabled(**rule))
        .count();
    let beside = [
        format!("rqx {}", env!("CARGO_PKG_VERSION")),
        format!(
            "{enabled}/{} rules \u{b7} {}",
            rqx_core::rule::RuleId::ALL.len(),
            config_file.unwrap_or("defaults")
        ),
        root.to_owned(),
        String::new(),
        String::new(),
    ];

    let mut out = String::from("\n");
    for (line, text) in rows.iter().zip(beside) {
        out.push_str("  ");
        for span in &line.spans {
            out.push_str(span.content.as_ref());
        }
        if !text.is_empty() {
            out.push_str("   ");
            out.push_str(&text);
        }
        out.push('\n');
    }
    out
}

/// What the interface was asked to run that it cannot run itself.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Chosen {
    Review,
}

type Worker = JoinHandle<anyhow::Result<ScanOutcome>>;

enum Mode {
    Menu,
    Scanning,
    Browsing(Tab),
}

struct App {
    session: Session,
    mode: Mode,
    menu: screens::menu::Menu,
    scanning: screens::scanning::Scanning,
    overview: screens::overview::Overview,
    findings: screens::findings::Findings,
    files: screens::files::Files,
    rules: screens::catalogue::Rules,
    coverage: screens::catalogue::CoverageView,
    settings: screens::settings::Settings,
    updates: Option<Receiver<ScanProgress>>,
    worker: Option<Worker>,
    help: bool,
    dirty: bool,
    quit: bool,
    chosen: Option<Chosen>,
}

/// Splits the borrow the way the compiler can see: the screen and the session are
/// disjoint fields, so each screen reads the session while writing only its own state.
macro_rules! parts {
    ($app:expr) => {{
        let app = &mut *$app;
        let screen: &mut dyn Screen = match app.mode {
            Mode::Menu => &mut app.menu,
            Mode::Scanning => &mut app.scanning,
            Mode::Browsing(Tab::Overview) => &mut app.overview,
            Mode::Browsing(Tab::Findings) => &mut app.findings,
            Mode::Browsing(Tab::Files) => &mut app.files,
            Mode::Browsing(Tab::Rules) => &mut app.rules,
            Mode::Browsing(Tab::Coverage) => &mut app.coverage,
            Mode::Browsing(Tab::Config) => &mut app.settings,
        };
        (screen, &app.session)
    }};
}

impl App {
    fn start_scan(&mut self) {
        let (sender, updates) = mpsc::sync_channel(1);
        let (path, config) = (self.session.path.clone(), self.session.config.clone());
        self.worker = Some(thread::spawn(move || scan::scan_with_progress(&path, &config, sender)));
        self.updates = Some(updates);
        self.session.scanning = true;
        self.session.failure = None;
        self.session.progress = ScanProgress::default();
        self.session.outcome = None;
        self.scanning = screens::scanning::Scanning::default();
        self.mode = Mode::Scanning;
    }

    /// Drains progress and adopts the finished scan. Returns whether anything visible
    /// changed, so the loop redraws only when it did.
    fn tick(&mut self) -> bool {
        let mut changed = false;
        if let Some(updates) = &self.updates {
            while let Ok(progress) = updates.try_recv() {
                self.session.progress = progress;
                changed = true;
            }
        }
        if self.session.scanning {
            self.scanning.sample(&self.session);
        }
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(worker) = self.worker.take()
        {
            self.session.scanning = false;
            match worker.join() {
                Ok(Ok(outcome)) => {
                    self.session.outcome = Some(Arc::new(outcome));
                    // Screens hold derived views of the outcome, so a new scan means a
                    // new set of them rather than a refresh flag on each.
                    self.overview = screens::overview::Overview::default();
                    self.findings = screens::findings::Findings::default();
                    self.files = screens::files::Files::default();
                    self.mode = Mode::Browsing(Tab::Overview);
                }
                Ok(Err(error)) => self.session.failure = Some(error.to_string()),
                Err(_) => self.session.failure = Some("the scan thread panicked".to_owned()),
            }
            changed = true;
        }
        changed
    }

    fn apply(&mut self, nav: Nav) {
        match nav {
            Nav::Stay => {}
            Nav::Redraw => self.dirty = true,
            Nav::Quit => self.quit = true,
            Nav::Scan => self.start_scan(),
            Nav::Review => {
                self.chosen = Some(Chosen::Review);
                self.quit = true;
            }
            Nav::To(tab) => {
                self.mode = Mode::Browsing(tab);
                self.dirty = true;
            }
            // Every screen leads back to the menu, and the menu is the only place
            // where leaving means leaving the program.
            Nav::Back => {
                self.mode = Mode::Menu;
                self.dirty = true;
            }
        }
    }

    fn handle(&mut self, key: KeyEvent) {
        if self.help {
            self.help = false;
            self.dirty = true;
            return;
        }
        if key.code == KeyCode::Char('?') {
            self.help = true;
            self.dirty = true;
            return;
        }

        if let Mode::Browsing(active) = self.mode
            && let Some(next) = tab_key(key, active, self.session.outcome.is_some())
        {
            self.mode = Mode::Browsing(next);
            self.dirty = true;
            return;
        }

        let nav = {
            let (screen, session) = parts!(self);
            screen.handle(key, session)
        };
        self.apply(nav);
    }

    fn render(&mut self, frame: &mut Frame<'_>) {
        let active = match self.mode {
            Mode::Browsing(tab) => Some(tab),
            _ => None,
        };
        let palette = self.session.palette;
        let help = self.help;
        let body = Chrome { session: &self.session, active }.render(frame);

        let keys = {
            let (screen, session) = parts!(self);
            screen.render(frame, body, session);
            screen.keys()
        };
        Chrome { session: &self.session, active }.render_footer(frame, keys);

        if help {
            chrome::help(frame, palette, keys);
        }
    }

    fn animating(&self) -> bool {
        self.session.scanning
            || match self.mode {
                Mode::Menu => self.menu.animating(&self.session),
                Mode::Scanning => self.scanning.animating(&self.session),
                Mode::Browsing(Tab::Overview) => self.overview.animating(&self.session),
                _ => false,
            }
    }
}

/// `tab` cycles, digits jump. A tab with nothing to show before a scan is skipped
/// rather than opened empty.
fn tab_key(key: KeyEvent, active: Tab, scanned: bool) -> Option<Tab> {
    let reachable = |tab: Tab| !tab.needs_outcome() || scanned;
    let index = Tab::ALL.iter().position(|tab| *tab == active).unwrap_or(0);
    let step = |delta: isize| {
        let count = Tab::ALL.len() as isize;
        let mut next = index as isize;
        for _ in 0..count {
            next = (next + delta).rem_euclid(count);
            if reachable(Tab::ALL[next as usize]) {
                return Some(Tab::ALL[next as usize]);
            }
        }
        None
    };
    match key.code {
        KeyCode::Tab | KeyCode::Right => step(1),
        KeyCode::BackTab | KeyCode::Left => step(-1),
        KeyCode::Char(digit @ '1'..='6') => {
            let wanted = Tab::ALL[digit as usize - '1' as usize];
            reachable(wanted).then_some(wanted)
        }
        _ => None,
    }
}

pub struct Launch<'a> {
    pub path: &'a Path,
    pub config: &'a RqxConfig,
    pub config_file: Option<String>,
    pub reduced_motion: bool,
}

/// One terminal, one panic hook, one restore. Everything the interface can do
/// happens inside the loop, so nothing is ever printed over a screen it has just
/// handed back.
pub fn launch(options: Launch<'_>) -> Result<Option<Chosen>, Decline> {
    install_panic_hook();
    let mut terminal =
        ratatui::try_init().map_err(|error| Decline::SetupFailed(error.to_string()))?;
    let outcome = run(&mut terminal, &options);
    ratatui::restore();
    outcome.map_err(|error| Decline::SetupFailed(error.to_string()))
}

fn run(terminal: &mut DefaultTerminal, options: &Launch<'_>) -> Result<Option<Chosen>> {
    let mut app = App {
        session: Session {
            palette: Palette::detect(),
            mascot: Sprite::parse(MASCOT),
            motion: Motion::new(options.reduced_motion),
            root: display_root(options.path),
            path: options.path.to_path_buf(),
            config: options.config.clone(),
            config_file: options.config_file.clone(),
            branch: crate::history::branch(options.path),
            outcome: None,
            progress: ScanProgress::default(),
            scanning: false,
            failure: None,
            base: None,
        },
        mode: Mode::Menu,
        menu: screens::menu::Menu::default(),
        scanning: screens::scanning::Scanning::default(),
        overview: screens::overview::Overview::default(),
        findings: screens::findings::Findings::default(),
        files: screens::files::Files::default(),
        rules: screens::catalogue::Rules::default(),
        coverage: screens::catalogue::CoverageView::default(),
        settings: screens::settings::Settings,
        updates: None,
        worker: None,
        help: false,
        dirty: true,
        quit: false,
        chosen: None,
    };

    while !app.quit {
        if app.dirty {
            terminal.draw(|frame| app.render(frame))?;
            app.dirty = false;
        }
        // A screen with nothing moving blocks on input rather than waking to discover
        // nothing changed, so a still interface costs nothing to leave open.
        let ready = if app.animating() { event::poll(motion::FRAME)? } else { true };
        if ready {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.handle(key),
                Event::Resize(..) => app.dirty = true,
                _ => {}
            }
        }
        app.dirty |= app.tick() || app.animating();
    }

    let _ = terminal.clear();
    Ok(app.chosen)
}

/// Installed once. Ratatui installs its own on every init, each wrapping the last, so
/// the old two-terminal design stacked them and a panic printed into a screen that
/// was still in raw mode.
fn install_panic_hook() {
    use std::sync::Once;

    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            ratatui::restore();
            eprintln!("\nrqx hit a bug and stopped.\n");
            previous(info);
            eprintln!(
                "\nPlease report this at https://github.com/jadhamwi/rqx/issues; \
                 run again with RUST_BACKTRACE=1 for more."
            );
        }));
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_working_directory_renders_as_a_dot_not_an_absolute_path() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(display_root(&cwd), ".");
        assert_eq!(display_root(Path::new(".")), ".");
    }

    #[test]
    fn tabs_skip_what_a_scan_has_not_produced_yet() {
        let key = |code| KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);
        // Overview, Findings and Files have nothing to show before a scan.
        assert_eq!(tab_key(key(KeyCode::Tab), Tab::Rules, false), Some(Tab::Coverage));
        assert_eq!(tab_key(key(KeyCode::Char('1')), Tab::Rules, false), None);
        assert_eq!(tab_key(key(KeyCode::Char('1')), Tab::Rules, true), Some(Tab::Overview));
    }

    #[test]
    fn tabs_wrap_in_both_directions_once_a_scan_exists() {
        let key = |code| KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);
        assert_eq!(tab_key(key(KeyCode::Tab), Tab::Config, true), Some(Tab::Overview));
        assert_eq!(tab_key(key(KeyCode::BackTab), Tab::Overview, true), Some(Tab::Config));
    }
}

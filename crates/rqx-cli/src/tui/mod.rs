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
use crate::check;
use crate::config::Resolved;
use crate::project::Project;
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
        crate::brand::Ink::Accent,
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
    ];

    // Padded rather than sized to the art, so a taller mascot does not silently lose
    // its bottom rows to a shorter `zip`.
    let beside = beside.into_iter().chain(std::iter::repeat(String::new()));
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

/// What a worker was asked for. A review is a scan of the whole project that reports
/// only the changed part, so it produces everything a scan does plus a verdict.
enum Work {
    Scan(ScanOutcome),
    Review(check::Review, check::Verdict),
}

type Worker = JoinHandle<anyhow::Result<Work>>;

/// Hands the project back with whether anything moved, because the project has to
/// live on the thread that re-analyses it and come home again afterwards.
type Refresher = JoinHandle<anyhow::Result<(Box<Project>, bool)>>;

/// Long enough not to `stat` the tree continuously, short enough that a save shows
/// up before you have looked away from it.
const BETWEEN_POLLS: std::time::Duration = std::time::Duration::from_millis(400);

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
    plan: screens::plan::Plan,
    rules: screens::catalogue::Rules,
    patterns: screens::patterns::Patterns,
    settings: screens::settings::Settings,
    updates: Option<Receiver<ScanProgress>>,
    worker: Option<Worker>,
    /// The project as last analysed, so a save costs one file rather than all of them.
    /// Built on the first refresh rather than at startup, because the first scan
    /// streams its progress and this one cannot.
    project: Option<Box<Project>>,
    refresher: Option<Refresher>,
    polled_at: std::time::Instant,
    /// The window once it is too small to lay out. Checked again on every resize
    /// rather than only before launch, because a split pane can shrink under it.
    cramped: Option<(u16, u16)>,
    help: bool,
    dirty: bool,
    quit: bool,
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
            Mode::Browsing(Tab::Plan) => &mut app.plan,
            Mode::Browsing(Tab::Patterns) => &mut app.patterns,
            Mode::Browsing(Tab::Rules) => &mut app.rules,
            Mode::Browsing(Tab::Config) => &mut app.settings,
        };
        (screen, &app.session)
    }};
}

impl App {
    fn start_scan(&mut self) {
        let (sender, updates) = mpsc::sync_channel(1);
        let (path, config) = (self.session.path.clone(), self.session.config.clone());
        self.begin(
            updates,
            thread::spawn(move || scan::scan_with_progress(&path, &config, sender).map(Work::Scan)),
        );
        self.session.base = None;
    }

    /// A review runs in the interface rather than handing the terminal back, so one
    /// menu choice is not the end of the session.
    fn start_review(&mut self, base: String) {
        let (sender, updates) = mpsc::sync_channel(1);
        let (path, config) = (self.session.path.clone(), self.session.config.clone());
        let against = base.clone();
        self.begin(
            updates,
            thread::spawn(move || {
                let review = check::review_watched(&path, &config, &against, Some(sender))?;
                let verdict = check::judge(&review, &config.config);
                Ok(Work::Review(review, verdict))
            }),
        );
        self.session.base = Some(base);
    }

    /// Re-analyses what changed and leaves the current report on screen while it
    /// runs, which is what makes it usable after every save rather than only on demand.
    fn start_refresh(&mut self) {
        if self.refresher.is_some() || self.session.scanning {
            return;
        }
        let taken = self.project.take();
        let (path, config) = (self.session.path.clone(), self.session.config.clone());
        self.session.refreshing = true;
        self.refresher = Some(thread::spawn(move || {
            let mut project = match taken {
                Some(project) => project,
                None => Box::new(Project::read(&path, config)?),
            };
            let seen = project.walk()?;
            let moved = project.refresh(&seen);
            Ok((project, moved))
        }));
    }

    /// Screens hold derived views of the outcome, so a new answer means a new set of
    /// them rather than a refresh flag on each.
    fn reset_screens(&mut self) {
        self.overview = screens::overview::Overview::default();
        self.findings = screens::findings::Findings::default();
        self.files = screens::files::Files::default();
        self.plan = screens::plan::Plan::default();
    }

    fn begin(&mut self, updates: Receiver<ScanProgress>, worker: Worker) {
        self.worker = Some(worker);
        self.updates = Some(updates);
        self.session.scanning = true;
        self.session.failure = None;
        self.session.progress = ScanProgress::default();
        self.session.outcome = None;
        self.session.reviewed = None;
        self.session.verdict = None;
        self.scanning = screens::scanning::Scanning::default();
        self.mode = Mode::Scanning;
        // A scan re-reads every file, so the map the refresh path keeps is stale the
        // moment one starts; rebuilding it lazily is cheaper than keeping it in step.
        self.project = None;
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
                Ok(Ok(work)) => {
                    match work {
                        Work::Scan(outcome) => self.session.outcome = Some(Arc::new(outcome)),
                        Work::Review(review, verdict) => {
                            self.session.reviewed = Some(Arc::new(review.reviewed));
                            self.session.verdict = Some(Arc::new(verdict));
                            self.session.outcome = Some(Arc::new(review.outcome));
                        }
                    }
                    self.reset_screens();
                    self.mode = Mode::Browsing(Tab::Overview);
                }
                Ok(Err(error)) => {
                    self.session.failure = Some(format!("{error:#}"));
                    self.mode = Mode::Menu;
                }
                Err(_) => {
                    self.session.failure = Some("the scan thread stopped unexpectedly".to_owned());
                    self.mode = Mode::Menu;
                }
            }
            changed = true;
        }
        if self.refresher.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(refresher) = self.refresher.take()
        {
            self.session.refreshing = false;
            match refresher.join() {
                Ok(Ok((project, moved))) => {
                    if moved || self.session.outcome.is_none() {
                        self.session.outcome = Some(project.outcome());
                        self.reset_screens();
                        if matches!(self.mode, Mode::Menu | Mode::Scanning) {
                            self.mode = Mode::Browsing(Tab::Overview);
                        }
                    }
                    self.project = Some(project);
                }
                Ok(Err(error)) => self.session.failure = Some(format!("{error:#}")),
                Err(_) => {
                    self.session.failure =
                        Some("the refresh thread stopped unexpectedly".to_owned());
                }
            }
            changed = true;
        }
        if self.session.watching
            && self.refresher.is_none()
            && !self.session.scanning
            && self.polled_at.elapsed() >= BETWEEN_POLLS
        {
            self.polled_at = std::time::Instant::now();
            self.start_refresh();
        }

        // Last, so a scan that has just landed is repainted on the frame it lands rather
        // than on the one after it.
        changed |= self.session.repaint_if_verdict_changed();
        changed
    }

    fn apply(&mut self, nav: Nav) {
        match nav {
            Nav::Stay => {}
            Nav::Redraw => self.dirty = true,
            Nav::Quit => self.quit = true,
            Nav::Scan => self.start_scan(),
            Nav::Refresh => self.start_refresh(),
            Nav::Watch => {
                self.session.watching = !self.session.watching;
                self.polled_at = std::time::Instant::now();
                self.dirty = true;
            }
            Nav::Review(base) => self.start_review(base),
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
            Nav::Inspect(file) => {
                self.findings.only(&file);
                self.mode = Mode::Browsing(Tab::Findings);
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

        let capturing = {
            let (screen, _) = parts!(self);
            screen.capturing()
        };
        if let Mode::Browsing(active) = self.mode
            && !capturing
            && let Some(next) = tab_key(key, active)
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
        if let Some((width, height)) = self.cramped {
            chrome::cramped(frame, self.session.palette, width, height);
            return;
        }
        let active = match self.mode {
            Mode::Browsing(tab) => Some(tab),
            _ => None,
        };
        let palette = self.session.palette;
        let help = self.help;
        let body = Chrome {
            session: &self.session,
            active,
        }
        .render(frame);

        let keys = {
            let (screen, session) = parts!(self);
            screen.render(frame, body, session);
            screen.keys()
        };
        Chrome {
            session: &self.session,
            active,
        }
        .render_footer(frame, keys);

        if help {
            chrome::help(frame, palette, keys);
        }
    }

    fn animating(&self) -> Option<std::time::Duration> {
        if self.session.scanning || self.session.repainting() {
            return Some(motion::FRAME);
        }
        // A refresh runs off-thread with the old report still on screen, and watching
        // has to wake to poll even with nothing on screen moving.
        if self.session.refreshing || self.session.watching {
            return Some(motion::IDLE);
        }
        let screen = match self.mode {
            Mode::Menu => self.menu.animating(&self.session),
            Mode::Scanning => self.scanning.animating(&self.session),
            Mode::Browsing(Tab::Overview) => self.overview.animating(&self.session),
            _ => None,
        };
        // The mascot blinks on every screen, not just the ones with a gauge on them, but
        // it is the only thing moving there and does not need the full frame rate.
        screen.or_else(|| (!self.session.motion.reduced()).then_some(motion::IDLE))
    }
}

/// `tab` cycles, digits jump. Every tab opens at any time: one that has nothing yet
/// says what it will hold and how to fill it, which is more use than refusing to open.
fn tab_key(key: KeyEvent, active: Tab) -> Option<Tab> {
    let index = Tab::ALL.iter().position(|tab| *tab == active).unwrap_or(0) as isize;
    let count = Tab::ALL.len() as isize;
    let step = |delta: isize| Some(Tab::ALL[(index + delta).rem_euclid(count) as usize]);
    match key.code {
        KeyCode::Tab | KeyCode::Right => step(1),
        KeyCode::BackTab | KeyCode::Left => step(-1),
        // The range follows `Tab::ALL`, so a tab added to the enum is reachable by
        // its digit without a second place to remember.
        KeyCode::Char(digit @ '1'..='9') if (digit as usize - '1' as usize) < Tab::ALL.len() => {
            Some(Tab::ALL[digit as usize - '1' as usize])
        }
        _ => None,
    }
}

pub struct Launch<'a> {
    pub path: &'a Path,
    pub config: &'a Resolved,
    pub reduced_motion: bool,
}

/// One terminal, one panic hook, one restore. Everything the interface can do
/// happens inside the loop, so nothing is ever printed over a screen it has just
/// handed back.
pub fn launch(options: Launch<'_>) -> Result<(), Decline> {
    install_panic_hook();
    let mut terminal =
        ratatui::try_init().map_err(|error| Decline::SetupFailed(error.to_string()))?;
    let outcome = run(&mut terminal, &options);
    ratatui::restore();
    let farewell = outcome.map_err(|error| Decline::SetupFailed(error.to_string()))?;
    print!("{farewell}");
    Ok(())
}

fn run(terminal: &mut DefaultTerminal, options: &Launch<'_>) -> Result<String> {
    let mut app = App {
        session: Session {
            palette: Palette::detect(),
            mascot: Sprite::parse(MASCOT),
            motion: Motion::new(options.reduced_motion),
            root: display_root(options.path),
            path: options.path.to_path_buf(),
            config_file: options.config.display_source(),
            config: options.config.clone(),
            branch: crate::history::branch(options.path),
            outcome: None,
            progress: ScanProgress::default(),
            scanning: false,
            refreshing: false,
            watching: false,
            failure: None,
            base: None,
            reviewed: None,
            verdict: None,
            painted: crate::brand::Ink::Accent,
            painted_at: std::time::Instant::now(),
        },
        mode: Mode::Menu,
        menu: screens::menu::Menu::default(),
        scanning: screens::scanning::Scanning::default(),
        overview: screens::overview::Overview::default(),
        findings: screens::findings::Findings::default(),
        files: screens::files::Files::default(),
        plan: screens::plan::Plan::default(),
        rules: screens::catalogue::Rules::default(),
        patterns: screens::patterns::Patterns::default(),
        settings: screens::settings::Settings::default(),
        updates: None,
        worker: None,
        project: None,
        refresher: None,
        polled_at: std::time::Instant::now(),
        cramped: None,
        help: false,
        dirty: true,
        quit: false,
    };

    while !app.quit {
        if app.dirty {
            terminal.draw(|frame| app.render(frame))?;
            app.dirty = false;
        }
        // A screen with nothing moving blocks on input rather than waking to discover
        // nothing changed, so a still interface costs nothing to leave open.
        let ready = match app.animating() {
            Some(interval) => event::poll(interval)?,
            None => true,
        };
        if ready {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.handle(key),
                Event::Resize(width, height) => {
                    app.cramped =
                        (width < MIN_WIDTH || height < MIN_HEIGHT).then_some((width, height));
                    app.dirty = true;
                }
                _ => {}
            }
        }
        app.dirty |= app.tick() || app.animating().is_some();
    }

    // Deliberately no `terminal.clear()`: leaving the alternate screen already puts
    // back what was there, and ratatui's clear asks the terminal where its cursor is
    // and blocks for the answer, which anything not emulating a terminal never sends.
    Ok(farewell(&app.session))
}

/// What the session amounted to, printed once the terminal is the shell's again. A
/// full-screen interface that vanishes leaves no trace of what it just told you, and
/// the command it names is the one that would print the whole thing.
fn farewell(session: &Session) -> String {
    let Some(outcome) = &session.outcome else {
        return String::new();
    };
    let health = session.health().unwrap_or(100);
    let subject = match &session.base {
        Some(base) => format!(
            "{} changed files against {base}",
            session.reviewed.as_ref().map_or(0, |files| files.len())
        ),
        None => format!("{} files", outcome.files_scanned),
    };
    let verdict = match &session.verdict {
        Some(verdict) if verdict.passed() => "  gate passed\n".to_owned(),
        Some(verdict) => format!("  gate failed: {}\n", verdict.failures.join("; ")),
        None => String::new(),
    };
    format!(
        "\n  rqx looked at {subject} \u{2014} health {health}/100, {} findings in {:.2}s\n{verdict}           `rqx scan {}` prints the whole report.\n\n",
        outcome.counts.total,
        outcome.elapsed_ms as f64 / 1000.0,
        session.root,
    )
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
                "\nPlease report this at https://github.com/jadhamwi21/rqx/issues; \
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

    /// The three project tabs used to be skipped before a scan, which left `tab`
    /// cycling three of six and the digits 1-3 doing nothing at all.
    #[test]
    fn every_tab_opens_before_a_scan_has_run() {
        let key = |code| KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);
        assert_eq!(tab_key(key(KeyCode::Tab), Tab::Rules), Some(Tab::Config));
        assert_eq!(
            tab_key(key(KeyCode::Char('1')), Tab::Rules),
            Some(Tab::Overview)
        );

        let mut seen = Vec::new();
        let mut active = Tab::Overview;
        for _ in 0..Tab::ALL.len() {
            seen.push(active);
            active = tab_key(key(KeyCode::Tab), active).expect("tab always moves");
        }
        assert_eq!(seen, Tab::ALL.to_vec());
        assert_eq!(active, Tab::Overview, "the cycle closes");
    }

    #[test]
    fn tabs_wrap_in_both_directions() {
        let key = |code| KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);
        assert_eq!(tab_key(key(KeyCode::Tab), Tab::Config), Some(Tab::Overview));
        assert_eq!(
            tab_key(key(KeyCode::BackTab), Tab::Overview),
            Some(Tab::Config)
        );
    }

    /// A filter that swallowed its own digits was the second half of the tab bug: the
    /// tab key was read before the screen composing text ever saw it.
    #[test]
    fn a_screen_composing_text_keeps_its_own_digits() {
        let key = KeyEvent::new(KeyCode::Char('1'), crossterm::event::KeyModifiers::NONE);
        let mut findings = screens::findings::Findings::default();
        assert!(!findings.capturing());
        findings.handle(
            KeyEvent::new(KeyCode::Char('/'), crossterm::event::KeyModifiers::NONE),
            &probe(),
        );
        assert!(findings.capturing(), "`/` starts a filter");
        assert_eq!(
            tab_key(key, Tab::Findings),
            Some(Tab::Overview),
            "the key still maps"
        );
    }

    fn probe() -> Session {
        Session::probe(Palette::detect())
    }
}

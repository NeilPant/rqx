pub mod app;
pub mod chrome;
pub mod motion;
pub mod palette;
pub mod screens;
pub mod sprite;

use std::fmt;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseEvent,
};
use crossterm::execute;
use ratatui::layout::Rect;
use ratatui::{DefaultTerminal, Frame};
use rqx_core::config::RqxConfig;

use crate::brand::Palette;
use crate::check;
use crate::config::Resolved;
use crate::project::Project;
use crate::scan::{self, ScanOutcome, ScanProgress};

use app::{Nav, Screen, Session, Tab};
use chrome::{Chrome, Hits, Landing};
use motion::Motion;
use palette::CommandPalette;
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

#[derive(Copy, Clone, PartialEq, Eq)]
enum Mode {
    Scanning,
    Browsing(Tab),
    /// One of the analyses that answers with text rather than a table.
    Reading,
}

struct App {
    session: Session,
    mode: Mode,
    scanning: screens::scanning::Scanning,
    overview: screens::overview::Overview,
    findings: screens::findings::Findings,
    files: screens::files::Files,
    complexity: screens::complexity::Complexity,
    graph: screens::graph::Graph,
    health: screens::health::Health,
    reader: screens::reader::Reader,
    plan: screens::plan::Plan,
    rules: screens::catalogue::Rules,
    patterns: screens::patterns::Patterns,
    settings: screens::settings::Settings,
    updates: Option<Receiver<ScanProgress>>,
    worker: Option<Worker>,
    /// The switch the running worker watches, kept so `esc` can throw it.
    cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// The project as last analysed, so a save costs one file rather than all of them.
    /// Built on the first refresh rather than at startup, because the first scan
    /// streams its progress and this one cannot.
    project: Option<Box<Project>>,
    refresher: Option<Refresher>,
    polled_at: std::time::Instant,
    /// The window once it is too small to lay out. Checked again on every resize
    /// rather than only before launch, because a split pane can shrink under it.
    cramped: Option<(u16, u16)>,
    /// What the command line asked to see. Held rather than applied once, because a
    /// rescan replaces every screen and the filter would go with it.
    filters: Filters,
    /// Where the last frame put the tab bar and the key hints, so a click can be
    /// answered without any of them being asked to remember their own geometry.
    hits: Hits,
    /// Where the screen itself was drawn, so a click can be told apart from one on
    /// the chrome without asking the screen.
    body: Rect,
    commands: Option<CommandPalette>,
    came_from: Vec<Mode>,
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
            Mode::Scanning => &mut app.scanning,
            Mode::Browsing(Tab::Overview) => &mut app.overview,
            Mode::Browsing(Tab::Findings) => &mut app.findings,
            Mode::Browsing(Tab::Files) => &mut app.files,
            Mode::Browsing(Tab::Complexity) => &mut app.complexity,
            Mode::Browsing(Tab::Graph) => &mut app.graph,
            Mode::Browsing(Tab::Health) => &mut app.health,
            Mode::Browsing(Tab::Plan) => &mut app.plan,
            Mode::Browsing(Tab::Patterns) => &mut app.patterns,
            Mode::Browsing(Tab::Rules) => &mut app.rules,
            Mode::Browsing(Tab::Config) => &mut app.settings,
            Mode::Reading => &mut app.reader,
        };
        (screen, &app.session)
    }};
}

impl App {
    fn start_scan(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let (sender, updates) = mpsc::sync_channel(1);
        let (path, config) = (self.session.path.clone(), self.session.config.clone());
        let watch = scan::Watch::new(sender);
        self.cancel = Some(Arc::clone(&watch.cancel));
        self.begin(
            updates,
            thread::spawn(move || scan::scan_with_progress(&path, &config, watch).map(Work::Scan)),
        );
        self.session.base = None;
    }

    /// Stops the running scan and throws away what it had, because a health score
    /// computed from half a project describes no project.
    fn cancel_scan(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// A review runs in the interface rather than handing the terminal back, so one
    /// menu choice is not the end of the session.
    fn start_review(&mut self, base: String) {
        if self.worker.is_some() {
            return;
        }
        let (sender, updates) = mpsc::sync_channel(1);
        let (path, config) = (self.session.path.clone(), self.session.config.clone());
        let against = base.clone();
        let watch = scan::Watch::new(sender);
        self.cancel = Some(Arc::clone(&watch.cancel));
        self.begin(
            updates,
            thread::spawn(move || {
                let review = check::review_watched(&path, &config, &against, Some(watch))?;
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
        self.findings = self.filters.clone().into_screen();
        self.files = screens::files::Files::default();
        self.complexity = screens::complexity::Complexity::default();
        self.graph = screens::graph::Graph::default();
        self.health = screens::health::Health::default();
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
            let stopped = self.cancel.take().is_some_and(|cancel| {
                cancel.load(std::sync::atomic::Ordering::Relaxed)
            });
            match worker.join() {
                // A cancelled scan is not an answer and not a failure: what was on
                // screen before it started stays there.
                _ if stopped => {}
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
                    // Only when the scan is still what is on screen. Leaving the
                    // scanning screen to read something else and being yanked back
                    // when the scan lands loses wherever you had got to.
                    if matches!(self.mode, Mode::Scanning) {
                        self.mode = Mode::Browsing(Tab::Overview);
                    }
                }
                // Stays on the scanning screen, which has always known how to draw a
                // failure and never had the chance: the mode used to change in the
                // same breath as the message was set.
                Ok(Err(error)) => self.session.failure = Some(format!("{error:#}")),
                Err(_) => {
                    self.session.failure = Some("the scan thread stopped unexpectedly".to_owned());
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
                        if matches!(self.mode, Mode::Scanning) {
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

    /// Each one writes and says what it wrote. None of them scan: the baseline
    /// records the findings on screen, which is the set the reader just looked at
    /// rather than a second one taken behind their back.
    fn run_task(&mut self, task: app::Task) -> anyhow::Result<String> {
        let root = &self.session.path;
        match task {
            app::Task::Baseline => {
                let Some(outcome) = &self.session.outcome else {
                    anyhow::bail!("nothing scanned yet \u{2014} press r first");
                };
                let path = crate::baseline::path_of(
                    root,
                    self.session.config.config.gate.baseline.as_deref(),
                );
                if path.exists() {
                    anyhow::bail!(
                        "{} already exists; `rqx baseline --update` takes it again",
                        path.display()
                    );
                }
                let taken = crate::baseline::Baseline::of(outcome);
                let recorded = taken.len();
                taken.write(&path)?;
                Ok(format!(
                    "recorded {recorded} findings in {}",
                    path.display()
                ))
            }
            app::Task::WriteConfig => {
                let path = root.join(rqx_core::config::CONFIG_FILE_NAME);
                if path.exists() {
                    anyhow::bail!("{} is already there", path.display());
                }
                std::fs::write(&path, rqx_core::config::TEMPLATE)?;
                Ok(format!(
                    "wrote {} \u{2014} every value is the default",
                    path.display()
                ))
            }
            app::Task::ClearCache => Ok(if crate::cache::clear(root)? {
                "the cache is gone; the next scan reads every file".to_owned()
            } else {
                "there was no cache to clear".to_owned()
            }),
        }
    }

    fn mouse(&mut self, event: MouseEvent) {
        if let Some(commands) = &mut self.commands {
            if let Some(chosen) = commands.click(event) {
                self.commands = None;
                if let Some(nav) = chosen {
                    self.apply(nav);
                }
            }
            self.dirty = true;
            return;
        }
        if self.help {
            self.help = false;
            self.dirty = true;
            return;
        }
        if app::is_press(event)
            && let Some(landing) = self.hits.at(event.column, event.row)
        {
            match landing {
                Landing::Tab(tab) => self.apply(Nav::To(tab)),
                // The hint names a key, so pressing it is exactly what clicking it
                // means; nothing has to be wired twice.
                Landing::Key(key) => {
                    if let Some(key) = key_named(key) {
                        self.handle(key);
                    }
                }
            }
            return;
        }
        // A wheel turn belongs to whatever is under the pointer, but a click outside
        // the body is the chrome's and was answered above.
        let inside = chrome::within(self.body, event.column, event.row);
        if !inside && app::is_press(event) {
            return;
        }
        let nav = {
            let (screen, session) = parts!(self);
            screen.click(event, session)
        };
        self.apply(nav);
    }

    /// Remembers where you were, so `esc` has somewhere to go. Bounded because a
    /// long session should not carry its whole history: a handful of steps back is
    /// what anyone reaches for, and the bottom of the stack leaves.
    fn go(&mut self, mode: Mode) {
        const REMEMBERED: usize = 16;
        if self.mode != mode {
            if self.came_from.len() == REMEMBERED {
                self.came_from.remove(0);
            }
            self.came_from.push(self.mode);
        }
        self.mode = mode;
        self.dirty = true;
    }

    fn apply(&mut self, nav: Nav) {
        match nav {
            Nav::Stay => {}
            Nav::Redraw => self.dirty = true,
            Nav::Quit => self.quit = true,
            Nav::Scan => self.start_scan(),
            Nav::Cancel => {
                self.cancel_scan();
                self.mode = Mode::Browsing(Tab::Overview);
                self.dirty = true;
            }
            // Answered by the palette, which asks for the revision before handing it
            // back as a review. Nothing else can act on it.
            Nav::Ask => {}
            Nav::Refresh => self.start_refresh(),
            Nav::Watch => {
                self.session.watching = !self.session.watching;
                self.polled_at = std::time::Instant::now();
                self.dirty = true;
            }
            Nav::Review(base) => self.start_review(base),
            Nav::To(tab) => self.go(Mode::Browsing(tab)),
            // Back through where you have been. There is no menu to land on any
            // more, so the bottom of the stack is the way out — which is what `esc`
            // means everywhere that has no parent screen.
            Nav::Back => match self.came_from.pop() {
                Some(mode) => {
                    self.mode = mode;
                    self.dirty = true;
                }
                None => self.quit = true,
            },
            Nav::Inspect(file) => {
                self.findings.only(&file);
                self.go(Mode::Browsing(Tab::Findings));
            }
            Nav::Explain(slug) => {
                self.rules.only(&slug);
                self.go(Mode::Browsing(Tab::Rules));
            }
            Nav::Read(page) => {
                self.reader.show(page, &self.session);
                self.go(Mode::Reading);
            }
            Nav::Do(task) => {
                self.session.notice = Some(match self.run_task(task) {
                    Ok(said) => said,
                    Err(error) => format!("{error:#}"),
                });
                self.dirty = true;
            }
        }
    }

    fn handle(&mut self, key: KeyEvent) {
        // Any key clears it, so the answer to "what did that do" is on screen until
        // the reader has moved on and never after.
        if self.session.notice.take().is_some() {
            self.dirty = true;
        }
        if let Some(commands) = &mut self.commands {
            if let Some(chosen) = commands.handle(key, &self.session) {
                self.commands = None;
                if let Some(nav) = chosen {
                    self.apply(nav);
                }
            }
            self.dirty = true;
            return;
        }
        if self.help {
            self.help = false;
            self.dirty = true;
            return;
        }
        // Asked before anything is intercepted: a screen taking text owns every
        // printable key, and `?` typed into a filter used to open the help overlay
        // and then eat the next keystroke closing it again.
        let capturing = {
            let (screen, _) = parts!(self);
            screen.capturing()
        };
        if key.code == KeyCode::Char('?') && !capturing {
            self.help = true;
            self.dirty = true;
            return;
        }
        // `ctrl-p` carries a modifier no text field wants, so it reaches the palette
        // from anywhere; a bare colon belongs in a search box first.
        let asked = (key.code == KeyCode::Char('p')
            && key.modifiers.contains(KeyModifiers::CONTROL))
            || (key.code == KeyCode::Char(':') && !capturing);
        if asked {
            let mut commands = CommandPalette::default();
            commands.open(&self.session);
            self.commands = Some(commands);
            self.dirty = true;
            return;
        }

        if let Mode::Browsing(active) = self.mode
            && !capturing
            && let Some(next) = tab_key(key, active)
        {
            self.go(Mode::Browsing(next));
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
        let (body, mut hits) = Chrome {
            session: &self.session,
            active,
        }
        .render(frame);

        let keys = {
            let (screen, session) = parts!(self);
            screen.render(frame, body, session);
            screen.keys()
        };
        hits.keys = Chrome {
            session: &self.session,
            active,
        }
        .render_footer(frame, keys);
        self.body = body;
        self.hits = hits;

        if let Some(commands) = &mut self.commands {
            commands.render(frame, palette);
            return;
        }
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
            Mode::Scanning => self.scanning.animating(&self.session),
            Mode::Browsing(Tab::Overview) => self.overview.animating(&self.session),
            _ => None,
        };
        // The mascot blinks on every screen, not just the ones with a gauge on them, but
        // it is the only thing moving there and does not need the full frame rate.
        screen.or_else(|| (!self.session.motion.reduced()).then_some(motion::IDLE))
    }
}

/// The key a footer hint names, so clicking the hint and pressing the key are the
/// same event. Only the shapes the hints actually use; `↑↓` and the like are a
/// label for a pair of keys rather than one, and clicking them means nothing.
fn key_named(label: &str) -> Option<KeyEvent> {
    let code = match label {
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Char(' '),
        "ctrl-p" => return Some(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
        _ => {
            let mut characters = label.chars();
            let only = characters.next()?;
            if characters.next().is_some() || !only.is_ascii_graphic() {
                return None;
            }
            KeyCode::Char(only)
        }
    };
    Some(KeyEvent::new(code, KeyModifiers::NONE))
}

/// `tab` cycles, digits jump. Every tab opens at any time: one that has nothing yet
/// says what it will hold and how to fill it, which is more use than refusing to open.
fn tab_key(key: KeyEvent, active: Tab) -> Option<Tab> {
    let count = Tab::ALL.len() as isize;
    let on_strip = Tab::ALL.iter().position(|tab| *tab == active);
    let index = on_strip.unwrap_or(0) as isize;
    // A tab that is not on the strip — Config, reached from the menu or the palette —
    // has no neighbour, so stepping from it re-enters the strip at its start rather
    // than at whichever end `unwrap_or` happened to pick.
    let step = |delta: isize| {
        Some(match on_strip {
            Some(_) => Tab::ALL[(index + delta).rem_euclid(count) as usize],
            None => Tab::ALL[0],
        })
    };
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
    pub mouse: bool,
    /// `--rule`, `--category` and `--path` as the command line gave them, so a
    /// filtered scan opens filtered.
    pub filters: Filters,
}

/// The three list filters `rqx scan` accepts, comma-joined the way the interface's
/// own filter form holds them.
#[derive(Clone, Default)]
pub struct Filters {
    pub rules: String,
    pub families: String,
    pub paths: String,
}

impl Filters {
    fn into_screen(self) -> screens::findings::Findings {
        screens::findings::Findings::given(self.rules, self.families, self.paths)
    }
}

/// Whether mouse capture is on, so the panic hook can undo exactly what setup did.
/// `ratatui::restore` knows nothing about the mouse, and a process that panics with
/// capture still on leaves the shell reading escape sequences as keystrokes.
static MOUSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn restore() {
    if MOUSE.swap(false, std::sync::atomic::Ordering::SeqCst) {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
}

/// Raw mode, the alternate screen and mouse capture go on together and come off
/// together. A guard rather than a pair of calls, because an early return, a normal
/// exit and a panic all have to undo the same list and three copies of it drift.
struct Screenful {
    inner: DefaultTerminal,
}

impl Screenful {
    fn open(mouse: bool) -> std::io::Result<Self> {
        let inner = ratatui::try_init()?;
        if mouse {
            execute!(std::io::stdout(), EnableMouseCapture)?;
            MOUSE.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(Self { inner })
    }
}

impl Drop for Screenful {
    fn drop(&mut self) {
        restore();
    }
}

/// One terminal, one panic hook, one restore. Everything the interface can do
/// happens inside the loop, so nothing is ever printed over a screen it has just
/// handed back.
pub fn launch(options: Launch<'_>) -> Result<(), Decline> {
    // After `try_init`, not before: ratatui installs a hook of its own on every init
    // and each wraps the last, so ours has to go on top to disable the mouse first.
    let mut terminal =
        Screenful::open(options.mouse).map_err(|error| Decline::SetupFailed(error.to_string()))?;
    install_panic_hook();
    let outcome = run(&mut terminal.inner, &options);
    drop(terminal);
    let farewell = outcome.map_err(|error| Decline::SetupFailed(error.to_string()))?;
    print!("{farewell}");
    Ok(())
}

fn run(terminal: &mut DefaultTerminal, options: &Launch<'_>) -> Result<String> {
    let filters = options.filters.clone();
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
            notice: None,
            base: None,
            reviewed: None,
            verdict: None,
            painted: crate::brand::Ink::Accent,
            painted_at: std::time::Instant::now(),
        },
        mode: Mode::Scanning,
        scanning: screens::scanning::Scanning::default(),
        overview: screens::overview::Overview::default(),
        findings: filters.clone().into_screen(),
        files: screens::files::Files::default(),
        complexity: screens::complexity::Complexity::default(),
        graph: screens::graph::Graph::default(),
        health: screens::health::Health::default(),
        reader: screens::reader::Reader::default(),
        plan: screens::plan::Plan::default(),
        rules: screens::catalogue::Rules::default(),
        patterns: screens::patterns::Patterns::default(),
        settings: screens::settings::Settings::default(),
        updates: None,
        worker: None,
        cancel: None,
        project: None,
        refresher: None,
        polled_at: std::time::Instant::now(),
        cramped: None,
        filters: filters.clone(),
        hits: Hits::default(),
        body: Rect::default(),
        commands: None,
        came_from: Vec::new(),
        help: false,
        dirty: true,
        quit: false,
    };

    // Scanning starts before the first frame. Landing on a menu and choosing "scan"
    // cost three keypresses and showed nothing of the project until they were spent.
    app.start_scan();

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
                Event::Mouse(mouse) => app.mouse(mouse),
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
            restore();
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
        let last = *Tab::ALL.last().expect("a strip");
        assert_eq!(tab_key(key(KeyCode::Tab), last), Some(Tab::Overview));
        assert_eq!(tab_key(key(KeyCode::BackTab), Tab::Overview), Some(last));
    }

    /// Config is reachable but not on the strip, so it has no neighbour to step to.
    #[test]
    fn a_tab_off_the_strip_steps_back_onto_its_start() {
        let key = |code| KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);
        assert_eq!(tab_key(key(KeyCode::Tab), Tab::Config), Some(Tab::Overview));
        assert_eq!(
            tab_key(key(KeyCode::BackTab), Tab::Config),
            Some(Tab::Overview)
        );
    }

    /// The digits reach the strip, so the strip has to fit in them.
    #[test]
    fn every_tab_on_the_strip_has_a_digit() {
        assert!(Tab::ALL.len() <= 9, "{} tabs", Tab::ALL.len());
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

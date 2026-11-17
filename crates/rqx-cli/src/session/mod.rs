//! The inline session that bare `rqx` opens.
//!
//! It scans the working directory on start, prints the result into the terminal's own
//! scrollback, and keeps a few rows at the bottom for the prompt. Nothing takes over
//! the screen, so what it printed is still there after it exits.

mod ansi;
mod commands;
pub mod fuzzy;
mod job;
pub mod motion;
pub mod sprite;
pub mod text;
mod view;
mod viewport;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::text::Line;

use crate::brand::{self, Ink, Palette};
use crate::config::Resolved;
use crate::scan::{self, ScanOutcome, ScanProgress};
use crate::term::{self, Decline};

use commands::{Action, Context, Invocation, Parsed};
use job::{Finished, Job};
use motion::Motion;
use sprite::Sprite;
use view::{IDLE_ROWS, Idle, Look, SCANNING_ROWS};
use viewport::Viewport;

const MASCOT: &str = include_str!("../../assets/mascot.txt");
const MIN_WIDTH: u16 = 50;
const MIN_HEIGHT: u16 = 14;
/// How long the loop may sleep when nothing is moving and no job is running.
const QUIET: Duration = Duration::from_secs(3600);
/// A second ctrl-c this soon after the first leaves the session.
const INTERRUPT_AGAIN: Duration = Duration::from_secs(1);

/// Checked before any terminal state is touched, for the reasons `term::interactive`
/// gives. The session needs less room than a full screen: a dozen rows and enough
/// columns for the header box.
pub fn gate() -> Result<(), Decline> {
    term::interactive((MIN_WIDTH, MIN_HEIGHT))
}

pub struct Launch<'a> {
    pub path: &'a Path,
    pub config: &'a Resolved,
    /// The level the scan runs under, which a command can compare its own against.
    pub level: Option<&'a str>,
    pub reduced_motion: bool,
}

/// Runs the session to its end. An `Err` means the terminal could not host it, and the
/// caller prints the plain report instead.
pub fn launch(options: Launch<'_>) -> Result<(), Decline> {
    let setup = |error: &dyn std::fmt::Display| Decline::SetupFailed(error.to_string());
    let viewport = Viewport::open(SCANNING_ROWS).map_err(|error| setup(&error))?;
    term::install_panic_hook(viewport::restore);
    let farewell = App::new(options, viewport)
        .run()
        .map_err(|error| setup(&error))?;
    print!("{farewell}");
    Ok(())
}

/// What a background job hands back.
enum Done {
    Scanned(Box<ScanOutcome>),
    Printed { bytes: Vec<u8>, rescan: bool },
}

/// What the viewport says the session is doing while a job runs.
enum Busy {
    Scanning,
    Running(String),
}

struct App {
    viewport: Viewport,
    look: Look,
    motion: Motion,
    sprite: Sprite,
    root: String,
    path: PathBuf,
    config: Resolved,
    level: Option<String>,
    outcome: Option<Arc<ScanOutcome>>,
    progress: ScanProgress,
    job: Option<(Job<Done>, Busy)>,
    announced: bool,
    input: String,
    last_interrupt: Option<Instant>,
    quit: bool,
}

impl App {
    fn new(options: Launch<'_>, viewport: Viewport) -> Self {
        Self {
            viewport,
            look: Look {
                palette: Palette::detect(),
            },
            motion: Motion::new(options.reduced_motion),
            sprite: Sprite::parse(MASCOT),
            root: display_root(options.path),
            path: options.path.to_path_buf(),
            config: options.config.clone(),
            level: options.level.map(str::to_owned),
            outcome: None,
            progress: ScanProgress::default(),
            job: None,
            announced: false,
            input: String::new(),
            last_interrupt: None,
            quit: false,
        }
    }

    fn run(mut self) -> Result<String> {
        self.start_scan();
        while !self.quit {
            self.fit_viewport()?;
            self.draw()?;
            if event::poll(self.wakeup())?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle(key)?;
            }
            self.tick()?;
        }
        let farewell = self.farewell();
        self.viewport.close()?;
        Ok(farewell)
    }

    /// A scan reads the project with the display cap off: the analysis commands judge
    /// the whole set, and the cap only ever shortened a list the session pages anyway.
    fn start_scan(&mut self) {
        let (path, mut config) = (self.path.clone(), self.config.clone());
        config.config.scan.max_findings = 0;
        self.progress = ScanProgress::default();
        let job = Job::start(move |watch| {
            scan::scan_with_progress(&path, &config, watch)
                .map(|outcome| Done::Scanned(Box::new(outcome)))
        });
        self.job = Some((job, Busy::Scanning));
    }

    fn health(&self) -> Option<u32> {
        self.outcome.as_ref().map(|outcome| {
            outcome
                .counts
                .weighted_health_score(outcome.code_lines, &outcome.impact)
        })
    }

    /// The plumage follows the verdict once there is one.
    fn signal(&self) -> Ink {
        if self.job.is_some() {
            return Ink::Accent;
        }
        brand::mascot_ink(self.health())
    }

    fn owl(&self) -> Vec<Line<'static>> {
        self.sprite.render_frame(
            self.look.palette,
            self.sprite.frame("idle"),
            self.signal(),
            usize::MAX,
        )
    }

    fn wanted_rows(&self) -> u16 {
        if self.job.is_some() {
            SCANNING_ROWS
        } else {
            IDLE_ROWS
        }
    }

    fn fit_viewport(&mut self) -> Result<()> {
        self.viewport.resize(self.wanted_rows())?;
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        let look = &self.look;
        if let Some((_, busy)) = &self.job {
            let owl = self.owl();
            let room = view::text_width(&owl, self.viewport.width());
            let spinner = self.motion.spinner();
            let text = match busy {
                Busy::Scanning => view::progress_lines(look, spinner, &self.progress, room),
                Busy::Running(doing) => view::busy_lines(look, spinner, doing),
            };
            self.viewport
                .draw(|frame| view::working(frame, look, owl, text))?;
            return Ok(());
        }
        let status = self.status();
        let state = Idle {
            input: &self.input,
            cursor: self.input.chars().count(),
            hints: "/help  /findings  /plan  /fix  /rescan  /exit",
            status: &status,
        };
        self.viewport
            .draw(|frame| view::idle(frame, look, &state))?;
        Ok(())
    }

    fn status(&self) -> String {
        match (&self.outcome, self.health()) {
            (Some(outcome), Some(health)) => format!(
                "health {health} \u{b7} {} findings",
                text::thousands(outcome.counts.total)
            ),
            _ => String::new(),
        }
    }

    fn wakeup(&self) -> Duration {
        if self.job.is_some() {
            motion::FRAME
        } else if self.motion.reduced() {
            QUIET
        } else {
            motion::IDLE
        }
    }

    fn tick(&mut self) -> Result<()> {
        let Some((job, _)) = &self.job else {
            return Ok(());
        };
        if let Some(progress) = job.progress() {
            self.progress = progress;
        }
        if !job.finished() {
            return Ok(());
        }
        let Some((job, _)) = self.job.take() else {
            return Ok(());
        };
        match job.join() {
            Finished::Done(Done::Scanned(outcome)) => self.adopt(*outcome)?,
            Finished::Done(Done::Printed { bytes, rescan }) => {
                self.print_output(&bytes)?;
                if rescan {
                    self.start_scan();
                }
            }
            Finished::Cancelled => self.say("Cancelled.")?,
            Finished::Failed(message) => self.fail(&message)?,
        }
        Ok(())
    }

    /// The scan is what the session knows from here on, so the header (the first time)
    /// and the worst findings go into scrollback the moment it lands.
    fn adopt(&mut self, outcome: ScanOutcome) -> Result<()> {
        let outcome = Arc::new(outcome);
        self.outcome = Some(Arc::clone(&outcome));
        let mut lines = Vec::new();
        if !self.announced {
            lines.extend(self.header());
            self.announced = true;
        }
        lines.extend(view::summary(&self.look, &outcome));
        self.viewport.print(&lines)?;
        Ok(())
    }

    fn header(&self) -> Vec<Line<'static>> {
        let (Some(outcome), Some(health)) = (&self.outcome, self.health()) else {
            return Vec::new();
        };
        view::header(
            &self.look,
            &self.owl(),
            outcome,
            &self.root,
            health,
            self.viewport.width(),
        )
    }

    fn say(&mut self, message: &str) -> Result<()> {
        self.viewport.print(&[view::note(&self.look, message)])?;
        Ok(())
    }

    /// The first line is the failure and the rest is what to do about it.
    fn fail(&mut self, message: &str) -> Result<()> {
        let mut lines = message.lines();
        let mut printed = vec![view::failure(&self.look, lines.next().unwrap_or("failed"))];
        printed.extend(lines.map(|line| view::note(&self.look, line)));
        self.viewport.print(&printed)?;
        Ok(())
    }

    fn print_output(&mut self, bytes: &[u8]) -> Result<()> {
        let lines = ansi::lines(&String::from_utf8_lossy(bytes), self.viewport.width());
        self.viewport.print(&lines)?;
        Ok(())
    }

    fn handle(&mut self, key: KeyEvent) -> Result<()> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('d') if control => self.quit = true,
            KeyCode::Char('c') if control => self.interrupt(),
            KeyCode::Char(character) if !control => self.input.push(character),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter if self.job.is_none() => self.submit()?,
            _ => {}
        }
        Ok(())
    }

    /// Stops a running job; otherwise clears the line, and leaves if that is the second
    /// time in a second.
    fn interrupt(&mut self) {
        if let Some((job, _)) = &self.job {
            job.cancel();
            return;
        }
        let again = self
            .last_interrupt
            .is_some_and(|at| at.elapsed() < INTERRUPT_AGAIN);
        if again {
            self.quit = true;
        }
        self.input.clear();
        self.last_interrupt = Some(Instant::now());
    }

    fn submit(&mut self) -> Result<()> {
        let line = std::mem::take(&mut self.input);
        let parsed = commands::parse(&line);
        if matches!(parsed, Ok(Parsed::Nothing)) {
            return Ok(());
        }
        self.viewport
            .print(&[view::echo(&self.look, line.trim())])?;
        match parsed {
            Ok(parsed) => self.dispatch(parsed),
            Err(message) => self.fail(&message),
        }
    }

    fn dispatch(&mut self, parsed: Parsed) -> Result<()> {
        match parsed {
            Parsed::Nothing => Ok(()),
            Parsed::Exit => {
                self.quit = true;
                Ok(())
            }
            Parsed::Rescan => {
                self.start_scan();
                Ok(())
            }
            Parsed::Help => {
                let lines = view::help(&self.look, &commands::help_rows(), self.viewport.width());
                self.viewport.print(&lines)?;
                Ok(())
            }
            Parsed::Clear => {
                self.viewport.clear_screen()?;
                let header = self.header();
                self.viewport.print(&header)?;
                Ok(())
            }
            Parsed::Watch => {
                self.say("Watching for changes is not built yet; /rescan scans again.")
            }
            Parsed::Printed(bytes) => self.print_output(&bytes),
            Parsed::Command(invocation) => self.run_command(*invocation),
        }
    }

    fn run_command(&mut self, invocation: Invocation) -> Result<()> {
        let doing = format!("running /{}", invocation.name);
        let context = Context {
            outcome: self.outcome.as_deref(),
            settings: &self.config,
            palette: self.look.palette,
            level: self.level.as_deref(),
        };
        match commands::run(invocation, &context) {
            Ok(Action::Print(bytes)) => self.print_output(&bytes),
            Ok(Action::Background { work, rescan }) => {
                let job = Job::start(move |_| work().map(|bytes| Done::Printed { bytes, rescan }));
                self.job = Some((job, Busy::Running(doing)));
                Ok(())
            }
            Err(error) => self.fail(&format!("{error:#}")),
        }
    }

    fn farewell(&self) -> String {
        match (&self.outcome, self.health()) {
            (Some(outcome), Some(health)) => view::farewell(outcome, health),
            _ => String::new(),
        }
    }
}

/// The directory the session is looking at, as a person would write it: relative to
/// their home when it is under it.
fn display_root(path: &Path) -> String {
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.and_then(|home| absolute.strip_prefix(&home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => absolute.display().to_string(),
    }
}

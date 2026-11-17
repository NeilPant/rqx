//! The inline session that bare `rqx` opens.
//!
//! It scans the working directory on start, prints the result into the terminal's own
//! scrollback, and keeps a few rows at the bottom for the prompt. Nothing takes over
//! the screen, so what it printed is still there after it exits.

pub mod fuzzy;
mod job;
pub mod motion;
pub mod sprite;
pub mod text;
mod view;
mod viewport;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::text::Line;

use crate::brand::{self, Ink, Palette};
use crate::config::Resolved;
use crate::scan::{self, ScanOutcome, ScanProgress};
use crate::term::{self, Decline};

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

/// Checked before any terminal state is touched, for the reasons `term::interactive`
/// gives. The session needs less room than a full screen: a dozen rows and enough
/// columns for the header box.
pub fn gate() -> Result<(), Decline> {
    term::interactive((MIN_WIDTH, MIN_HEIGHT))
}

pub struct Launch<'a> {
    pub path: &'a Path,
    pub config: &'a Resolved,
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
    Scanned(ScanOutcome),
}

struct App {
    viewport: Viewport,
    look: Look,
    motion: Motion,
    sprite: Sprite,
    root: String,
    path: PathBuf,
    config: Resolved,
    outcome: Option<Arc<ScanOutcome>>,
    progress: ScanProgress,
    job: Option<Job<Done>>,
    input: String,
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
            outcome: None,
            progress: ScanProgress::default(),
            job: None,
            input: String::new(),
            quit: false,
        }
    }

    fn run(mut self) -> Result<String> {
        self.start_scan();
        while !self.quit {
            self.fit_viewport()?;
            self.draw()?;
            if event::poll(self.wakeup())? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.handle(key),
                    _ => {}
                }
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
        self.job = Some(Job::start(move |watch| {
            scan::scan_with_progress(&path, &config, watch).map(Done::Scanned)
        }));
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
        if self.job.is_some() {
            let (owl, spinner) = (self.owl(), self.motion.spinner());
            let progress = &self.progress;
            self.viewport
                .draw(|frame| view::scanning(frame, look, owl, spinner, progress))?;
            return Ok(());
        }
        let status = self.status();
        let state = Idle {
            input: &self.input,
            cursor: self.input.chars().count(),
            hints: "/exit leaves",
            status: &status,
        };
        self.viewport
            .draw(|frame| view::idle(frame, look, &state))?;
        Ok(())
    }

    fn status(&self) -> String {
        match (&self.outcome, self.health()) {
            (Some(outcome), Some(health)) => {
                format!("health {health} \u{b7} {} findings", outcome.counts.total)
            }
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
        let Some(job) = &self.job else {
            return Ok(());
        };
        if let Some(progress) = job.progress() {
            self.progress = progress;
        }
        if !job.finished() {
            return Ok(());
        }
        let Some(job) = self.job.take() else {
            return Ok(());
        };
        match job.join() {
            Finished::Done(Done::Scanned(outcome)) => self.adopt(outcome)?,
            Finished::Cancelled => self.say("Scan cancelled.")?,
            Finished::Failed(message) => self.say(&format!("The scan failed: {message}"))?,
        }
        Ok(())
    }

    /// The scan is what the session knows from here on, so the header and the first
    /// findings go into scrollback the moment it lands.
    fn adopt(&mut self, outcome: ScanOutcome) -> Result<()> {
        let outcome = Arc::new(outcome);
        self.outcome = Some(Arc::clone(&outcome));
        let health = self.health().unwrap_or(100);
        let mut lines = view::header(
            &self.look,
            &self.owl(),
            &outcome,
            &self.root,
            health,
            self.viewport.width(),
        );
        lines.extend(view::summary(&self.look, &outcome));
        self.viewport.print(&lines)?;
        Ok(())
    }

    fn say(&mut self, message: &str) -> Result<()> {
        self.viewport
            .print(&[Line::styled(format!(" {message}"), self.look.palette.dim())])?;
        Ok(())
    }

    fn handle(&mut self, key: KeyEvent) {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('d') if control => self.quit = true,
            KeyCode::Char('c') if control => {
                if let Some(job) = &self.job {
                    job.cancel();
                } else {
                    self.input.clear();
                }
            }
            KeyCode::Char(character) if !control => self.input.push(character),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter => self.submit(),
            _ => {}
        }
    }

    fn submit(&mut self) {
        let line = std::mem::take(&mut self.input);
        if matches!(line.trim(), "/exit" | "/quit") {
            self.quit = true;
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

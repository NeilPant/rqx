//! The inline session that bare `rqx` opens.
//!
//! It scans the working directory on start, prints the result into the terminal's own
//! scrollback, and keeps a few rows at the bottom for the prompt. Nothing takes over
//! the screen, so what it printed is still there after it exits.

mod ansi;
mod commands;
mod completion;
mod delta;
mod editor;
mod fuzzy;
mod job;
mod motion;
mod owl;
mod prompt;
mod recall;
pub(crate) mod sprite;
mod text;
mod view;
mod viewport;
mod watching;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::text::Line;

use rqx_core::Cleanliness;

use crate::brand::{self, Ink, Palette};
use crate::config::Resolved;
use crate::project::Project;
use crate::scan::{self, ScanOutcome, ScanProgress};
use crate::term::{self, Decline};

use commands::{Action, Context, Invocation, Parsed, Target, WatchMode};
use completion::{Catalogue, Suggestion};
use job::{Finished, Job};
use motion::Motion;
use owl::Owl;
use prompt::Prompt;
use recall::Recall;
use sprite::Sprite;
use view::{Idle, Look, Menu, MenuRow, SCANNING_ROWS};
use viewport::Viewport;
use watching::{Change, Watcher};

const MASCOT: &str = include_str!("../../assets/mascot-compact.txt");
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
    Watching(Box<Project>),
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
    /// The running job's own clock, which the owl's reading loop starts from.
    job_clock: Option<Motion>,
    owl: Owl,
    root: String,
    path: PathBuf,
    config: Resolved,
    level: Option<String>,
    outcome: Option<Arc<ScanOutcome>>,
    progress: ScanProgress,
    job: Option<(Job<Done>, Busy)>,
    announced: bool,
    watcher: Option<Watcher>,
    /// Where the findings the summary listed are, which `/open 2` counts in.
    listing: Vec<(String, u32)>,
    /// A waiver that is waiting for its reason: the next line typed is the reason.
    asking: Option<crate::waive::Order>,
    prompt: Prompt,
    recall: Recall,
    catalogue: Catalogue,
    menu: MenuState,
    last_interrupt: Option<Instant>,
    quit: bool,
}

/// What the completion menu holds for the line as it is now.
#[derive(Default)]
struct MenuState {
    suggestions: Vec<Suggestion>,
    selected: usize,
    /// Closed with esc, or hidden because the line came from the history. It stays
    /// closed through further typing until a `-` or Tab asks for it, or the line is
    /// emptied or sent.
    dismissed: bool,
    /// Where the word is that Tab asked the flags for, so the menu keeps offering them
    /// while that word is typed and not for the next one.
    flags_from: Option<usize>,
}

impl MenuState {
    /// What Tab opens when no menu is showing: the command's flags, kept on offer while
    /// the word they start is typed.
    fn flags_asked_for(catalogue: &Catalogue, before: &str) -> Self {
        Self {
            suggestions: catalogue.suggest_with_flags(before),
            selected: 0,
            dismissed: false,
            flags_from: Some(completion::word_start(before)),
        }
    }

    /// The menu after typing into the line it was built for. A menu closed with esc
    /// stays closed through the typing, and `reopens` (a `-`, or an emptied line) opens
    /// it again.
    fn after_edit(&self, catalogue: &Catalogue, before: &str, reopens: bool) -> Self {
        let flags_from = self
            .flags_from
            .filter(|from| *from == completion::word_start(before));
        Self {
            suggestions: if flags_from.is_some() {
                catalogue.suggest_with_flags(before)
            } else {
                catalogue.suggest(before)
            },
            selected: 0,
            dismissed: self.dismissed && !reopens,
            flags_from,
        }
    }

    fn visible(&self) -> bool {
        !self.dismissed && !self.suggestions.is_empty()
    }

    fn chosen(&self) -> Option<&Suggestion> {
        self.visible()
            .then(|| self.suggestions.get(self.selected))
            .flatten()
    }

    fn step(&mut self, forward: bool) {
        let count = self.suggestions.len();
        self.selected = if forward {
            (self.selected + 1) % count
        } else {
            (self.selected + count - 1) % count
        };
    }
}

impl App {
    fn new(options: Launch<'_>, viewport: Viewport) -> Self {
        let recall = Recall::open();
        Self {
            viewport,
            look: Look {
                palette: Palette::detect(),
            },
            motion: Motion::new(options.reduced_motion),
            job_clock: None,
            owl: Owl::new(Sprite::parse(MASCOT)),
            root: display_root(options.path),
            path: options.path.to_path_buf(),
            config: options.config.clone(),
            level: options.level.map(str::to_owned),
            outcome: None,
            progress: ScanProgress::default(),
            job: None,
            announced: false,
            watcher: None,
            listing: Vec::new(),
            asking: None,
            prompt: Prompt::with_history(recall.load()),
            recall,
            catalogue: Catalogue::new([]),
            menu: MenuState::default(),
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

    /// The analysis commands judge the whole set, and the display cap only ever
    /// shortened a list the session pages anyway, so the session reads with it off.
    fn uncapped_config(&self) -> Resolved {
        let mut config = self.config.clone();
        config.config.scan.max_findings = 0;
        config
    }

    fn start_scan(&mut self) {
        let (path, config) = (self.path.clone(), self.uncapped_config());
        self.progress = ScanProgress::default();
        let job = Job::start(move |watch| {
            scan::scan_costed(&path, &config, Some(watch))
                .map(|outcome| Done::Scanned(Box::new(outcome)))
        });
        self.job = Some((job, Busy::Scanning));
    }

    fn health(&self) -> Option<u32> {
        self.outcome.as_deref().map(health_of)
    }

    /// The plumage follows the verdict once there is one.
    fn signal(&self) -> Ink {
        if self.job.is_some() {
            return Ink::Accent;
        }
        brand::mascot_ink(self.health())
    }

    fn wanted_rows(&self) -> u16 {
        if self.job.is_some() {
            SCANNING_ROWS
        } else {
            view::idle_rows(self.menu.visible())
        }
    }

    fn fit_viewport(&mut self) -> Result<()> {
        self.viewport.resize(self.wanted_rows())?;
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        let look = &self.look;
        if let Some((_, busy)) = &self.job {
            let signal = self.signal();
            let reduced = self.motion.reduced();
            let clock = self.job_clock.get_or_insert_with(|| Motion::new(reduced));
            let owl = self.owl.glancing(clock, look.palette, signal);
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
        let input = self.prompt.text();
        let rows: Vec<MenuRow<'_>> = self
            .menu
            .suggestions
            .iter()
            .map(|suggestion| MenuRow {
                label: &suggestion.label,
                about: &suggestion.about,
            })
            .collect();
        let state = Idle {
            input: &input,
            cursor: self.prompt.cursor(),
            menu: self.menu.visible().then_some(Menu {
                rows: &rows,
                selected: self.menu.selected,
            }),
            status: &status,
        };
        self.viewport
            .draw(|frame| view::idle(frame, look, &state))?;
        Ok(())
    }

    fn status(&self) -> String {
        let (Some(outcome), Some(health)) = (&self.outcome, self.health()) else {
            return String::new();
        };
        let mut parts = vec![
            format!("health {health}"),
            Cleanliness::of(health, outcome.counts.total)
                .label()
                .to_owned(),
            format!("{} findings", text::thousands(outcome.counts.total)),
        ];
        if self.watcher.is_some() {
            parts.push("watching".to_owned());
        }
        parts.join(" \u{b7} ")
    }

    fn wakeup(&self) -> Duration {
        let watching = self.watcher.as_ref().map_or(QUIET, Watcher::wakeup);
        let own = if self.job.is_some() {
            motion::FRAME
        } else {
            QUIET
        };
        own.min(watching)
    }

    fn tick(&mut self) -> Result<()> {
        self.tick_job()?;
        self.tick_watcher()
    }

    fn tick_watcher(&mut self) -> Result<()> {
        let Some(watcher) = &mut self.watcher else {
            return Ok(());
        };
        match watcher.advance() {
            Ok(Some(change)) => self.apply(change),
            Ok(None) => Ok(()),
            Err(message) => {
                self.watcher = None;
                self.fail(&format!("watching stopped: {message}"))
            }
        }
    }

    /// The project as it is now replaces the scan the commands read, and what moved
    /// goes into scrollback.
    fn apply(&mut self, change: Change) -> Result<()> {
        let lines: Vec<Line<'static>> = delta::describe(&change.before, &change.after)
            .iter()
            .map(|text| view::update(&self.look, text))
            .collect();
        self.outcome = Some(Arc::clone(&change.after));
        self.catalogue = Catalogue::new(change.after.analyzed.iter().cloned());
        self.refresh_menu();
        self.viewport.print(&lines)?;
        Ok(())
    }

    fn tick_job(&mut self) -> Result<()> {
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
        self.job_clock = None;
        match job.join() {
            Finished::Done(Done::Scanned(outcome)) => self.adopt(*outcome)?,
            Finished::Done(Done::Watching(project)) => {
                self.watcher = Some(Watcher::new(project));
                self.say("Watching for changes. /watch off stops.")?;
            }
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
        self.catalogue = Catalogue::new(outcome.analyzed.iter().cloned());
        self.listing = view::listed_places(&outcome);
        self.refresh_menu();
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
            &self.owl.still(self.look.palette, self.signal()),
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
            _ if self.job.is_some() => {}
            KeyCode::Esc if self.asking.is_some() => self.cancel_asking()?,
            KeyCode::Char('?')
                if !control && self.asking.is_none() && self.prompt.text().is_empty() =>
            {
                self.viewport.print(&view::shortcuts(&self.look))?;
            }
            KeyCode::Enter => self.enter()?,
            KeyCode::Tab => self.tab(),
            KeyCode::Esc => self.menu.dismissed = true,
            KeyCode::Up => self.up(),
            KeyCode::Down => self.down(),
            _ => {
                if self.prompt.edit(key) {
                    self.refresh_menu_after_edit(key);
                }
            }
        }
        Ok(())
    }

    fn refresh_menu(&mut self) {
        let suggestions = if self.asking.is_some() {
            Vec::new()
        } else {
            self.catalogue.suggest(&self.prompt.before_cursor())
        };
        self.menu = MenuState {
            suggestions,
            selected: 0,
            dismissed: false,
            flags_from: None,
        };
    }

    /// The menu for a line that was just typed into. Unlike a fresh line it stays
    /// closed if esc closed it, until a `-` is typed.
    fn refresh_menu_after_edit(&mut self, key: KeyEvent) {
        if self.asking.is_some() {
            return self.refresh_menu();
        }
        let reopens = key.code == KeyCode::Char('-') || self.prompt.text().is_empty();
        self.menu = self
            .menu
            .after_edit(&self.catalogue, &self.prompt.before_cursor(), reopens);
    }

    /// Takes the highlighted suggestion into the line. With no menu open, opens the
    /// flags of the command being typed, which nothing else offers until a `-`.
    fn tab(&mut self) {
        if self.menu.chosen().is_some() {
            self.accept();
        } else if self.asking.is_none() {
            self.menu = MenuState::flags_asked_for(&self.catalogue, &self.prompt.before_cursor());
        }
    }

    /// Takes the highlighted suggestion into the line.
    fn accept(&mut self) {
        let Some(chosen) = self.menu.chosen().cloned() else {
            return;
        };
        self.prompt
            .replace_before_cursor(chosen.from, &chosen.insert);
        self.refresh_menu();
    }

    /// With the menu open the arrows move through it; otherwise they walk the history,
    /// which hides the menu so the next arrow keeps walking it.
    fn up(&mut self) {
        if self.menu.visible() {
            self.menu.step(false);
        } else {
            self.prompt.recall_older();
            self.menu.dismissed = true;
        }
    }

    fn down(&mut self) {
        if self.menu.visible() {
            self.menu.step(true);
        } else {
            self.prompt.recall_newer();
            self.menu.dismissed = true;
        }
    }

    /// Enter completes what the menu is pointing at, unless the line already says it,
    /// and only then runs the line.
    fn enter(&mut self) -> Result<()> {
        if self
            .menu
            .chosen()
            .is_some_and(|chosen| !chosen.changes_nothing())
        {
            self.accept();
            return Ok(());
        }
        self.submit()
    }

    /// Stops a running job; otherwise clears the line, and leaves if that is the second
    /// time in a second.
    fn interrupt(&mut self) {
        if let Some((job, _)) = &self.job {
            job.cancel();
            return;
        }
        if self.asking.is_some() {
            let _ = self.cancel_asking();
            return;
        }
        let again = self
            .last_interrupt
            .is_some_and(|at| at.elapsed() < INTERRUPT_AGAIN);
        if again {
            self.quit = true;
        }
        self.prompt.clear();
        self.refresh_menu();
        self.last_interrupt = Some(Instant::now());
    }

    fn cancel_asking(&mut self) -> Result<()> {
        self.asking = None;
        self.prompt.clear();
        self.refresh_menu();
        self.say("Cancelled.")
    }

    /// The line is the reason for the waiver that was waiting for one.
    fn submit_reason(&mut self, order: crate::waive::Order) -> Result<()> {
        let reason = self.prompt.submit();
        self.refresh_menu();
        if reason.trim().is_empty() {
            self.asking = Some(order);
            return self.fail("a waiver needs a reason; esc cancels");
        }
        self.viewport
            .print(&[view::echo(&self.look, reason.trim())])?;
        let order = crate::waive::Order {
            reason: reason.trim().to_owned(),
            ..order
        };
        self.perform(commands::waiving(order), "running /waive".to_owned())
    }

    fn submit(&mut self) -> Result<()> {
        if let Some(order) = self.asking.take() {
            return self.submit_reason(order);
        }
        let line = self.prompt.submit();
        self.refresh_menu();
        let parsed = commands::parse(&line);
        if matches!(parsed, Ok(Parsed::Nothing)) {
            return Ok(());
        }
        self.recall.append(&line);
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
            Parsed::Watch(mode) => self.watch(mode),
            Parsed::Open(target) => self.open(target),
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
            Ok(action) => self.perform(action, doing),
            Err(error) => self.fail(&format!("{error:#}")),
        }
    }

    fn perform(&mut self, action: Action, doing: String) -> Result<()> {
        match action {
            Action::Print(bytes) => self.print_output(&bytes),
            Action::AskReason(order) => {
                self.say(&format!(
                    "Why waive `{}` at {}? Type the reason and press enter; esc cancels.",
                    order.rule, order.at
                ))?;
                self.asking = Some(order);
                self.refresh_menu();
                Ok(())
            }
            Action::Background { work, rescan } => {
                let job = Job::start(move |_| work().map(|bytes| Done::Printed { bytes, rescan }));
                self.job = Some((job, Busy::Running(doing)));
                Ok(())
            }
        }
    }

    fn watch(&mut self, mode: WatchMode) -> Result<()> {
        let on = match mode {
            WatchMode::On => true,
            WatchMode::Off => false,
            WatchMode::Toggle => self.watcher.is_none(),
        };
        if !on {
            let was = self.watcher.take().is_some();
            return self.say(if was {
                "Stopped watching."
            } else {
                "Not watching."
            });
        }
        if self.watcher.is_some() {
            return self.say("Already watching.");
        }
        let (path, config) = (self.path.clone(), self.uncapped_config());
        let job = Job::start(move |_| {
            Project::read(&path, config).map(|project| Done::Watching(Box::new(project)))
        });
        self.job = Some((job, Busy::Running("starting /watch".to_owned())));
        Ok(())
    }

    /// Hands the place to the editor and takes the terminal back when it exits. With
    /// no editor configured the place is printed, which is still enough to act on.
    fn open(&mut self, target: Target) -> Result<()> {
        let (file, line) = match target {
            Target::Place { file, line } => (file, line),
            Target::Listed(number) => match self.listing.get(number - 1) {
                Some((file, line)) => (file.clone(), *line),
                None => {
                    return self.fail(&format!(
                        "there is no finding {number} in the list; it holds {}",
                        self.listing.len()
                    ));
                }
            },
        };
        let Some(editor) = editor::configured() else {
            return self.say(&format!(
                "$VISUAL and $EDITOR are not set; the finding is at {file}:{line}"
            ));
        };
        let on_disk = self.path.join(&file);
        let Some(launch) = editor::launch(&editor, &on_disk.to_string_lossy(), line) else {
            return self.fail("$VISUAL or $EDITOR is set but names no program");
        };
        self.viewport.suspend()?;
        let ran = std::process::Command::new(&launch.program)
            .args(&launch.arguments)
            .status();
        self.viewport.resume()?;
        match ran {
            Ok(_) => Ok(()),
            Err(error) => self.fail(&format!("could not start {}: {error}", launch.program)),
        }
    }

    fn farewell(&self) -> String {
        match (&self.outcome, self.health()) {
            (Some(outcome), Some(health)) => view::farewell(outcome, health),
            _ => String::new(),
        }
    }
}

/// The score the header, the status line and a watch's delta all quote.
fn health_of(outcome: &ScanOutcome) -> u32 {
    outcome
        .counts
        .weighted_health_score(outcome.code_lines, &outcome.impact)
}

/// The directory the session is looking at, as a person would write it: relative to
/// their home when it is under it.
fn display_root(path: &Path) -> String {
    let absolute = rqx_core::paths::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.and_then(|home| absolute.strip_prefix(&home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => absolute.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(count: usize) -> MenuState {
        MenuState {
            suggestions: (0..count)
                .map(|at| Suggestion {
                    label: format!("/c{at}"),
                    about: String::new(),
                    from: 0,
                    typed: String::new(),
                    insert: format!("/c{at} "),
                })
                .collect(),
            selected: 0,
            dismissed: false,
            flags_from: None,
        }
    }

    #[test]
    fn the_choice_wraps_in_both_directions() {
        let mut menu = menu(3);
        menu.step(false);
        assert_eq!(menu.selected, 2);
        menu.step(true);
        assert_eq!(menu.selected, 0);
        menu.step(true);
        assert_eq!(menu.chosen().map(|s| s.label.as_str()), Some("/c1"));
    }

    fn catalogue() -> Catalogue {
        Catalogue::new([])
    }

    #[test]
    fn a_menu_closed_with_esc_stays_closed_until_a_dash() {
        let catalogue = catalogue();
        let mut menu = MenuState::default().after_edit(&catalogue, "/plan", false);
        assert!(menu.visible());
        menu.dismissed = true;
        let typing = menu.after_edit(&catalogue, "/plan ", false);
        assert!(!typing.visible());
        let typing = typing.after_edit(&catalogue, "/plan b", false);
        assert!(!typing.visible());
        let dashed = typing.after_edit(&catalogue, "/plan b-", true);
        assert!(!dashed.dismissed);
    }

    #[test]
    fn esc_then_a_dash_offers_the_flags_again() {
        let catalogue = catalogue();
        let mut menu = MenuState::default().after_edit(&catalogue, "/plan ", false);
        menu.dismissed = true;
        let dashed = menu.after_edit(&catalogue, "/plan -", true);
        assert!(dashed.visible());
        assert!(dashed.chosen().is_some_and(|s| s.label.starts_with("--")));
    }

    #[test]
    fn an_emptied_line_opens_the_menu_again() {
        let catalogue = catalogue();
        let mut menu = MenuState::default().after_edit(&catalogue, "/pl", false);
        menu.dismissed = true;
        let emptied = menu.after_edit(&catalogue, "", true);
        assert!(!emptied.dismissed);
        let slash = emptied.after_edit(&catalogue, "/", false);
        assert!(slash.visible());
    }

    #[test]
    fn tab_opens_the_flags_and_keeps_them_while_their_word_is_typed() {
        let catalogue = catalogue();
        let asked = MenuState::flags_asked_for(&catalogue, "/plan ");
        assert!(asked.visible());
        let typing = asked.after_edit(&catalogue, "/plan bud", false);
        assert_eq!(typing.chosen().map(|s| s.label.as_str()), Some("--budget"));
        let next_word = typing.after_edit(&catalogue, "/plan bud ", false);
        assert!(!next_word.visible());
    }

    #[test]
    fn a_dismissed_or_empty_menu_offers_nothing() {
        let mut shown = menu(2);
        assert!(shown.visible());
        shown.dismissed = true;
        assert!(!shown.visible());
        assert!(shown.chosen().is_none());
        assert!(!menu(0).visible());
    }
}

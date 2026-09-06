use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::brand::{Ink, Palette};
use crate::check::Verdict;
use crate::config::Resolved;
use crate::scan::{ScanOutcome, ScanProgress};

use super::motion::Motion;
use super::sprite::Sprite;

/// What a screen asks the app to do next. Returning this rather than mutating a
/// `should_quit` flag is what lets every screen be left without leaving the program.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Nav {
    /// Nothing changed; do not redraw.
    Stay,
    Redraw,
    Back,
    To(Tab),
    Scan,
    /// Re-analyse only what changed on disk. What a fix, a waiver or a save asks for:
    /// the answer is the same as a scan's, and reaching it costs one file.
    Refresh,
    /// Start or stop re-analysing whenever a file changes.
    Watch,
    /// Chosen, but not yet runnable: a review needs a revision, and the palette asks
    /// for one before it hands this back as a `Review`.
    Ask,
    Review(String),
    /// Show one file's findings: the Files screen's answer to "what is wrong here".
    Inspect(String),
    /// Open one rule in the catalogue.
    Explain(String),
    /// Run an analysis whose answer is prose, and show what it printed.
    Read(Page),
    /// Write something to disk, and say what happened.
    Do(Task),
    Quit,
}

/// The commands that change a file rather than answer a question. Kept apart from
/// the rest because each one needs its result said out loud: a menu entry that
/// silently wrote to disk would be the wrong kind of quiet.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Task {
    Baseline,
    WriteConfig,
    ClearCache,
}

impl Task {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Baseline => "Record a baseline",
            Self::WriteConfig => "Write RQX.toml",
            Self::ClearCache => "Clear the cache",
        }
    }

    pub const fn about(self) -> &'static str {
        match self {
            Self::Baseline => "the debt there is now, so a gate judges only what a change adds",
            Self::WriteConfig => "every setting at its default, commented, for you to edit",
            Self::ClearCache => "throw away what the last scans remembered",
        }
    }
}

/// An analysis whose answer is a page of text rather than a table to move around in.
/// Every one of them already renders to a `Write`, so the interface asks for the same
/// answer the command line prints rather than building a second one.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Page {
    Recommend,
    Suppressions,
    Techniques,
    Levels,
}

impl Page {
    pub const ALL: [Self; 4] = [
        Self::Recommend,
        Self::Suppressions,
        Self::Techniques,
        Self::Levels,
    ];

    /// The command's own name, which is what the palette matches on.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Recommend => "recommend",
            Self::Suppressions => "suppressions",
            Self::Techniques => "techniques",
            Self::Levels => "levels",
        }
    }

    pub const fn about(self) -> &'static str {
        match self {
            Self::Recommend => "what to do about this project, not what is wrong with it",
            Self::Suppressions => "every finding a comment waives, and why",
            Self::Techniques => "every refactoring rqx prescribes",
            Self::Levels => "what each cleanliness level changes",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    Findings,
    Files,
    Complexity,
    Graph,
    Health,
    Plan,
    Patterns,
    Rules,
    /// Off the strip: the settings are read once and the palette reaches them, so a
    /// tenth tab would cost every other tab a digit for something looked at once.
    Config,
}

impl Tab {
    /// Nine, because the digits reach nine. A tenth would need a second way to jump,
    /// and the palette is that way already — which is why Config left the strip.
    pub const ALL: [Self; 9] = [
        Self::Overview,
        Self::Findings,
        Self::Files,
        Self::Complexity,
        Self::Graph,
        Self::Health,
        Self::Plan,
        Self::Patterns,
        Self::Rules,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Findings => "Findings",
            Self::Files => "Files",
            Self::Complexity => "Complexity",
            Self::Graph => "Graph",
            Self::Health => "Health",
            Self::Plan => "Plan",
            Self::Patterns => "Patterns",
            Self::Rules => "Rules",
            Self::Config => "Config",
        }
    }

    /// Whether the tab is about the project rather than about the tool. The three that
    /// are stand empty until a scan has run, and say so.
    pub const fn needs_outcome(self) -> bool {
        !matches!(self, Self::Rules | Self::Config)
    }

    /// One line of what the tab will hold, for the state before a scan.
    pub const fn promise(self) -> &'static str {
        match self {
            Self::Overview => "the health score, the severity split and where to start",
            Self::Findings => "every finding, with the code it is about",
            Self::Files => "one row per file, worst first",
            Self::Complexity => "every function it measured, worst first",
            Self::Graph => "what imports what, and every cycle in it",
            Self::Health => "where the score comes from, family by family",
            Self::Plan => "the work ranked by what it buys against what it costs",
            Self::Patterns => "which design patterns are in use, broken, or would fit",
            Self::Rules => "the rule catalogue",
            Self::Config => "the settings this run will use",
        }
    }
}

/// What the run is, as opposed to what any one screen is showing. Built once and
/// borrowed per frame, so the palette and the sprite are parsed once rather than at
/// every level that happens to need them.
pub struct Session {
    pub palette: Palette,
    pub mascot: Sprite,
    pub motion: Motion,
    pub root: String,
    pub path: PathBuf,
    pub config: Resolved,
    /// Where the settings came from, for the header. `None` means the defaults.
    pub config_file: Option<String>,
    pub branch: Option<String>,
    pub outcome: Option<Arc<ScanOutcome>>,
    pub progress: ScanProgress,
    pub scanning: bool,
    /// A refresh keeps the last report on screen while it runs, so this is separate
    /// from `scanning`, which replaces it.
    pub refreshing: bool,
    pub watching: bool,
    pub failure: Option<String>,
    /// What the last thing that wrote to disk did. Shown until the next one runs.
    pub notice: Option<String>,
    /// What the last scan was: a whole project, or a diff against a revision.
    pub base: Option<String>,
    /// The files a review covers. `None` means the run was a whole-project scan and
    /// every screen shows everything.
    pub reviewed: Option<Arc<BTreeSet<String>>>,
    pub verdict: Option<Arc<Verdict>>,
    /// The verdict colour the mascot was last painted in, and when. The wipe runs from
    /// that moment, so a new verdict arrives by being painted rather than by appearing.
    pub painted: Ink,
    pub painted_at: Instant,
}

/// How long the mascot takes to wipe in. Short enough to be over before anyone reads the
/// header, long enough that the colour registers as having changed.
const PAINT: Duration = Duration::from_millis(250);

impl Session {
    /// Whether a file belongs to what is on screen. A review scans the whole project
    /// so the import graph stays honest, then reports only the part that changed.
    pub fn in_scope(&self, file: &str) -> bool {
        self.reviewed
            .as_ref()
            .is_none_or(|reviewed| reviewed.contains(file))
    }

    pub fn health(&self) -> Option<u32> {
        self.outcome.as_ref().map(|outcome| {
            outcome
                .counts
                .weighted_health_score(outcome.code_lines, &outcome.impact)
        })
    }

    /// The whole body carries the verdict rather than the eyes, because three eye
    /// pixels are below the size most terminals render legibly.
    pub fn signal(&self) -> Ink {
        if self.failure.is_some() {
            return Ink::Danger;
        }
        if self.scanning {
            return Ink::Accent;
        }
        crate::brand::mascot_ink(self.health())
    }

    /// A blink is the only thing that moves. The spinner and the sweeping gauge already
    /// say a scan is running, and a mascot that marched as well said it twice.
    pub fn mascot_frame(&self) -> usize {
        let blinking = self.motion.phase(Duration::from_millis(5200)) > 0.94;
        self.mascot.frame(if blinking { "blink" } else { "idle" })
    }

    /// How many columns of the mascot to draw. With motion off `eased` is already
    /// finished, so this is the whole width on the first frame and there is no branch
    /// for it here.
    pub fn reveal(&self) -> usize {
        let progress = self.motion.eased(self.painted_at, PAINT);
        ((progress * f32::from(self.mascot.width())).ceil() as usize).max(1)
    }

    pub fn repainting(&self) -> bool {
        !self.motion.finished(self.painted_at, PAINT)
    }

    /// Restarts the wipe when the verdict colour changes. Driven from the loop rather
    /// than from each field `signal` reads, because missing one of them would leave the
    /// mascot changing colour in a single frame.
    pub fn repaint_if_verdict_changed(&mut self) -> bool {
        let signal = self.signal();
        if signal == self.painted {
            return false;
        }
        self.painted = signal;
        self.painted_at = Instant::now();
        true
    }
}

#[cfg(test)]
impl Session {
    /// A session with nothing in it, for the screens' own rendering tests.
    pub fn probe(palette: Palette) -> Self {
        Self {
            palette,
            mascot: Sprite::parse(include_str!("../../assets/mascot.txt")),
            motion: Motion::new(true),
            root: "src".to_owned(),
            path: PathBuf::from("."),
            config: Resolved::default(),
            config_file: None,
            branch: Some("main".to_owned()),
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
            painted: Ink::Accent,
            painted_at: Instant::now(),
        }
    }
}

pub trait Screen {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session);
    fn handle(&mut self, key: KeyEvent, session: &Session) -> Nav;
    /// A click, a drag or a wheel turn inside the screen's own area. Defaulted, so a
    /// screen that has nothing to click keeps compiling and simply ignores the mouse.
    /// The screen remembers where it drew itself; nothing else knows its geometry.
    fn click(&mut self, _event: MouseEvent, _session: &Session) -> Nav {
        Nav::Stay
    }
    /// How often this screen wants waking. `None` costs no wakeups at all: the loop
    /// blocks on input until something arrives.
    fn animating(&self, _session: &Session) -> Option<Duration> {
        None
    }
    /// Whether the screen is composing text. Tab switching is held back while it is,
    /// so a filter can contain a digit without the view changing underneath it.
    fn capturing(&self) -> bool {
        false
    }
    fn keys(&self) -> &'static [(&'static str, &'static str)];
}

/// Which row a click landed on, in a list drawn at `area` whose first `above` lines
/// are border and header rather than rows. `offset` is the row currently at the top,
/// which is what makes this work on a list that has been scrolled.
pub fn row_at(area: Rect, above: u16, offset: usize, event: MouseEvent) -> Option<usize> {
    let first = area.y.checked_add(above)?;
    let inside = event.column >= area.x
        && event.column < area.x.saturating_add(area.width)
        && event.row >= first
        && event.row < area.y.saturating_add(area.height);
    inside.then(|| offset + usize::from(event.row - first))
}

/// How far a wheel turn moves a list. Three lines is what a terminal reports one
/// notch as, and matching it is what makes the wheel feel like the wheel.
pub const WHEEL: isize = 3;

/// The wheel, as a number of rows to move. `None` for anything that is not a turn.
pub const fn wheel(event: MouseEvent) -> Option<isize> {
    match event.kind {
        MouseEventKind::ScrollDown => Some(WHEEL),
        MouseEventKind::ScrollUp => Some(-WHEEL),
        _ => None,
    }
}

pub const fn is_press(event: MouseEvent) -> bool {
    matches!(event.kind, MouseEventKind::Down(_))
}

/// `q` and `ctrl-c` leave from anywhere, `esc` steps back one level, and `r` runs a
/// scan so a tab standing empty can say how to fill it. Screens that take typed input
/// handle their own keys first and never see these.
pub fn global(key: KeyEvent) -> Option<Nav> {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Nav::Quit),
        KeyCode::Char('q') => Some(Nav::Quit),
        KeyCode::Char('r') => Some(Nav::Scan),
        KeyCode::Char('w') => Some(Nav::Watch),
        KeyCode::Esc => Some(Nav::Back),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::ColorDepth;

    fn awake() -> Session {
        let mut session = Session::probe(Palette::with_depth(ColorDepth::Truecolor));
        session.motion = Motion::new(false);
        session
    }

    /// The wipe is what makes a verdict arrive rather than appear, so it has to restart
    /// on the colour and not on the scan: a scan that lands on the same band is not news.
    #[test]
    fn the_mascot_repaints_only_when_the_verdict_colour_changes() {
        let mut session = awake();
        assert!(!session.repaint_if_verdict_changed(), "nothing changed yet");

        session.failure = Some("the scan thread stopped unexpectedly".to_owned());
        assert!(
            session.repaint_if_verdict_changed(),
            "a failure did not repaint"
        );
        assert!(session.repainting(), "the wipe is not running");
        assert!(
            !session.repaint_if_verdict_changed(),
            "the same verdict repainted twice"
        );

        session.failure = None;
        assert!(
            session.repaint_if_verdict_changed(),
            "clearing a failure did not repaint"
        );
    }

    /// A wipe that started long ago is over, and one that never started covers the whole
    /// sprite: neither may leave the mascot half drawn.
    #[test]
    fn a_finished_wipe_draws_every_column() {
        let mut session = awake();
        session.painted_at = Instant::now() - PAINT * 2;
        assert!(!session.repainting());
        assert!(session.reveal() >= session.mascot.width() as usize);
    }

    /// With motion off there is nothing to watch arrive, so the mascot is whole on the
    /// first frame rather than wiping in at a speed nobody asked for.
    #[test]
    fn reduced_motion_paints_the_whole_mascot_at_once() {
        let session = Session::probe(Palette::with_depth(ColorDepth::Truecolor));
        assert!(session.motion.reduced());
        assert!(session.reveal() >= session.mascot.width() as usize);
        assert!(!session.repainting());
    }
}

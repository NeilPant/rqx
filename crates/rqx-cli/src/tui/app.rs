use std::path::PathBuf;
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use rqx_core::config::RqxConfig;

use crate::brand::{Ink, Palette};
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
    Review,
    Quit,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    Findings,
    Files,
    Rules,
    Coverage,
    Config,
}

impl Tab {
    pub const ALL: [Self; 6] =
        [Self::Overview, Self::Findings, Self::Files, Self::Rules, Self::Coverage, Self::Config];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Findings => "Findings",
            Self::Files => "Files",
            Self::Rules => "Rules",
            Self::Coverage => "Coverage",
            Self::Config => "Config",
        }
    }

    /// Whether the tab has anything to show before a scan has run. The catalogue
    /// views are about the tool, not about the project, so they always do.
    pub const fn needs_outcome(self) -> bool {
        matches!(self, Self::Overview | Self::Findings | Self::Files)
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
    pub config: RqxConfig,
    /// Where the settings came from, for the header. `None` means the defaults.
    pub config_file: Option<String>,
    pub branch: Option<String>,
    pub outcome: Option<Arc<ScanOutcome>>,
    pub progress: ScanProgress,
    pub scanning: bool,
    pub failure: Option<String>,
    /// What the last scan was: a whole project, or a diff against a revision.
    pub base: Option<String>,
}

impl Session {
    pub fn rules_enabled(&self) -> usize {
        rqx_core::rule::RuleId::ALL
            .iter()
            .filter(|rule| self.config.rules.enabled(**rule))
            .count()
    }

    pub fn health(&self) -> Option<u32> {
        self.outcome
            .as_ref()
            .map(|outcome| outcome.counts.weighted_health_score(outcome.code_lines, &outcome.impact))
    }

    /// The mascot's eyes carry the verdict, so the art is never decoration: before a
    /// scan it is its own colour, during one it pulses, afterwards it is the grade.
    pub fn signal(&self) -> Ink {
        if self.scanning {
            return Ink::Accent;
        }
        if self.failure.is_some() {
            return Ink::Danger;
        }
        match self.health() {
            None => Ink::Signal,
            Some(80..=100) => Ink::Good,
            Some(50..=79) => Ink::Warn,
            Some(_) => Ink::Danger,
        }
    }

    pub fn mascot_frame(&self) -> usize {
        let named = |name: &str| self.mascot.frame(name);
        if self.scanning {
            // Eyes track across the file list while the worker is alive.
            return match (self.motion.phase(std::time::Duration::from_millis(900)) * 4.0) as u32 {
                0 | 2 => named("idle"),
                1 => named("look-left"),
                _ => named("look-right"),
            };
        }
        match self.health() {
            Some(80..=100) => named("pleased"),
            Some(0..=49) => named("alert"),
            // A slow blink, so a still screen is not a frozen one.
            _ if self.motion.phase(std::time::Duration::from_millis(4200)) > 0.96 => named("blink"),
            _ => named("idle"),
        }
    }
}

pub trait Screen {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session);
    fn handle(&mut self, key: KeyEvent, session: &Session) -> Nav;
    /// Whether this screen wants a clock. A screen that says no costs no wakeups.
    fn animating(&self, _session: &Session) -> bool {
        false
    }
    fn keys(&self) -> &'static [(&'static str, &'static str)];
}

/// `q` and `ctrl-c` leave from anywhere, `esc` steps back one level. Screens that
/// take typed input handle their own keys first and never see these.
pub fn global(key: KeyEvent) -> Option<Nav> {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Nav::Quit),
        KeyCode::Char('q') => Some(Nav::Quit),
        KeyCode::Esc => Some(Nav::Back),
        _ => None,
    }
}

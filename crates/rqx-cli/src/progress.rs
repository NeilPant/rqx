use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::brand::{Ink, Palette, paint};

/// Paced by the clock rather than by file count: 500 files is 20ms on a tree of small
/// modules and several seconds on a tree of large ones, so a fixed count either flickers
/// or looks frozen.
const REPAINT: Duration = Duration::from_millis(80);

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const FILLED: char = '█';
const EMPTY: char = '░';
const BAR: usize = 22;

/// What the scan is doing. The walk is the only phase with a count to divide, but the
/// four that follow it are the ones that used to run with the screen already wiped, so a
/// large repository ended every scan looking as though it had hung.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    Counting,
    #[default]
    Scanning,
    Duplicates,
    Imports,
    History,
    CoChange,
}

impl Phase {
    /// In the order a scan passes through them, which is what lets a reader see
    /// where it is rather than only what it is doing.
    pub const ALL: [Self; 6] = [
        Self::Counting,
        Self::Scanning,
        Self::Duplicates,
        Self::Imports,
        Self::History,
        Self::CoChange,
    ];

    /// Two words at most: the stepper shows all six at once.
    pub const fn short(self) -> &'static str {
        match self {
            Self::Counting => "finding",
            Self::Scanning => "scanning",
            Self::Duplicates => "duplicates",
            Self::Imports => "imports",
            Self::History => "history",
            Self::CoChange => "co-change",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Counting => "finding files",
            Self::Scanning => "scanning",
            Self::Duplicates => "grouping duplicates",
            Self::Imports => "resolving imports",
            Self::History => "reading history",
            Self::CoChange => "comparing changes",
        }
    }
}

/// Set once from `--motion`, and read wherever a bar is built. Colour is already a
/// process-wide choice that `Palette::detect` reads back from `anstream`; motion is the
/// same kind of decision, and threading it through `scan`'s signature would put a
/// display setting in the analysis API.
static ANIMATED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_animated(animated: bool) {
    ANIMATED.store(animated, std::sync::atomic::Ordering::Relaxed);
}

fn animated() -> bool {
    ANIMATED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Draws one line, in place, on the terminal. Silent when stderr is not one, so a
/// redirected run writes nothing and a pipe stays clean.
pub struct Bar {
    palette: Palette,
    animated: bool,
    live: bool,
    painted: Instant,
    frame: usize,
    width: usize,
    dirty: bool,
}

impl Bar {
    pub fn new(palette: Palette) -> Self {
        Self {
            palette,
            animated: animated(),
            live: std::io::stderr().is_terminal(),
            // Far enough in the past that the first update paints rather than waiting.
            painted: Instant::now() - REPAINT,
            frame: 0,
            width: terminal_width(),
            dirty: false,
        }
    }

    /// A silent bar still answers, so callers never branch on whether one is drawing.
    pub fn hidden() -> Self {
        Self {
            palette: Palette::with_depth(crate::brand::ColorDepth::Monochrome),
            animated: false,
            live: false,
            painted: Instant::now(),
            frame: 0,
            width: 0,
            dirty: false,
        }
    }

    fn spinner(&mut self) -> char {
        if !self.animated {
            return '·';
        }
        self.frame = (self.frame + 1) % SPINNER.len();
        SPINNER[self.frame]
    }

    pub fn update(
        &mut self,
        phase: Phase,
        done: u32,
        total: Option<u32>,
        findings: u32,
        file: &str,
    ) {
        if !self.live || self.painted.elapsed() < REPAINT {
            return;
        }
        self.painted = Instant::now();
        // Re-read per paint rather than per scan, so a window resized mid-run redraws
        // to the new width instead of wrapping for the rest of the scan.
        self.width = terminal_width();
        let spinner = self.spinner();
        let line = match total.filter(|total| *total > 0 && phase == Phase::Scanning) {
            Some(total) => self.scanning(spinner, done, total, findings, file),
            None => self.working(spinner, phase, done, findings),
        };
        self.paint(&line);
    }

    fn scanning(&self, spinner: char, done: u32, total: u32, findings: u32, file: &str) -> String {
        let ratio = f64::from(done.min(total)) / f64::from(total);
        let filled = (ratio * BAR as f64).round() as usize;
        let bar = format!(
            "{}{}",
            paint(
                self.palette.style(Ink::Accent),
                FILLED.to_string().repeat(filled)
            ),
            paint(
                self.palette.style(Ink::Line),
                EMPTY.to_string().repeat(BAR - filled)
            ),
        );
        let counts = format!("{done}/{total}");
        let head = format!(
            "  {} {bar} {:>3}%  {counts}  ",
            paint(self.palette.style(Ink::Accent), spinner),
            (ratio * 100.0).round() as u32,
        );
        // Widths are measured on the visible text, not on the escape sequences that
        // colour it, or every coloured run would be treated as far wider than it draws.
        let used = 2 + 1 + 1 + BAR + 1 + 4 + 2 + counts.width() + 2;
        let tail = format!("  {findings} findings");
        let room = self.width.saturating_sub(used + tail.width());
        format!(
            "{head}{}{}",
            paint(self.palette.style(Ink::Text), elide(file, room)),
            paint(self.palette.style(Ink::Muted), tail),
        )
    }

    fn working(&self, spinner: char, phase: Phase, done: u32, findings: u32) -> String {
        let head = format!(
            "  {} {}",
            paint(self.palette.style(Ink::Accent), spinner),
            paint(self.palette.style(Ink::Text), phase.label()),
        );
        // Nothing has been counted yet while the walk is still finding files, and
        // "0 files · 0 findings" reads as a result rather than as a phase starting.
        if done == 0 {
            return head;
        }
        format!(
            "{head}  {}",
            paint(
                self.palette.style(Ink::Muted),
                format!("{done} files · {findings} findings"),
            ),
        )
    }

    fn paint(&mut self, line: &str) {
        let mut stderr = std::io::stderr();
        // Erase the whole line rather than overwriting it with spaces: a path wider than
        // the last one left its tail on screen. The trailing `\r` parks the cursor at
        // column 0, which is why this never has to hide and restore it — a scan
        // interrupted with Ctrl-C cannot leave the terminal without one.
        let _ = write!(stderr, "\r\x1b[2K{line}\r");
        let _ = stderr.flush();
        self.dirty = true;
    }

    pub fn clear(&mut self) {
        if self.live && self.dirty {
            let mut stderr = std::io::stderr();
            let _ = write!(stderr, "\r\x1b[2K");
            let _ = stderr.flush();
            self.dirty = false;
        }
    }
}

impl Drop for Bar {
    fn drop(&mut self) {
        self.clear();
    }
}

fn terminal_width() -> usize {
    crossterm::terminal::size().map_or(80, |(columns, _)| columns as usize)
}

/// Keeps the end of a path, which is the part that identifies it, and measures in
/// display columns so a CJK path does not overrun the line it was cut to fit.
fn elide(path: &str, room: usize) -> String {
    if room == 0 {
        return String::new();
    }
    if path.width() <= room {
        return path.to_owned();
    }
    let mut kept = String::new();
    let mut width = 0;
    for character in path.chars().rev() {
        let next = width + character.width().unwrap_or(0);
        if next > room.saturating_sub(1) {
            break;
        }
        width = next;
        kept.push(character);
    }
    format!("…{}", kept.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_path_is_left_alone() {
        assert_eq!(elide("src/a.ts", 20), "src/a.ts");
    }

    #[test]
    fn a_long_path_keeps_its_end() {
        let out = elide("src/very/deep/nested/module.ts", 12);
        assert!(out.ends_with("module.ts"), "{out}");
        assert!(out.width() <= 12, "{out} is {} wide", out.width());
    }

    /// Truncating by character count overran the line for wide glyphs and left the tail
    /// of the previous path on screen.
    #[test]
    fn width_is_measured_in_columns_not_characters() {
        let out = elide("src/日本語/module.ts", 10);
        assert!(out.width() <= 10, "{out} is {} wide", out.width());
    }

    #[test]
    fn no_room_draws_nothing() {
        assert_eq!(elide("src/a.ts", 0), "");
    }
}

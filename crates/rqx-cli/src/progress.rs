use std::io::Write;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::ValueEnum;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::brand::{Ink, Palette, paint};
use crate::scan::{ScanProgress, Watch};
use crate::term::{self, Stream};

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

/// Where the scan's progress goes, decided once from the flags and handed to whatever
/// scans. It is a value rather than a process-wide setting so that two scans in one
/// process (a test, a watch loop, the interface) cannot disagree about it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mode {
    display: Display,
    animated: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Display {
    Hidden,
    Bar,
    Lines,
}

/// What `--progress` asks for. `auto` is what a person at a terminal wants and what a
/// pipeline does not: it draws only where a bar can redraw and nobody is reading a log.
#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Style {
    /// A bar on a terminal, nothing in CI or when stderr is redirected
    Auto,
    /// A bar whenever stderr is a terminal, CI included
    Bar,
    /// A line on stderr every two seconds and at each phase, for a job log
    Plain,
    /// Nothing
    None,
}

impl Mode {
    pub fn new(style: Style, animated: bool) -> Self {
        let terminal = term::is_terminal(Stream::Stderr);
        Self {
            display: display_for(style, terminal, term::in_ci()),
            animated,
        }
    }

    pub const fn hidden() -> Self {
        Self {
            display: Display::Hidden,
            animated: false,
        }
    }

    const fn is_hidden(self) -> bool {
        matches!(self.display, Display::Hidden)
    }
}

/// A bar needs a terminal on stderr to redraw in, so a redirected run never gets one
/// whatever was asked for. Plain lines are for logs, which is where there is no terminal.
fn display_for(style: Style, terminal: bool, in_ci: bool) -> Display {
    match style {
        Style::Auto if terminal && !in_ci => Display::Bar,
        Style::Bar if terminal => Display::Bar,
        Style::Plain => Display::Lines,
        Style::Auto | Style::Bar | Style::None => Display::Hidden,
    }
}

/// How many updates may wait for the drawer. The scan never blocks on it: a full queue
/// drops the update, and the drawer only ever wants the newest.
const QUEUE: usize = 64;

/// Runs `work` with a watch that is drawn as `mode` says, and has finished drawing, and
/// cleared its line, by the time this returns. Nothing is spawned when there is nothing
/// to draw, so a scan in a pipeline pays for no updates at all.
pub fn drawing<T>(mode: Mode, work: impl FnOnce(Option<Watch>) -> Result<T>) -> Result<T> {
    if mode.is_hidden() {
        return work(None);
    }
    let (updates, receiver) = mpsc::sync_channel(QUEUE);
    let drawer = follow(receiver, mode);
    let result = work(Some(Watch::new(updates)));
    let _ = drawer.join();
    result
}

/// Draws a scan's updates until the scan drops its end of the channel.
pub fn follow(updates: Receiver<ScanProgress>, mode: Mode) -> JoinHandle<()> {
    thread::spawn(move || match mode.display {
        Display::Hidden => {}
        Display::Bar => {
            let mut bar = Bar::new(Palette::detect(), mode.animated);
            for progress in updates {
                bar.update(&progress);
            }
        }
        Display::Lines => print_lines(&updates, &mut std::io::stderr()),
    })
}

/// How often a log gets a line while nothing changes phase, which is slow enough not
/// to fill a job's log and fast enough that a stalled job is visibly stalled.
const LINE_EVERY: Duration = Duration::from_secs(2);

/// One line per phase and one every `LINE_EVERY`, including while a long phase sends
/// nothing, so a silent stretch of history reading still shows the job is alive.
fn print_lines(updates: &Receiver<ScanProgress>, out: &mut impl Write) {
    let began = Instant::now();
    let mut printed: Option<(Instant, Phase)> = None;
    let mut latest = ScanProgress::default();
    loop {
        let arrived = updates.recv_timeout(LINE_EVERY);
        if matches!(arrived, Err(RecvTimeoutError::Disconnected)) {
            return;
        }
        if let Ok(progress) = arrived {
            latest = progress;
        }
        let due =
            printed.is_none_or(|(at, phase)| phase != latest.phase || at.elapsed() >= LINE_EVERY);
        if due {
            let _ = writeln!(out, "{}", line_of(&latest, began.elapsed()));
            printed = Some((Instant::now(), latest.phase));
        }
    }
}

fn line_of(progress: &ScanProgress, elapsed: Duration) -> String {
    let findings = thousands(progress.findings);
    let seen = thousands(progress.files_scanned);
    let counted = progress
        .total
        .filter(|total| *total > 0 && progress.phase == Phase::Scanning);
    let position = match counted {
        Some(total) => format!(
            "{} {seen}/{} files",
            progress.phase.label(),
            thousands(total)
        ),
        None => format!("{} · {seen} files", progress.phase.label()),
    };
    format!(
        "rqx: {position} · {findings} findings · {:.1}s",
        elapsed.as_secs_f64()
    )
}

fn thousands(number: u32) -> String {
    let digits = number.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// Draws one line, in place, on stderr. Clears it when dropped.
struct Bar {
    palette: Palette,
    animated: bool,
    painted: Instant,
    frame: usize,
    width: usize,
    dirty: bool,
}

impl Bar {
    fn new(palette: Palette, animated: bool) -> Self {
        Self {
            palette,
            animated,
            // Far enough in the past that the first update paints rather than waiting.
            painted: Instant::now() - REPAINT,
            frame: 0,
            width: terminal_width(),
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

    fn update(&mut self, progress: &ScanProgress) {
        if self.painted.elapsed() < REPAINT {
            return;
        }
        self.painted = Instant::now();
        // Re-read per paint rather than per scan, so a window resized mid-run redraws
        // to the new width instead of wrapping for the rest of the scan.
        self.width = terminal_width();
        let spinner = self.spinner();
        let file = progress.file.as_deref().unwrap_or("");
        let counted = progress
            .total
            .filter(|total| *total > 0 && progress.phase == Phase::Scanning);
        let line = match counted {
            Some(total) => self.scanning(
                spinner,
                progress.files_scanned,
                total,
                progress.findings,
                file,
            ),
            None => self.working(
                spinner,
                progress.phase,
                progress.files_scanned,
                progress.findings,
            ),
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

    fn clear(&mut self) {
        if self.dirty {
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
    term::width(Stream::Stderr).unwrap_or(80)
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
    fn auto_draws_a_bar_only_on_a_terminal_outside_ci() {
        assert_eq!(display_for(Style::Auto, true, false), Display::Bar);
        assert_eq!(display_for(Style::Auto, true, true), Display::Hidden);
        assert_eq!(display_for(Style::Auto, false, false), Display::Hidden);
    }

    #[test]
    fn a_bar_is_drawn_in_ci_when_asked_for_but_never_without_a_terminal() {
        assert_eq!(display_for(Style::Bar, true, true), Display::Bar);
        assert_eq!(display_for(Style::Bar, false, false), Display::Hidden);
    }

    #[test]
    fn plain_lines_are_for_logs_so_they_need_no_terminal() {
        assert_eq!(display_for(Style::Plain, false, true), Display::Lines);
        assert_eq!(display_for(Style::None, true, false), Display::Hidden);
    }

    #[test]
    fn a_line_names_the_phase_the_counts_and_the_time() {
        let scanning = ScanProgress {
            files_scanned: 1204,
            total: Some(3456),
            findings: 31,
            phase: Phase::Scanning,
            ..ScanProgress::default()
        };

        assert_eq!(
            line_of(&scanning, Duration::from_millis(4200)),
            "rqx: scanning 1,204/3,456 files · 31 findings · 4.2s"
        );
    }

    #[test]
    fn a_phase_after_the_walk_has_no_total_to_divide_by() {
        let grouping = ScanProgress {
            files_scanned: 3456,
            total: Some(3456),
            findings: 31,
            phase: Phase::Duplicates,
            ..ScanProgress::default()
        };

        assert_eq!(
            line_of(&grouping, Duration::from_secs(9)),
            "rqx: grouping duplicates · 3,456 files · 31 findings · 9.0s"
        );
    }

    #[test]
    fn thousands_are_separated_by_commas() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn a_log_gets_a_line_when_the_phase_changes_and_not_for_every_file() {
        let (updates, receiver) = mpsc::sync_channel(8);
        for phase in [Phase::Scanning, Phase::Scanning, Phase::Duplicates] {
            updates
                .send(ScanProgress {
                    phase,
                    ..ScanProgress::default()
                })
                .unwrap();
        }
        drop(updates);
        let mut out = Vec::new();

        print_lines(&receiver, &mut out);

        let written = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(lines.len(), 2, "{written}");
        assert!(lines[0].starts_with("rqx: scanning"), "{written}");
        assert!(
            lines[1].starts_with("rqx: grouping duplicates"),
            "{written}"
        );
    }

    #[test]
    fn a_hidden_mode_gives_the_work_nothing_to_report_to() {
        let handed = drawing(Mode::hidden(), |watch| Ok(watch.is_some())).unwrap();

        assert!(!handed);
    }

    #[test]
    fn a_failed_scan_is_still_reported_once_drawing_has_finished() {
        let failed: Result<()> = drawing(Mode::hidden(), |_| anyhow::bail!("no such directory"));

        assert!(
            failed
                .unwrap_err()
                .to_string()
                .contains("no such directory")
        );
    }

    #[test]
    fn the_drawer_stops_when_the_scan_drops_its_end() {
        let (updates, receiver) = mpsc::sync_channel(1);
        let drawer = follow(receiver, Mode::hidden());
        drop(updates);

        assert!(drawer.join().is_ok());
    }

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

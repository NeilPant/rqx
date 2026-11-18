//! The alternate screen the viewer draws on. Entering it keeps the session's own screen
//! (scrollback, prompt and status line) untouched underneath, and leaving it puts that
//! screen back with the cursor where it was.
//!
//! The mouse is captured only while the viewer is open so its wheel scrolls; it is
//! released when the viewer closes, when it fails, and from the panic hook.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::view::Look;
use super::viewer::{Step, Viewer};
use super::viewport;

/// Whether the alternate screen is up. The panic hook is process-wide and cannot be
/// handed the session, so this is how it knows there is a screen to leave: leaving
/// one that was never entered moves the cursor on some terminals.
static SHOWN: Presence = Presence::new();

/// Shows `viewer` until it is closed. Raw mode is already on: the session holds it.
pub fn show(viewer: &mut Viewer, look: &Look) -> io::Result<()> {
    let _screen = Screen::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    let size = terminal.size()?;
    viewer.resize((size.width, size.height));
    loop {
        terminal.draw(|frame| viewer.draw(frame, look))?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if viewer.key(key) == Step::Close {
                    return Ok(());
                }
            }
            Event::Mouse(mouse) => viewer.mouse(mouse),
            Event::Resize(columns, rows) => {
                terminal.autoresize()?;
                viewer.resize((columns, rows));
            }
            _ => {}
        }
    }
}

/// Everything the session takes from the terminal, given back; installed as the
/// session's panic hook.
pub fn restore() {
    SHOWN.leave(&mut io::stdout());
    viewport::restore();
}

/// Leaves the alternate screen however `show` ends, an error included.
struct Screen;

impl Screen {
    fn enter() -> io::Result<Self> {
        let screen = Self;
        SHOWN.enter(&mut io::stdout())?;
        Ok(screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        SHOWN.leave(&mut io::stdout());
    }
}

struct Presence {
    up: AtomicBool,
}

impl Presence {
    const fn new() -> Self {
        Self {
            up: AtomicBool::new(false),
        }
    }

    /// Marked as up before the first byte, so a failure halfway in is still undone.
    fn enter(&self, out: &mut impl Write) -> io::Result<()> {
        self.up.store(true, Ordering::SeqCst);
        execute!(out, EnterAlternateScreen, EnableMouseCapture)
    }

    /// Safe to call any number of times: only the first call after `enter` writes.
    fn leave(&self, out: &mut impl Write) {
        if self.up.swap(false, Ordering::SeqCst) {
            let _ = execute!(out, DisableMouseCapture, LeaveAlternateScreen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALTERNATE_ON: &str = "\x1b[?1049h";
    const ALTERNATE_OFF: &str = "\x1b[?1049l";
    const MOUSE_ON: &str = "\x1b[?1000h";
    const MOUSE_OFF: &str = "\x1b[?1000l";

    fn written(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn entering_takes_the_alternate_screen_and_the_mouse() {
        let presence = Presence::new();
        let mut out = Vec::new();
        presence.enter(&mut out).unwrap();
        let entered = written(&out);
        assert!(entered.contains(ALTERNATE_ON), "{entered:?}");
        assert!(entered.contains(MOUSE_ON), "{entered:?}");
    }

    #[test]
    fn leaving_releases_the_mouse_before_the_screen_and_only_once() {
        let presence = Presence::new();
        presence.enter(&mut Vec::new()).unwrap();
        let mut out = Vec::new();
        presence.leave(&mut out);
        let left = written(&out);
        let (mouse, screen) = (left.find(MOUSE_OFF), left.find(ALTERNATE_OFF));
        assert!(mouse.is_some() && screen.is_some(), "{left:?}");
        assert!(mouse < screen);
        let mut again = Vec::new();
        presence.leave(&mut again);
        assert!(again.is_empty(), "a second leave writes nothing");
    }

    #[test]
    fn a_screen_never_entered_is_never_left() {
        let mut out = Vec::new();
        Presence::new().leave(&mut out);
        assert!(out.is_empty());
    }
}

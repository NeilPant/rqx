//! What a command printed, kept after it went into scrollback so `ctrl-o` and
//! `/open-last` can open it again in the full-height viewer.

use std::io;

use super::view::{self, Look};
use super::viewer::Viewer;
use super::viewport::Viewport;
use super::{ansi, fullscreen};

const NOTHING_KEPT: &str = "There is no output to open yet.";

#[derive(Default)]
pub struct LongOutput {
    /// The line that produced what is printed next, which titles the viewer.
    running: String,
    kept: Option<Kept>,
}

struct Kept {
    title: String,
    text: String,
}

impl LongOutput {
    /// Called with each line as it is sent.
    pub fn running(&mut self, line: &str) {
        line.clone_into(&mut self.running);
    }

    /// Prints a command's output into scrollback and keeps it for the viewer.
    pub fn print(&mut self, viewport: &mut Viewport, bytes: &[u8]) -> io::Result<()> {
        let text = String::from_utf8_lossy(bytes).into_owned();
        viewport.print(&ansi::lines(&text, viewport.width()))?;
        self.kept = Some(Kept {
            title: self.running.clone(),
            text,
        });
        Ok(())
    }

    /// Opens the kept output in the viewer and, once it closes, redraws the session's
    /// rows, which the alternate screen left as they were.
    pub fn open(&self, viewport: &mut Viewport, look: &Look) -> io::Result<()> {
        let Some(kept) = &self.kept else {
            return viewport.print(&[view::note(look, NOTHING_KEPT)]);
        };
        let size = crossterm::terminal::size().unwrap_or((80, 24));
        let mut viewer = Viewer::new(&kept.title, &kept.text, size);
        let shown = fullscreen::show(&mut viewer, look);
        viewport.repaint()?;
        shown
    }
}

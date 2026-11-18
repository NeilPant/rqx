//! The few rows at the bottom of the terminal that the session redraws, and the
//! scrollback above them that it only ever appends to.
//!
//! There is no alternate screen and no mouse capture, so the terminal keeps its own
//! scrollback and selection and everything the session printed is still there after it
//! exits. Raw mode is the only thing taken away, and `restore` gives it back on every
//! way out: a normal return, an error and a panic.

use std::io::{self, Stdout};

use crossterm::cursor::{MoveTo, Show};
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use ratatui::backend::CrosstermBackend;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport as Region};

type Screen = Terminal<CrosstermBackend<Stdout>>;

/// Lines are handed to the terminal in pieces, so a command that prints thousands of
/// them never asks for one buffer as tall as its whole output.
const PRINT_CHUNK: usize = 500;

pub struct Viewport {
    screen: Screen,
    height: u16,
}

impl Viewport {
    pub fn open(height: u16) -> io::Result<Self> {
        enable_raw_mode()?;
        match screen(height) {
            Ok(screen) => Ok(Self { screen, height }),
            Err(error) => {
                restore();
                Err(error)
            }
        }
    }

    /// The viewport is fixed-height once made, so changing it means clearing the old
    /// rows and making a new one where they were. The caller does this on a change of
    /// what is shown, never per keystroke.
    pub fn resize(&mut self, height: u16) -> io::Result<()> {
        if height == self.height {
            return Ok(());
        }
        self.vacate()?;
        self.screen = screen(height)?;
        self.height = height;
        Ok(())
    }

    /// Gives the terminal back for a program that wants all of it, such as an editor:
    /// the rows are cleared and raw mode is dropped.
    pub fn suspend(&mut self) -> io::Result<()> {
        self.vacate()?;
        disable_raw_mode()?;
        execute!(io::stdout(), Show)
    }

    /// Takes the terminal again after `suspend`, with a fresh viewport where the old
    /// one was.
    pub fn resume(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        self.screen = screen(self.height)?;
        Ok(())
    }

    /// Wipes what is visible and puts the viewport back at the top. The terminal's
    /// scrollback is left alone: that is the user's, not the session's.
    pub fn clear_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
        self.screen = screen(self.height)?;
        Ok(())
    }

    /// Draws every row again on the next `draw`, for when something else has had the
    /// screen, such as the viewer on the alternate screen.
    pub fn repaint(&mut self) -> io::Result<()> {
        self.screen.clear()
    }

    pub fn width(&self) -> u16 {
        crossterm::terminal::size().map_or(80, |(columns, _)| columns)
    }

    pub fn draw(&mut self, render: impl FnOnce(&mut Frame<'_>)) -> io::Result<()> {
        self.screen.draw(render).map(|_| ())
    }

    /// Appends above the viewport, into the terminal's own scrollback. The lines must
    /// already fit the width: the buffer has no wrapping of its own.
    pub fn print(&mut self, lines: &[Line<'static>]) -> io::Result<()> {
        for chunk in lines.chunks(PRINT_CHUNK) {
            let height = u16::try_from(chunk.len()).unwrap_or(u16::MAX);
            self.screen.insert_before(height, |buffer| {
                Paragraph::new(chunk.to_vec()).render(buffer.area, buffer);
            })?;
        }
        Ok(())
    }

    /// Wipes the viewport and leaves the cursor where it was, so whatever the caller
    /// prints next starts on the row the session's own rows used to.
    pub fn close(mut self) -> io::Result<()> {
        self.vacate()
    }

    /// Clears the rows and parks the cursor on the first of them. Ratatui's own `clear`
    /// puts the cursor back where it was, which is the prompt, so a new viewport made
    /// from there would start a row lower every time.
    fn vacate(&mut self) -> io::Result<()> {
        let top = self.screen.get_frame().area().as_position();
        self.screen.clear()?;
        self.screen.set_cursor_position(top)
    }
}

impl Drop for Viewport {
    fn drop(&mut self) {
        restore();
    }
}

fn screen(height: u16) -> io::Result<Screen> {
    Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions {
            viewport: Region::Inline(height),
        },
    )
}

/// What the session took from the terminal, given back. Safe to call twice and from a
/// panic hook, because every step ignores a failure: there is nothing better to do
/// about one while already on the way out.
pub fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), Show);
}

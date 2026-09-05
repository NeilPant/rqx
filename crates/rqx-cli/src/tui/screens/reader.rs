use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::brand::{ColorDepth, Ink, Palette};
use crate::tui::app::{Nav, Screen, Session, global, wheel};
use crate::tui::palette::Page;

/// The analyses that answer with prose rather than with a table to move around in.
/// They already render to a `Write`, so this asks them for their answer with colour
/// turned off and shows it — one screen instead of five that would each be a page of
/// text with a scrollbar on it.
#[derive(Default)]
pub struct Reader {
    page: Option<Page>,
    lines: Vec<String>,
    offset: usize,
    height: u16,
}

impl Reader {
    pub fn show(&mut self, page: Page, session: &Session) {
        self.page = Some(page);
        self.offset = 0;
        self.lines = render(page, session)
            .unwrap_or_else(|error| format!("\n  {error:#}\n"))
            .lines()
            .map(str::to_owned)
            .collect();
    }

    fn scroll(&mut self, delta: isize) {
        let last = self
            .lines
            .len()
            .saturating_sub(usize::from(self.height).max(1));
        self.offset = (self.offset as isize + delta).clamp(0, last as isize) as usize;
    }
}

/// Colour is off rather than translated: the reporters paint with ANSI, and parsing
/// escape sequences back into ratatui styles to re-emit them is a lot of machinery
/// for text that is already legible without it.
fn render(page: Page, session: &Session) -> anyhow::Result<String> {
    let plain = Palette::with_depth(ColorDepth::Monochrome);
    let mut out: Vec<u8> = Vec::new();
    match page {
        Page::Techniques => crate::report::techniques(plain, &mut out)?,
        Page::Levels => crate::report::levels(plain, &mut out)?,
        _ => {
            let Some(outcome) = &session.outcome else {
                return Ok("\n  nothing scanned yet \u{2014} press r\n".to_owned());
            };
            match page {
                Page::Recommend => {
                    crate::recommend::render(outcome, &session.config, None, plain, &mut out)?;
                }
                Page::Suppressions => {
                    crate::report::waivers(&session.path, outcome, false, plain, &mut out)?
                }
                Page::Patterns => crate::report::advice(outcome, plain, &mut out)?,
                Page::Techniques | Page::Levels => unreachable!("answered above"),
            }
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

impl Screen for Reader {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        self.height = area.height.saturating_sub(1);
        let shown: Vec<Line<'static>> = self
            .lines
            .iter()
            .skip(self.offset)
            .take(usize::from(self.height))
            .map(|line| Line::styled(line.clone(), Style::new().fg(palette.color(Ink::Text))))
            .collect();

        frame.render_widget(
            Paragraph::new(shown).block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(palette.rule()),
            ),
            area,
        );

        let mut scrollbar = ScrollbarState::new(self.lines.len()).position(self.offset);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(palette.dim()),
            area,
            &mut scrollbar,
        );
    }

    fn handle(&mut self, key: KeyEvent, _session: &Session) -> Nav {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.scroll(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll(-1),
            KeyCode::PageDown => self.scroll(isize::from(self.height.max(1) as i16)),
            KeyCode::PageUp => self.scroll(-isize::from(self.height.max(1) as i16)),
            KeyCode::Home => self.scroll(isize::MIN / 2),
            KeyCode::End => self.scroll(isize::MAX / 2),
            _ => return global(key).unwrap_or(Nav::Stay),
        }
        Nav::Redraw
    }

    fn click(&mut self, event: MouseEvent, _session: &Session) -> Nav {
        let Some(rows) = wheel(event) else {
            return Nav::Stay;
        };
        self.scroll(rows);
        Nav::Redraw
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[("↑↓", "scroll"), ("esc", "back"), ("ctrl-p", "commands")]
    }
}

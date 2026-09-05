use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::brand::Ink;
use crate::tui::app::{Session, Tab};

/// A tab that opens before a scan says what it will hold rather than refusing to
/// open, which is what the tab key used to do instead.
pub fn nothing_yet(frame: &mut Frame<'_>, area: Rect, session: &Session, tab: Tab) {
    let palette = session.palette;
    let lines = vec![
        Line::default(),
        Line::styled(
            format!("  {} will show {}.", tab.title(), tab.promise()),
            palette.dim(),
        ),
        Line::default(),
        Line::styled(
            "  press r to scan this project",
            Style::new().fg(palette.color(Ink::Accent)),
        ),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

pub mod catalogue;
pub mod complexity;
pub mod files;
pub mod findings;
pub mod graph;
pub mod health;
pub mod menu;
pub mod overview;
pub mod patterns;
pub mod plan;
pub mod reader;
pub mod scanning;
pub mod settings;

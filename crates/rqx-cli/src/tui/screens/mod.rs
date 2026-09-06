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

/// A scan that ran and found nothing to show here. Not the same as one that has not
/// run: four screens used to tell a clean project to press `r`, which it just had.
pub fn nothing_found(frame: &mut Frame<'_>, area: Rect, session: &Session, what: &str) {
    let palette = session.palette;
    let lines = vec![
        Line::default(),
        Line::styled(format!("  No {what}."), palette.dim()),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

pub mod catalogue;
pub mod complexity;
pub mod files;
pub mod findings;
pub mod graph;
pub mod health;
pub mod overview;
pub mod patterns;
pub mod plan;
pub mod reader;
pub mod scanning;
pub mod settings;

#[cfg(test)]
mod vocabulary {
    use super::{
        catalogue, complexity, files, findings, graph, health, overview, patterns, plan, reader,
        scanning, settings,
    };
    use crate::tui::app::Screen;
    use crate::tui::chrome::{EVERYWHERE, VOCABULARY};

    fn every_screen() -> Vec<(&'static str, Box<dyn Screen>)> {
        vec![
            ("overview", Box::<overview::Overview>::default()),
            ("findings", Box::<findings::Findings>::default()),
            ("files", Box::<files::Files>::default()),
            ("complexity", Box::<complexity::Complexity>::default()),
            ("graph", Box::<graph::Graph>::default()),
            ("health", Box::<health::Health>::default()),
            ("plan", Box::<plan::Plan>::default()),
            ("patterns", Box::<patterns::Patterns>::default()),
            ("catalogue", Box::<catalogue::Rules>::default()),
            ("settings", Box::<settings::Settings>::default()),
            ("reader", Box::<reader::Reader>::default()),
            ("scanning", Box::<scanning::Scanning>::default()),
        ]
    }

    /// The word before the `·`: what the key does, without what this screen's version
    /// of it acts on.
    fn meaning(label: &str) -> &str {
        label.split(" \u{b7} ").next().unwrap_or(label)
    }

    #[test]
    fn keys_mean_one_thing() {
        let spoken: std::collections::HashMap<&str, &str> = VOCABULARY.iter().copied().collect();
        let mut seen: std::collections::HashMap<&str, (&str, &str)> =
            std::collections::HashMap::new();

        for (screen, keys) in every_screen() {
            for (key, label) in keys.keys() {
                if let Some(word) = spoken.get(key) {
                    assert_eq!(
                        meaning(label),
                        *word,
                        "{screen} gives `{key}` a meaning the vocabulary does not"
                    );
                }
                if let Some((first, elsewhere)) = seen.get(key) {
                    assert_eq!(
                        meaning(label),
                        *first,
                        "`{key}` means one thing on {elsewhere} and another on {screen}"
                    );
                } else {
                    seen.insert(key, (meaning(label), screen));
                }
                assert!(
                    !EVERYWHERE.iter().any(|(global, _)| global == key),
                    "{screen} lists `{key}`, which every screen answers and the overlay adds once"
                );
            }
        }
    }
}

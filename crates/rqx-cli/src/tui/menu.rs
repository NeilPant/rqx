use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::brand::{Ink, Palette};

use super::dashboard;
use super::sprite::Sprite;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Choice {
    Scan,
    Review,
    Rules,
    Quit,
}

impl Choice {
    const ALL: [Self; 4] = [Self::Scan, Self::Review, Self::Rules, Self::Quit];

    const fn label(self) -> &'static str {
        match self {
            Self::Scan => "Scan whole project",
            Self::Review => "Review changes against origin/HEAD",
            Self::Rules => "Rule catalogue",
            Self::Quit => "Quit",
        }
    }
}

/// Opens on `Scan`, so the command this replaced is still one keystroke away.
#[derive(Default)]
pub struct Menu {
    cursor: usize,
    pub chosen: Option<Choice>,
}

impl Menu {
    pub fn handle(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = (self.cursor + 1).min(Choice::ALL.len() - 1);
            }
            KeyCode::Up | KeyCode::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Enter => self.chosen = Some(Choice::ALL[self.cursor]),
            KeyCode::Char('q') | KeyCode::Esc => self.chosen = Some(Choice::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.chosen = Some(Choice::Quit);
            }
            _ => return false,
        }
        true
    }

    pub fn render(&self, frame: &mut Frame<'_>, palette: Palette, mascot: &Sprite, root: &str) {
        let [head, body, footer] = Layout::vertical([
            Constraint::Length(mascot.cell_height() + 1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        dashboard::render_head(frame, palette, mascot, root, head);

        let lines: Vec<Line<'static>> = Choice::ALL
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                let selected = index == self.cursor;
                let marker = if selected { "\u{25b8} " } else { "  " };
                let style = if selected {
                    Style::new().fg(palette.color(Ink::Accent)).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(palette.color(Ink::Text))
                };
                Line::from(vec![
                    Span::styled(marker, Style::new().fg(palette.color(Ink::Accent))),
                    Span::styled(choice.label(), style),
                ])
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), body);
        frame.render_widget(
            Paragraph::new(Line::styled("\u{2191}\u{2193} move    enter choose    q quit", palette.dim())),
            footer,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn enter_on_an_untouched_menu_scans() {
        let mut menu = Menu::default();
        menu.handle(press(KeyCode::Enter));
        assert_eq!(menu.chosen, Some(Choice::Scan));
    }

    #[test]
    fn movement_stops_at_both_ends() {
        let mut menu = Menu::default();
        for _ in 0..10 {
            menu.handle(press(KeyCode::Up));
        }
        menu.handle(press(KeyCode::Enter));
        assert_eq!(menu.chosen, Some(Choice::Scan));

        let mut menu = Menu::default();
        for _ in 0..10 {
            menu.handle(press(KeyCode::Down));
        }
        menu.handle(press(KeyCode::Enter));
        assert_eq!(menu.chosen, Some(Choice::Quit));
    }

    fn draw(menu: &Menu, width: u16, height: u16) -> String {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mascot = Sprite::parse(include_str!("../../assets/mascot.txt"));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                menu.render(frame, Palette::with_depth(crate::brand::ColorDepth::Monochrome), &mascot, "src")
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_choice_is_drawn_with_the_cursor_on_the_first() {
        let screen = draw(&Menu::default(), 60, 12);
        for choice in Choice::ALL {
            assert!(screen.contains(choice.label()), "{} is missing:\n{screen}", choice.label());
        }
        let scan = screen.lines().find(|line| line.contains("Scan whole project")).unwrap();
        assert!(scan.trim_start().starts_with('\u{25b8}'), "the cursor should open on Scan: {scan}");
    }

    #[test]
    fn the_smallest_supported_terminal_still_renders_without_panicking() {
        let screen = draw(&Menu::default(), 60, 20);
        assert!(screen.contains("Quit"), "{screen}");
    }

    #[test]
    fn esc_leaves_without_choosing_anything_to_run() {
        let mut menu = Menu::default();
        menu.handle(press(KeyCode::Esc));
        assert_eq!(menu.chosen, Some(Choice::Quit));
    }
}

//! Turns what a renderer wrote for a terminal back into styled lines.
//!
//! Every renderer writes ANSI to a `&mut dyn Write`. The session needs those bytes as
//! ratatui lines to hand to `insert_before`, wrapped to the width of the window because
//! a buffer row cannot spill onto the next one the way a terminal's own output does.
//! Only colour and weight are kept; cursor movement and anything else is dropped.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

const TAB: usize = 4;

pub fn lines(text: &str, width: u16) -> Vec<Line<'static>> {
    let mut painter = Painter::new(usize::from(width).max(1));
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\x1b' => escape(&mut characters, &mut painter),
            '\n' => painter.end_line(),
            '\r' => {}
            '\t' => (0..TAB).for_each(|_| painter.put(' ')),
            control if control.is_control() => {}
            printable => painter.put(printable),
        }
    }
    painter.finish()
}

fn escape(characters: &mut std::iter::Peekable<std::str::Chars<'_>>, painter: &mut Painter) {
    match characters.next() {
        Some('[') => {
            let mut parameters = String::new();
            for next in characters.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&next) {
                    if next == 'm' {
                        painter.style = apply(painter.style, &parameters);
                    }
                    return;
                }
                parameters.push(next);
            }
        }
        // A link or a title: skipped through its terminator, which is a bell or ESC \.
        Some(']') => {
            while let Some(next) = characters.next() {
                if next == '\x07' {
                    return;
                }
                if next == '\x1b' {
                    characters.next();
                    return;
                }
            }
        }
        _ => {}
    }
}

struct Painter {
    width: usize,
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    text: String,
    column: usize,
    style: Style,
    text_style: Style,
}

impl Painter {
    fn new(width: usize) -> Self {
        Self {
            width,
            lines: Vec::new(),
            spans: Vec::new(),
            text: String::new(),
            column: 0,
            style: Style::new(),
            text_style: Style::new(),
        }
    }

    fn put(&mut self, character: char) {
        let step = character.width().unwrap_or(0);
        if self.column + step > self.width {
            self.end_line();
        }
        if self.style != self.text_style {
            self.seal();
            self.text_style = self.style;
        }
        self.text.push(character);
        self.column += step;
    }

    fn seal(&mut self) {
        if !self.text.is_empty() {
            self.spans.push(Span::styled(
                std::mem::take(&mut self.text),
                self.text_style,
            ));
        }
    }

    fn end_line(&mut self) {
        self.seal();
        self.lines.push(Line::from(std::mem::take(&mut self.spans)));
        self.column = 0;
    }

    /// A last line that was never ended still counts; a trailing newline does not add
    /// an empty one.
    fn finish(mut self) -> Vec<Line<'static>> {
        if self.column > 0 || !self.text.is_empty() || !self.spans.is_empty() {
            self.end_line();
        }
        self.lines
    }
}

fn apply(mut style: Style, parameters: &str) -> Style {
    let mut codes = parameters
        .split([';', ':'])
        .map(|code| code.parse::<u16>().unwrap_or(0));
    if parameters.is_empty() {
        return Style::new();
    }
    while let Some(code) = codes.next() {
        style = match code {
            0 => Style::new(),
            1 => style.add_modifier(Modifier::BOLD),
            2 => style.add_modifier(Modifier::DIM),
            3 => style.add_modifier(Modifier::ITALIC),
            4 => style.add_modifier(Modifier::UNDERLINED),
            7 => style.add_modifier(Modifier::REVERSED),
            22 => style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => style.remove_modifier(Modifier::ITALIC),
            24 => style.remove_modifier(Modifier::UNDERLINED),
            27 => style.remove_modifier(Modifier::REVERSED),
            30..=37 => style.fg(basic(code - 30, false)),
            39 => style.fg(Color::Reset),
            40..=47 => style.bg(basic(code - 40, false)),
            49 => style.bg(Color::Reset),
            90..=97 => style.fg(basic(code - 90, true)),
            100..=107 => style.bg(basic(code - 100, true)),
            38 => match extended(&mut codes) {
                Some(color) => style.fg(color),
                None => style,
            },
            48 => match extended(&mut codes) {
                Some(color) => style.bg(color),
                None => style,
            },
            _ => style,
        };
    }
    style
}

/// What follows a `38` or `48`: `5;n` for the 256-colour table, `2;r;g;b` for true colour.
fn extended(codes: &mut impl Iterator<Item = u16>) -> Option<Color> {
    match codes.next()? {
        5 => Some(Color::Indexed(codes.next()? as u8)),
        2 => {
            let (red, green, blue) = (codes.next()?, codes.next()?, codes.next()?);
            Some(Color::Rgb(red as u8, green as u8, blue as u8))
        }
        _ => None,
    }
}

fn basic(index: u16, bright: bool) -> Color {
    match (index, bright) {
        (0, false) => Color::Black,
        (1, false) => Color::Red,
        (2, false) => Color::Green,
        (3, false) => Color::Yellow,
        (4, false) => Color::Blue,
        (5, false) => Color::Magenta,
        (6, false) => Color::Cyan,
        (_, false) => Color::Gray,
        (0, true) => Color::DarkGray,
        (1, true) => Color::LightRed,
        (2, true) => Color::LightGreen,
        (3, true) => Color::LightYellow,
        (4, true) => Color::LightBlue,
        (5, true) => Color::LightMagenta,
        (6, true) => Color::LightCyan,
        (_, true) => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn plain_text_becomes_one_line_per_newline() {
        assert_eq!(
            text(&lines("one\ntwo\n\nfour\n", 20)),
            ["one", "two", "", "four"]
        );
    }

    #[test]
    fn a_missing_final_newline_still_ends_the_line() {
        assert_eq!(text(&lines("one\ntwo", 20)), ["one", "two"]);
    }

    #[test]
    fn colour_and_weight_survive_and_a_reset_clears_them() {
        let drawn = lines("\x1b[1m\x1b[38;2;200;155;106mhead\x1b[0m tail", 40);
        let spans = &drawn[0].spans;
        assert_eq!(spans[0].content, "head");
        assert_eq!(spans[0].style.fg, Some(Color::Rgb(200, 155, 106)));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[1].content, " tail");
        assert_eq!(spans[1].style, Style::new());
    }

    #[test]
    fn the_sixteen_and_two_hundred_fifty_six_colour_forms_are_read() {
        let drawn = lines("\x1b[31ma\x1b[0m\x1b[93mb\x1b[0m\x1b[48;5;179mc", 40);
        assert_eq!(drawn[0].spans[0].style.fg, Some(Color::Red));
        assert_eq!(drawn[0].spans[1].style.fg, Some(Color::LightYellow));
        assert_eq!(drawn[0].spans[2].style.bg, Some(Color::Indexed(179)));
    }

    #[test]
    fn a_line_longer_than_the_window_wraps_and_keeps_its_style() {
        let drawn = lines("\x1b[31mabcdefgh\x1b[0m", 5);
        assert_eq!(text(&drawn), ["abcde", "fgh"]);
        assert_eq!(drawn[1].spans[0].style.fg, Some(Color::Red));
    }

    #[test]
    fn wide_characters_count_for_their_columns() {
        assert_eq!(
            text(&lines("\u{4e2d}\u{6587}\u{5b57}", 4)),
            ["\u{4e2d}\u{6587}", "\u{5b57}"]
        );
    }

    #[test]
    fn links_and_cursor_movement_are_dropped_not_printed() {
        let drawn = lines(
            "\x1b]8;;https://example.com\x07link\x1b]8;;\x07\x1b[2K done",
            40,
        );
        assert_eq!(text(&drawn), ["link done"]);
    }

    #[test]
    fn tabs_become_spaces_and_carriage_returns_vanish() {
        assert_eq!(text(&lines("a\tb\r\n", 20)), ["a    b"]);
    }
}

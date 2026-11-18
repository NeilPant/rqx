//! A full-height reader for one command's output: scrolling, paging and search over
//! lines that were already rendered. It holds state and draws a frame; `fullscreen`
//! owns the terminal while it is open.

use std::ops::Range;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::brand::Ink;

use super::ansi;
use super::text::{clip, thousands};
use super::view::Look;

/// Rows the header and the footer take from the window.
const CHROME_ROWS: u16 = 2;
/// Rows one notch of the mouse wheel moves.
const WHEEL_STEP: isize = 3;
const KEYS: &str = "\u{2191}\u{2193} j k line \u{b7} space b page \u{b7} g G top/bottom \u{b7} / search \u{b7} n N next/previous \u{b7} q close";

/// Whether the viewer stays open after a key.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Stay,
    Close,
}

pub struct Viewer {
    title: String,
    text: String,
    rows: Vec<Line<'static>>,
    plain: Vec<String>,
    top: usize,
    page: usize,
    search: Search,
}

#[derive(Default)]
struct Search {
    /// What is being typed after `/`, while it is typed.
    typing: Option<String>,
    query: String,
    /// The rows with at least one match, in order.
    hits: Vec<usize>,
    /// Which of `hits` `n` and `N` last moved to.
    current: Option<usize>,
}

impl Viewer {
    /// `text` is a renderer's output, escapes included; it is wrapped to `width` and
    /// paged by what fits in `height` under the header and above the footer.
    pub fn new(title: &str, text: &str, (width, height): (u16, u16)) -> Self {
        let mut viewer = Self {
            title: title.to_owned(),
            text: text.to_owned(),
            rows: Vec::new(),
            plain: Vec::new(),
            top: 0,
            page: 1,
            search: Search::default(),
        };
        viewer.resize((width, height));
        viewer
    }

    /// Wraps the text again for a window of another size, keeping the first row shown
    /// where the window allows it.
    pub fn resize(&mut self, (width, height): (u16, u16)) {
        self.rows = ansi::lines(&self.text, width);
        self.plain = self.rows.iter().map(plain).collect();
        self.page = usize::from(height.saturating_sub(CHROME_ROWS)).max(1);
        self.search.hits = hits(&self.plain, &self.search.query);
        self.search.current = None;
        self.top = self.top.min(self.last_top());
    }

    pub fn key(&mut self, key: KeyEvent) -> Step {
        if self.search.typing.is_some() {
            self.type_query(key);
            return Step::Stay;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Step::Close,
            KeyCode::Char('c') if control => return Step::Close,
            KeyCode::Up | KeyCode::Char('k') => self.scroll(-1),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => self.scroll(1),
            KeyCode::PageUp | KeyCode::Char('b') => self.scroll(-self.page_step()),
            KeyCode::PageDown | KeyCode::Char(' ' | 'f') => self.scroll(self.page_step()),
            KeyCode::Home | KeyCode::Char('g') => self.top = 0,
            KeyCode::End | KeyCode::Char('G') => self.top = self.last_top(),
            KeyCode::Char('/') => self.search.typing = Some(String::new()),
            KeyCode::Char('n') => self.next_hit(true),
            KeyCode::Char('N') => self.next_hit(false),
            _ => {}
        }
        Step::Stay
    }

    pub fn mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll(-WHEEL_STEP),
            MouseEventKind::ScrollDown => self.scroll(WHEEL_STEP),
            _ => {}
        }
    }

    pub fn draw(&self, frame: &mut Frame<'_>, look: &Look) {
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        let width = usize::from(header.width);
        frame.render_widget(Paragraph::new(self.header(look, width)), header);
        let shown: Vec<Line<'static>> = (self.top..self.rows.len())
            .take(self.page)
            .map(|row| self.marked(row))
            .collect();
        frame.render_widget(Paragraph::new(shown), body);
        frame.render_widget(Paragraph::new(self.footer(look, width)), footer);
        if let Some(typed) = &self.search.typing {
            let column = u16::try_from(typed.chars().count() + 1).unwrap_or(u16::MAX);
            frame.set_cursor_position((footer.x + column.min(footer.width), footer.y));
        }
    }

    fn header(&self, look: &Look, width: usize) -> Line<'static> {
        let total = self.rows.len();
        let first = if total == 0 { 0 } else { self.top + 1 };
        let last = (self.top + self.page).min(total);
        let mut facts = format!(
            "lines {}\u{2013}{} of {}",
            thousands(count(first)),
            thousands(count(last)),
            thousands(count(total))
        );
        if !self.search.query.is_empty() {
            facts.push_str(&format!(" \u{b7} {}", self.found()));
        }
        let title = clip(&self.title, width.saturating_sub(facts.len() + 6));
        Line::from(vec![
            Span::styled(
                format!(" {title}"),
                Style::new()
                    .fg(look.palette.color(Ink::Accent))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" \u{b7} {facts}"), look.palette.dim()),
        ])
    }

    fn found(&self) -> String {
        let hits = self.search.hits.len();
        match (hits, self.search.current) {
            (0, _) => format!("no match for `{}`", self.search.query),
            (_, Some(at)) => format!("match {} of {}", at + 1, thousands(count(hits))),
            (_, None) => format!("{} rows match", thousands(count(hits))),
        }
    }

    fn footer(&self, look: &Look, width: usize) -> Line<'static> {
        match &self.search.typing {
            Some(typed) => Line::from(vec![
                Span::styled("/", look.palette.dim()),
                Span::raw(clip(typed, width.saturating_sub(1))),
            ]),
            None => Line::styled(
                format!(" {}", clip(KEYS, width.saturating_sub(1))),
                look.palette.dim(),
            ),
        }
    }

    /// A row with every match of the search shown reversed.
    fn marked(&self, row: usize) -> Line<'static> {
        let line = &self.rows[row];
        let found = matches(&self.plain[row], &self.search.query);
        if found.is_empty() {
            return line.clone();
        }
        highlight(line, &found)
    }

    fn type_query(&mut self, key: KeyEvent) {
        let Some(typed) = &mut self.search.typing else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.search.typing = None,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.typing = None;
            }
            KeyCode::Backspace if typed.is_empty() => self.search.typing = None,
            KeyCode::Backspace => {
                typed.pop();
            }
            KeyCode::Enter => {
                let query = self.search.typing.take().unwrap_or_default();
                self.find(&query);
            }
            KeyCode::Char(character) => typed.push(character),
            _ => {}
        }
    }

    /// Searches for `query` and moves to the first match at or below the top row.
    fn find(&mut self, query: &str) {
        self.search.query = query.to_owned();
        self.search.hits = hits(&self.plain, query);
        self.search.current = None;
        let from = self.search.hits.iter().position(|row| *row >= self.top);
        if let Some(at) = from.or_else(|| (!self.search.hits.is_empty()).then_some(0)) {
            self.go_to_hit(at);
        }
    }

    /// The next match below the top row, or above it for `N`, wrapping at either end.
    fn next_hit(&mut self, forward: bool) {
        let hits = &self.search.hits;
        if hits.is_empty() {
            return;
        }
        let at = match (self.search.current, forward) {
            (Some(current), true) => (current + 1) % hits.len(),
            (Some(current), false) => (current + hits.len() - 1) % hits.len(),
            (None, true) => hits.iter().position(|row| *row > self.top).unwrap_or(0),
            (None, false) => hits
                .iter()
                .rposition(|row| *row < self.top)
                .unwrap_or(hits.len() - 1),
        };
        self.go_to_hit(at);
    }

    fn go_to_hit(&mut self, at: usize) {
        self.search.current = Some(at);
        self.top = self.search.hits[at].min(self.last_top());
    }

    fn scroll(&mut self, by: isize) {
        self.top = self.top.saturating_add_signed(by).min(self.last_top());
    }

    fn page_step(&self) -> isize {
        isize::try_from(self.page).unwrap_or(isize::MAX)
    }

    /// The lowest the top row can go: the last page is a full page, not one line.
    fn last_top(&self) -> usize {
        self.rows.len().saturating_sub(self.page)
    }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn plain(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// The rows of `plain` that hold `query`.
fn hits(plain: &[String], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    plain
        .iter()
        .enumerate()
        .filter(|(_, row)| !matches(row, query).is_empty())
        .map(|(at, _)| at)
        .collect()
}

/// Where `query` occurs in `row`, as byte ranges. Lowercase queries ignore case, and a
/// query with a capital letter in it does not, as in `less` and `vim` with smartcase.
fn matches(row: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let sensitive = query.chars().any(char::is_uppercase);
    let (haystack, needle) = if sensitive {
        (row.to_owned(), query.to_owned())
    } else {
        (row.to_ascii_lowercase(), query.to_ascii_lowercase())
    };
    haystack
        .match_indices(&needle)
        .map(|(start, found)| start..start + found.len())
        .collect()
}

/// `line` with the bytes in `found` reversed, splitting spans where a match starts or
/// ends inside one. The ranges index the line's text with its spans joined.
fn highlight(line: &Line<'static>, found: &[Range<usize>]) -> Line<'static> {
    let mut spans = Vec::new();
    let mut offset = 0;
    for span in &line.spans {
        let text = span.content.as_ref();
        let end = offset + text.len();
        let mut cuts = vec![0, text.len()];
        for range in found {
            for edge in [range.start, range.end] {
                if edge > offset && edge < end && text.is_char_boundary(edge - offset) {
                    cuts.push(edge - offset);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for piece in cuts.windows(2) {
            let (from, to) = (piece[0], piece[1]);
            let inside = found
                .iter()
                .any(|range| range.start <= offset + from && offset + to <= range.end);
            let style = if inside {
                span.style.add_modifier(Modifier::REVERSED)
            } else {
                span.style
            };
            spans.push(Span::styled(text[from..to].to_owned(), style));
        }
        offset = end;
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyEventState;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    use super::*;
    use crate::brand::{ColorDepth, Palette};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn numbered(count: usize) -> String {
        (1..=count).map(|at| format!("row {at}\n")).collect()
    }

    /// A window of 40 columns whose body holds `page` rows.
    fn viewer(text: &str, page: u16) -> Viewer {
        Viewer::new("/findings", text, (40, page + CHROME_ROWS))
    }

    fn look() -> Look {
        Look {
            palette: Palette::with_depth(ColorDepth::Monochrome),
        }
    }

    #[test]
    fn the_page_is_the_window_less_the_header_and_footer() {
        let shown = viewer(&numbered(100), 10);
        assert_eq!(shown.page, 10);
        let tiny = Viewer::new("/x", &numbered(5), (40, 1));
        assert_eq!(
            tiny.page, 1,
            "a window too small for its chrome still shows a row"
        );
    }

    #[test]
    fn scrolling_stops_at_the_first_row_and_at_the_last_full_page() {
        let mut shown = viewer(&numbered(100), 10);
        shown.key(press(KeyCode::Up));
        assert_eq!(shown.top, 0);
        shown.key(press(KeyCode::Char('j')));
        shown.key(press(KeyCode::Down));
        assert_eq!(shown.top, 2);
        shown.key(press(KeyCode::Char('G')));
        assert_eq!(shown.top, 90);
        shown.key(press(KeyCode::Char('j')));
        assert_eq!(shown.top, 90, "the last page stays full");
        shown.key(press(KeyCode::Char('g')));
        assert_eq!(shown.top, 0);
    }

    #[test]
    fn paging_moves_a_whole_page_and_clamps_at_either_end() {
        let mut shown = viewer(&numbered(25), 10);
        shown.key(press(KeyCode::Char(' ')));
        assert_eq!(shown.top, 10);
        shown.key(press(KeyCode::PageDown));
        assert_eq!(shown.top, 15);
        shown.key(press(KeyCode::Char('b')));
        assert_eq!(shown.top, 5);
        shown.key(press(KeyCode::PageUp));
        assert_eq!(shown.top, 0);
    }

    #[test]
    fn output_shorter_than_the_page_does_not_scroll() {
        let mut shown = viewer(&numbered(3), 10);
        shown.key(press(KeyCode::Char('G')));
        shown.key(press(KeyCode::Char(' ')));
        assert_eq!(shown.top, 0);
    }

    #[test]
    fn the_wheel_scrolls_three_rows_a_notch() {
        let mut shown = viewer(&numbered(100), 10);
        let wheel = |kind| MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        shown.mouse(wheel(MouseEventKind::ScrollDown));
        assert_eq!(shown.top, 3);
        shown.mouse(wheel(MouseEventKind::ScrollUp));
        shown.mouse(wheel(MouseEventKind::ScrollUp));
        assert_eq!(shown.top, 0);
    }

    #[test]
    fn q_esc_and_ctrl_c_close_and_other_keys_keep_it_open() {
        for code in [KeyCode::Char('q'), KeyCode::Esc] {
            assert_eq!(viewer("a\n", 5).key(press(code)), Step::Close);
        }
        let mut control_c = press(KeyCode::Char('c'));
        control_c.modifiers = KeyModifiers::CONTROL;
        assert_eq!(viewer("a\n", 5).key(control_c), Step::Close);
        assert_eq!(viewer("a\n", 5).key(press(KeyCode::Char('x'))), Step::Stay);
    }

    #[test]
    fn a_resize_rewraps_and_keeps_the_top_row_in_range() {
        let mut shown = viewer(&"x".repeat(80), 10);
        assert_eq!(shown.rows.len(), 2);
        shown.resize((20, 12));
        assert_eq!(shown.rows.len(), 4);
        let mut long = viewer(&numbered(100), 10);
        long.key(press(KeyCode::Char('G')));
        long.resize((40, 52));
        assert_eq!(long.top, 50);
    }

    fn search(viewer: &mut Viewer, query: &str) {
        viewer.key(press(KeyCode::Char('/')));
        for character in query.chars() {
            viewer.key(press(KeyCode::Char(character)));
        }
        viewer.key(press(KeyCode::Enter));
    }

    #[test]
    fn matching_ignores_escapes_and_lowercase_queries_ignore_case() {
        assert_eq!(matches("Dead export, dead code", "dead"), [0..4, 13..17]);
        assert_eq!(matches("Dead export, dead code", "Dead"), vec![0..4]);
        assert!(matches("anything", "").is_empty());
        let coloured = ansi::lines("\x1b[31mdead\x1b[0m-\x1b[1mexport\x1b[0m\n", 40);
        assert_eq!(matches(&plain(&coloured[0]), "d-e"), vec![3..6]);
    }

    #[test]
    fn a_search_jumps_to_the_first_match_and_n_walks_the_rest() {
        let text: String = (1..=100)
            .map(|at| {
                if at % 25 == 0 {
                    format!("dead-export {at}\n")
                } else {
                    format!("row {at}\n")
                }
            })
            .collect();
        let mut shown = viewer(&text, 10);
        search(&mut shown, "dead");
        assert_eq!(shown.search.hits, [24, 49, 74, 99]);
        assert_eq!(shown.top, 24);
        shown.key(press(KeyCode::Char('n')));
        assert_eq!(shown.top, 49);
        shown.key(press(KeyCode::Char('n')));
        shown.key(press(KeyCode::Char('n')));
        assert_eq!(
            shown.top, 90,
            "a match on the last page leaves the page full"
        );
        shown.key(press(KeyCode::Char('n')));
        assert_eq!(shown.top, 24, "n wraps to the first match");
        shown.key(press(KeyCode::Char('N')));
        assert_eq!(shown.search.current, Some(3));
        assert_eq!(shown.found(), "match 4 of 4");
    }

    #[test]
    fn esc_while_typing_cancels_the_search_but_not_the_viewer() {
        let mut shown = viewer(&numbered(30), 10);
        shown.key(press(KeyCode::Char('/')));
        shown.key(press(KeyCode::Char('r')));
        assert_eq!(
            shown.key(press(KeyCode::Char('q'))),
            Step::Stay,
            "q is typed"
        );
        assert_eq!(shown.key(press(KeyCode::Esc)), Step::Stay);
        assert!(shown.search.typing.is_none());
        assert!(shown.search.query.is_empty());
        assert_eq!(shown.key(press(KeyCode::Esc)), Step::Close);
    }

    #[test]
    fn a_search_with_no_match_says_so_and_stays_put() {
        let mut shown = viewer(&numbered(30), 10);
        shown.key(press(KeyCode::Char(' ')));
        search(&mut shown, "absent");
        assert_eq!(shown.top, 10);
        assert_eq!(shown.found(), "no match for `absent`");
    }

    #[test]
    fn a_match_is_reversed_across_the_spans_it_spans() {
        let line = Line::from(vec![
            Span::styled("dead", Style::new().fg(Color::Red)),
            Span::raw("-export"),
        ]);
        let marked = highlight(&line, &matches("dead-export", "d-e"));
        let pieces: Vec<(&str, bool)> = marked
            .spans
            .iter()
            .map(|span| {
                (
                    span.content.as_ref(),
                    span.style.add_modifier.contains(Modifier::REVERSED),
                )
            })
            .collect();
        assert_eq!(
            pieces,
            [("dea", false), ("d", true), ("-e", true), ("xport", false)]
        );
        assert_eq!(marked.spans[1].style.fg, Some(Color::Red));
    }

    #[test]
    fn the_frame_has_the_title_and_position_above_and_the_keys_below() {
        let mut terminal = Terminal::new(TestBackend::new(60, 6)).unwrap();
        let mut shown = Viewer::new("/findings", &numbered(1575), (60, 6));
        shown.key(press(KeyCode::Char(' ')));
        terminal.draw(|frame| shown.draw(frame, &look())).unwrap();
        let buffer = terminal.backend().buffer();
        let row = |at: u16| -> String {
            (0..60)
                .map(|column| buffer[(column, at)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        };
        assert_eq!(row(0), " /findings \u{b7} lines 5\u{2013}8 of 1,575");
        assert_eq!(row(1), "row 5");
        assert_eq!(row(4), "row 8");
        assert!(
            row(5).starts_with(" \u{2191}\u{2193} j k line"),
            "{}",
            row(5)
        );
    }
}

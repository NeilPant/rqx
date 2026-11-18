//! The completion menu's state: the suggestions for the line as it is now, which one is
//! chosen, whether the menu is open, and which rows of a long list are in view.
//!
//! The list is never cut short. The menu shows a window of it that follows the choice,
//! and the window's height is the caller's to say because it depends on the terminal.

use super::completion::{self, Catalogue, Suggestion};
use unicode_width::UnicodeWidthStr;

use super::view::IDLE_ROWS;

/// The most rows the menu shows at once.
pub const MAX_ROWS: usize = 10;
/// The row under the window that says how much of the list is out of view.
const OVERFLOW_ROWS: u16 = 1;
/// Rows of the terminal above the session's own that stay on screen with the menu open.
const CONTEXT_ROWS: u16 = 2;

/// How many suggestions fit in a terminal this tall: all of `MAX_ROWS` when there is
/// room, and fewer when the prompt, the status line, the overflow row and a little of
/// the scrollback would not otherwise fit. Never less than one.
pub fn window_rows(terminal_rows: u16) -> usize {
    let spare = terminal_rows.saturating_sub(IDLE_ROWS + OVERFLOW_ROWS + CONTEXT_ROWS);
    usize::from(spare).clamp(1, MAX_ROWS)
}

/// What the completion menu holds for the line as it is now.
#[derive(Default)]
pub struct MenuState {
    pub suggestions: Vec<Suggestion>,
    pub selected: usize,
    /// The first suggestion in view. Kept in step with `selected` by `reveal`.
    top: usize,
    /// Closed with esc, or hidden because the line came from the history. It stays
    /// closed through further typing until a `-` or Tab asks for it, or the line is
    /// emptied or sent.
    pub dismissed: bool,
    /// Where the word is that Tab asked the flags for, so the menu keeps offering them
    /// while that word is typed and not for the next one.
    flags_from: Option<usize>,
}

impl MenuState {
    /// A menu freshly opened on these suggestions, with the first one chosen.
    pub fn new(suggestions: Vec<Suggestion>) -> Self {
        Self {
            suggestions,
            ..Self::default()
        }
    }

    /// What Tab opens when no menu is showing: the command's flags, kept on offer while
    /// the word they start is typed.
    pub fn flags_asked_for(catalogue: &Catalogue, before: &str) -> Self {
        Self {
            suggestions: catalogue.suggest_with_flags(before),
            flags_from: Some(completion::word_start(before)),
            ..Self::default()
        }
    }

    /// The menu after typing into the line it was built for. A menu closed with esc
    /// stays closed through the typing, and `reopens` (a `-`, or an emptied line) opens
    /// it again.
    pub fn after_edit(&self, catalogue: &Catalogue, before: &str, reopens: bool) -> Self {
        let flags_from = self
            .flags_from
            .filter(|from| *from == completion::word_start(before));
        Self {
            suggestions: if flags_from.is_some() {
                catalogue.suggest_with_flags(before)
            } else {
                catalogue.suggest(before)
            },
            dismissed: self.dismissed && !reopens,
            flags_from,
            ..Self::default()
        }
    }

    pub fn visible(&self) -> bool {
        !self.dismissed && !self.suggestions.is_empty()
    }

    pub fn chosen(&self) -> Option<&Suggestion> {
        self.visible()
            .then(|| self.suggestions.get(self.selected))
            .flatten()
    }

    /// One row at a time, wrapping at the ends.
    pub fn step(&mut self, forward: bool, window: usize) {
        let count = self.suggestions.len();
        let at = if forward {
            (self.selected + 1) % count
        } else {
            (self.selected + count - 1) % count
        };
        self.select(at, window);
    }

    /// A window's worth of rows at a time, stopping at the ends.
    pub fn page(&mut self, forward: bool, window: usize) {
        self.nudge(forward, window, window);
    }

    pub fn first(&mut self, window: usize) {
        self.select(0, window);
    }

    pub fn last(&mut self, window: usize) {
        self.select(self.suggestions.len().saturating_sub(1), window);
    }

    fn nudge(&mut self, forward: bool, by: usize, window: usize) {
        let last = self.suggestions.len().saturating_sub(1);
        let at = if forward {
            (self.selected + by).min(last)
        } else {
            self.selected.saturating_sub(by)
        };
        self.select(at, window);
    }

    fn select(&mut self, at: usize, window: usize) {
        self.selected = at.min(self.suggestions.len().saturating_sub(1));
        self.reveal(window);
    }

    /// Moves the window the least that keeps the choice in it, and keeps it from
    /// running past the end of the list, which a taller terminal or a shorter list
    /// would otherwise leave as blank rows.
    pub fn reveal(&mut self, window: usize) {
        let window = window.max(1);
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + window {
            self.top = self.selected + 1 - window;
        }
        self.top = self.top.min(self.suggestions.len().saturating_sub(window));
    }

    /// The suggestions in view.
    pub fn shown(&self, window: usize) -> &[Suggestion] {
        let end = (self.top + window).min(self.suggestions.len());
        &self.suggestions[self.top.min(end)..end]
    }

    /// The choice's place among `shown`.
    pub fn selected_in_view(&self) -> usize {
        self.selected.saturating_sub(self.top)
    }

    /// The widest label in the whole list, which keeps the descriptions in one column
    /// while the window moves over it.
    pub fn label_width(&self) -> usize {
        self.suggestions
            .iter()
            .map(|suggestion| suggestion.label.width())
            .max()
            .unwrap_or(0)
    }

    pub fn hidden_above(&self) -> usize {
        self.top
    }

    pub fn hidden_below(&self, window: usize) -> usize {
        self.suggestions
            .len()
            .saturating_sub(self.top + self.shown(window).len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(count: usize) -> MenuState {
        MenuState {
            suggestions: (0..count)
                .map(|at| Suggestion {
                    label: format!("/c{at}"),
                    about: String::new(),
                    from: 0,
                    typed: String::new(),
                    insert: format!("/c{at} "),
                })
                .collect(),
            ..MenuState::default()
        }
    }

    fn range_in_view(menu: &MenuState, window: usize) -> std::ops::Range<usize> {
        menu.hidden_above()..menu.suggestions.len() - menu.hidden_below(window)
    }

    #[test]
    fn the_choice_wraps_in_both_directions() {
        let mut menu = menu(3);
        menu.step(false, 6);
        assert_eq!(menu.selected, 2);
        menu.step(true, 6);
        assert_eq!(menu.selected, 0);
        menu.step(true, 6);
        assert_eq!(menu.chosen().map(|s| s.label.as_str()), Some("/c1"));
    }

    fn catalogue() -> Catalogue {
        Catalogue::new([])
    }

    #[test]
    fn a_menu_closed_with_esc_stays_closed_until_a_dash() {
        let catalogue = catalogue();
        let mut menu = MenuState::default().after_edit(&catalogue, "/plan", false);
        assert!(menu.visible());
        menu.dismissed = true;
        let typing = menu.after_edit(&catalogue, "/plan ", false);
        assert!(!typing.visible());
        let typing = typing.after_edit(&catalogue, "/plan b", false);
        assert!(!typing.visible());
        let dashed = typing.after_edit(&catalogue, "/plan b-", true);
        assert!(!dashed.dismissed);
    }

    #[test]
    fn esc_then_a_dash_offers_the_flags_again() {
        let catalogue = catalogue();
        let mut menu = MenuState::default().after_edit(&catalogue, "/plan ", false);
        menu.dismissed = true;
        let dashed = menu.after_edit(&catalogue, "/plan -", true);
        assert!(dashed.visible());
        assert!(dashed.chosen().is_some_and(|s| s.label.starts_with("--")));
    }

    #[test]
    fn an_emptied_line_opens_the_menu_again() {
        let catalogue = catalogue();
        let mut menu = MenuState::default().after_edit(&catalogue, "/pl", false);
        menu.dismissed = true;
        let emptied = menu.after_edit(&catalogue, "", true);
        assert!(!emptied.dismissed);
        let slash = emptied.after_edit(&catalogue, "/", false);
        assert!(slash.visible());
    }

    #[test]
    fn tab_opens_the_flags_and_keeps_them_while_their_word_is_typed() {
        let catalogue = catalogue();
        let asked = MenuState::flags_asked_for(&catalogue, "/plan ");
        assert!(asked.visible());
        let typing = asked.after_edit(&catalogue, "/plan bud", false);
        assert_eq!(typing.chosen().map(|s| s.label.as_str()), Some("--budget"));
        let next_word = typing.after_edit(&catalogue, "/plan bud ", false);
        assert!(!next_word.visible());
    }

    #[test]
    fn a_dismissed_or_empty_menu_offers_nothing() {
        let mut shown = menu(2);
        assert!(shown.visible());
        shown.dismissed = true;
        assert!(!shown.visible());
        assert!(shown.chosen().is_none());
        assert!(!menu(0).visible());
    }

    #[test]
    fn the_window_stays_put_until_the_choice_leaves_it() {
        let mut menu = menu(30);
        assert_eq!(range_in_view(&menu, 10), 0..10);
        for _ in 0..9 {
            menu.step(true, 10);
        }
        assert_eq!(menu.selected, 9);
        assert_eq!(range_in_view(&menu, 10), 0..10);
        menu.step(true, 10);
        assert_eq!(menu.selected, 10);
        assert_eq!(range_in_view(&menu, 10), 1..11);
        assert_eq!(menu.selected_in_view(), 9);
    }

    #[test]
    fn the_window_scrolls_back_up_when_the_choice_goes_above_it() {
        let mut menu = menu(30);
        menu.last(10);
        assert_eq!(range_in_view(&menu, 10), 20..30);
        for _ in 0..10 {
            menu.step(false, 10);
        }
        assert_eq!(menu.selected, 19);
        assert_eq!(range_in_view(&menu, 10), 19..29);
        assert_eq!(menu.selected_in_view(), 0);
    }

    #[test]
    fn wrapping_at_either_end_brings_the_other_end_into_view() {
        let mut menu = menu(30);
        menu.step(false, 10);
        assert_eq!(menu.selected, 29);
        assert_eq!(range_in_view(&menu, 10), 20..30);
        menu.step(true, 10);
        assert_eq!(menu.selected, 0);
        assert_eq!(range_in_view(&menu, 10), 0..10);
    }

    #[test]
    fn a_page_moves_by_the_window_and_stops_at_the_ends() {
        let mut menu = menu(25);
        menu.page(true, 10);
        assert_eq!((menu.selected, range_in_view(&menu, 10)), (10, 1..11));
        menu.page(true, 10);
        menu.page(true, 10);
        assert_eq!((menu.selected, range_in_view(&menu, 10)), (24, 15..25));
        menu.page(false, 10);
        assert_eq!(menu.selected, 14);
        menu.page(false, 10);
        menu.page(false, 10);
        assert_eq!((menu.selected, range_in_view(&menu, 10)), (0, 0..10));
    }

    #[test]
    fn home_and_end_jump_to_the_ends_of_the_list() {
        let mut menu = menu(30);
        menu.last(10);
        assert_eq!((menu.selected, range_in_view(&menu, 10)), (29, 20..30));
        menu.first(10);
        assert_eq!((menu.selected, range_in_view(&menu, 10)), (0, 0..10));
    }

    #[test]
    fn a_list_that_fits_has_nothing_hidden() {
        let mut menu = menu(4);
        menu.last(10);
        assert_eq!(range_in_view(&menu, 10), 0..4);
        assert_eq!((menu.hidden_above(), menu.hidden_below(10)), (0, 0));
        menu.page(true, 10);
        assert_eq!(menu.selected, 3);
    }

    #[test]
    fn a_taller_window_pulls_the_list_back_so_no_row_is_blank() {
        let mut menu = menu(30);
        menu.last(5);
        assert_eq!(range_in_view(&menu, 5), 25..30);
        menu.reveal(10);
        assert_eq!(range_in_view(&menu, 10), 20..30);
    }

    #[test]
    fn a_shorter_window_keeps_the_choice_in_view() {
        let mut menu = menu(30);
        menu.last(10);
        menu.reveal(3);
        assert_eq!(range_in_view(&menu, 3), 27..30);
        assert_eq!(menu.selected_in_view(), 2);
    }

    #[test]
    fn typing_starts_the_list_again_from_the_top() {
        let catalogue = catalogue();
        let mut menu = MenuState::default().after_edit(&catalogue, "/", false);
        menu.last(10);
        assert!(menu.hidden_above() > 0);
        let narrowed = menu.after_edit(&catalogue, "/s", false);
        assert_eq!(narrowed.selected, 0);
        assert_eq!(narrowed.hidden_above(), 0);
    }

    #[test]
    fn the_window_is_ten_rows_unless_the_terminal_is_short() {
        assert_eq!(window_rows(60), 10);
        assert_eq!(window_rows(17), 10);
        assert_eq!(window_rows(16), 9);
        assert_eq!(window_rows(14), 7);
        assert_eq!(window_rows(8), 1);
        assert_eq!(window_rows(0), 1);
    }

    #[test]
    fn the_menu_and_the_session_always_fit_the_terminal() {
        for rows in 14..60u16 {
            let window = u16::try_from(window_rows(rows)).unwrap();
            let height = IDLE_ROWS + window + 1;
            assert!(
                height + CONTEXT_ROWS <= rows,
                "{rows} rows, {height} needed"
            );
        }
    }
}

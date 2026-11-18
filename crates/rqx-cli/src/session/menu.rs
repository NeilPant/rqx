//! The completion menu's state: the suggestions for the line as it is now, which one is
//! chosen, and whether the menu is open.

use super::completion::{self, Catalogue, Suggestion};

/// What the completion menu holds for the line as it is now.
#[derive(Default)]
pub struct MenuState {
    pub suggestions: Vec<Suggestion>,
    pub selected: usize,
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
            selected: 0,
            dismissed: false,
            flags_from: Some(completion::word_start(before)),
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
            selected: 0,
            dismissed: self.dismissed && !reopens,
            flags_from,
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

    pub fn step(&mut self, forward: bool) {
        let count = self.suggestions.len();
        self.selected = if forward {
            (self.selected + 1) % count
        } else {
            (self.selected + count - 1) % count
        };
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
            selected: 0,
            dismissed: false,
            flags_from: None,
        }
    }

    #[test]
    fn the_choice_wraps_in_both_directions() {
        let mut menu = menu(3);
        menu.step(false);
        assert_eq!(menu.selected, 2);
        menu.step(true);
        assert_eq!(menu.selected, 0);
        menu.step(true);
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
}

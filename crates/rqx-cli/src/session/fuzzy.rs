//! Ranking for anything the user types a few letters of: the command palette and the
//! session's completion menu both ask the same question, so they share the answer.

/// A subsequence match, ranked by how tightly the letters sit together and how early
/// they start; lower is better. Not a full fuzzy scorer: the labels are short, and
/// "cplx" reaching `complexity` is the whole requirement.
pub fn score(label: &str, typed: &str) -> Option<usize> {
    let typed = typed.to_lowercase();
    if typed.is_empty() {
        return Some(0);
    }
    let label = label.to_lowercase();
    if let Some(at) = label.find(&typed) {
        return Some(at);
    }
    let mut rank = 0;
    let mut last = None;
    let mut haystack = label.char_indices();
    for wanted in typed.chars() {
        let at = haystack.find(|(_, character)| *character == wanted)?.0;
        rank += at - last.unwrap_or(0) + 1;
        last = Some(at);
    }
    Some(rank + label.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_beats_a_scattered_match() {
        let prefix = score("complexity", "com").expect("a match");
        let scattered = score("cross-module", "com").expect("a match");
        assert!(prefix < scattered, "{prefix} !< {scattered}");
    }

    #[test]
    fn the_letters_of_a_word_reach_it_in_order() {
        assert!(score("complexity", "cplx").is_some());
        assert!(
            score("complexity", "xlpc").is_none(),
            "order does not matter"
        );
    }

    #[test]
    fn nothing_typed_matches_everything_equally() {
        assert_eq!(score("anything", ""), Some(0));
    }

    #[test]
    fn case_is_ignored_on_both_sides() {
        assert!(score("Complexity", "CPLX").is_some());
    }
}

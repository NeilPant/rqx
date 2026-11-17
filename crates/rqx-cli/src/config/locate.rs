//! Where a name sits in a config file, for the problems a parse cannot place.
//!
//! The parser reports the line of what it refuses. What only the merged config can
//! know — a rule name that is not in the catalogue, a threshold nothing reads — has
//! lost its line by then, so it is found again by reading the text.

use super::files::Position;

fn is_name_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | '.')
}

fn uncommented(line: &str) -> &str {
    line.find(" #").map_or(line, |start| &line[..start])
}

fn position_of(line: usize, column: usize) -> Position {
    Position {
        line: line as u64 + 1,
        column: column as u64 + 1,
    }
}

/// The first place `name` stands as a whole word, such as an item of a list.
pub fn name(text: &str, name: &str) -> Option<Position> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with('#'))
        .find_map(|(number, line)| {
            let code = uncommented(line);
            code.match_indices(name)
                .find(|(start, _)| {
                    let before = code[..*start].chars().next_back();
                    let after = code[start + name.len()..].chars().next();
                    !before.is_some_and(is_name_character) && !after.is_some_and(is_name_character)
                })
                .map(|(start, _)| position_of(number, code[..start].chars().count()))
        })
}

/// The first place `key` is written as a mapping key.
pub fn key(text: &str, key: &str) -> Option<Position> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with('#'))
        .find_map(|(number, line)| {
            let rest = line.trim_start().trim_start_matches("- ");
            let column = line.len() - rest.len();
            rest.strip_prefix(key)?
                .trim_start()
                .starts_with(':')
                .then(|| position_of(number, line[..column].chars().count()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# disabled: [commented]\nrules:\n  disabled: [magic-number, nonsense] # nonsense\nthresholds:\n  max_params: 3\n";

    #[test]
    fn a_name_is_found_as_a_whole_word_outside_comments() {
        assert_eq!(
            name(TEXT, "nonsense"),
            Some(Position {
                line: 3,
                column: 28
            })
        );
        assert_eq!(name(TEXT, "magic"), None);
        assert_eq!(name(TEXT, "commented"), None);
    }

    #[test]
    fn a_key_is_found_where_it_opens_a_line() {
        assert_eq!(
            key(TEXT, "max_params"),
            Some(Position { line: 5, column: 3 })
        );
        assert_eq!(key(TEXT, "nonsense"), None);
    }
}

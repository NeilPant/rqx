//! Greedy word wrapping for the prose in the terminal report, measured in display
//! columns so a wide character takes the two it draws and a colour code takes none.

use unicode_width::UnicodeWidthChar;

/// `text` as lines no wider than `width`, broken at spaces. A word wider than a line
/// keeps a line to itself rather than being cut, because a path or an identifier split
/// in two cannot be searched for. Text that already fits is returned untouched.
pub fn lines(text: &str, width: usize) -> Vec<String> {
    let mut wrapped = Vec::new();
    for paragraph in text.lines() {
        if visible_width(paragraph) <= width {
            wrapped.push(paragraph.to_owned());
        } else {
            wrap_paragraph(paragraph, width, &mut wrapped);
        }
    }
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
}

fn wrap_paragraph(paragraph: &str, width: usize, wrapped: &mut Vec<String>) {
    let mut line = String::new();
    let mut used = 0;
    for word in paragraph.split_whitespace() {
        let span = visible_width(word);
        if used > 0 && used + 1 + span > width {
            wrapped.push(std::mem::take(&mut line));
            used = 0;
        }
        if used > 0 {
            line.push(' ');
            used += 1;
        }
        line.push_str(word);
        used += span;
    }
    wrapped.push(line);
}

/// Columns the text takes on screen. A colour code is skipped whole: counting its
/// characters would make every painted word look wider than it draws.
pub fn visible_width(text: &str) -> usize {
    let mut width = 0;
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            skip_escape(&mut characters);
        } else {
            width += character.width().unwrap_or(0);
        }
    }
    width
}

fn skip_escape(characters: &mut std::str::Chars<'_>) {
    if characters.next() != Some('[') {
        return;
    }
    for character in characters.by_ref() {
        if ('@'..='~').contains(&character) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_that_fits_is_one_untouched_line() {
        assert_eq!(lines("a  short   line", 40), ["a  short   line"]);
    }

    #[test]
    fn a_line_breaks_at_the_last_space_that_fits() {
        assert_eq!(lines("one two three four", 9), ["one two", "three", "four"]);
    }

    #[test]
    fn a_line_may_be_exactly_as_wide_as_the_room() {
        assert_eq!(lines("abc def", 7), ["abc def"]);
        assert_eq!(lines("abc defg", 7), ["abc", "defg"]);
    }

    #[test]
    fn a_word_wider_than_the_room_is_kept_whole() {
        assert_eq!(
            lines("see src/very/long/path.ts now", 8),
            ["see", "src/very/long/path.ts", "now"]
        );
    }

    #[test]
    fn wide_characters_count_for_the_columns_they_draw() {
        assert_eq!(visible_width("日本語"), 6);
        // Three of these are six columns, so a fourth does not fit in eight.
        assert_eq!(lines("日本語 日本語", 8), ["日本語", "日本語"]);
        assert_eq!(lines("日本 日本", 9), ["日本 日本"]);
    }

    #[test]
    fn a_colour_code_takes_no_room() {
        let painted = "\u{1b}[32mgreen\u{1b}[0m word";
        assert_eq!(visible_width(painted), 10);
        assert_eq!(lines(painted, 10), [painted]);
    }

    #[test]
    fn empty_text_is_one_empty_line() {
        assert_eq!(lines("", 10), [""]);
    }
}

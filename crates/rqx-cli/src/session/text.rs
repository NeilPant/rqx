//! Small text helpers the session's views share.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn thousands(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `text` cut to `width` columns, with an ellipsis where it was cut.
pub fn clip(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut clipped = String::new();
    let mut used = 0;
    for character in text.chars() {
        let step = character.width().unwrap_or(0);
        if used + step + 1 > width {
            break;
        }
        clipped.push(character);
        used += step;
    }
    clipped.push('\u{2026}');
    clipped
}

/// Seconds as a reader wants them: tenths below ten, whole above.
pub fn seconds(milliseconds: u128) -> String {
    if milliseconds < 10_000 {
        format!("{:.2}s", milliseconds as f64 / 1000.0)
    } else {
        format!("{}s", milliseconds / 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_groups_digits_in_threes() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(3456), "3,456");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn clipping_marks_the_cut_and_leaves_short_text_alone() {
        assert_eq!(clip("src/api.ts", 20), "src/api.ts");
        assert_eq!(clip("src/api/client.ts", 8), "src/api\u{2026}");
        assert_eq!(clip("anything", 0), "\u{2026}");
    }

    #[test]
    fn short_scans_are_timed_to_the_hundredth() {
        assert_eq!(seconds(410), "0.41s");
        assert_eq!(seconds(12_400), "12s");
    }
}

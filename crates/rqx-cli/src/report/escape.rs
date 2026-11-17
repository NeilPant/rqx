/// The five characters XML and HTML reserve. Hand-written because a whole parser crate
/// to emit a page of markup is a dependency nobody should audit.
pub fn markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 has no escape for these at all, so they are dropped rather
            // than written literally, which would make the document unparseable.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reserved_character_is_escaped() {
        assert_eq!(markup(r#"a<b>&"c'"#), "a&lt;b&gt;&amp;&quot;c&apos;");
    }

    #[test]
    fn a_control_character_is_dropped_rather_than_written() {
        assert_eq!(markup("a\u{1}b"), "ab");
        assert_eq!(markup("a\tb"), "a\tb");
    }
}

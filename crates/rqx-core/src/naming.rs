/// Names that describe a category rather than a value. Kept deliberately short:
/// this rule is the only `Confidence::Low` rule in Rqx and ships disabled, so a
/// wide list would cost trust without buying a single actionable finding.
const PLACEHOLDERS: &[&str] = &[
    "data", "info", "temp", "tmp", "obj", "res", "ret", "foo", "bar", "baz", "qux", "thing",
    "things", "stuff", "misc", "aux", "dummy", "asdf", "blah",
];

pub fn classify(identifier: &str) -> Option<&'static str> {
    let lowered = identifier.to_ascii_lowercase();
    let stem = lowered.trim_end_matches(|c: char| c.is_ascii_digit());
    if !PLACEHOLDERS.contains(&stem) {
        return None;
    }
    if stem.len() == lowered.len() {
        Some("names a category, not the value it holds")
    } else {
        Some("is a numbered placeholder — the digit is doing the naming")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_placeholders_and_their_numbered_forms() {
        assert!(classify("data").is_some());
        assert!(classify("Temp").is_some());
        assert_ne!(classify("data2"), classify("data"));
        assert!(classify("data2").is_some());
    }

    #[test]
    fn leaves_real_names_alone() {
        for name in ["order", "reconcileBatch", "userId", "dataSource", "metadata", "i", "_"] {
            assert_eq!(classify(name), None, "{name} should not be flagged");
        }
    }
}

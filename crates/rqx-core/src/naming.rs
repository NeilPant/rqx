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

/// The closest candidate within an edit or three, for the moment a reader mistypes a
/// rule or a family. Sixty slugs printed as a list is not an answer to a typo.
pub fn nearest<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let input = input.to_ascii_lowercase();
    candidates
        .iter()
        .map(|candidate| {
            (
                *candidate,
                distance(&input, &candidate.to_ascii_lowercase()),
            )
        })
        .filter(|(_, distance)| *distance <= 3)
        .min_by_key(|(candidate, distance)| (*distance, candidate.len()))
        .map(|(candidate, _)| candidate)
}

/// Two rows rather than a matrix: the whole point is to rank sixty short strings.
fn distance(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (row, letter) in left.chars().enumerate() {
        current[0] = row + 1;
        for (column, other) in right.iter().enumerate() {
            let substitution = previous[column] + usize::from(letter != *other);
            current[column + 1] = substitution
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_near_miss_is_suggested_and_a_wild_guess_is_not() {
        let slugs = ["unused-export", "unused-file", "long-function"];
        assert_eq!(nearest("unused-exprot", &slugs), Some("unused-export"));
        assert_eq!(nearest("unused-exports", &slugs), Some("unused-export"));
        assert_eq!(nearest("banana", &slugs), None);
    }

    #[test]
    fn flags_placeholders_and_their_numbered_forms() {
        assert!(classify("data").is_some());
        assert!(classify("Temp").is_some());
        assert_ne!(classify("data2"), classify("data"));
        assert!(classify("data2").is_some());
    }

    #[test]
    fn leaves_real_names_alone() {
        for name in [
            "order",
            "reconcileBatch",
            "userId",
            "dataSource",
            "metadata",
            "i",
            "_",
        ] {
            assert_eq!(classify(name), None, "{name} should not be flagged");
        }
    }
}

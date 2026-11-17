//! "Did you mean" for a name that matched nothing.

/// The candidate closest to `name`, when one is close enough to be a likely typo
/// rather than a coincidence. Edit distance bounded by a third of the name's length,
/// so a three-letter word is not "corrected" into an unrelated one.
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let allowed = (name.chars().count() / 3).clamp(1, 3);
    candidates
        .into_iter()
        .map(|candidate| (distance(name, candidate), candidate))
        .filter(|(far, _)| *far <= allowed)
        .min_by_key(|(far, _)| *far)
        .map(|(_, candidate)| candidate)
}

/// Levenshtein distance over characters, two rows at a time.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (row, left) in a.chars().enumerate() {
        let mut current = vec![row + 1];
        for (column, right) in b.iter().enumerate() {
            let substitution = previous[column] + usize::from(left != *right);
            let insertion = current[column] + 1;
            let deletion = previous[column + 1] + 1;
            current.push(substitution.min(insertion).min(deletion));
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typo_finds_the_name_it_was_meant_to_be() {
        assert_eq!(
            closest("max_param", ["max_params", "nesting_depth"]),
            Some("max_params")
        );
        assert_eq!(
            closest("disabld", ["disabled", "enabled", "categories"]),
            Some("disabled")
        );
        assert_eq!(
            closest("legacy-adoptoin", ["legacy-adoption", "strict"]),
            Some("legacy-adoption")
        );
    }

    #[test]
    fn something_unrelated_finds_nothing() {
        assert_eq!(closest("banana", ["max_params", "nesting_depth"]), None);
        assert_eq!(closest("xyz", ["strict", "perf"]), None);
    }
}

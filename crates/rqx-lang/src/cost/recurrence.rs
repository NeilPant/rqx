//! What a function's calls to itself do to the size of the problem. A call with `n - 1`
//! is a linear walk, one with `n / 2` a halving, and everything else is not told apart.
//! The argument is read as text: it is short, and the shapes that matter are spelled
//! the same few ways.

use rqx_core::Language;
use rqx_core::cost::Shrink;

/// What one self-call does to the argument at the position of a parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reduction {
    pub shrink: Shrink,
    /// The argument is built as a copy of part of the parameter, so every call pays for
    /// its size on top of the recursion.
    pub copies: bool,
}

const OTHER: Reduction = Reduction {
    shrink: Shrink::Other,
    copies: false,
};

/// Names that stand for "the rest of the input" or "half of it".
const REST_NAMES: [&str; 5] = ["rest", "tail", "remaining", "others", "remainder"];
const HALF_NAMES: [&str; 4] = ["mid", "middle", "half", "halved"];

/// How `argument` compares with `parameter`, as the text of both.
pub fn reduction(language: Language, argument: &str, parameter: &str) -> Reduction {
    let text: String = argument.chars().filter(|c| !c.is_whitespace()).collect();
    let text = strip_parentheses(&text);
    if text == parameter || parameter.is_empty() {
        return OTHER;
    }
    if REST_NAMES.contains(&text) {
        return Reduction {
            shrink: Shrink::Dec1,
            copies: false,
        };
    }
    if HALF_NAMES.contains(&text) {
        return Reduction {
            shrink: Shrink::Half,
            copies: false,
        };
    }
    if let Some(rest) = text.strip_prefix(parameter) {
        return from_derived(language, rest);
    }
    // `Math.floor(n / 2)` and `int(n / 2)` wrap the expression they pass on.
    if text.contains(parameter)
        && let Some(inner) = text
            .find('(')
            .and_then(|open| text.strip_suffix(')').map(|body| &body[open + 1..]))
    {
        let inner = strip_parentheses(inner);
        if inner.starts_with(parameter) {
            return reduction(language, inner, parameter);
        }
    }
    OTHER
}

fn strip_parentheses(text: &str) -> &str {
    let mut text = text;
    while is_wrapped(text) {
        text = &text[1..text.len() - 1];
    }
    text
}

/// Whether one pair of parentheses encloses all of `text`, which `(a)+(b)` does not.
fn is_wrapped(text: &str) -> bool {
    if !text.starts_with('(') || !text.ends_with(')') {
        return false;
    }
    let mut depth = 0_u32;
    for (index, character) in text.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && index != text.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// What follows the parameter's name in the argument: `-1`, `/2`, `[1:]`, `.slice(1)`.
fn from_derived(language: Language, rest: &str) -> Reduction {
    let dec1 = Reduction {
        shrink: Shrink::Dec1,
        copies: false,
    };
    let half = Reduction {
        shrink: Shrink::Half,
        copies: false,
    };
    if let Some(amount) = rest.strip_prefix(['-', '+']) {
        return if amount.chars().all(|c| c.is_ascii_digit()) && !amount.is_empty() {
            dec1
        } else {
            OTHER
        };
    }
    for halving in ["/2", "//2", ">>1", "/=2"] {
        if rest == halving {
            return half;
        }
    }
    if let Some(divisor) = rest.strip_prefix("//").or_else(|| rest.strip_prefix('/'))
        && divisor.parse::<u32>().is_ok_and(|divisor| divisor >= 2)
    {
        return half;
    }
    slice_of(language, rest).unwrap_or(OTHER)
}

/// A slice of the parameter: from the front, which leaves one fewer, or around the
/// middle, which leaves half.
fn slice_of(language: Language, rest: &str) -> Option<Reduction> {
    let (inside, copies) = slice_arguments(language, rest)?;
    let halves = HALF_NAMES.iter().any(|name| inside.contains(name))
        || inside.contains("/2")
        || inside.contains(">>1");
    if halves {
        return Some(Reduction {
            shrink: Shrink::Half,
            copies,
        });
    }
    let drops_one = ["1:", "1,", "1)", ":-1", "1.."]
        .iter()
        .any(|shape| inside.starts_with(shape) || inside.ends_with(shape))
        || inside == "1";
    drops_one.then_some(Reduction {
        shrink: Shrink::Dec1,
        copies,
    })
}

/// The inside of the slice a language spells after a name, and whether making one
/// copies what it covers.
fn slice_arguments(language: Language, rest: &str) -> Option<(&str, bool)> {
    match language {
        Language::Python => Some((rest.strip_prefix('[')?.strip_suffix(']')?, true)),
        // A slice shares the array it was cut from.
        Language::Go => Some((rest.strip_prefix('[')?.strip_suffix(']')?, false)),
        Language::TypeScript | Language::JavaScript => [".slice(", ".substring(", ".substr("]
            .iter()
            .find_map(|opener| rest.strip_prefix(opener))
            .and_then(|inside| inside.strip_suffix(')'))
            .map(|inside| (inside, true)),
        Language::Java => rest
            .strip_prefix(".substring(")
            .map(|inside| (inside.trim_end_matches(')'), true))
            .or_else(|| {
                rest.strip_prefix(".subList(")
                    .map(|inside| (inside.trim_end_matches(')'), false))
            }),
        Language::Cpp => None,
    }
}

/// How several self-calls together shrink a problem: only when they all do it the
/// same way.
pub fn combined(reductions: &[Reduction]) -> Reduction {
    let Some(first) = reductions.first() else {
        return OTHER;
    };
    if reductions.iter().any(|other| other.shrink != first.shrink) {
        return OTHER;
    }
    Reduction {
        shrink: first.shrink,
        copies: reductions.iter().any(|reduction| reduction.copies),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shrink(language: Language, argument: &str) -> (Shrink, bool) {
        let result = reduction(language, argument, "n");
        (result.shrink, result.copies)
    }

    #[test]
    fn counting_down_by_a_constant_is_one_smaller() {
        for argument in ["n - 1", "n-2", "(n - 1)", "n + 1"] {
            assert_eq!(
                shrink(Language::TypeScript, argument),
                (Shrink::Dec1, false),
                "{argument}"
            );
        }
    }

    #[test]
    fn dividing_or_shifting_by_two_halves() {
        assert_eq!(shrink(Language::Java, "n / 2"), (Shrink::Half, false));
        assert_eq!(shrink(Language::Python, "n // 2"), (Shrink::Half, false));
        assert_eq!(shrink(Language::Cpp, "n >> 1"), (Shrink::Half, false));
        assert_eq!(shrink(Language::Go, "n / 4"), (Shrink::Half, false));
        assert_eq!(
            shrink(Language::TypeScript, "Math.floor(n / 2)"),
            (Shrink::Half, false)
        );
        assert_eq!(
            shrink(Language::Python, "int(n / 2)"),
            (Shrink::Half, false)
        );
    }

    #[test]
    fn a_name_that_says_the_rest_or_the_half_is_believed() {
        assert_eq!(shrink(Language::Python, "rest"), (Shrink::Dec1, false));
        assert_eq!(shrink(Language::Python, "mid"), (Shrink::Half, false));
    }

    #[test]
    fn dropping_the_first_element_is_one_smaller_and_copies_where_a_slice_copies() {
        assert_eq!(shrink(Language::Python, "n[1:]"), (Shrink::Dec1, true));
        assert_eq!(
            shrink(Language::TypeScript, "n.slice(1)"),
            (Shrink::Dec1, true)
        );
        assert_eq!(
            shrink(Language::Java, "n.substring(1)"),
            (Shrink::Dec1, true)
        );
        assert_eq!(
            shrink(Language::Java, "n.subList(1, n.size())"),
            (Shrink::Dec1, false)
        );
        assert_eq!(shrink(Language::Go, "n[1:]"), (Shrink::Dec1, false));
    }

    #[test]
    fn cutting_around_the_middle_is_half() {
        assert_eq!(shrink(Language::Python, "n[:mid]"), (Shrink::Half, true));
        assert_eq!(shrink(Language::Python, "n[mid:]"), (Shrink::Half, true));
        assert_eq!(
            shrink(Language::TypeScript, "n.slice(0, mid)"),
            (Shrink::Half, true)
        );
        assert_eq!(shrink(Language::Go, "n[:mid]"), (Shrink::Half, false));
    }

    #[test]
    fn anything_else_is_not_told_apart() {
        for argument in ["n", "n * 2", "k", "n - m", "left", "n % 2", "f(n)"] {
            assert_eq!(
                shrink(Language::Python, argument).0,
                Shrink::Other,
                "{argument}"
            );
        }
    }

    #[test]
    fn several_calls_shrink_the_problem_only_if_they_all_do_it_alike() {
        let dec1 = Reduction {
            shrink: Shrink::Dec1,
            copies: false,
        };
        let copying = Reduction {
            shrink: Shrink::Dec1,
            copies: true,
        };
        let half = Reduction {
            shrink: Shrink::Half,
            copies: false,
        };
        assert_eq!(combined(&[dec1, dec1]), dec1);
        assert_eq!(combined(&[dec1, copying]), copying);
        assert_eq!(combined(&[half, half]), half);
        assert_eq!(combined(&[dec1, half]).shrink, Shrink::Other);
        assert_eq!(combined(&[]).shrink, Shrink::Other);
    }
}

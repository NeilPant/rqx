//! The line above a corpus function that says what its cost is:
//!
//! ```text
//! // @complexity time=O(n*m) space=O(n) conf=verified [rule=linear-op-in-loop] [none=sort-in-loop]
//! ```
//!
//! `#` replaces `//` in Python. `time` and `space` are orders in the grammar of
//! `rqx_core::cost::Order`; `space` is the auxiliary space, counting the call stack.
//! `rule` names the rules that must fire on the function, separated by commas, `none`
//! one that must not, and `may` rules that cover the same code under another name, so
//! that firing counts for nothing and against nothing. `scan-in-loop` and
//! `linear-op-in-loop` overlap this way on a call that walks a collection in a loop.
//! `scope=local` says the time is what the function costs on its own, with no call to
//! another function and no recursion in it, so the per-file extraction alone can be
//! held to it. A function the extraction gets wrong is left unmarked, so that the check
//! stays a hard one for the functions it covers and the miss is still scored in the
//! composed outcome. Values may hold spaces (`O(n log n)`), so a field runs until the next
//! `key=`.

use std::fmt;

use rqx_core::cost::{Conf, Order};

const TAG: &str = "@complexity";
const KEYS: [&str; 7] = ["time", "space", "conf", "rule", "none", "may", "scope"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Annotation {
    /// One-based line of the comment.
    pub line: usize,
    /// One-based line of the first code line after it, which holds the function.
    pub target_line: usize,
    pub time: Order,
    pub space: Order,
    pub conf: Conf,
    pub rule: Option<String>,
    pub none: Option<String>,
    pub may: Option<String>,
    /// The time is the function's own, with nothing composed in.
    pub local: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// How a language starts a line comment, from the file extension.
pub fn comment_marker(extension: &str) -> Option<&'static str> {
    match extension {
        "ts" | "js" | "go" | "java" | "cpp" | "hpp" => Some("//"),
        "py" => Some("#"),
        _ => None,
    }
}

/// Every annotation in a file, and every mistake made writing one. A tag that is
/// malformed is a mistake and is not also an annotation, so a corpus with mistakes
/// cannot be counted as though it were sound.
pub fn parse(marker: &str, text: &str) -> (Vec<Annotation>, Vec<Problem>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut annotations = Vec::new();
    let mut problems = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(body) = tagged_body(marker, line) else {
            continue;
        };
        let number = index + 1;
        let target = lines[index + 1..]
            .iter()
            .position(|next| is_code(marker, next))
            .map(|offset| index + 1 + offset + 1);
        match (fields(body), target) {
            (Err(message), _) => problems.push(Problem {
                line: number,
                message,
            }),
            (Ok(_), None) => problems.push(Problem {
                line: number,
                message: "no code follows the annotation".to_owned(),
            }),
            (Ok(fields), Some(target_line)) => match annotation(number, target_line, &fields) {
                Ok(found) => annotations.push(found),
                Err(message) => problems.push(Problem {
                    line: number,
                    message,
                }),
            },
        }
    }
    (annotations, problems)
}

fn tagged_body<'a>(marker: &str, line: &'a str) -> Option<&'a str> {
    let comment = line.trim_start().strip_prefix(marker)?;
    comment.trim_start().strip_prefix(TAG)
}

fn is_code(marker: &str, line: &str) -> bool {
    let line = line.trim();
    !line.is_empty() && !line.starts_with(marker)
}

type Fields = Vec<(String, String)>;

fn fields(body: &str) -> Result<Fields, String> {
    let mut found: Fields = Vec::new();
    for word in body.split_whitespace() {
        match word.split_once('=') {
            Some((key, value)) if KEYS.contains(&key) => {
                if found.iter().any(|(seen, _)| seen == key) {
                    return Err(format!("`{key}` is given twice"));
                }
                found.push((key.to_owned(), value.to_owned()));
            }
            Some((key, _)) if !key.contains(['(', ')']) => {
                return Err(format!("`{key}` is not a field; expected one of {KEYS:?}"));
            }
            _ => match found.last_mut() {
                Some((_, value)) => {
                    value.push(' ');
                    value.push_str(word);
                }
                None => return Err(format!("`{word}` comes before any field")),
            },
        }
    }
    Ok(found)
}

fn annotation(line: usize, target_line: usize, fields: &Fields) -> Result<Annotation, String> {
    let get = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    let required = |key: &str| get(key).ok_or_else(|| format!("`{key}` is missing"));
    let order = |key: &str| -> Result<Order, String> {
        let text = required(key)?;
        text.parse()
            .map_err(|error| format!("`{key}={text}` is not an order: {error}"))
    };
    Ok(Annotation {
        line,
        target_line,
        time: order("time")?,
        space: order("space")?,
        conf: required("conf")?.parse()?,
        rule: get("rule").map(str::to_owned),
        none: get("none").map(str::to_owned),
        may: get("may").map(str::to_owned),
        local: match get("scope") {
            None => false,
            Some("local") => true,
            Some(other) => return Err(format!("`scope={other}` is not a scope: only `local`")),
        },
    })
}

/// The name a function is declared with, read from its first line. Enough for the
/// declaration styles the corpus uses; anything else answers `None` and the function
/// is identified by its line instead.
pub fn function_name(declaration: &str) -> Option<String> {
    let line = declaration.trim();
    let line = line.strip_prefix("func (").map_or(line, |rest| {
        rest.split_once(')').map_or(rest, |(_, after)| after)
    });
    let before_parameters = without_type_arguments(line.split('(').next()?);
    let before_assignment = before_parameters
        .split_once('=')
        .map_or(before_parameters.as_str(), |(name, _)| name);
    before_assignment
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .rfind(|word| !word.is_empty())
        .map(str::to_owned)
}

/// `text` without anything between angle brackets, so that `Map<K, V> find` and
/// `sort<T extends Record<string, any>>` leave the name last.
fn without_type_arguments(text: &str) -> String {
    let mut depth = 0_u32;
    let mut kept = String::new();
    for character in text.chars() {
        match character {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => kept.push(character),
            _ => {}
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Annotation {
        let (found, problems) = parse("//", text);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(found.len(), 1);
        found.into_iter().next().unwrap()
    }

    fn problem(text: &str) -> String {
        let (found, problems) = parse("//", text);
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(problems.len(), 1, "{problems:?}");
        problems[0].message.clone()
    }

    #[test]
    fn a_full_annotation_gives_both_orders_the_confidence_and_the_expectations() {
        let found = one(
            "// @complexity time=O(n*m) space=O(n) conf=verified rule=quadratic-via-call none=sort-in-loop\n\
             function f() {}\n",
        );
        assert_eq!(found.time, "O(n·m)".parse().unwrap());
        assert_eq!(found.space, "O(n)".parse().unwrap());
        assert_eq!(found.conf, Conf::Verified);
        assert_eq!(found.rule.as_deref(), Some("quadratic-via-call"));
        assert_eq!(found.none.as_deref(), Some("sort-in-loop"));
        assert_eq!((found.line, found.target_line), (1, 2));
    }

    #[test]
    fn a_rule_that_may_fire_is_recorded_beside_the_ones_that_must_and_must_not() {
        let found = one(
            "// @complexity time=O(n) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop\n\
             function f() {}\n",
        );
        assert_eq!(found.rule.as_deref(), Some("scan-in-loop"));
        assert_eq!(found.may.as_deref(), Some("linear-op-in-loop"));
    }

    #[test]
    fn a_local_scope_is_recorded_and_no_other_is_accepted() {
        let found = one("// @complexity time=O(n) space=O(1) conf=verified scope=local\nfn() {}\n");
        assert!(found.local);
        assert!(!one("// @complexity time=O(n) space=O(1) conf=verified\nfn() {}\n").local);
        assert!(
            problem("// @complexity time=O(n) space=O(1) conf=verified scope=global\nf();\n")
                .contains("`scope=global` is not a scope")
        );
    }

    #[test]
    fn an_order_with_spaces_runs_until_the_next_field() {
        let found = one("// @complexity time=O(n log n) space=O(n) conf=heuristic\nfn() {}\n");
        assert_eq!(found.time, "O(n log n)".parse().unwrap());
        assert_eq!(found.conf, Conf::Heuristic);
        let found = one("// @complexity time=O(n*m log m) space=O(1) conf=verified\nfn() {}\n");
        assert_eq!(found.time, "O(n·m log m)".parse().unwrap());
    }

    #[test]
    fn the_target_is_the_first_code_line_after_blank_lines_and_comments() {
        let found = one(
            "const a = 1;\n// @complexity time=O(1) space=O(1) conf=verified\n\n// note\nfunction f() {}\n",
        );
        assert_eq!((found.line, found.target_line), (2, 5));
    }

    #[test]
    fn python_comments_start_with_a_hash() {
        let (found, problems) = parse(
            "#",
            "# @complexity time=O(n) space=O(1) conf=verified\ndef f(xs):\n    pass\n",
        );
        assert!(problems.is_empty());
        assert_eq!(found.len(), 1);
        assert_eq!(comment_marker("py"), Some("#"));
        assert_eq!(comment_marker("go"), Some("//"));
        assert_eq!(comment_marker("rs"), None);
    }

    #[test]
    fn a_tag_in_ordinary_text_is_not_an_annotation() {
        let (found, problems) = parse(
            "//",
            "let s = \"// @complexity\";\n// the @complexity tag\n",
        );
        assert!(found.is_empty() && problems.is_empty());
    }

    #[test]
    fn mistakes_name_what_is_wrong() {
        let cases = [
            (
                "// @complexity time=O(n) conf=verified\nf();\n",
                "`space` is missing",
            ),
            (
                "// @complexity time=O(n) space=O(1)\nf();\n",
                "`conf` is missing",
            ),
            (
                "// @complexity time=big space=O(1) conf=verified\nf();\n",
                "`time=big` is not an order",
            ),
            (
                "// @complexity time=O(n) space=O(1) conf=sure\nf();\n",
                "`sure` is not a confidence",
            ),
            (
                "// @complexity time=O(n) time=O(1) space=O(1) conf=verified\nf();\n",
                "`time` is given twice",
            ),
            (
                "// @complexity speed=O(n) time=O(n) space=O(1) conf=verified\nf();\n",
                "`speed` is not a field",
            ),
            (
                "// @complexity time=O(n) space=O(1) conf=verified\n",
                "no code follows",
            ),
            ("// @complexity O(n)\nf();\n", "comes before any field"),
        ];
        for (text, expected) in cases {
            let message = problem(text);
            assert!(message.contains(expected), "{text:?}: {message}");
        }
    }

    #[test]
    fn a_function_is_named_from_its_declaration() {
        let cases = [
            (
                "export function sumAll(values: number[]): number {",
                "sumAll",
            ),
            (
                "export const hasDuplicate = (values: number[]) => {",
                "hasDuplicate",
            ),
            ("def has_duplicate(values):", "has_duplicate"),
            ("func SumAll(values []int) int {", "SumAll"),
            ("func (s *Stack) Push(value int) {", "Push"),
            (
                "    public static int sumAll(List<Integer> values) {",
                "sumAll",
            ),
            (
                "long long sum_all(const std::vector<int>& values) {",
                "sum_all",
            ),
            (
                "export function sortKeys<T extends Record<string, any>>(obj: T): T {",
                "sortKeys",
            ),
            ("    public Node<K, V> first() {", "first"),
        ];
        for (declaration, expected) in cases {
            assert_eq!(function_name(declaration).as_deref(), Some(expected));
        }
        assert_eq!(function_name("{"), None);
    }
}

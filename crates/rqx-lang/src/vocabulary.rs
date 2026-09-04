//! What Rqx reads out of names and prose rather than out of syntax. The mechanisms
//! are the same in every language; the words are not, so the words arrive from
//! `LanguageProfile` and only the mechanism lives here.

/// Numbers that carry their own meaning wherever they appear. Without this list the
/// real magic numbers would be buried under every loop bound in the codebase.
pub const CONVENTIONAL: [f64; 7] = [0.0, 1.0, 2.0, -1.0, 10.0, 100.0, 1000.0];

/// What a test framework calls its suites, its cases and its assertions, and how it
/// spells "skip this one" and "run only this one".
pub struct TestVocabulary {
    pub suite_roots: &'static [&'static str],
    /// Roots whose bodies must assert. A suite asserts through the cases inside it.
    pub case_roots: &'static [&'static str],
    pub assertion_roots: &'static [&'static str],
    /// Suffixes and prefixes that skip or focus, as (marker, is_prefix).
    pub skip_markers: &'static [(&'static str, bool)],
    pub focus_markers: &'static [(&'static str, bool)],
    /// Calls that stop the clock, which every run pays for.
    pub delay_calls: &'static [&'static str],
    /// Annotations written above a test that skip or focus it, as
    /// (dotted name, "skipped" | "focused"). Frameworks that mark a test this way
    /// never spell it into the call, so the call vocabulary above cannot see them.
    pub annotations: &'static [(&'static str, &'static str)],
    /// How this framework names a test by convention, when it names one at all.
    pub case_prefixes: &'static [&'static str],
    /// Calls made from inside a test that skip or focus the run, matched on their
    /// last segment because the receiver is named by whoever wrote the test.
    pub control_calls: &'static [(&'static str, &'static str)],
}

impl TestVocabulary {
    pub fn root_of(name: &str) -> &str {
        name.split('.').next().unwrap_or(name)
    }

    pub fn is_assertion(&self, name: &str) -> bool {
        let root = Self::root_of(name);
        self.assertion_roots
            .iter()
            .any(|candidate| root == *candidate || root.ends_with(candidate))
    }

    pub fn is_test(&self, name: &str) -> bool {
        let root = Self::root_of(name);
        self.suite_roots
            .iter()
            .chain(self.case_roots)
            .any(|candidate| {
                root == *candidate
                    || self
                        .skip_markers
                        .iter()
                        .chain(self.focus_markers)
                        .filter(|(_, prefix)| *prefix)
                        .any(|(marker, _)| root.strip_prefix(marker) == Some(candidate))
            })
    }

    pub fn is_case(&self, name: &str) -> bool {
        self.case_roots.contains(&Self::root_of(name))
    }

    /// A function whose own name marks it as a test, which is how frameworks without
    /// a registration call find their cases.
    pub fn names_a_case(&self, name: &str) -> bool {
        self.case_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
    }

    pub fn annotation(&self, name: &str) -> Option<&'static str> {
        let name = name
            .trim_start_matches('@')
            .split('(')
            .next()
            .unwrap_or(name)
            .trim();
        self.annotations
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, verdict)| *verdict)
    }

    /// `("skipped" | "focused", the directive as written)`, or nothing.
    pub fn directive(&self, name: &str) -> Option<(&'static str, &'static str)> {
        let root = Self::root_of(name);
        let known =
            |bare: &str| self.suite_roots.contains(&bare) || self.case_roots.contains(&bare);
        for (verdict, markers) in [
            ("skipped", self.skip_markers),
            ("focused", self.focus_markers),
        ] {
            for (marker, is_prefix) in markers {
                if *is_prefix {
                    if root.strip_prefix(marker).is_some_and(known) {
                        return Some((verdict, marker));
                    }
                } else if known(root) && name.rsplit('.').next() == Some(*marker) {
                    return Some((verdict, marker));
                }
            }
        }
        None
    }

    /// `t.Skip()` — the verdict a call inside a test passes on the test around it.
    pub fn control(&self, name: &str) -> Option<&'static str> {
        let last = name.rsplit('.').next()?;
        self.control_calls
            .iter()
            .find(|(candidate, _)| *candidate == last)
            .map(|(_, verdict)| *verdict)
    }

    pub fn is_delay(&self, name: &str) -> bool {
        let root = Self::root_of(name);
        self.delay_calls
            .iter()
            .any(|candidate| root == *candidate || name.ends_with(&format!(".{candidate}")))
    }
}

/// Prose can end in a semicolon, so a comment counts as code only when most of its
/// lines both end like a statement and contain statement syntax.
pub fn looks_like_code(text: &str, endings: &[&str], markers: &[&str], operators: &[&str]) -> bool {
    let body: Vec<&str> = text
        .lines()
        .map(|line| line.trim().trim_start_matches(['/', '*', '#', ' ']).trim())
        .filter(|line| !line.is_empty())
        .collect();
    if body.is_empty() {
        return false;
    }
    let coded = body
        .iter()
        .filter(|line| {
            // A language with no statement terminator has nothing to end with, so
            // the markers and the assignment carry the test alone.
            let closes = endings.is_empty() || endings.iter().any(|ending| line.ends_with(ending));
            let syntax = operators.iter().any(|operator| line.contains(operator))
                || markers.iter().any(|marker| line.starts_with(marker));
            closes && syntax
        })
        .count();
    coded > 0 && coded * 2 >= body.len()
}

/// Markers that record who wrote something and when. Version control answers both
/// questions, and unlike the comment it is not left behind by the next edit.
pub fn journal_marker(text: &str) -> Option<&'static str> {
    [
        "@author",
        "@date",
        "@created",
        "@modified",
        "Created by",
        "Created on",
        "Last modified",
    ]
    .into_iter()
    .find(|marker| text.contains(marker))
}

/// Splits an identifier the way a reader does, so `getUserName` and `get user name`
/// compare equal.
pub fn identifier_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for character in name.chars() {
        if character == '_' || character == '-' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else if character.is_uppercase() && !current.is_empty() {
            words.push(std::mem::take(&mut current));
            current.push(character.to_ascii_lowercase());
        } else {
            current.push(character.to_ascii_lowercase());
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words.retain(|word| word.len() > 2);
    words
}

/// A comment is redundant when every word in it is already in the name underneath.
/// Requiring at least two words keeps `// x` and decorative dividers out of it.
pub fn redundant_subject(text: &str, following: &str, keywords: &[&str]) -> Option<String> {
    const FILLER: [&str; 12] = [
        "the", "this", "that", "and", "for", "with", "from", "into", "returns", "return", "gets",
        "sets",
    ];

    let prose: Vec<String> = text
        .trim_start_matches(['/', '*', '#', ' '])
        .trim_end_matches(['*', '/', ' '])
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 2)
        .map(str::to_ascii_lowercase)
        .filter(|word| !FILLER.contains(&word.as_str()))
        .collect();
    if prose.len() < 2 {
        return None;
    }

    let declared = following
        .split_once('=')
        .map_or(following, |(left, _)| left);
    let declared = declared.split_once('(').map_or(declared, |(left, _)| left);
    let subject = declared
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .rfind(|token| token.len() > 2 && !keywords.contains(token))?;
    let name = identifier_words(subject);
    if name.is_empty() {
        return None;
    }
    prose
        .iter()
        .all(|word| name.contains(word))
        .then(|| subject.to_owned())
}

/// Only the two languages a string can hold without ambiguity. Guessing at CSS from
/// braces and colons would flag every object written across lines.
pub fn foreign_language(text: &str) -> Option<&'static str> {
    let upper = text.to_ascii_uppercase();
    if [
        "SELECT ",
        "INSERT INTO",
        "UPDATE ",
        "DELETE FROM",
        "CREATE TABLE",
    ]
    .iter()
    .any(|clause| upper.contains(clause))
    {
        return Some("SQL");
    }
    ["<div", "<span", "<table", "<html", "<section", "<button"]
        .iter()
        .any(|tag| text.contains(tag))
        .then_some("HTML")
}

pub fn convention_of(name: &str) -> Option<&'static str> {
    if name.chars().all(|c| !c.is_lowercase()) {
        // SCREAMING_CASE is the one convention nobody mixes by accident.
        return None;
    }
    if name.contains('_') && !name.starts_with('_') {
        return Some("snake_case");
    }
    (name.chars().any(char::is_uppercase) && name.starts_with(char::is_lowercase))
        .then_some("camelCase")
}

/// A language where the initial capital carries visibility rather than meaning spells
/// the same concept two ways, so the first letter is normalised before any name is
/// read for what it says.
fn lower_initial(name: &str) -> String {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) => first.to_lowercase().chain(characters).collect(),
        None => String::new(),
    }
}

/// A name that promises an answer rather than a change. Asking a question should
/// never alter what the next question returns.
pub fn query_prefix(name: &str) -> bool {
    let name = lower_initial(name);
    ["get", "is", "has", "should", "can"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|rest| rest.chars().next().is_some_and(char::is_uppercase))
    })
}

/// Bare verbs that name the act of running rather than what is done. Anything with
/// an object attached — `handleTimeout`, `processOrder` — says enough and is left be.
pub fn vague_verb(name: &str) -> bool {
    const VAGUE: [&str; 10] = [
        "handle", "process", "manage", "doIt", "doWork", "doStuff", "perform", "operate", "stuff",
        "action",
    ];
    VAGUE.contains(&lower_initial(name).as_str())
}

/// A value that configures where the program points or who it talks to. Restricted
/// to what cannot be anything else: a URL or an absolute path with real segments.
pub fn configuration_literal(text: &str) -> Option<&'static str> {
    if text.starts_with("http://") || text.starts_with("https://") || text.contains("://") {
        return Some("a URL");
    }
    (text.starts_with('/') && text.matches('/').count() >= 2 && !text.contains(' '))
        .then_some("an absolute path")
}

/// Verbs that mean the same thing to a reader. Splitting a name into one of these
/// plus its object is how the same concept is found under two names.
const SYNONYMS: [&[&str]; 5] = [
    &["get", "fetch", "retrieve", "load", "read"],
    &["set", "put", "save", "store", "write"],
    &["remove", "delete", "destroy", "drop", "erase"],
    &["make", "create", "build", "construct"],
    &["find", "search", "lookup", "locate"],
];

pub fn verb_and_object(name: &str) -> Option<(&'static str, String)> {
    let name = lower_initial(name);
    SYNONYMS
        .iter()
        .flat_map(|family| family.iter())
        .find_map(|verb| {
            let rest = name.strip_prefix(*verb)?;
            let head = rest.chars().next()?;
            // `getOrder` and `get_order` name the same thing; the separator is the only
            // difference and it is a house style, not a meaning.
            let separated = head.is_uppercase() || head == '_';
            separated.then(|| (*verb, rest.trim_start_matches('_').to_ascii_lowercase()))
        })
}

pub fn same_family(left: &str, right: &str) -> bool {
    SYNONYMS
        .iter()
        .any(|family| family.contains(&left) && family.contains(&right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_told_from_prose_by_how_its_lines_end() {
        let endings = [";", "{", "}"];
        let markers = ["return ", "const ", "let "];
        assert!(looks_like_code(
            "// const legacy = total * 0.2;",
            &endings,
            &markers,
            &[" = "]
        ));
        assert!(!looks_like_code(
            "// this returns the total",
            &endings,
            &markers,
            &[" = "]
        ));
    }

    #[test]
    fn a_python_comment_is_code_when_it_ends_like_python() {
        let endings = [":", ")"];
        let markers = ["return ", "def ", "if "];
        assert!(looks_like_code(
            "# if total > 0:",
            &endings,
            &markers,
            &[" = "]
        ));
        assert!(!looks_like_code(
            "# explains the total",
            &endings,
            &markers,
            &[" = "]
        ));
    }
}

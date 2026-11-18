//! What Rqx reads out of names and prose rather than out of syntax. The mechanisms
//! are the same in every language; the words are not, so the words arrive from
//! `LanguageProfile` and only the mechanism lives here.

/// Numbers that carry their own meaning wherever they appear. Without this list the
/// real magic numbers would be buried under every loop bound in the codebase.
pub const CONVENTIONAL: [f64; 7] = [0.0, 1.0, 2.0, -1.0, 10.0, 100.0, 1000.0];

/// What the performance rules recognise by name in one language. Names are the
/// weakest evidence in the tool, which is why every rule reading this list reports
/// at heuristic confidence and every list is exact rather than a prefix.
pub struct PerfVocabulary {
    /// Methods that walk a whole collection to answer one question.
    pub scans: &'static [&'static str],
    /// The same, written as a free function taking the collection as an argument.
    pub scan_calls: &'static [&'static str],
    pub regex_constructors: &'static [&'static str],
    /// Type names whose declaration builds a regex, where construction is not a call.
    pub regex_types: &'static [&'static str],
    /// Calls that hold the thread until the work is done.
    pub blocking: &'static [&'static str],
    /// Receivers a blocking call is written on, so `time.sleep` is told from a
    /// project's own `sleep` helper.
    pub blocking_receivers: &'static [&'static str],
    /// Calls that stop the thread for a fixed time. Distinct from `blocking`,
    /// which waits for work to finish: this waits for nothing.
    pub sleep_calls: &'static [&'static str],
    /// How this language spells its string type, for the accumulator that is a
    /// string rather than a list.
    pub string_types: &'static [&'static str],
}

/// Clients that talk to something outside the process. Shared, because a service
/// names its database the same way in every language it is written in.
pub const IO_RECEIVERS: [&str; 32] = [
    "db",
    "prisma",
    "knex",
    "sequelize",
    "mongoose",
    "model",
    "repo",
    "repository",
    "em",
    "orm",
    "session",
    "cursor",
    "conn",
    "connection",
    "client",
    "http",
    "axios",
    "requests",
    "httpx",
    "urllib",
    "sql",
    "jdbctemplate",
    "entitymanager",
    "resttemplate",
    "webclient",
    "redis",
    "s3",
    "bucket",
    "queue",
    "producer",
    "pool",
    "collection",
];

/// What such a client is asked to do. Matched whole rather than by prefix, so a
/// `getName` on a value named `client` is not a round trip.
pub const IO_METHODS: [&str; 27] = [
    "get",
    "post",
    "put",
    "patch",
    "delete",
    "query",
    "execute",
    "exec",
    "select",
    "insert",
    "update",
    "upsert",
    "save",
    "load",
    "fetch",
    "send",
    "publish",
    "read",
    "write",
    "call",
    "invoke",
    "request",
    "exchange",
    "retrieve",
    "getforobject",
    "getforentity",
    "postforobject",
];

/// Calls that reach the network with no receiver to give them away.
pub const IO_BARE: [&str; 5] = [
    "fetch",
    "sqlite3_exec",
    "curl_easy_perform",
    "mysql_query",
    "pqexec",
];

/// Names suggesting a value is already keyed, where a scan is a lookup and costs
/// nothing.
pub const HASHED_HINTS: [&str; 11] = [
    "set", "map", "index", "lookup", "cache", "byid", "by_id", "dict", "hash", "seen", "visited",
];

/// Names suggesting an answer is kept between calls, which is what makes a
/// two-branch recursion linear instead of exponential.
pub const MEMO_HINTS: [&str; 6] = ["memo", "cache", "seen", "dp", "table", "visited"];

/// Constructions that key their contents, so searching one is not a scan.
pub const KEYED_CONSTRUCTIONS: [&str; 16] = [
    "new set",
    "new map",
    "new weakmap",
    "new weakset",
    "set(",
    "dict(",
    "make(map",
    "map[",
    "map<",
    "set<",
    "dict[",
    "hashmap",
    "hashset",
    "unordered_",
    "std::set",
    "std::map",
];

/// Whether a dotted name has a segment naming a client. Any segment, because an ORM
/// spells the model between the client and the call: `prisma.user.findMany`.
pub fn reaches_outside(receiver: &str, extra: &[String]) -> bool {
    receiver.split('.').any(|segment| {
        let segment = segment.to_ascii_lowercase();
        IO_RECEIVERS.contains(&segment.as_str())
            || extra.iter().any(|name| name.eq_ignore_ascii_case(&segment))
    })
}

pub fn asks_for_data(method: &str, extra: &[String]) -> bool {
    let lower = method.to_ascii_lowercase();
    IO_METHODS.contains(&lower.as_str())
        || lower.starts_with("find")
        || extra.iter().any(|name| name.eq_ignore_ascii_case(method))
}

/// Case-insensitive substring test that does not allocate, because it runs against
/// every identifier in every file.
pub fn mentions_any(name: &str, words: &[&str]) -> bool {
    let name = name.as_bytes();
    words.iter().any(|word| {
        let word = word.as_bytes();
        word.len() <= name.len()
            && name
                .windows(word.len())
                .any(|window| window.eq_ignore_ascii_case(word))
    })
}

/// What a test framework calls its suites, its cases and its assertions, and how it
/// spells "skip this one" and "run only this one".
pub struct TestVocabulary {
    pub suite_roots: &'static [&'static str],
    /// Roots whose bodies must assert. A suite asserts through the cases inside it.
    pub case_roots: &'static [&'static str],
    pub assertion_roots: &'static [&'static str],
    /// Hooks that prepare the ground before a case runs.
    pub setup_roots: &'static [&'static str],
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
    /// Whether the framework also fixes the parameter list of a case it finds by
    /// name. Go's runner will not call `func TestX()` at all, so its `*testing.T` is
    /// required whether the body reads it or not; a pytest case takes the fixtures it
    /// asks for, so an unused one there is still the author's to delete.
    pub case_signature_is_fixed: bool,
    /// Calls made from inside a test that skip or focus the run, matched on their
    /// last segment because the receiver is named by whoever wrote the test.
    pub control_calls: &'static [(&'static str, &'static str)],
}

impl TestVocabulary {
    pub fn root_of(name: &str) -> &str {
        name.split('.').next().unwrap_or(name)
    }

    pub fn is_setup(&self, name: &str) -> bool {
        let root = Self::root_of(name);
        self.setup_roots.contains(&root)
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

    /// Whether this function's parameters belong to the framework rather than to
    /// whoever wrote it, so shortening the list is not an edit anyone can make.
    pub fn owns_the_signature_of(&self, name: &str) -> bool {
        self.case_signature_is_fixed && self.names_a_case(name)
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
/// A comment written to a person rather than about the code — the residue of a
/// conversation that ended before the commit. Matched case-insensitively at the start
/// of the comment's own text, because "Note that this" is chat and "note that this"
/// buried mid-sentence in a real explanation is not.
pub fn chat_residue(text: &str) -> Option<&'static str> {
    const OPENINGS: [&str; 14] = [
        "here's",
        "here is the",
        "let me",
        "i've added",
        "i have added",
        "i'll",
        "as requested",
        "feel free",
        "hope this helps",
        "note that this",
        "in this implementation",
        "this implementation",
        "certainly",
        "sure,",
    ];
    let body = text
        .trim_start_matches(['/', '#', '*', '!', '-', ' ', '\t'])
        .to_lowercase();
    OPENINGS
        .into_iter()
        .find(|opening| body.starts_with(opening))
}

/// Characters a divider is drawn with.
const RULE_CHARACTERS: [char; 8] = ['-', '=', '~', '*', '#', '_', '+', '/'];

/// A comment that is a divider, or a heading of one or two words fenced by one, rather
/// than a sentence: `// ======`, `// --- helpers ---`, `# === STATE ===`. The rule's fix
/// deletes the line, so anything else is left alone: a URL, a licence line, an
/// encoding declaration and a line of commented-out code are not dividers.
pub fn banner(text: &str) -> bool {
    let body = text.trim_start_matches(['/', '#', '!']).trim();
    if body.chars().count() < 4 || carries_url(body) || licence_notice(body) {
        return false;
    }
    let is_rule = |character: &char| RULE_CHARACTERS.contains(character);
    if body
        .chars()
        .all(|character| is_rule(&character) || character.is_whitespace())
    {
        return body.chars().filter(is_rule).count() >= 4;
    }
    let lead = body.chars().take_while(is_rule).count();
    let trail = body.chars().rev().take_while(is_rule).count();
    if lead < 2 || trail < 2 || lead + trail < 4 {
        return false;
    }
    let heading: String = body
        .chars()
        .skip(lead)
        .take(body.chars().count() - lead - trail)
        .collect();
    let heading = heading.trim();
    !heading.is_empty()
        && heading.split_whitespace().count() <= 2
        && heading
            .chars()
            .all(|character| character.is_alphanumeric() || character.is_whitespace())
}

/// Text that belongs to a licence or copyright notice. A notice is a legal statement
/// about the file, so no fix may remove any part of it.
pub fn licence_notice(text: &str) -> bool {
    const PHRASES: [&str; 10] = [
        "license",
        "licence",
        "copyright",
        "spdx",
        "all rights reserved",
        "\u{a9}",
        "(c) ",
        "creative commons",
        "public domain",
        "warranty",
    ];
    const WORDS: [&str; 12] = [
        "apache",
        "mit",
        "gpl",
        "lgpl",
        "agpl",
        "bsd",
        "mozilla",
        "mpl",
        "isc",
        "unlicense",
        "eupl",
        "cddl",
    ];
    let lower = text.to_lowercase();
    PHRASES.iter().any(|phrase| lower.contains(phrase))
        || lower
            .split(|character: char| !character.is_alphanumeric())
            .any(|word| WORDS.contains(&word))
}

/// A comment that points somewhere. A URL is a reference someone follows, so it is
/// never decoration.
pub fn carries_url(text: &str) -> bool {
    text.contains("://") || text.contains("www.")
}

/// A placeholder that reads as real. Harmless in a test and a bug report waiting to
/// happen anywhere else.
pub fn placeholder_value(text: &str) -> Option<&'static str> {
    const STANDINS: [&str; 10] = [
        "example.com",
        "example.org",
        "lorem ipsum",
        "john doe",
        "jane doe",
        "test@test",
        "your_api_key",
        "your-api-key",
        "changeme",
        "foo@bar",
    ];
    let body = text.to_lowercase();
    STANDINS.into_iter().find(|standin| body.contains(standin))
}

/// A body that says the work has not been done yet. The words are the whole of the
/// evidence, which is why the rule reading this reports heuristically.
pub fn unfinished_marker(text: &str) -> Option<&'static str> {
    const MARKERS: [&str; 6] = [
        "not implemented",
        "notimplemented",
        "unimplemented",
        "not yet implemented",
        "todo",
        "coming soon",
    ];
    let body = text.to_lowercase();
    MARKERS.into_iter().find(|marker| body.contains(marker))
}

/// A name that states what a thing is not. Reading it inside a condition means
/// undoing the negation twice, once for the name and once for the test.
pub fn negated_name(name: &str) -> Option<&'static str> {
    const NEGATIONS: [&str; 7] = ["not_", "is_not", "no_", "non_", "cannot", "isnt", "dont"];
    // Split on the case boundary before folding it away, or `notReady` becomes one
    // word and the marker is gone.
    let spaced = snake_of(&name.replace('-', "_")).to_lowercase();
    NEGATIONS
        .into_iter()
        .find(|marker| spaced.starts_with(marker) || spaced.contains(&format!("_{marker}")))
}

/// One name in a single spelling, so `isNotReady` and `is_not_ready` compare equal.
fn snake_of(name: &str) -> String {
    let mut out = String::new();
    for (at, character) in name.char_indices() {
        if character.is_uppercase() && at > 0 {
            out.push('_');
        }
        out.push(character.to_ascii_lowercase());
    }
    out
}

/// Whether a name reads as a question. A value that answers true or false is read
/// inside a condition, and a question is what a condition is.
pub fn asks_a_question(name: &str) -> bool {
    const OPENERS: [&str; 12] = [
        "is", "are", "has", "have", "can", "should", "was", "were", "will", "does", "did", "must",
    ];
    let first = name
        .split(['_', '-'])
        .next()
        .unwrap_or(name)
        .trim_start_matches('_');
    let head: String = first
        .char_indices()
        .take_while(|(at, c)| *at == 0 || c.is_lowercase())
        .map(|(_, c)| c.to_ascii_lowercase())
        .collect();
    OPENERS.contains(&head.as_str())
}

/// A test name that names nothing. The run prints it and the reader is no wiser.
pub fn says_nothing(label: &str) -> bool {
    const EMPTY: [&str; 10] = [
        "test",
        "tests",
        "it works",
        "works",
        "should work",
        "case",
        "todo",
        "example",
        "basic",
        "simple",
    ];
    let flat = label.trim().to_lowercase().replace(['_', '-'], " ");
    // The framework's own prefix is not part of what the author called the case.
    let bare = flat
        .trim_start_matches("test")
        .trim_start_matches(' ')
        .trim();
    EMPTY.contains(&flat.as_str())
        || EMPTY.contains(&bare)
        || bare.is_empty()
        || bare.chars().all(|c| c.is_ascii_digit())
}

/// A comment that records work still to do.
pub fn todo_marker(text: &str) -> Option<&'static str> {
    const MARKERS: [&str; 5] = ["TODO", "FIXME", "HACK", "XXX", "REVIEW"];
    MARKERS.into_iter().find(|marker| text.contains(marker))
}

/// The same, when it says nothing about who or what will do it. A marker with
/// nothing attached is a note to nobody.
pub fn unattached_todo(text: &str) -> Option<&'static str> {
    let marker = todo_marker(text)?;
    // A ticket, a name or a link is somebody to ask; a bare marker is not.
    // A comment opens with `#` in some languages, so only a number behind it is a
    // ticket rather than the comment's own marker.
    let numbered = text
        .split('#')
        .skip(1)
        .any(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
    let attached = text.contains("http")
        || text.contains('@')
        || numbered
        || text
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .any(|word| {
                let mut halves = word.splitn(2, '-');
                let (Some(project), Some(number)) = (halves.next(), halves.next()) else {
                    return false;
                };
                project.len() >= 2
                    && project.chars().all(|c| c.is_ascii_uppercase())
                    && !number.is_empty()
                    && number.chars().all(|c| c.is_ascii_digit())
            });
    (!attached).then_some(marker)
}

/// An apology is a comment that knows the code is wrong and leaves it that way.
pub fn apology(text: &str) -> Option<&'static str> {
    const REGRETS: [&str; 6] = [
        "sorry",
        "this is ugly",
        "this is a hack",
        "i know this is bad",
        "forgive",
        "apologies",
    ];
    let body = text.to_lowercase();
    REGRETS.into_iter().find(|regret| body.contains(regret))
}

/// An emoji anywhere outside a test. Not a defect on its own, which is why it is
/// `Info` — it is a signal about who wrote the line and how carefully.
pub fn emoji(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(character as u32,
            0x1F300..=0x1FAFF | 0x2600..=0x27BF | 0x1F000..=0x1F0FF | 0xFE0F | 0x2B00..=0x2BFF)
    })
}

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

//! Syntax colouring for a page that shows code. The grammars rqx already parses with
//! say what each token is, so the classes come from the tree rather than from a second
//! set of regular expressions that would disagree with it on the first template string.

use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;

use rqx_core::Language;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::tree::{kind_of, text_of};
use crate::{holds_markup, parse, support};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Comment,
    String,
    Number,
    Keyword,
    Type,
    Function,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub byte_range: Range<usize>,
    pub kind: Kind,
}

/// Kinds that spell a type in some grammar, whatever the language calls its primitives.
/// The word inside one is anonymous and alphabetic, which would otherwise read as a
/// keyword.
const TYPE_KINDS: [&str; 7] = [
    "type_identifier",
    "predefined_type",
    "primitive_type",
    "integral_type",
    "floating_point_type",
    "boolean_type",
    "void_type",
];

const CHARACTER_KINDS: [&str; 3] = ["char_literal", "character_literal", "rune_literal"];

/// The coloured runs of `source`, in order and never overlapping. Everything not
/// listed is `Kind::Plain`. A file the grammar cannot parse in time has none.
pub fn spans(language: Language, source: &str) -> Vec<Span> {
    spans_with(language, false, source)
}

/// The same for a file known by its name, which is what decides whether the markup
/// grammar reads it. `None` for a file rqx does not read.
pub fn spans_for_file(display: &str, source: &str) -> Option<Vec<Span>> {
    let language = Language::of_path(Path::new(display))?;
    Some(spans_with(language, holds_markup(display), source))
}

fn spans_with(language: Language, markup: bool, source: &str) -> Vec<Span> {
    let profile = support(language).profile;
    let Ok(tree) = parse(language, profile, markup, source) else {
        return Vec::new();
    };
    let mut classifier = Classifier {
        profile,
        source,
        function_names: HashSet::new(),
    };
    let mut spans = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        let (kind, whole) = classifier.classify(node);
        if kind != Kind::Plain && node.start_byte() < node.end_byte() {
            spans.push(Span {
                byte_range: node.byte_range(),
                kind,
            });
        }
        if !whole && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return spans;
            }
        }
    }
}

struct Classifier<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    /// Names found while visiting the declaration or call that owns them, which the
    /// walk reaches before the name itself.
    function_names: HashSet<usize>,
}

impl Classifier<'_> {
    /// What a node is, and whether that is all of it: a string's children are its
    /// contents, so the walk does not go on to colour them again.
    fn classify(&mut self, node: Node<'_>) -> (Kind, bool) {
        let kind = kind_of(node);
        let profile = self.profile;
        if !node.is_named() {
            return (self.keyword_or_plain(node), true);
        }
        if profile.comment_kinds.contains(&kind) {
            return (Kind::Comment, true);
        }
        if is_string(kind) || profile.string_kinds.contains(&kind) {
            return (Kind::String, true);
        }
        if profile.number_kinds.contains(&kind) {
            return (Kind::Number, true);
        }
        if TYPE_KINDS.contains(&kind) {
            return (Kind::Type, true);
        }
        self.remember_function_name(node, kind);
        if self.function_names.contains(&node.id()) {
            return (Kind::Function, true);
        }
        (Kind::Plain, false)
    }

    fn keyword_or_plain(&self, node: Node<'_>) -> Kind {
        let word = text_of(node, self.source);
        let alphabetic =
            !word.is_empty() && word.chars().all(|c| c.is_ascii_alphabetic() || c == '_');
        if alphabetic && node.child_count() == 0 {
            Kind::Keyword
        } else {
            Kind::Plain
        }
    }

    fn remember_function_name(&mut self, node: Node<'_>, kind: &str) {
        let profile = self.profile;
        let name = if profile.function_kinds.contains(&kind) || profile.method_kinds.contains(&kind)
        {
            declared_name(node)
        } else if profile.call_kinds.contains(&kind) {
            self.called_name(node)
        } else {
            None
        };
        if let Some(name) = name {
            self.function_names.insert(name.id());
        }
    }

    /// The name a call reaches for: the last segment of `a.b.c()`, not `a`.
    fn called_name<'tree>(&self, call: Node<'tree>) -> Option<Node<'tree>> {
        let profile = self.profile;
        let callee = call.child_by_field_name(profile.callee_field)?;
        let kind = kind_of(callee);
        if profile.identifier_kinds.contains(&kind) {
            return Some(callee);
        }
        if profile.member_access_kinds.contains(&kind) {
            return callee.child_by_field_name(profile.property_field);
        }
        if profile.qualified_name_kinds.contains(&kind) {
            return callee.child_by_field_name("name");
        }
        None
    }
}

fn is_string(kind: &str) -> bool {
    kind.contains("string") && !kind.ends_with("_start") && !kind.ends_with("_end")
        || CHARACTER_KINDS.contains(&kind)
}

/// The name of a function or method as its grammar spells it: a `name` field, or in
/// C++ the identifier at the bottom of the declarator chain.
fn declared_name(declaration: Node<'_>) -> Option<Node<'_>> {
    if let Some(name) = declaration.child_by_field_name("name") {
        return Some(name);
    }
    let mut current = declaration.child_by_field_name("declarator")?;
    let mut through_function = false;
    loop {
        through_function |= kind_of(current) == "function_declarator";
        match current.child_by_field_name("declarator") {
            Some(inner) => current = inner,
            None => break,
        }
    }
    if kind_of(current) == "qualified_identifier" {
        current = current.child_by_field_name("name")?;
    }
    through_function.then_some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each coloured run as `kind:text`, so a test reads as the highlighting it wants.
    fn coloured(language: Language, source: &str) -> Vec<String> {
        spans(language, source)
            .into_iter()
            .map(|span| format!("{:?}:{}", span.kind, &source[span.byte_range]))
            .collect()
    }

    fn assert_has(runs: &[String], wanted: &[&str]) {
        for run in wanted {
            assert!(
                runs.iter().any(|found| found == run),
                "no {run} in {runs:?}"
            );
        }
    }

    #[test]
    fn typescript_is_coloured() {
        let source = "// note\nconst count: number = 42;\nfunction greet(name: string): Greeting {\n  return `hi ${name}` + format('x');\n}\n";
        let runs = coloured(Language::TypeScript, source);
        assert_has(
            &runs,
            &[
                "Comment:// note",
                "Keyword:const",
                "Type:number",
                "Number:42",
                "Keyword:function",
                "Function:greet",
                "Type:string",
                "Type:Greeting",
                "Keyword:return",
                "String:`hi ${name}`",
                "Function:format",
                "String:'x'",
            ],
        );
    }

    #[test]
    fn javascript_is_coloured_including_a_method_call() {
        let source = "const total = items.reduce((a, b) => a + b, 0.5);\nclass A { run() { return null } }\n";
        let runs = coloured(Language::JavaScript, source);
        assert_has(
            &runs,
            &[
                "Keyword:const",
                "Function:reduce",
                "Number:0.5",
                "Keyword:class",
                "Function:run",
            ],
        );
        assert!(!runs.iter().any(|run| run == "Function:items"), "{runs:?}");
    }

    #[test]
    fn python_is_coloured() {
        let source =
            "# note\ndef area(radius):\n    return round(3.14 * radius ** 2, 2) + len('x')\n";
        let runs = coloured(Language::Python, source);
        assert_has(
            &runs,
            &[
                "Comment:# note",
                "Keyword:def",
                "Function:area",
                "Keyword:return",
                "Function:round",
                "Number:3.14",
                "Number:2",
                "Function:len",
                "String:'x'",
            ],
        );
    }

    #[test]
    fn go_is_coloured() {
        let source = "package main\n\n// note\nfunc (s *Server) Run(port int) string {\n\treturn fmt.Sprint(\"p\", 'x', 8080)\n}\n";
        let runs = coloured(Language::Go, source);
        assert_has(
            &runs,
            &[
                "Comment:// note",
                "Keyword:package",
                "Keyword:func",
                "Function:Run",
                "Type:Server",
                "Type:int",
                "Type:string",
                "Keyword:return",
                "Function:Sprint",
                "String:\"p\"",
                "String:'x'",
                "Number:8080",
            ],
        );
    }

    #[test]
    fn java_is_coloured() {
        let source = "/* note */\npublic class Box {\n  private int size = 10;\n  void run(String name) { System.out.println(\"hi\"); }\n}\n";
        let runs = coloured(Language::Java, source);
        assert_has(
            &runs,
            &[
                "Comment:/* note */",
                "Keyword:public",
                "Keyword:class",
                "Type:int",
                "Number:10",
                "Type:void",
                "Function:run",
                "Type:String",
                "Function:println",
                "String:\"hi\"",
            ],
        );
    }

    #[test]
    fn cpp_is_coloured() {
        let source = "// note\nint Box::area(double w) {\n  return std::max(w, 2.5) + 'c';\n}\n";
        let runs = coloured(Language::Cpp, source);
        assert_has(
            &runs,
            &[
                "Comment:// note",
                "Type:int",
                "Function:area",
                "Type:double",
                "Keyword:return",
                "Function:max",
                "Number:2.5",
                "String:'c'",
            ],
        );
    }

    #[test]
    fn spans_are_ordered_and_never_overlap() {
        let source = "const a = 'x'; // c\nfunction f() { return g(1) }\n";
        let found = spans(Language::TypeScript, source);
        for pair in found.windows(2) {
            assert!(
                pair[0].byte_range.end <= pair[1].byte_range.start,
                "{pair:?}"
            );
        }
    }

    #[test]
    fn a_file_is_read_by_its_name() {
        assert!(spans_for_file("notes.md", "# hi").is_none());
        let jsx = spans_for_file("a.tsx", "const v = <div>hi</div>;\n").expect("a language");
        assert!(jsx.iter().any(|span| span.kind == Kind::Keyword));
    }
}

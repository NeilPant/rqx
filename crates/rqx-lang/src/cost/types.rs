//! What a name holds, as far as the source says. A declared or constructed type is a
//! fact the cost tables can rely on; a guess from a name is only a hint. There is no
//! type checker behind this: `auto` and `var` give nothing, and the answer for those
//! is that the type is not known.

use rqx_core::Language;
use tree_sitter::Node;

use crate::names::Names;
use crate::tree::{descendants, kind_of};
use crate::vocabulary::{HASHED_HINTS, mentions_any};

/// The head of a type: `Map` for `Map<string, number>`, `list` for `list[int]`, `Array`
/// for `number[]`. Facts built from a declaration or a constructor are `declared`; a
/// fact guessed from a name is not, and the cost it leads to is only a heuristic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeFact {
    pub head: String,
    pub declared: bool,
}

/// The head of a value guessed to answer by key from its name alone, where the
/// declaration says nothing. Searching one is a lookup, so no table entry charges for
/// it.
pub const KEYED_GUESS: &str = "<keyed>";

impl TypeFact {
    pub fn declared(head: impl Into<String>) -> Self {
        Self {
            head: head.into(),
            declared: true,
        }
    }

    pub fn guessed(head: impl Into<String>) -> Self {
        Self {
            head: head.into(),
            declared: false,
        }
    }

    pub fn is_keyed_guess(&self) -> bool {
        self.head == KEYED_GUESS
    }
}

/// Words a declaration puts before the type that are not part of it.
const QUALIFIERS: [&str; 9] = [
    "const",
    "readonly",
    "final",
    "static",
    "mutable",
    "volatile",
    "constexpr",
    "typename",
    "struct",
];

/// What a language spells for "a type I could not name".
const OPAQUE: [&str; 8] = [
    "auto", "var", "let", "any", "unknown", "void", "dynamic", "None",
];

/// The head of a written type, with the language's spellings of the same container
/// made one: `T[]` and `Array<T>` are both `Array`, `List[int]` and `list[int]` are
/// both `list`. Matches how the cost tables spell their receivers, which are put
/// through the same function.
pub fn canonical_head(language: Language, spelled: &str) -> Option<String> {
    let text = spelled.trim().trim_start_matches(':').trim();
    let head = match language {
        Language::Go => go_head(text),
        Language::TypeScript | Language::JavaScript => script_head(text),
        Language::Cpp | Language::Java | Language::Python => word_head(text),
    }?;
    let head = alias(language, &head);
    (!OPAQUE.contains(&head.as_str()) && !is_type_parameter(&head)).then_some(head)
}

fn is_type_parameter(head: &str) -> bool {
    head.len() == 1 && head.chars().all(|c| c.is_ascii_uppercase())
}

fn go_head(text: &str) -> Option<String> {
    let text = text.trim_start_matches('*').trim();
    if text.starts_with("[]") {
        return Some("slice".to_owned());
    }
    if let Some(after) = text.strip_prefix('[')
        && after.split(']').next().is_some_and(|inside| {
            !inside.is_empty() && inside.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
    {
        return Some("array".to_owned());
    }
    if text.starts_with("map[") {
        return Some("map".to_owned());
    }
    if text.starts_with("chan") {
        return Some("chan".to_owned());
    }
    word_head(text)
}

fn script_head(text: &str) -> Option<String> {
    let text = text.trim_start_matches("readonly ").trim();
    if text.starts_with('{') {
        return Some("object".to_owned());
    }
    // `Item[] | null` is an `Item[]` where it is not null, which is where it is used.
    let members: Vec<&str> = text
        .split('|')
        .map(str::trim)
        .filter(|member| !matches!(*member, "null" | "undefined" | ""))
        .collect();
    let [only] = members.as_slice() else {
        return None;
    };
    if only.ends_with("[]") {
        return Some("Array".to_owned());
    }
    word_head(only)
}

/// The leading word of a type, past qualifiers, pointers and namespaces.
fn word_head(text: &str) -> Option<String> {
    let mut text = text.trim();
    loop {
        let stripped = QUALIFIERS
            .iter()
            .find_map(|word| text.strip_prefix(word).filter(|rest| rest.starts_with(' ')));
        match stripped {
            Some(rest) => text = rest.trim_start(),
            None => break,
        }
    }
    let text = text.trim_end_matches(['&', '*', ' ', '?']);
    if text.ends_with("[]") {
        return Some("array".to_owned());
    }
    let end = text.find(['<', '[', '(']).unwrap_or(text.len());
    let before_arguments = text[..end].trim();
    let segment = before_arguments
        .rsplit([':', '.'])
        .find(|part| !part.is_empty())?;
    // `unsigned int` and `long long` read as their last word.
    let word = segment.split_whitespace().next_back()?;
    word.chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        .then(|| word.to_owned())
}

fn alias(language: Language, head: &str) -> String {
    let aliases: &[(&str, &str)] = match language {
        Language::Python => &[
            ("List", "list"),
            ("Dict", "dict"),
            ("DefaultDict", "dict"),
            ("defaultdict", "dict"),
            ("OrderedDict", "dict"),
            ("Counter", "dict"),
            ("Set", "set"),
            ("FrozenSet", "set"),
            ("frozenset", "set"),
            ("Tuple", "tuple"),
            ("Deque", "deque"),
            ("Sequence", "list"),
            ("Iterable", "list"),
        ],
        Language::TypeScript | Language::JavaScript => &[
            ("ReadonlyArray", "Array"),
            ("ReadonlySet", "Set"),
            ("ReadonlyMap", "Map"),
            ("Record", "object"),
        ],
        Language::Cpp | Language::Go | Language::Java => &[],
    };
    aliases
        .iter()
        .find(|(from, _)| *from == head)
        .map_or_else(|| head.to_owned(), |(_, to)| (*to).to_owned())
}

/// Whether values of this type are counts, which is what makes one a size when a loop
/// runs up to it.
pub fn is_integer(language: Language, head: &str) -> bool {
    let integers: &[&str] = match language {
        Language::Python => &["int"],
        Language::TypeScript | Language::JavaScript => &["number", "bigint"],
        Language::Java => &["int", "long", "short", "byte", "Integer", "Long", "Short"],
        Language::Go => &[
            "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32",
            "uint64", "uintptr",
        ],
        Language::Cpp => &[
            "int",
            "long",
            "short",
            "size_t",
            "ssize_t",
            "ptrdiff_t",
            "int32_t",
            "int64_t",
            "uint32_t",
            "uint64_t",
            "size_type",
        ],
    };
    integers.contains(&head)
}

/// The type a literal builds, by node kind.
fn literal_head(language: Language, kind: &str) -> Option<&'static str> {
    let heads: &[(&str, &str)] = match language {
        Language::TypeScript | Language::JavaScript => &[
            ("array", "Array"),
            ("object", "object"),
            ("string", "string"),
            ("template_string", "string"),
        ],
        Language::Python => &[
            ("list", "list"),
            ("list_comprehension", "list"),
            ("dictionary", "dict"),
            ("dictionary_comprehension", "dict"),
            ("set", "set"),
            ("set_comprehension", "set"),
            ("tuple", "tuple"),
            ("string", "str"),
            ("concatenated_string", "str"),
        ],
        Language::Java => &[
            ("string_literal", "String"),
            ("array_creation_expression", "array"),
            ("array_initializer", "array"),
        ],
        Language::Go => &[
            ("interpreted_string_literal", "string"),
            ("raw_string_literal", "string"),
        ],
        Language::Cpp => &[
            ("string_literal", "string"),
            ("raw_string_literal", "string"),
        ],
    };
    heads
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map(|(_, head)| *head)
}

/// Calls that build a collection by name rather than by syntax: `set()`, `sorted(x)`.
fn constructor_call_head(language: Language, name: &str) -> Option<&'static str> {
    let heads: &[(&str, &str)] = match language {
        Language::Python => &[
            ("list", "list"),
            ("sorted", "list"),
            ("set", "set"),
            ("frozenset", "set"),
            ("dict", "dict"),
            ("defaultdict", "dict"),
            ("OrderedDict", "dict"),
            ("Counter", "dict"),
            ("deque", "deque"),
            ("tuple", "tuple"),
            ("str", "str"),
        ],
        Language::TypeScript | Language::JavaScript => &[
            ("Array", "Array"),
            ("Array.from", "Array"),
            ("Array.of", "Array"),
            ("Object.keys", "Array"),
            ("Object.values", "Array"),
            ("Object.entries", "Array"),
            ("String", "string"),
        ],
        Language::Java => &[
            ("List.of", "List"),
            ("Set.of", "Set"),
            ("Map.of", "Map"),
            ("Arrays.asList", "List"),
        ],
        Language::Go => &[],
        Language::Cpp => &[],
    };
    heads
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, head)| *head)
}

/// Methods whose answer is a fresh collection of the same kind as their receiver's,
/// or an array in the languages where that is all they make.
fn array_producing_method(language: Language, name: &str) -> bool {
    match language {
        Language::TypeScript | Language::JavaScript => matches!(
            name,
            "map" | "filter" | "slice" | "concat" | "split" | "flat" | "flatMap" | "toSorted"
        ),
        Language::Python => matches!(name, "split" | "splitlines" | "keys" | "values" | "items"),
        Language::Java | Language::Go | Language::Cpp => false,
    }
}

/// What a freshly built value is, read from the expression that builds it. Anything
/// built here is a fact the author wrote, so it is `declared`.
pub fn constructed_fact(names: &Names<'_>, value: Node<'_>) -> Option<TypeFact> {
    let profile = names.profile;
    let language = profile.language;
    let value = names.see_through(value);
    let kind = kind_of(value);
    if let Some(head) = literal_head(language, kind) {
        return Some(TypeFact::declared(head));
    }
    if let Some(field) = profile.constructor_field
        && let Some(constructed) = value.child_by_field_name(field)
        && profile.is_call(value)
        && let Some(head) = canonical_head(language, names.text(constructed))
    {
        return Some(TypeFact::declared(head));
    }
    match language {
        Language::Go => go_constructed(names, value),
        _ if profile.is_call(value) => {
            let called = names.called_name(value)?;
            if let Some(head) = constructor_call_head(language, &called) {
                return Some(TypeFact::declared(head));
            }
            let tail = called.rsplit('.').next().unwrap_or(&called);
            array_producing_method(language, tail).then(|| TypeFact::guessed("Array"))
        }
        _ => None,
    }
}

/// `[]int{}`, `map[string]int{}` and `make(T, n)` carry their type in the first place
/// the grammar puts one.
fn go_constructed(names: &Names<'_>, value: Node<'_>) -> Option<TypeFact> {
    let kind = kind_of(value);
    if kind == "composite_literal" {
        let spelled = names.text(value.child_by_field_name("type")?);
        return canonical_head(Language::Go, spelled).map(TypeFact::declared);
    }
    if names.profile.is_call(value) {
        let called = names.called_name(value)?;
        if matches!(called.as_str(), "make" | "new") {
            let first = names.arguments(value).into_iter().next()?;
            return canonical_head(Language::Go, names.text(first)).map(TypeFact::declared);
        }
    }
    None
}

/// A declaration's three parts, read off whichever fields its grammar fills.
pub struct Declaration<'a> {
    pub name: &'a str,
    pub spelled_type: Option<&'a str>,
    pub value: Option<Node<'a>>,
}

/// The names a declaring node introduces, with the type written beside each and the
/// value it starts as. A Go `a, b []int` introduces two names that share one type.
pub fn declarations<'a>(names: &Names<'a>, node: Node<'a>) -> Vec<Declaration<'a>> {
    let profile = names.profile;
    if let Some(inner) = node.child_by_field_name("declarator")
        && profile.declarator_kinds.contains(&kind_of(inner))
    {
        return declarations(names, inner);
    }
    let typed_by = |holder: Node<'a>| {
        holder
            .child_by_field_name("type")
            .map(|kind| names.text(kind))
    };
    let spelled_type = typed_by(node).or_else(|| node.parent().and_then(typed_by));
    let value = node
        .child_by_field_name("value")
        .or_else(|| node.child_by_field_name("right"))
        .map(|value| unwrap_list(names, value));

    let mut declared = Vec::new();
    let mut cursor = node.walk();
    let labelled: Vec<Node<'a>> = node
        .children_by_field_name("name", &mut cursor)
        .collect::<Vec<_>>();
    let holders = if labelled.is_empty() {
        ["pattern", "declarator", "left"]
            .iter()
            .find_map(|field| node.child_by_field_name(field))
            .into_iter()
            .collect()
    } else {
        labelled
    };
    for holder in holders {
        let holder = unwrap_list(names, holder);
        if let Some(name) = bound_identifier(profile, holder) {
            declared.push(Declaration {
                name: names.text(name),
                spelled_type,
                value,
            });
        }
    }
    if declared.is_empty() {
        declared.extend(bare_parameter(names, node, spelled_type));
    }
    declared
}

/// A parameter with no field for its name: Python's bare `n` and its `n: int`, which
/// holds the name as its first child.
fn bare_parameter<'a>(
    names: &Names<'a>,
    node: Node<'a>,
    spelled_type: Option<&'a str>,
) -> Option<Declaration<'a>> {
    let profile = names.profile;
    let name = if profile.is_identifier(node)
        && profile
            .parameter_list_kinds
            .contains(&node.parent().map_or("", kind_of))
    {
        node
    } else if profile.parameter_kinds.contains(&kind_of(node)) {
        node.named_child(0)
            .filter(|first| profile.is_identifier(*first))?
    } else {
        return None;
    };
    Some(Declaration {
        name: names.text(name),
        spelled_type,
        value: None,
    })
}

/// Go writes the left of an assignment as a list even when it holds one name.
fn unwrap_list<'a>(names: &Names<'a>, node: Node<'a>) -> Node<'a> {
    let node = names.see_through(node);
    if names.profile.expression_list_kinds.contains(&kind_of(node)) {
        return node.named_child(0).unwrap_or(node);
    }
    node
}

/// The identifier a declarator binds, through the pointer, reference and initializer
/// wrappers C++ puts around it.
fn bound_identifier<'a>(
    profile: &crate::profile::LanguageProfile,
    holder: Node<'a>,
) -> Option<Node<'a>> {
    if profile.is_identifier(holder) {
        return Some(holder);
    }
    holder
        .child_by_field_name("declarator")
        .and_then(|inner| bound_identifier(profile, inner))
        .or_else(|| {
            descendants(holder)
                .into_iter()
                .find(|child| profile.is_identifier(*child))
        })
}

/// Names that say a value is a lookup table: a set, a map, an index, a cache.
pub fn names_a_keyed_value(name: &str) -> bool {
    let last = name.rsplit('.').next().unwrap_or(name);
    mentions_any(last, &HASHED_HINTS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(language: Language, spelled: &str) -> Option<String> {
        canonical_head(language, spelled)
    }

    #[test]
    fn typescript_spells_one_array_three_ways() {
        for spelled in [
            "number[]",
            "Array<number>",
            "readonly number[]",
            ": ReadonlyArray<T>",
        ] {
            assert_eq!(
                head(Language::TypeScript, spelled).as_deref(),
                Some("Array"),
                "{spelled}"
            );
        }
        assert_eq!(
            head(Language::TypeScript, "Map<string, number>").as_deref(),
            Some("Map")
        );
        assert_eq!(
            head(Language::TypeScript, "Item[] | null").as_deref(),
            Some("Array")
        );
        assert_eq!(head(Language::TypeScript, "string | number"), None);
        assert_eq!(
            head(Language::TypeScript, "{ a: number }").as_deref(),
            Some("object")
        );
    }

    #[test]
    fn python_hints_agree_with_the_builtin_names() {
        for spelled in ["list[int]", "List[int]", "typing.List[int]"] {
            assert_eq!(
                head(Language::Python, spelled).as_deref(),
                Some("list"),
                "{spelled}"
            );
        }
        assert_eq!(
            head(Language::Python, "Dict[str, int]").as_deref(),
            Some("dict")
        );
        assert_eq!(head(Language::Python, "set[int]").as_deref(), Some("set"));
        assert_eq!(head(Language::Python, "str").as_deref(), Some("str"));
    }

    #[test]
    fn go_types_name_their_shape() {
        assert_eq!(head(Language::Go, "[]string").as_deref(), Some("slice"));
        assert_eq!(head(Language::Go, "[4]int").as_deref(), Some("array"));
        assert_eq!(
            head(Language::Go, "map[string][]int").as_deref(),
            Some("map")
        );
        assert_eq!(head(Language::Go, "*Store").as_deref(), Some("Store"));
        assert_eq!(
            head(Language::Go, "strings.Builder").as_deref(),
            Some("Builder")
        );
    }

    #[test]
    fn java_and_cpp_drop_generics_namespaces_and_qualifiers() {
        assert_eq!(
            head(Language::Java, "List<String>").as_deref(),
            Some("List")
        );
        assert_eq!(
            head(Language::Java, "java.util.Map<K, V>").as_deref(),
            Some("Map")
        );
        assert_eq!(head(Language::Java, "int[]").as_deref(), Some("array"));
        assert_eq!(head(Language::Java, "var"), None);
        assert_eq!(
            head(Language::Cpp, "const std::vector<int>&").as_deref(),
            Some("vector")
        );
        assert_eq!(
            head(Language::Cpp, "std::unordered_map<std::string, int>").as_deref(),
            Some("unordered_map")
        );
        assert_eq!(head(Language::Cpp, "auto"), None);
        assert_eq!(
            head(Language::Cpp, "unsigned long").as_deref(),
            Some("long")
        );
    }

    #[test]
    fn a_type_parameter_is_not_a_type() {
        assert_eq!(head(Language::Java, "T"), None);
        assert_eq!(head(Language::TypeScript, "T"), None);
    }

    #[test]
    fn counts_are_the_integer_types_of_each_language() {
        assert!(is_integer(Language::Python, "int"));
        assert!(is_integer(Language::TypeScript, "number"));
        assert!(is_integer(Language::Go, "int64"));
        assert!(is_integer(Language::Cpp, "size_t"));
        assert!(!is_integer(Language::Java, "String"));
    }

    #[test]
    fn a_name_that_says_lookup_is_a_guess() {
        assert!(names_a_keyed_value("userById"));
        assert!(names_a_keyed_value("this.cache"));
        assert!(!names_a_keyed_value("items"));
        assert!(TypeFact::guessed(KEYED_GUESS).is_keyed_guess());
        assert!(!TypeFact::declared("Map").is_keyed_guess());
    }

    use crate::cost::testing::{names_of, nodes_of, tree_of};

    /// The first declaration of `name` in `source`, as `(declared type head, constructed
    /// head)`.
    fn facts_of(
        language: Language,
        source: &str,
        declaring: &str,
        name: &str,
    ) -> (Option<String>, Option<TypeFact>) {
        let tree = tree_of(language, source);
        let names = names_of(language, source);
        let declaration = nodes_of(&tree, declaring)
            .into_iter()
            .flat_map(|node| declarations(&names, node))
            .find(|declaration| declaration.name == name)
            .unwrap_or_else(|| panic!("no declaration of {name}"));
        (
            declaration
                .spelled_type
                .and_then(|spelled| canonical_head(language, spelled)),
            declaration
                .value
                .and_then(|value| constructed_fact(&names, value)),
        )
    }

    #[test]
    fn typescript_reads_annotations_and_constructors() {
        let source = "function f(xs: number[], m: Map<string, number>) {\n  const seen = new Set<number>();\n  const rows = [];\n  const parts = line.split(',');\n}";
        let facts = |declaring, name| facts_of(Language::TypeScript, source, declaring, name);
        assert_eq!(
            facts("required_parameter", "xs").0.as_deref(),
            Some("Array")
        );
        assert_eq!(facts("required_parameter", "m").0.as_deref(), Some("Map"));
        assert_eq!(
            facts("variable_declarator", "seen").1,
            Some(TypeFact::declared("Set"))
        );
        assert_eq!(
            facts("variable_declarator", "rows").1,
            Some(TypeFact::declared("Array"))
        );
        assert_eq!(
            facts("variable_declarator", "parts").1,
            Some(TypeFact::guessed("Array"))
        );
    }

    #[test]
    fn python_reads_hints_and_builtin_constructors() {
        let source = "def f(xs: list[int], n, ys: dict[str, int] = None):\n    seen: set[int] = set()\n    out = []\n    table = {}\n    ordered = sorted(xs)\n";
        let facts = |declaring, name| facts_of(Language::Python, source, declaring, name);
        assert_eq!(facts("typed_parameter", "xs").0.as_deref(), Some("list"));
        assert_eq!(facts("identifier", "n"), (None, None));
        assert_eq!(
            facts("typed_default_parameter", "ys").0.as_deref(),
            Some("dict")
        );
        let seen = facts("assignment", "seen");
        assert_eq!(seen.0.as_deref(), Some("set"));
        assert_eq!(seen.1, Some(TypeFact::declared("set")));
        assert_eq!(
            facts("assignment", "out").1,
            Some(TypeFact::declared("list"))
        );
        assert_eq!(
            facts("assignment", "table").1,
            Some(TypeFact::declared("dict"))
        );
        assert_eq!(
            facts("assignment", "ordered").1,
            Some(TypeFact::declared("list"))
        );
    }

    #[test]
    fn java_reads_declared_types_and_constructed_ones() {
        let source = "class A { void f(List<String> xs) {\n  Set<Integer> s = new HashSet<>();\n  var q = new LinkedList<Integer>();\n  var o = List.of(1);\n} }";
        let facts = |declaring, name| facts_of(Language::Java, source, declaring, name);
        assert_eq!(facts("formal_parameter", "xs").0.as_deref(), Some("List"));
        let s = facts("variable_declarator", "s");
        assert_eq!(s.0.as_deref(), Some("Set"));
        assert_eq!(s.1, Some(TypeFact::declared("HashSet")));
        let q = facts("variable_declarator", "q");
        assert_eq!(q.0, None);
        assert_eq!(q.1, Some(TypeFact::declared("LinkedList")));
        assert_eq!(
            facts("variable_declarator", "o").1,
            Some(TypeFact::declared("List"))
        );
    }

    #[test]
    fn go_reads_declared_types_literals_and_make() {
        let source = "package a\nfunc f(xs []int, a, b string) {\n  seen := make(map[string]bool)\n  buf := []int{}\n  var out string\n}";
        let facts = |declaring, name| facts_of(Language::Go, source, declaring, name);
        assert_eq!(
            facts("parameter_declaration", "xs").0.as_deref(),
            Some("slice")
        );
        assert_eq!(
            facts("parameter_declaration", "b").0.as_deref(),
            Some("string")
        );
        assert_eq!(
            facts("short_var_declaration", "seen").1,
            Some(TypeFact::declared("map"))
        );
        assert_eq!(
            facts("short_var_declaration", "buf").1,
            Some(TypeFact::declared("slice"))
        );
        assert_eq!(facts("var_spec", "out").0.as_deref(), Some("string"));
    }

    #[test]
    fn cpp_reads_declared_types_through_references_and_initializers() {
        let source = "int f(const std::vector<int>& xs, int n) {\n  std::set<int> s;\n  std::string out = \"\";\n  auto it = xs.begin();\n}";
        let facts = |declaring, name| facts_of(Language::Cpp, source, declaring, name);
        assert_eq!(
            facts("parameter_declaration", "xs").0.as_deref(),
            Some("vector")
        );
        assert_eq!(
            facts("parameter_declaration", "n").0.as_deref(),
            Some("int")
        );
        assert_eq!(facts("declaration", "s").0.as_deref(), Some("set"));
        let out = facts("declaration", "out");
        assert_eq!(out.0.as_deref(), Some("string"));
        assert_eq!(out.1, Some(TypeFact::declared("string")));
        assert_eq!(facts("declaration", "it").0, None);
    }
}

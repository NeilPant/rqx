use std::collections::HashSet;

use rqx_core::Language;
use rqx_core::facts::UnusedParameter;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::scope::{BindingKind, Scopes};
use crate::tree::{find_within, kind_of, span_of, text_of};
use crate::visibility::{Visibility, header_words};

/// Who decides the shape of a parameter list. Only a signature the author owns can
/// be shortened: a callback's arity is fixed by whatever calls it, and an override's
/// by the thing it overrides, so an unused parameter there is not a deletion anyone
/// can make locally.
#[derive(Copy, Clone, PartialEq, Eq)]
enum SignatureOwner {
    Author,
    Caller,
    Supertype,
}

struct Grader<'a> {
    profile: &'static LanguageProfile,
    source: &'a str,
    scopes: &'a Scopes,
    owner: Vec<SignatureOwner>,
    enclosing: Vec<Option<String>>,
    exported_depth: u32,
    /// Names the file mentions without calling them, where a function handed to
    /// someone else as a callback is told apart only by that. Java's are the names it
    /// passes as method references.
    handed_out: HashSet<&'a str>,
    /// Go: the file carries a build constraint, so it is one of several platform
    /// variants that share their signatures.
    platform_specific: bool,
    /// Who outside this file can call each function.
    visibility: Visibility<'a>,
    out: Vec<UnusedParameter>,
}

impl<'a> Grader<'a> {
    fn owner(&self) -> SignatureOwner {
        self.owner.last().copied().unwrap_or(SignatureOwner::Author)
    }

    fn name_of(&self, parameter: Node<'a>) -> Option<(&'a str, Node<'a>)> {
        if self.profile.is_identifier(parameter) {
            return Some((text_of(parameter, self.source), parameter));
        }
        // Named field first: a grammar that counts type names as identifiers would
        // otherwise answer `String` for `String subject`, and then find that name
        // used nowhere.
        let declared = self
            .profile
            .binding_fields
            .iter()
            .find_map(|field| parameter.child_by_field_name(field));
        if let Some(declared) = declared.filter(|node| self.profile.is_identifier(*node)) {
            return Some((text_of(declared, self.source), declared));
        }
        // C++ wraps the name in what it is a reference or pointer to: `const T& lhs`
        // declares `lhs` inside a reference declarator. One that holds no name,
        // `const T&`, declares nothing, and `T` is its type. Java's varargs parameter
        // `LinkOption... options` holds its name in a declarator child of its own.
        let declared = declared.or_else(|| {
            let mut cursor = parameter.walk();
            parameter
                .named_children(&mut cursor)
                .find(|child| kind_of(*child) == "variable_declarator")
        });
        if let Some(declared) = declared.filter(|node| kind_of(*node).ends_with("declarator")) {
            return self
                .declared_name(declared)
                .map(|name| (text_of(name, self.source), name));
        }
        // A parameter written as a bare type, Go's `func(int)` or C++'s `f(Row*)`, has
        // no name at all, and the identifier inside its type is not one.
        let type_span = declared
            .is_none()
            .then(|| parameter.child_by_field_name("type"))
            .flatten()
            .map(span_of);
        crate::tree::descendants(parameter)
            .into_iter()
            .filter(|child| {
                type_span.is_none_or(|within| {
                    let span = span_of(*child);
                    span.start < within.start || span.end > within.end
                })
            })
            .find(|child| self.profile.is_identifier(*child))
            .map(|child| (text_of(child, self.source), child))
    }

    /// The identifier a declarator binds, looking through the reference, pointer and
    /// parenthesised declarators around it.
    fn declared_name(&self, declarator: Node<'a>) -> Option<Node<'a>> {
        let mut current = declarator;
        loop {
            if self.profile.is_identifier(current) {
                return Some(current);
            }
            current = current.child_by_field_name("declarator").or_else(|| {
                let mut cursor = current.walk();
                current
                    .named_children(&mut cursor)
                    .filter(|child| !self.profile.is_comment(*child))
                    .last()
            })?;
        }
    }

    /// A parameter that also declares a field — TypeScript's
    /// `constructor(private readonly x: T)` — is read wherever the field is, which
    /// this file cannot see from the constructor body alone.
    fn declares_a_field(&self, parameter: Node<'a>) -> bool {
        let mut cursor = parameter.walk();
        parameter.children(&mut cursor).any(|child| {
            self.profile
                .field_declaring_modifiers
                .contains(&kind_of(child))
        })
    }

    /// A function wrapped in a decorator other than `staticmethod` or `classmethod`:
    /// a route, a fixture, an abstract method, a property setter. Its parameters are
    /// what the decorator's caller passes.
    fn is_registered(&self, function: Node<'a>) -> bool {
        let Some(decorated) = function
            .parent()
            .filter(|parent| self.profile.decorated_kinds.contains(&kind_of(*parent)))
        else {
            return false;
        };
        let mut cursor = decorated.walk();
        decorated
            .named_children(&mut cursor)
            .filter(|child| self.profile.decorator_kinds.contains(&kind_of(*child)))
            .any(|decorator| {
                let text = text_of(decorator, self.source)
                    .trim_start_matches('@')
                    .trim();
                !matches!(text, "staticmethod" | "classmethod")
            })
    }

    /// Whether the function is defined in a class body, directly or under decorators,
    /// rather than inside another function.
    fn is_method(&self, function: Node<'a>) -> bool {
        let mut current = function.parent();
        while let Some(node) = current {
            if self.profile.is_class(node) {
                return true;
            }
            if self.profile.is_function(node) {
                return false;
            }
            current = node.parent();
        }
        false
    }

    /// A body that only documents, passes or refuses: `...`, `pass`, a docstring, a
    /// lone `raise`, or `raise NotImplementedError` at the end.
    fn is_stub(&self, function: Node<'a>) -> bool {
        let Some(body) = function.child_by_field_name("body") else {
            return true;
        };
        let mut cursor = body.walk();
        let statements: Vec<Node<'a>> = body
            .named_children(&mut cursor)
            .filter(|statement| !self.profile.is_comment(*statement))
            .collect();
        let documents = |statement: &Node<'a>| {
            kind_of(*statement) == "pass_statement"
                || (kind_of(*statement) == "expression_statement"
                    && statement
                        .named_child(0)
                        .is_some_and(|value| matches!(kind_of(value), "string" | "ellipsis")))
        };
        let rest: Vec<&Node<'a>> = statements.iter().filter(|s| !documents(s)).collect();
        // `raise NotImplementedError` at the end says "subclasses fill this in",
        // whatever checks come before it.
        if rest.last().is_some_and(|last| {
            kind_of(**last) == "raise_statement"
                && text_of(**last, self.source).contains("NotImplementedError")
        }) {
            return true;
        }
        match rest.as_slice() {
            [] => true,
            [only] => self.profile.throw_kinds.contains(&kind_of(**only)),
            _ => false,
        }
    }

    /// A signature the runtime, a supertype or a framework calls with whatever it
    /// passes: Java's and C++'s `main`, an override, and a method carrying an
    /// annotation or a decorator.
    fn is_called_by_contract(&self, function: Node<'a>, name: Option<&str>) -> bool {
        let language = self.profile.language;
        if matches!(language, Language::Java | Language::Cpp) && name == Some("main") {
            return true;
        }
        if language == Language::Cpp {
            return header_words(function, self.source)
                .any(|word| matches!(word, "virtual" | "override" | "final"));
        }
        let decorated = |node: Node<'a>| self.profile.decorator_kinds.contains(&kind_of(node));
        let mut cursor = function.walk();
        let marked = function.named_children(&mut cursor).any(|child| {
            kind_of(child) == "override_modifier"
                || decorated(child)
                || (self
                    .profile
                    .modifier_container_kinds
                    .contains(&kind_of(child))
                    && child.named_children(&mut child.walk()).any(&decorated))
        });
        marked || function.prev_named_sibling().is_some_and(decorated)
    }

    /// A signature some other declaration dictates, which the language does not make
    /// the file say. Go satisfies interfaces without naming them, so any method may be
    /// one an interface fixes; a function literal has the type of the field, variable
    /// or parameter it is given to; a function named without being called is handed
    /// to someone; and a file built for one platform keeps the signatures its siblings
    /// for the others declare. Java calls the methods of an interface, an abstract
    /// class or an anonymous class through a type the method does not name, and
    /// elsewhere a function handed out by name is called with what its caller passes.
    /// In any
    /// language a body that does nothing or only refuses is a default for others to
    /// replace.
    fn signature_is_fixed_elsewhere(&self, function: Node<'a>, name: Option<&str>) -> bool {
        if self.is_stub(function) {
            return true;
        }
        match self.profile.language {
            Language::Go => {
                function.child_by_field_name("receiver").is_some()
                    || kind_of(function) == "func_literal"
                    || self.platform_specific
                    || name.is_some_and(|name| self.handed_out.contains(name))
            }
            Language::Java => self.is_overridable_java_member(function),
            Language::Python | Language::Cpp => false,
            Language::TypeScript | Language::JavaScript => {
                name.is_some_and(|name| self.handed_out.contains(name))
            }
        }
    }

    /// A method of an interface, an abstract class or an anonymous class, a
    /// `protected` method a subclass may replace, or one handed out as `this::name`.
    fn is_overridable_java_member(&self, function: Node<'a>) -> bool {
        let words: Vec<&str> = header_words(function, self.source).collect();
        if words.contains(&"protected") && !words.contains(&"final") {
            return true;
        }
        if self
            .names_of(function)
            .is_some_and(|name| self.handed_out.contains(name))
        {
            return true;
        }
        let Some(body) = function.parent() else {
            return false;
        };
        let Some(owner) = body.parent() else {
            return false;
        };
        match kind_of(owner) {
            "interface_declaration" | "object_creation_expression" => true,
            "class_declaration" => header_words(owner, self.source).any(|word| word == "abstract"),
            _ => false,
        }
    }

    fn names_of(&self, function: Node<'a>) -> Option<&'a str> {
        function
            .child_by_field_name("name")
            .map(|name| text_of(name, self.source))
    }

    fn is_used(&self, name: &str, node: Node<'a>) -> bool {
        let span = span_of(node);
        self.scopes
            .of_kind(BindingKind::Parameter)
            .find(|binding| binding.span == span && binding.name == name)
            .is_none_or(|binding| !binding.references.is_empty())
    }

    fn record(&mut self, function: Node<'a>) {
        if self.owner() != SignatureOwner::Author {
            return;
        }
        // An overload signature, `declare function`, or abstract method has no body,
        // so of course nothing reads its parameters — the implementation elsewhere
        // does. Reporting these is reporting the absence of a body.
        if function.child_by_field_name("body").is_none() {
            return;
        }
        let Some(list) = input_list(self.profile, function) else {
            return;
        };
        let mut cursor = list.walk();
        let parameters: Vec<Node<'a>> = list
            .named_children(&mut cursor)
            .filter(|child| self.profile.parameter_kinds.contains(&kind_of(child)))
            .collect();
        let total = parameters.len() as u32;

        let name = function
            .child_by_field_name("name")
            .or_else(|| {
                find_within(
                    function,
                    self.profile.declarator_wrapper_kinds,
                    self.profile.block_kinds,
                )
                .into_iter()
                .next()
                .and_then(|declarator| {
                    let mut cursor = declarator.walk();
                    declarator.named_children(&mut cursor).find(|child| {
                        self.profile.is_identifier(*child)
                            || self.profile.property_kinds.contains(&kind_of(child))
                    })
                })
            })
            .map(|node| text_of(node, self.source).to_owned())
            .or_else(|| self.enclosing.last().cloned().flatten());

        // Go's runner finds a case by name and calls it with a `*testing.T`. Delete
        // the parameter and the function is no longer a test the runner will call —
        // it stops running, silently, which `rqx fix --write` did to this project's
        // own Go fixtures.
        if name
            .as_deref()
            .is_some_and(|name| self.profile.test.owns_the_signature_of(name))
        {
            return;
        }
        // `__exit__(self, kind, value, trace)` has the shape its protocol calls it with,
        // and a decorated function is called by whatever the decorator handed it to.
        if name.as_deref().is_some_and(is_protocol_method)
            || self.is_registered(function)
            || self.is_called_by_contract(function, name.as_deref())
        {
            return;
        }
        // Python declares no interfaces, so a base class spells one as methods whose
        // body only documents or refuses, for subclasses to override; and a function
        // passed or returned by name is called with whatever its caller supplies.
        // A method is skipped as well: a base class's default that returns a fixed
        // answer looks exactly like one that forgot its argument, and without the class
        // hierarchy the two cannot be told apart (about 5 in 14 right on the standard
        // library when methods were included).
        if self.profile.language == Language::Python
            && (self.is_method(function)
                || name
                    .as_deref()
                    .is_some_and(|name| self.handed_out.contains(name)))
        {
            return;
        }

        if self.signature_is_fixed_elsewhere(function, name.as_deref()) {
            return;
        }

        let mut unused = Vec::new();
        for (index, parameter) in parameters.iter().enumerate() {
            // `*args` and `**kwargs` take whatever a caller passes, used or not.
            if self.declares_a_field(*parameter) || kind_of(*parameter).contains("splat") {
                continue;
            }
            let Some((identifier, node)) = self.name_of(*parameter) else {
                continue;
            };
            // A leading underscore is the ecosystem's way of saying "required by the
            // signature, deliberately ignored"; reporting it would be reporting the
            // author's own annotation back at them.
            // A name that says so, and Java's `Void` marker that only tells two
            // constructors apart, are kept on purpose too.
            let declared_unused = matches!(identifier, "ignore" | "ignored" | "unused" | "dummy")
                || parameter
                    .child_by_field_name("type")
                    .is_some_and(|written| text_of(written, self.source) == "Void");
            if self.profile.is_ignored_name(identifier)
                || self.profile.self_names.contains(&identifier)
                || declared_unused
            {
                continue;
            }
            if self.is_used(identifier, node) {
                continue;
            }
            unused.push((index as u32, identifier.to_owned(), span_of(node)));
        }

        let last_used = parameters
            .iter()
            .enumerate()
            .rposition(|(index, _)| !unused.iter().any(|(dead, ..)| *dead == index as u32));
        let exported =
            self.exported_depth > 0 || self.visibility.is_public(function, name.as_deref());
        for (index, parameter_name, span) in unused {
            let trailing = last_used.is_none_or(|last| index as usize > last);
            self.out.push(UnusedParameter {
                name: parameter_name,
                span,
                function: name.clone(),
                position: index + 1,
                total,
                exported,
                trailing,
            });
        }
    }

    fn walk(&mut self, node: Node<'a>) {
        let kind = kind_of(node);
        let profile = self.profile;
        let mut owners = 0;
        let mut names = 0;

        if profile.export_kinds.contains(&kind) {
            self.exported_depth += 1;
        }
        if profile.argument_list_kinds.contains(&kind) {
            self.owner.push(SignatureOwner::Caller);
            owners += 1;
        }
        if profile.is_class(node) {
            let mut cursor = node.walk();
            let inherited = node
                .named_children(&mut cursor)
                .any(|child| profile.heritage_kinds.contains(&kind_of(child)));
            self.owner.push(if inherited {
                SignatureOwner::Supertype
            } else {
                SignatureOwner::Author
            });
            owners += 1;
        }
        if profile.declarator_kinds.contains(&kind)
            || profile.property_definition_kinds.contains(&kind)
        {
            self.enclosing.push(
                node.child_by_field_name("name")
                    .map(|name| text_of(name, self.source).to_owned()),
            );
            names += 1;
            // A binding with a declared type has its function shape imposed by that
            // type, so its arity is not the author's to change.
            let mut cursor = node.walk();
            let annotated = node
                .named_children(&mut cursor)
                .any(|child| profile.type_annotation_kinds.contains(&kind_of(child)))
                || profile.property_definition_kinds.contains(&kind);
            self.owner.push(if annotated {
                SignatureOwner::Caller
            } else {
                SignatureOwner::Author
            });
            owners += 1;
        }
        if profile.method_kinds.contains(&kind) {
            self.enclosing.push(
                node.child_by_field_name("name")
                    .map(|name| text_of(name, self.source).to_owned()),
            );
            names += 1;
        }
        if profile.is_function(node) {
            self.record(node);
            // A function declared inside a callback still owns its own signature.
            self.owner.push(SignatureOwner::Author);
            owners += 1;
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.walk(child);
        }

        for _ in 0..owners {
            self.owner.pop();
        }
        for _ in 0..names {
            self.enclosing.pop();
        }
        if profile.export_kinds.contains(&kind) {
            self.exported_depth -= 1;
        }
    }
}

/// Every name the file mentions other than to call it or to define it: `return
/// select`, `parser.Handler = self.on_start`, `register(on_exit)`.
fn names_used_without_a_call<'s>(
    profile: &LanguageProfile,
    source: &'s str,
    root: Node<'_>,
) -> HashSet<&'s str> {
    crate::tree::descendants(root)
        .into_iter()
        .filter(|node| profile.is_identifier(*node))
        .filter(|node| {
            let Some(parent) = node.parent() else {
                return false;
            };
            // `self.on_start` names `on_start`; the call is around the attribute.
            let (named, holder) = if kind_of(parent) == "attribute" {
                if crate::tree::field_name_under(parent, *node) != Some("attribute") {
                    return false;
                }
                (parent, parent.parent())
            } else {
                (*node, Some(parent))
            };
            let Some(holder) = holder else {
                return true;
            };
            let called = profile.is_call(holder)
                && holder.child_by_field_name(profile.callee_field) == Some(named);
            let defined =
                profile.is_function(holder) && holder.child_by_field_name("name") == Some(named);
            !called && !defined
        })
        .map(|node| text_of(node, source))
        .collect()
}

/// `__exit__`, `__eq__`, `__getitem__`: a method the language calls with arguments it
/// chooses. `__init__` and `__new__` take what the caller constructing the object
/// passes, so an unused one there is the author's to delete.
fn is_protocol_method(name: &str) -> bool {
    // `load_tests(loader, tests, pattern)` is the hook unittest calls by name.
    name == "load_tests"
        || (name.len() > 4
            && name.starts_with("__")
            && name.ends_with("__")
            && !matches!(name, "__init__" | "__new__"))
}

/// The parameter list a function's callers fill. A Go signature can hold three lists,
/// receiver, parameters and results, and a function type written in a signature
/// brings its own, so the one the grammar files under `parameters` is preferred.
pub(crate) fn input_list<'tree>(
    profile: &LanguageProfile,
    function: Node<'tree>,
) -> Option<Node<'tree>> {
    let lists = find_within(function, profile.parameter_list_kinds, profile.block_kinds);
    lists
        .iter()
        .copied()
        .find(|list| crate::tree::field_name(*list) == Some("parameters"))
        .or_else(|| lists.last().copied())
}

/// Whether a parameter binding is one of its function's inputs, rather than a Go
/// receiver or named result, or a name inside a function type in the signature.
pub(crate) fn is_input_parameter(
    profile: &LanguageProfile,
    root: Node<'_>,
    name: rqx_core::span::Span,
) -> bool {
    let mut current = root.descendant_for_byte_range(name.start as usize, name.end as usize);
    let mut list = None;
    let mut below = None;
    while let Some(node) = current {
        if list.is_none() && profile.parameter_list_kinds.contains(&kind_of(node)) {
            list = Some(node);
        }
        if profile.is_function(node) {
            // `x => x + 1` and Java's `x -> x + 1` name their one parameter with no
            // list around it.
            let lone = below.is_some_and(|child| {
                matches!(
                    crate::tree::field_name_under(node, child),
                    Some("parameter" | "parameters")
                )
            });
            return lone || (list.is_some() && input_list(profile, node) == list);
        }
        below = Some(node);
        current = node.parent();
    }
    false
}

pub fn unused(
    profile: &'static LanguageProfile,
    source: &str,
    root: Node<'_>,
    scopes: &Scopes,
) -> Vec<UnusedParameter> {
    unused_in_file(profile, "", source, root, scopes)
}

/// Go's operating systems and architectures, which a file name ends with to build
/// only there: `cpu_freebsd.go`, `asm_arm64.go`.
const GO_PLATFORMS: [&str; 30] = [
    "aix",
    "android",
    "darwin",
    "dragonfly",
    "freebsd",
    "illumos",
    "ios",
    "js",
    "linux",
    "netbsd",
    "openbsd",
    "plan9",
    "solaris",
    "wasip1",
    "windows",
    "zos",
    "386",
    "amd64",
    "arm",
    "arm64",
    "loong64",
    "mips",
    "mips64",
    "mips64le",
    "mipsle",
    "ppc64",
    "ppc64le",
    "riscv64",
    "s390x",
    "wasm",
];

/// Whether a Go file builds for some platforms only, by a constraint comment or by
/// the platform its name ends with.
fn builds_for_some_platforms(display: &str, source: &str) -> bool {
    let stem = std::path::Path::new(display)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = stem.strip_suffix("_test").unwrap_or(&stem);
    let named = stem
        .rsplit('_')
        .next()
        .is_some_and(|last| stem.contains('_') && GO_PLATFORMS.contains(&last));
    named
        || source
            .lines()
            .map(str::trim_start)
            .any(|line| line.starts_with("//go:build") || line.starts_with("// +build"))
}

/// The same answer as [`unused`], for a file whose name is known: Go spells a file's
/// platform in its name as well as in a comment.
pub fn unused_in_file(
    profile: &'static LanguageProfile,
    display: &str,
    source: &str,
    root: Node<'_>,
    scopes: &Scopes,
) -> Vec<UnusedParameter> {
    let mut grader = Grader {
        profile,
        source,
        scopes,
        owner: Vec::new(),
        enclosing: Vec::new(),
        exported_depth: 0,
        handed_out: match profile.language {
            Language::Java => method_references(source),
            // A C++ prototype names its function without calling it too.
            Language::Cpp => HashSet::new(),
            _ => names_used_without_a_call(profile, source, root),
        },
        platform_specific: profile.language == Language::Go
            && builds_for_some_platforms(display, source),
        visibility: Visibility::new(profile, source, root),
        out: Vec::new(),
    };
    grader.walk(root);
    grader.out
}

/// The names a Java file passes as method references, `this::ignoreBody` or
/// `Parser::parse`, whose parameter lists the functional interface fixes.
fn method_references(source: &str) -> HashSet<&str> {
    source
        .split("::")
        .skip(1)
        .filter_map(|after| {
            let name = after
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                .next()?;
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

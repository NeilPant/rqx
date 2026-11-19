//! What an edit has to leave in place for the program, its build and its tools to
//! behave as before. `rqx fix` compares a file before and after its edits on these,
//! after the per-rule checks, so a fix that slips past a rule's own refusal is still
//! withheld.

use std::collections::{BTreeMap, BTreeSet};

use rqx_core::Language;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::scope::BindingKind;
use crate::tree::{descendants, descendants_outside, kind_of, text_of};
use crate::{AnalysisError, support};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Invariants {
    /// Licence notices, URLs, tool directives and the Go file header, by text.
    protected_comments: BTreeMap<String, u32>,
    /// Labels a statement carries. Removing one a jump names breaks the build.
    labels: BTreeMap<String, u32>,
    /// Labels a `goto`, `break` or `continue` names. Go refuses a label nothing
    /// jumps to, and a C++ build with warnings as errors does too.
    jump_targets: BTreeSet<String>,
    /// Python functions holding a `yield`, which makes each of them a generator.
    generators: BTreeMap<String, u32>,
    /// Python `global` and `nonlocal` statements, which apply to the whole function.
    scope_declarations: BTreeMap<String, u32>,
    /// Every mention of a name some `global` or `nonlocal` statement declares. Python
    /// refuses a `nonlocal` with no binding to refer to, and an unread one is still
    /// that binding.
    declared_names: BTreeMap<String, u32>,
    /// C++ calls spelled as macros, which can expand to anything.
    macros: BTreeMap<String, u32>,
    /// Go imports some code in the file still reads. Go refuses an unused import.
    used_imports: BTreeSet<String>,
    /// Go locals nothing reads. Go refuses one, so an edit may not add any.
    unread_locals: BTreeMap<String, u32>,
    /// Headers of the functions code outside this project may call.
    public_signatures: BTreeMap<String, Vec<String>>,
}

impl Invariants {
    pub fn of(language: Language, markup: bool, source: &str) -> Result<Self, AnalysisError> {
        let support = support(language);
        let tree = crate::parse(language, support.profile, markup, source)?;
        Ok(Self::of_tree(support.profile, source, tree.root_node()))
    }

    pub(crate) fn of_tree(profile: &'static LanguageProfile, source: &str, root: Node<'_>) -> Self {
        let protected_comments = counted(
            crate::notices::protected_in(profile, source, root)
                .into_iter()
                .map(|span| span.text(source).trim().to_owned()),
        );
        let mut invariants = Self {
            protected_comments,
            labels: counted(
                crate::removal::labels(source, root)
                    .into_iter()
                    .map(str::to_owned),
            ),
            jump_targets: crate::removal::jump_targets(source, root)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            public_signatures: crate::visibility::public_signatures(profile, source, root),
            ..Self::default()
        };
        match profile.language {
            Language::Python => {
                invariants.generators = counted(generators(profile, source, root));
                invariants.scope_declarations = counted(
                    descendants(root)
                        .into_iter()
                        .filter(|node| {
                            matches!(kind_of(*node), "global_statement" | "nonlocal_statement")
                        })
                        .map(|node| squashed(text_of(node, source))),
                );
                let declared = declared_nonlocal(source, root);
                invariants.declared_names = counted(
                    descendants(root)
                        .into_iter()
                        .filter(|node| profile.is_identifier(*node))
                        .map(|node| text_of(node, source))
                        .filter(|name| declared.contains(name))
                        .map(str::to_owned),
                );
            }
            Language::Cpp => {
                invariants.macros = counted(
                    descendants(root)
                        .into_iter()
                        .filter(|node| profile.is_call(*node))
                        .filter_map(|call| call.child_by_field_name(profile.callee_field))
                        .map(|callee| text_of(callee, source))
                        .filter(|name| crate::removal::is_macro_name(name))
                        .map(str::to_owned),
                );
            }
            Language::Go => {
                invariants.used_imports = used_go_imports(source, root);
                let scopes = crate::scope::resolve(profile, source, root);
                invariants.unread_locals = counted(
                    scopes
                        .unused()
                        .filter(|binding| {
                            binding.kind == BindingKind::Local
                                && scopes.is_local(binding.scope)
                                && binding.name != "_"
                        })
                        .map(|binding| binding.name.clone()),
                );
            }
            Language::TypeScript | Language::JavaScript | Language::Java => {}
        }
        invariants
    }

    /// The first thing `after` no longer keeps, said the way a reader acts on it.
    pub fn broken_by(&self, after: &Self) -> Option<String> {
        if let Some(comment) = lost(&self.protected_comments, &after.protected_comments) {
            return Some(format!(
                "removes the comment `{comment}`, which a tool or a reader relies on"
            ));
        }
        if let Some(label) = lost(&self.labels, &after.labels) {
            return Some(format!("removes the label `{label}`"));
        }
        if let Some(label) = self.jump_targets.difference(&after.jump_targets).next() {
            return Some(format!("removes the last jump to the label `{label}`"));
        }
        if let Some(function) = lost(&self.generators, &after.generators) {
            return Some(format!(
                "removes the `yield` that makes `{function}` a generator"
            ));
        }
        if let Some(statement) = lost(&self.scope_declarations, &after.scope_declarations) {
            return Some(format!(
                "removes `{statement}`, which applies to the whole function"
            ));
        }
        if let Some(name) = lost(&self.declared_names, &after.declared_names) {
            return Some(format!(
                "removes a use of `{name}`, which a `global` or `nonlocal` statement names"
            ));
        }
        if let Some(name) = lost(&self.macros, &after.macros) {
            return Some(format!("removes a call to the macro `{name}`"));
        }
        if let Some(import) = self.used_imports.difference(&after.used_imports).next() {
            return Some(format!(
                "leaves the import {import} unused, which Go refuses to compile"
            ));
        }
        if let Some(local) = lost(&after.unread_locals, &self.unread_locals) {
            return Some(format!(
                "leaves `{local}` declared and not used, which Go refuses to compile"
            ));
        }
        if let Some((function, _)) = self.public_signatures.iter().find(|(name, headers)| {
            after
                .public_signatures
                .get(*name)
                .is_some_and(|now| now != *headers)
        }) {
            return Some(format!(
                "changes the signature of `{function}`, which code outside this project may call"
            ));
        }
        None
    }
}

fn counted(items: impl IntoIterator<Item = String>) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_default() += 1;
    }
    counts
}

/// An entry `before` holds more of than `after` does.
fn lost<'a>(before: &'a BTreeMap<String, u32>, after: &BTreeMap<String, u32>) -> Option<&'a str> {
    before
        .iter()
        .find(|(item, count)| after.get(*item).is_none_or(|left| left < count))
        .map(|(item, _)| item.as_str())
}

/// The names a Python file's `global` and `nonlocal` statements declare.
pub(crate) fn declared_nonlocal<'s>(source: &'s str, root: Node<'_>) -> BTreeSet<&'s str> {
    descendants(root)
        .into_iter()
        .filter(|node| matches!(kind_of(*node), "global_statement" | "nonlocal_statement"))
        .flat_map(|statement| {
            let mut cursor = statement.walk();
            statement
                .named_children(&mut cursor)
                .map(|name| text_of(name, source))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The name of every Python function with a `yield` of its own.
fn generators(profile: &LanguageProfile, source: &str, root: Node<'_>) -> Vec<String> {
    descendants(root)
        .into_iter()
        .filter(|node| kind_of(*node) == "function_definition")
        .filter(|function| {
            descendants_outside(*function, |node| {
                node != *function && (profile.is_function(node) || profile.is_class(node))
            })
            .into_iter()
            .any(|node| kind_of(node) == "yield")
        })
        .filter_map(|function| function.child_by_field_name("name"))
        .map(|name| text_of(name, source).to_owned())
        .collect()
}

/// The import paths of a Go file whose package some selector in the file names. Go
/// names a package by its declaration, not its path, so a selector matches a path when
/// it matches the alias or the path's last element, read the way module authors
/// spell it: `yaml` for `gopkg.in/yaml.v3`, `redis` for `github.com/redis/go-redis/v9`.
fn used_go_imports(source: &str, root: Node<'_>) -> BTreeSet<String> {
    let selected: BTreeSet<String> = descendants(root)
        .into_iter()
        .filter_map(|node| match kind_of(node) {
            "selector_expression" => node.child_by_field_name("operand"),
            "qualified_type" => node.child_by_field_name("package"),
            _ => None,
        })
        .map(|name| text_of(name, source).to_lowercase())
        .collect();
    descendants(root)
        .into_iter()
        .filter(|node| kind_of(*node) == "import_spec")
        .filter_map(|spec| {
            let path = spec.child_by_field_name("path")?;
            let path = text_of(path, source).trim_matches(['"', '`']).to_owned();
            let alias = spec
                .child_by_field_name("name")
                .map(|name| text_of(name, source).to_lowercase());
            let used = match alias.as_deref() {
                // A blank or dot import is used by being there.
                Some("_" | ".") => false,
                Some(alias) => selected.contains(alias),
                None => {
                    let element = package_element(&path);
                    selected.iter().any(|name| {
                        let name: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
                        !name.is_empty() && (element.contains(&name) || name.contains(&element))
                    })
                }
            };
            used.then(|| format!("\"{path}\""))
        })
        .collect()
}

/// The last element of an import path that names the package, lowercased and with
/// version suffixes, `go-` and `-go` and punctuation removed.
fn package_element(path: &str) -> String {
    let mut elements = path.rsplit('/');
    let mut element = elements.next().unwrap_or(path);
    let is_major_version = |text: &str| {
        text.strip_prefix('v')
            .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
    };
    if is_major_version(element) {
        element = elements.next().unwrap_or(element);
    }
    let element = element.split(".v").next().unwrap_or(element).to_lowercase();
    let element = element
        .strip_prefix("go-")
        .or_else(|| element.strip_suffix("-go"))
        .unwrap_or(&element);
    element.chars().filter(|c| c.is_alphanumeric()).collect()
}

fn squashed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn broken(language: Language, before: &str, after: &str) -> Option<String> {
        let before = Invariants::of(language, false, before).expect("parses");
        let after = Invariants::of(language, false, after).expect("parses");
        before.broken_by(&after)
    }

    #[test]
    fn removing_a_label_a_goto_names_is_refused() {
        let before = "package p\n\nfunc f(n int) int {\n\tif n > 3 {\n\t\tgoto done\n\t}\n\treturn 0\ndone:\n\treturn 1\n}\n";
        let after =
            "package p\n\nfunc f(n int) int {\n\tif n > 3 {\n\t\tgoto done\n\t}\n\treturn 0\n}\n";
        assert_eq!(
            broken(Language::Go, before, after).as_deref(),
            Some("removes the label `done`")
        );
    }

    #[test]
    fn removing_a_build_constraint_is_refused() {
        let before = "//go:build ignore\n\npackage main\n\nfunc main() {}\n";
        let after = "package main\n\nfunc main() {}\n";
        assert!(broken(Language::Go, before, after).is_some());
    }

    #[test]
    fn removing_the_only_yield_is_refused() {
        let before = "def events(log):\n    log('start')\n    return\n    yield\n";
        let after = "def events(log):\n    log('start')\n    return\n";
        assert_eq!(
            broken(Language::Python, before, after).as_deref(),
            Some("removes the `yield` that makes `events` a generator")
        );
    }

    #[test]
    fn removing_the_last_use_of_a_go_import_or_local_is_refused() {
        let before = "package p\n\nimport \"fmt\"\n\nfunc f() int {\n\tx := 1\n\treturn 1\n\tfmt.Println(x)\n}\n";
        let after = "package p\n\nimport \"fmt\"\n\nfunc f() int {\n\tx := 1\n\treturn 1\n}\n";
        assert_eq!(
            broken(Language::Go, before, after).as_deref(),
            Some("leaves the import \"fmt\" unused, which Go refuses to compile")
        );
        let before = "package p\n\nfunc f() int {\n\tx := 1\n\treturn 1\n\tprintln(x)\n}\n";
        let after = "package p\n\nfunc f() int {\n\tx := 1\n\treturn 1\n}\n";
        assert_eq!(
            broken(Language::Go, before, after).as_deref(),
            Some("leaves `x` declared and not used, which Go refuses to compile")
        );
    }

    /// `functools._lru_cache_wrapper`: `full = False` is never read where it is
    /// assigned, but the nested `wrapper` declares it `nonlocal`.
    #[test]
    fn removing_the_binding_a_nonlocal_names_is_refused() {
        let before = "def outer():\n    full = False\n    def inner():\n        nonlocal full\n        full = True\n    return inner\n";
        let after = "def outer():\n    def inner():\n        nonlocal full\n        full = True\n    return inner\n";
        assert_eq!(
            broken(Language::Python, before, after).as_deref(),
            Some("removes a use of `full`, which a `global` or `nonlocal` statement names")
        );
    }

    #[test]
    fn removing_a_macro_call_or_a_suppression_is_refused() {
        let before = "int g() {\n  return 1;\n  NOTREACHED();\n}\n";
        let after = "int g() {\n  return 1;\n}\n";
        assert!(broken(Language::Cpp, before, after).is_some());
        let before = "import os  # noqa: F401\n";
        let after = "import os\n";
        assert!(broken(Language::Python, before, after).is_some());
    }

    #[test]
    fn changing_a_public_signature_is_refused_and_a_private_one_is_not() {
        let before = "__all__ = ['handler']\n\ndef handler(event, context):\n    return 'ok'\n\ndef _helper(a, b):\n    return a\n";
        let after = "__all__ = ['handler']\n\ndef handler(event):\n    return 'ok'\n\ndef _helper(a, b):\n    return a\n";
        assert_eq!(
            broken(Language::Python, before, after).as_deref(),
            Some("changes the signature of `handler`, which code outside this project may call")
        );
        let after = "__all__ = ['handler']\n\ndef handler(event, context):\n    return 'ok'\n\ndef _helper(a):\n    return a\n";
        assert_eq!(broken(Language::Python, before, after), None);
        let before = "public class App { public static void main(String[] args) { } }\n";
        let after = "public class App { public static void main() { } }\n";
        assert!(broken(Language::Java, before, after).is_some());
    }

    #[test]
    fn dead_code_that_held_none_of_these_passes() {
        let before = "package p\n\nimport \"fmt\"\n\nfunc f() int {\n\tfmt.Println(1)\n\treturn 1\n\tfmt.Println(2)\n}\n";
        let after =
            "package p\n\nimport \"fmt\"\n\nfunc f() int {\n\tfmt.Println(1)\n\treturn 1\n}\n";
        assert_eq!(broken(Language::Go, before, after), None);
    }

    #[test]
    fn a_versioned_or_prefixed_path_names_its_package() {
        assert_eq!(package_element("gopkg.in/yaml.v3"), "yaml");
        assert_eq!(package_element("github.com/redis/go-redis/v9"), "redis");
        assert_eq!(package_element("github.com/klauspost/compress/s2"), "s2");
    }
}

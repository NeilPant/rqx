//! What a Python module offers to other modules, and what it asks for. Python has no
//! export keyword: a module-level name without a leading underscore is public, and
//! `__all__` narrows that when it is present.

use rqx_core::facts::{ExportKind, ExportedSymbol, Heritage, ImportRequest, ModuleFacts};
use tree_sitter::Node;

use crate::tree::{kind_of, span_of, text_of};

fn named_child_of_kinds<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| kinds.contains(&kind_of(child)))
}

/// `from .. import x` names a package two levels up; the dots are the whole address.
fn import_specifier(node: Node<'_>, source: &str) -> Option<String> {
    let module = node.child_by_field_name("module_name")?;
    Some(text_of(module, source).to_owned())
}

/// What the target exports, what this file calls it, and whether the statement took
/// everything.
fn imported_names(node: Node<'_>, source: &str) -> (Vec<String>, Vec<String>, bool) {
    let mut names = Vec::new();
    let mut locals = Vec::new();
    let mut cursor = node.walk();
    let module = node.child_by_field_name("module_name");
    for child in node.named_children(&mut cursor) {
        if Some(child) == module {
            continue;
        }
        match kind_of(child) {
            "wildcard_import" => return (Vec::new(), Vec::new(), true),
            "dotted_name" | "identifier" => {
                names.push(text_of(child, source).to_owned());
                locals.push(text_of(child, source).to_owned());
            }
            "aliased_import" => {
                if let Some(name) = child.child_by_field_name("name") {
                    names.push(text_of(name, source).to_owned());
                }
                if let Some(alias) = child.child_by_field_name("alias") {
                    locals.push(text_of(alias, source).to_owned());
                }
            }
            _ => {}
        }
    }
    (names, locals, false)
}

fn public(name: &str) -> bool {
    !name.starts_with('_')
}

fn declared(node: Node<'_>, source: &str) -> Option<(String, rqx_core::span::Span, ExportKind)> {
    let kind = match kind_of(node) {
        "function_definition" => ExportKind::Function,
        "class_definition" => ExportKind::Class,
        _ => return None,
    };
    let name = node.child_by_field_name("name")?;
    Some((text_of(name, source).to_owned(), span_of(name), kind))
}

/// `__all__ = ["a", "b"]` is the module saying what it means to offer, and it
/// overrides the underscore convention wherever it appears.
fn explicit_exports(module: Node<'_>, source: &str) -> Option<Vec<String>> {
    let mut cursor = module.walk();
    for statement in module.named_children(&mut cursor) {
        let Some(assignment) = named_child_of_kinds(statement, &["assignment"]) else {
            continue;
        };
        let Some(target) = assignment.child_by_field_name("left") else {
            continue;
        };
        if text_of(target, source) != "__all__" {
            continue;
        }
        let Some(list) = assignment.child_by_field_name("right") else {
            continue;
        };
        let mut inner = list.walk();
        return Some(
            list.named_children(&mut inner)
                .filter(|child| kind_of(child) == "string")
                .map(|child| text_of(child, source).trim_matches(['"', '\'']).to_owned())
                .collect(),
        );
    }
    None
}

fn collect_class(class: Node<'_>, source: &str, facts: &mut ModuleFacts) {
    let Some(name) = class.child_by_field_name("name") else {
        return;
    };
    let child = text_of(name, source).to_owned();
    let Some(bases) = class.child_by_field_name("superclasses") else {
        return;
    };
    let mut cursor = bases.walk();
    let mut abstract_base = false;
    for base in bases.named_children(&mut cursor) {
        let parent = text_of(base, source).to_owned();
        if parent == "ABC" || parent.ends_with(".ABC") || parent == "Protocol" {
            abstract_base = true;
        }
        facts.implemented.push(parent.clone());
        facts.heritage.push(Heritage {
            child: child.clone(),
            parent,
            span: span_of(name),
        });
    }
    // An abstract base with no subclass is the speculative generality this reports;
    // Python spells "abstract" by inheriting ABC rather than with a keyword.
    if abstract_base {
        facts.abstractions.push(ExportedSymbol {
            name: child,
            span: span_of(name),
            kind: ExportKind::Class,
        });
    }
}

pub fn module_facts(source: &str, root: Node<'_>) -> ModuleFacts {
    let mut facts = ModuleFacts::default();
    let listed = explicit_exports(root, source);

    let mut cursor = root.walk();
    for statement in root.named_children(&mut cursor) {
        match kind_of(statement) {
            "import_statement" => {
                let mut inner = statement.walk();
                for target in statement.named_children(&mut inner) {
                    let specifier = match kind_of(target) {
                        "aliased_import" => target
                            .child_by_field_name("name")
                            .map(|name| text_of(name, source).to_owned()),
                        "dotted_name" => Some(text_of(target, source).to_owned()),
                        _ => None,
                    };
                    if let Some(specifier) = specifier {
                        facts.imports.push(ImportRequest {
                            specifier,
                            names: Vec::new(),
                            locals: Vec::new(),
                            span: span_of(statement),
                            // `import a.b` binds the whole module, so any name in it
                            // may be reached through the dotted path.
                            takes_everything: true,
                            re_export: false,
                        });
                    }
                }
            }
            "import_from_statement" => {
                let Some(specifier) = import_specifier(statement, source) else {
                    continue;
                };
                let (names, locals, wildcard) = imported_names(statement, source);
                facts.imports.push(ImportRequest {
                    specifier,
                    names,
                    locals,
                    span: span_of(statement),
                    takes_everything: wildcard,
                    re_export: false,
                });
            }
            _ => {}
        }

        let declaration = if kind_of(statement) == "decorated_definition" {
            statement.named_child(statement.named_child_count().saturating_sub(1))
        } else {
            Some(statement)
        };
        let Some(declaration) = declaration else {
            continue;
        };

        if kind_of(declaration) == "class_definition" {
            collect_class(declaration, source, &mut facts);
        }
        if let Some((name, span, kind)) = declared(declaration, source) {
            if listed
                .as_ref()
                .map_or_else(|| public(&name), |all| all.contains(&name))
            {
                facts.exports.push(ExportedSymbol { name, span, kind });
            }
            continue;
        }
        if let Some(assignment) = named_child_of_kinds(declaration, &["assignment"])
            && let Some(target) = assignment.child_by_field_name("left")
            && kind_of(target) == "identifier"
        {
            let name = text_of(target, source).to_owned();
            if name != "__all__"
                && listed
                    .as_ref()
                    .map_or_else(|| public(&name), |all| all.contains(&name))
            {
                facts.exports.push(ExportedSymbol {
                    name,
                    span: span_of(target),
                    kind: ExportKind::Variable,
                });
            }
        }
    }
    facts.used_locally = crate::module_graph::read_where_declared(&facts.exports, source);
    facts
}

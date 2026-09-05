//! The names a TypeScript file offers to other files, and the ones it asks for.

use rqx_core::facts::{ExportKind, ExportedSymbol, Heritage, ImportRequest, ModuleFacts};
use tree_sitter::Node;

use crate::tree::{descendants, span_of, text_of};

fn named_child_of_kinds<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()))
}

fn string_value<'src>(node: Node<'_>, source: &'src str) -> Option<&'src str> {
    let fragment = named_child_of_kinds(node, &["string_fragment"])?;
    Some(text_of(fragment, source))
}

fn declared(node: Node<'_>, source: &str) -> Option<(String, rqx_core::span::Span, ExportKind)> {
    let kind = match node.kind() {
        "function_declaration" | "generator_function_declaration" | "function_signature" => {
            ExportKind::Function
        }
        "class_declaration" | "abstract_class_declaration" => ExportKind::Class,
        "interface_declaration" => ExportKind::Interface,
        "type_alias_declaration" => ExportKind::TypeAlias,
        "enum_declaration" => ExportKind::Enum,
        _ => return None,
    };
    let name = node.child_by_field_name("name")?;
    Some((text_of(name, source).to_owned(), span_of(name), kind))
}

/// Every `export` form TypeScript has, because "nothing imports this name" is only
/// true if the name was found in the first place. A form missed here becomes a
/// dead-export finding about a symbol the project uses.
fn collect_exports(statement: Node<'_>, source: &str, facts: &mut ModuleFacts) {
    if let Some(clause) = named_child_of_kinds(statement, &["export_clause"]) {
        let source_module = named_child_of_kinds(statement, &["string"])
            .and_then(|node| string_value(node, source));
        let mut cursor = clause.walk();
        let specifiers: Vec<Node<'_>> = clause
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "export_specifier")
            .collect();
        // A re-export is both: it uses the target's name and offers one of its own.
        if let Some(specifier) = source_module {
            facts.imports.push(ImportRequest {
                specifier: specifier.to_owned(),
                names: specifiers
                    .iter()
                    .filter_map(|node| node.child_by_field_name("name"))
                    .map(|name| text_of(name, source).to_owned())
                    .collect(),
                locals: Vec::new(),
                span: span_of(statement),
                takes_everything: false,
                re_export: true,
            });
        }
        for specifier in specifiers {
            let exported = specifier
                .child_by_field_name("alias")
                .or_else(|| specifier.child_by_field_name("name"));
            if let Some(exported) = exported {
                facts.exports.push(ExportedSymbol {
                    name: text_of(exported, source).to_owned(),
                    span: span_of(specifier),
                    kind: ExportKind::Variable,
                });
            }
        }
        return;
    }

    // `export * from "./x"` uses every name the target offers.
    if let Some(module) = named_child_of_kinds(statement, &["string"])
        && statement.child_by_field_name("declaration").is_none()
    {
        if let Some(specifier) = string_value(module, source) {
            facts.imports.push(ImportRequest {
                specifier: specifier.to_owned(),
                names: Vec::new(),
                locals: Vec::new(),
                span: span_of(statement),
                takes_everything: true,
                re_export: true,
            });
        }
        return;
    }

    let Some(declaration) = statement.child_by_field_name("declaration") else {
        if text_of(statement, source).starts_with("export default") {
            facts.has_default_export = true;
            facts.exports.push(ExportedSymbol {
                name: "default".to_owned(),
                span: span_of(statement),
                kind: ExportKind::Default,
            });
        }
        return;
    };

    if let Some((name, span, kind)) = declared(declaration, source) {
        facts.exports.push(ExportedSymbol { name, span, kind });
        return;
    }
    if matches!(
        declaration.kind(),
        "lexical_declaration" | "variable_declaration"
    ) {
        let mut cursor = declaration.walk();
        for declarator in declaration.named_children(&mut cursor) {
            if let Some(name) = declarator.child_by_field_name("name")
                && name.kind() == "identifier"
            {
                facts.exports.push(ExportedSymbol {
                    name: text_of(name, source).to_owned(),
                    span: span_of(name),
                    kind: ExportKind::Variable,
                });
            }
        }
    }
}

fn collect_import(statement: Node<'_>, source: &str, facts: &mut ModuleFacts) {
    let Some(module) = named_child_of_kinds(statement, &["string"]) else {
        return;
    };
    let Some(specifier) = string_value(module, source) else {
        return;
    };
    let mut names = Vec::new();
    let mut locals = Vec::new();
    let mut takes_everything = false;
    if let Some(clause) = named_child_of_kinds(statement, &["import_clause"]) {
        let mut cursor = clause.walk();
        for child in clause.named_children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    names.push("default".to_owned());
                    locals.push(text_of(child, source).to_owned());
                }
                "namespace_import" => {
                    takes_everything = true;
                    if let Some(alias) = child.named_child(0) {
                        locals.push(text_of(alias, source).to_owned());
                    }
                }
                "named_imports" => {
                    let mut inner = child.walk();
                    for specifier in child.named_children(&mut inner) {
                        if let Some(name) = specifier.child_by_field_name("name") {
                            names.push(text_of(name, source).to_owned());
                        }
                        if let Some(bound) = specifier
                            .child_by_field_name("alias")
                            .or_else(|| specifier.child_by_field_name("name"))
                        {
                            locals.push(text_of(bound, source).to_owned());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    facts.imports.push(ImportRequest {
        specifier: specifier.to_owned(),
        names,
        locals,
        span: span_of(statement),
        takes_everything,
        re_export: false,
    });
}

/// Interfaces are deliberately absent from `abstractions`. TypeScript's are
/// structural: the vast majority describe a data shape and are never `implements`-ed
/// by anything, so "nothing implements it" says nothing about whether it is
/// speculative. An abstract class with no subclass is unambiguous.
fn collect_class(class: Node<'_>, source: &str, facts: &mut ModuleFacts) {
    let Some(name) = class.child_by_field_name("name") else {
        return;
    };
    let child = text_of(name, source).to_owned();
    if class.kind() == "abstract_class_declaration" {
        facts.abstractions.push(ExportedSymbol {
            name: child.clone(),
            span: span_of(name),
            kind: ExportKind::Class,
        });
    }
    let Some(heritage) = named_child_of_kinds(class, &["class_heritage"]) else {
        return;
    };
    let mut cursor = heritage.walk();
    for clause in heritage.named_children(&mut cursor) {
        let mut inner = clause.walk();
        for parent in clause.named_children(&mut inner) {
            let parent_name = match parent.kind() {
                "identifier" | "type_identifier" => text_of(parent, source).to_owned(),
                _ => continue,
            };
            facts.implemented.push(parent_name.clone());
            if clause.kind() == "extends_clause" {
                facts.heritage.push(Heritage {
                    child: child.clone(),
                    parent: parent_name,
                    span: span_of(name),
                });
            }
        }
    }
}

fn collect_interface(interface: Node<'_>, source: &str, facts: &mut ModuleFacts) {
    let mut cursor = interface.walk();
    for child in interface.named_children(&mut cursor) {
        if child.kind() != "extends_type_clause" {
            continue;
        }
        let mut inner = child.walk();
        for parent in child.named_children(&mut inner) {
            if matches!(parent.kind(), "identifier" | "type_identifier") {
                facts.implemented.push(text_of(parent, source).to_owned());
            }
        }
    }
}

pub fn module_facts(source: &str, root: Node<'_>) -> ModuleFacts {
    let mut facts = ModuleFacts::default();
    for node in descendants(root) {
        match node.kind() {
            "export_statement" => collect_exports(node, source, &mut facts),
            "import_statement" => collect_import(node, source, &mut facts),
            "class_declaration" | "abstract_class_declaration" => {
                collect_class(node, source, &mut facts);
            }
            "interface_declaration" => collect_interface(node, source, &mut facts),
            _ => {}
        }
    }
    facts
}

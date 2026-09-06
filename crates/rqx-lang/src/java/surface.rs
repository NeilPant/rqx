//! What a Java file offers and asks for. `public` is the export keyword; the imports
//! are the `import` lines plus every type the file names, because a class in the same
//! package is used without importing it.

use std::collections::BTreeSet;

use rqx_core::facts::{ExportKind, ExportedSymbol, Heritage, ImportRequest, ModuleFacts};
use tree_sitter::Node;

use crate::tree::{descendants, kind_of, span_of, text_of};

fn has_modifier(node: Node<'_>, source: &str, marker: &str) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| kind_of(child) == "modifiers")
        .any(|modifiers| {
            text_of(modifiers, source)
                .split_whitespace()
                .any(|word| word == marker)
        })
}

fn package_of(root: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .find(|child| kind_of(child) == "package_declaration")
        .and_then(|declaration| declaration.named_child(0))
        .map(|name| text_of(name, source).to_owned())
}

fn export_kind(node: Node<'_>) -> ExportKind {
    match kind_of(node) {
        "interface_declaration" => ExportKind::Interface,
        "enum_declaration" => ExportKind::Enum,
        _ => ExportKind::Class,
    }
}

fn collect_heritage(class: Node<'_>, source: &str, facts: &mut ModuleFacts) {
    let Some(name) = class.child_by_field_name("name") else {
        return;
    };
    let child = text_of(name, source).to_owned();
    let mut cursor = class.walk();
    for clause in class.named_children(&mut cursor) {
        let extends = kind_of(clause) == "superclass";
        if !extends && !matches!(kind_of(clause), "super_interfaces" | "extends_interfaces") {
            continue;
        }
        for parent in descendants(clause)
            .into_iter()
            .filter(|node| kind_of(node) == "type_identifier")
        {
            let label = text_of(parent, source).to_owned();
            facts.implemented.push(label.clone());
            if extends {
                facts.heritage.push(Heritage {
                    child: child.clone(),
                    parent: label,
                    span: span_of(name),
                });
            }
        }
    }
}

pub fn module_facts(source: &str, root: Node<'_>) -> ModuleFacts {
    let mut facts = ModuleFacts::default();
    let package = package_of(root, source);
    let nodes = descendants(root);

    let mut declared: BTreeSet<String> = BTreeSet::new();
    for node in &nodes {
        let kind = kind_of(node);
        if kind == "import_declaration" {
            if let Some(name) = node.named_child(0) {
                let specifier = text_of(name, source).to_owned();
                // The simple name is what the rest of the file writes; a wildcard
                // import binds no name of its own.
                let bound = specifier
                    .rsplit('.')
                    .next()
                    .filter(|simple| *simple != "*" && !text_of(*node, source).ends_with(".*;"))
                    .map(str::to_owned);
                let bound: Vec<String> = bound.into_iter().collect();
                facts.imports.push(ImportRequest {
                    specifier,
                    names: bound.clone(),
                    locals: bound,
                    span: span_of(*node),
                    // An import names one type, and that type is the whole of what
                    // the importing file takes.
                    takes_everything: false,
                    re_export: false,
                });
            }
            continue;
        }
        if !matches!(
            kind,
            "class_declaration"
                | "interface_declaration"
                | "record_declaration"
                | "enum_declaration"
        ) {
            continue;
        }
        let Some(name) = node.child_by_field_name("name") else {
            continue;
        };
        declared.insert(text_of(name, source).to_owned());
        collect_heritage(*node, source, &mut facts);

        // An interface or abstract class nothing implements is the speculative
        // generality this reports.
        if kind == "interface_declaration" || has_modifier(*node, source, "abstract") {
            facts.abstractions.push(ExportedSymbol {
                name: text_of(name, source).to_owned(),
                span: span_of(name),
                kind: export_kind(*node),
            });
        }
        if has_modifier(*node, source, "public") {
            facts.exports.push(ExportedSymbol {
                name: text_of(name, source).to_owned(),
                span: span_of(name),
                kind: export_kind(*node),
            });
            // A public method is part of the surface too: one public type per file
            // makes the type-level question the same as the file-level one, and the
            // method is where a Java API is actually dead.
            if let Some(body) = node.child_by_field_name("body") {
                let mut inner = body.walk();
                for member in body.named_children(&mut inner) {
                    if kind_of(member) != "method_declaration"
                        || !has_modifier(member, source, "public")
                    {
                        continue;
                    }
                    if let Some(label) = member.child_by_field_name("name") {
                        facts.exports.push(ExportedSymbol {
                            name: text_of(label, source).to_owned(),
                            span: span_of(label),
                            kind: ExportKind::Function,
                        });
                    }
                }
            }
        }
    }

    // A class in the same package is used without an import line, so every type this
    // file names and does not declare is a dependency the graph has to see.
    if let Some(package) = package {
        let mut referenced: BTreeSet<String> = BTreeSet::new();
        for node in &nodes {
            if kind_of(node) != "type_identifier" {
                continue;
            }
            let name = text_of(*node, source);
            if !declared.contains(name) {
                referenced.insert(name.to_owned());
            }
        }
        // What this file takes from another is the type it names plus every method it
        // calls; a call site is the only place a Java method's use is visible.
        let called: Vec<String> = nodes
            .iter()
            .filter(|node| kind_of(*node) == "method_invocation")
            .filter_map(|node| node.child_by_field_name("name"))
            .map(|name| text_of(name, source).to_owned())
            .collect();
        for name in referenced {
            let mut names = vec![name.clone()];
            names.extend(called.iter().cloned());
            facts.imports.push(ImportRequest {
                locals: Vec::new(),
                specifier: format!("{package}.{name}"),
                names,
                span: rqx_core::span::Span::new(0, 0),
                takes_everything: false,
                re_export: false,
            });
        }
        for import in &mut facts.imports {
            if import.names.is_empty() {
                let leaf = import
                    .specifier
                    .rsplit('.')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                import.names = std::iter::once(leaf)
                    .chain(called.iter().cloned())
                    .collect();
            }
        }
    }
    facts
}

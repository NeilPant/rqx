//! What a C++ translation unit offers and asks for. C++ has no export keyword: a
//! header declares the surface, and `#include` is how another file takes it.

use rqx_core::facts::{ExportKind, ExportedSymbol, Heritage, ImportRequest, ModuleFacts};
use tree_sitter::Node;

use crate::tree::{descendants, find_within, span_of, text_of};

const DECLARATOR_KINDS: [&str; 3] = [
    "function_declarator",
    "pointer_declarator",
    "reference_declarator",
];

fn declared_name<'tree>(node: Node<'tree>) -> Option<Node<'tree>> {
    let declarator = find_within(node, &DECLARATOR_KINDS, &["compound_statement"])
        .into_iter()
        .next()?;
    let mut cursor = declarator.walk();
    declarator
        .named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "identifier" | "field_identifier"))
}

fn collect_class(class: Node<'_>, source: &str, facts: &mut ModuleFacts) {
    let Some(name) = class.child_by_field_name("name") else {
        return;
    };
    let child = text_of(name, source).to_owned();
    facts.exports.push(ExportedSymbol {
        name: child.clone(),
        span: span_of(name),
        kind: ExportKind::Class,
    });

    // A class with a pure virtual member is C++'s abstract base, and one nothing
    // derives from is the speculative generality this reports.
    if text_of(class, source).contains("= 0;") {
        facts.abstractions.push(ExportedSymbol {
            name: child.clone(),
            span: span_of(name),
            kind: ExportKind::Class,
        });
    }

    let mut cursor = class.walk();
    for clause in class
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "base_class_clause")
    {
        for parent in descendants(clause)
            .into_iter()
            .filter(|node| node.kind() == "type_identifier")
        {
            let label = text_of(parent, source).to_owned();
            facts.implemented.push(label.clone());
            facts.heritage.push(Heritage {
                child: child.clone(),
                parent: label,
                span: span_of(name),
            });
        }
    }
}

pub fn module_facts(source: &str, root: Node<'_>) -> ModuleFacts {
    let mut facts = ModuleFacts::default();
    let nodes = descendants(root);

    // An include pastes the header in, so what the file takes from it is every name
    // it goes on to use.
    let used: Vec<String> = nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind(),
                "identifier" | "type_identifier" | "field_identifier"
            )
        })
        .map(|node| text_of(*node, source).to_owned())
        .collect();

    for node in nodes.iter().copied() {
        match node.kind() {
            "preproc_include" => {
                let Some(path) = node.child_by_field_name("path") else {
                    continue;
                };
                // Angled includes are recorded even though none of them can name a
                // file inside the project: `<boost/asio.hpp>` is a dependency the
                // project chose, and only this pass is in a position to see it.
                facts.imports.push(ImportRequest {
                    specifier: text_of(path, source)
                        .trim_matches(['"', '<', '>'])
                        .to_owned(),
                    names: Vec::new(),
                    span: span_of(node),
                    takes_everything: false,
                });
            }
            "class_specifier" | "struct_specifier" => collect_class(node, source, &mut facts),
            "function_definition" | "declaration" => {
                // Only a declaration outside any class is part of the file's surface;
                // a member belongs to the class that holds it.
                let inside_class = node
                    .parent()
                    .is_some_and(|parent| parent.kind() == "field_declaration_list");
                if inside_class {
                    continue;
                }
                if let Some(name) = declared_name(node) {
                    facts.exports.push(ExportedSymbol {
                        name: text_of(name, source).to_owned(),
                        span: span_of(name),
                        kind: ExportKind::Function,
                    });
                }
            }
            _ => {}
        }
    }
    for import in &mut facts.imports {
        import.names = used.clone();
    }
    facts
}

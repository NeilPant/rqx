//! What a Go package offers and asks for. Go has no export keyword: an identifier
//! declared at package level is public exactly when it starts with a capital.

use rqx_core::facts::{ExportKind, ExportedSymbol, ImportRequest, ModuleFacts};
use tree_sitter::Node;

use crate::classes::ClassUnit;
use crate::profile::LanguageProfile;
use crate::tree::{descendants, span_of, text_of};

fn exported(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

fn string_value<'src>(node: Node<'_>, source: &'src str) -> &'src str {
    text_of(node, source).trim_matches(['"', '`'])
}

fn record(facts: &mut ModuleFacts, name: Node<'_>, source: &str, kind: ExportKind) {
    let text = text_of(name, source);
    if exported(text) {
        facts.exports.push(ExportedSymbol {
            name: text.to_owned(),
            span: span_of(name),
            kind,
        });
    }
}

pub fn module_facts(source: &str, root: Node<'_>) -> ModuleFacts {
    let mut facts = ModuleFacts::default();
    let mut cursor = root.walk();
    for declaration in root.named_children(&mut cursor) {
        match declaration.kind() {
            "import_declaration" => {
                for spec in descendants(declaration)
                    .into_iter()
                    .filter(|node| node.kind() == "import_spec")
                {
                    let Some(path) = spec.child_by_field_name("path") else {
                        continue;
                    };
                    facts.imports.push(ImportRequest {
                        specifier: string_value(path, source).to_owned(),
                        names: Vec::new(),
                        span: span_of(spec),
                        takes_everything: false,
                    });
                }
            }
            "function_declaration" => {
                if let Some(name) = declaration.child_by_field_name("name") {
                    record(&mut facts, name, source, ExportKind::Function);
                }
            }
            "method_declaration" => {}
            "type_declaration" => {
                for spec in descendants(declaration)
                    .into_iter()
                    .filter(|node| node.kind() == "type_spec")
                {
                    let Some(name) = spec.child_by_field_name("name") else {
                        continue;
                    };
                    let interface = spec
                        .child_by_field_name("type")
                        .is_some_and(|body| body.kind() == "interface_type");
                    record(
                        &mut facts,
                        name,
                        source,
                        if interface {
                            ExportKind::Interface
                        } else {
                            ExportKind::Class
                        },
                    );
                    // An interface nothing satisfies is the speculative generality
                    // this reports; Go has no `implements`, so satisfaction is read
                    // from the method sets in `implemented`.
                    if interface && exported(text_of(name, source)) {
                        facts.abstractions.push(ExportedSymbol {
                            name: text_of(name, source).to_owned(),
                            span: span_of(name),
                            kind: ExportKind::Interface,
                        });
                    }
                }
            }
            "var_declaration" | "const_declaration" => {
                for spec in descendants(declaration)
                    .into_iter()
                    .filter(|node| matches!(node.kind(), "var_spec" | "const_spec"))
                {
                    if let Some(name) = spec.child_by_field_name("name") {
                        record(&mut facts, name, source, ExportKind::Variable);
                    }
                }
            }
            _ => {}
        }
    }

    // Go imports a package rather than a name, so what the file actually takes is
    // whatever it writes after the package identifier: `pkg.Name`.
    let selectors: Vec<(String, String)> = descendants(root)
        .into_iter()
        .filter(|node| node.kind() == "selector_expression")
        .filter_map(|node| {
            let operand = node.child_by_field_name("operand")?;
            let field = node.child_by_field_name("field")?;
            (operand.kind() == "identifier").then(|| {
                (
                    text_of(operand, source).to_owned(),
                    text_of(field, source).to_owned(),
                )
            })
        })
        .collect();
    for import in &mut facts.imports {
        let package = import
            .specifier
            .rsplit('/')
            .next()
            .unwrap_or(&import.specifier)
            .to_owned();
        import.names = selectors
            .iter()
            .filter(|(operand, _)| *operand == package)
            .map(|(_, name)| name.clone())
            .collect();
    }

    // A named type satisfies an interface by having its methods, and the only thing
    // this file can see is which types have methods at all.
    for method in descendants(root)
        .into_iter()
        .filter(|node| node.kind() == "method_declaration")
    {
        if let Some(owner) = receiver_type(method, source) {
            facts.implemented.push(owner);
        }
    }
    facts
}

/// `func (e *Engine) Run()` — the receiver's type, with any pointer stripped, is the
/// type the method belongs to.
pub fn receiver_type(method: Node<'_>, source: &str) -> Option<String> {
    let receiver = method.child_by_field_name("receiver")?;
    descendants(receiver)
        .into_iter()
        .find(|node| node.kind() == "type_identifier")
        .map(|node| text_of(node, source).to_owned())
}

/// A Go type and the methods declared for it anywhere in the file. Nothing in the
/// grammar joins the two, so the pairing is done by receiver type.
pub fn units<'tree>(
    _profile: &'static LanguageProfile,
    source: &str,
    root: Node<'tree>,
) -> Vec<ClassUnit<'tree>> {
    let nodes = descendants(root);
    let mut found: Vec<ClassUnit<'tree>> = Vec::new();

    for spec in nodes
        .iter()
        .copied()
        .filter(|node| node.kind() == "type_spec")
    {
        let (Some(name), Some(body)) = (
            spec.child_by_field_name("name"),
            spec.child_by_field_name("type"),
        ) else {
            continue;
        };
        if !matches!(body.kind(), "struct_type" | "interface_type") {
            continue;
        }
        let label = text_of(name, source).to_owned();
        let mut members: Vec<Node<'tree>> = descendants(body)
            .into_iter()
            .filter(|node| node.kind() == "field_declaration")
            .collect();
        members.extend(
            nodes
                .iter()
                .copied()
                .filter(|node| node.kind() == "method_declaration")
                .filter(|node| receiver_type(*node, source).as_deref() == Some(label.as_str())),
        );
        // `func NewEngine() *Engine` is where an Engine's fields are set up: Go has
        // no constructor, and the convention is the whole of it.
        members.extend(
            nodes
                .iter()
                .copied()
                .filter(|node| node.kind() == "function_declaration")
                .filter(|node| {
                    node.child_by_field_name("name")
                        .is_some_and(|id| text_of(id, source) == format!("New{label}"))
                }),
        );
        found.push(ClassUnit {
            // Embedding is not inheritance, so there is nothing a Go type promises by
            // its declaration alone.
            supertypes: Vec::new(),
            name: label,
            span: span_of(name),
            members,
            // Embedding is not inheritance: an embedded type is a field, and nothing
            // about it can be refused or overridden.
            has_heritage: false,
        });
    }
    found
}

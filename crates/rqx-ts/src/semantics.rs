use oxc::ast::ast::{Declaration, FormalParameters, Function, ModuleDeclaration, Program};
use oxc::ast_visit::{Visit, walk};
use oxc::semantic::Semantic;
use oxc::span::{GetSpan, Span};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub enum ExportKind {
    Function,
    Class,
    Interface,
    TypeAlias,
    Enum,
    Variable,
    Default,
}

impl ExportKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::TypeAlias => "type",
            Self::Enum => "enum",
            Self::Variable => "value",
            Self::Default => "default",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExportedSymbol {
    pub name: String,
    #[serde(with = "crate::span_io")]
    pub span: Span,
    pub kind: ExportKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ImportRequest {
    pub specifier: String,
    pub names: Vec<String>,
    #[serde(with = "crate::span_io")]
    pub span: Span,
    /// A namespace import or `export *` uses every name the target offers, so the
    /// target's exports cannot be judged unused from the names list alone.
    pub takes_everything: bool,
}

/// Who decides the shape of a parameter list. Only a signature the author owns can
/// be shortened: a callback's arity is fixed by whatever calls it, and an override's
/// by the thing it overrides, so an unused parameter there is a contract being
/// honoured rather than dead weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignatureOwner {
    Author,
    Caller,
    Supertype,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UnusedParameter {
    pub name: String,
    #[serde(with = "crate::span_io")]
    pub span: Span,
    pub function: Option<String>,
    /// One-based, so it reads the way a person counts arguments.
    pub position: u32,
    pub total: u32,
    pub exported: bool,
    /// Deleting a trailing parameter is a local edit; deleting one with parameters
    /// after it renumbers every call site, which is a different-sized change.
    pub trailing: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ModuleFacts {
    pub exports: Vec<ExportedSymbol>,
    pub imports: Vec<ImportRequest>,
    pub unused_parameters: Vec<UnusedParameter>,
    pub has_default_export: bool,
    /// Interfaces and abstract classes declared here, and the names anything in this
    /// file implements or extends. The graph pass subtracts one from the other.
    pub abstractions: Vec<ExportedSymbol>,
    pub implemented: Vec<String>,
    /// Every `extends` in this file, paired. `implemented` flattens the same names
    /// away, which answers "is anything a subclass" but never "a subclass of what".
    pub heritage: Vec<Heritage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Heritage {
    pub child: String,
    pub parent: String,
    #[serde(with = "crate::span_io")]
    pub span: Span,
}

struct Collector<'s> {
    semantic: &'s Semantic<'s>,
    facts: ModuleFacts,
    exported_depth: u32,
    enclosing: Vec<Option<String>>,
    owner: Vec<SignatureOwner>,
}

impl<'s> Collector<'s> {
    fn owner(&self) -> SignatureOwner {
        self.owner.last().copied().unwrap_or(SignatureOwner::Author)
    }

    fn record_parameters(&mut self, params: &FormalParameters<'_>, name: Option<String>) {
        if self.owner() != SignatureOwner::Author {
            return;
        }
        let total = params.items.len() as u32;
        let mut unused = Vec::new();
        for (index, param) in params.items.iter().enumerate() {
            // `constructor(private readonly foo: Bar)` declares a field; the
            // parameter having no reference in the constructor body says nothing.
            if param.accessibility.is_some() || param.readonly {
                continue;
            }
            let oxc::ast::ast::BindingPattern::BindingIdentifier(binding) = &param.pattern else {
                continue;
            };
            // A leading underscore is the ecosystem's way of saying "required by the
            // signature, deliberately ignored"; reporting it would be reporting the
            // author's own annotation back at them.
            if binding.name.starts_with('_') {
                continue;
            }
            let Some(symbol_id) = binding.symbol_id.get() else {
                continue;
            };
            if !self.semantic.scoping().symbol_is_unused(symbol_id) {
                continue;
            }
            unused.push((index as u32, binding.name.to_string(), binding.span));
        }

        let last_used = params.items.iter().enumerate().rposition(|(index, _)| {
            !unused.iter().any(|(unused_index, ..)| *unused_index == index as u32)
        });
        for (index, param_name, span) in unused {
            let trailing = last_used.is_none_or(|last| index as usize > last);
            self.facts.unused_parameters.push(UnusedParameter {
                name: param_name,
                span,
                function: name.clone(),
                position: index + 1,
                total,
                exported: self.exported_depth > 0,
                trailing,
            });
        }
    }

    fn export_declaration(&mut self, declaration: &Declaration<'_>) {
        let (name, span, kind) = match declaration {
            Declaration::FunctionDeclaration(function) => match &function.id {
                Some(id) => (id.name.to_string(), id.span, ExportKind::Function),
                None => return,
            },
            Declaration::ClassDeclaration(class) => match &class.id {
                Some(id) => (id.name.to_string(), id.span, ExportKind::Class),
                None => return,
            },
            Declaration::TSInterfaceDeclaration(interface) => {
                (interface.id.name.to_string(), interface.id.span, ExportKind::Interface)
            }
            Declaration::TSTypeAliasDeclaration(alias) => {
                (alias.id.name.to_string(), alias.id.span, ExportKind::TypeAlias)
            }
            Declaration::TSEnumDeclaration(enumeration) => {
                (enumeration.id.name.to_string(), enumeration.id.span, ExportKind::Enum)
            }
            Declaration::VariableDeclaration(variables) => {
                for declarator in &variables.declarations {
                    if let oxc::ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id {
                        self.facts.exports.push(ExportedSymbol {
                            name: id.name.to_string(),
                            span: id.span,
                            kind: ExportKind::Variable,
                        });
                    }
                }
                return;
            }
            _ => return,
        };
        self.facts.exports.push(ExportedSymbol { name, span, kind });
    }
}

impl<'a> Visit<'a> for Collector<'_> {
    // Interfaces are deliberately absent. TypeScript's are structural: the vast
    // majority describe a data shape and are never `implements`-ed by anything, so
    // "nothing implements it" says nothing about whether it is speculative. An
    // abstract class with no subclass is unambiguous, and is what this rule reports.
    fn visit_ts_interface_declaration(
        &mut self,
        interface: &oxc::ast::ast::TSInterfaceDeclaration<'a>,
    ) {
        for parent in &interface.extends {
            if let Some(name) = parent.type_name.get_identifier_reference() {
                self.facts.implemented.push(name.name.to_string());
            }
        }
        walk::walk_ts_interface_declaration(self, interface);
    }

    fn visit_function(&mut self, function: &Function<'a>, flags: oxc::syntax::scope::ScopeFlags) {
        let name = function.id.as_ref().map(|id| id.name.to_string());
        let name = name.or_else(|| self.enclosing.last().cloned().flatten());
        // An overload signature, `declare function`, or abstract method has no body,
        // so of course nothing reads its parameters — the implementation elsewhere
        // does. Reporting these is reporting the absence of a body.
        if function.body.is_some() {
            self.record_parameters(&function.params, name);
        }
        // A function declared inside a callback still owns its own signature.
        self.owner.push(SignatureOwner::Author);
        walk::walk_function(self, function, flags);
        self.owner.pop();
    }

    fn visit_arrow_function_expression(
        &mut self,
        arrow: &oxc::ast::ast::ArrowFunctionExpression<'a>,
    ) {
        let name = self.enclosing.last().cloned().flatten();
        self.record_parameters(&arrow.params, name);
        self.owner.push(SignatureOwner::Author);
        walk::walk_arrow_function_expression(self, arrow);
        self.owner.pop();
    }

    fn visit_method_definition(&mut self, method: &oxc::ast::ast::MethodDefinition<'a>) {
        self.enclosing.push(method.key.static_name().map(|name| name.into_owned()));
        walk::walk_method_definition(self, method);
        self.enclosing.pop();
    }

    fn visit_class(&mut self, class: &oxc::ast::ast::Class<'a>) {
        if let Some(id) = &class.id
            && class.r#abstract
        {
            self.facts.abstractions.push(ExportedSymbol {
                name: id.name.to_string(),
                span: id.span,
                kind: ExportKind::Class,
            });
        }
        for implemented in &class.implements {
            if let Some(name) = implemented.expression.get_identifier_reference() {
                self.facts.implemented.push(name.name.to_string());
            }
        }
        if let Some(heritage) = &class.heritage
            && let Some(name) = heritage.expression.get_identifier_reference()
        {
            self.facts.implemented.push(name.name.to_string());
            if let Some(id) = &class.id {
                self.facts.heritage.push(Heritage {
                    child: id.name.to_string(),
                    parent: name.name.to_string(),
                    span: id.span,
                });
            }
        }
        
        let inherited = class.heritage.is_some() || !class.implements.is_empty();
        self.owner
            .push(if inherited { SignatureOwner::Supertype } else { SignatureOwner::Author });
        walk::walk_class(self, class);
        self.owner.pop();
    }

    fn visit_argument(&mut self, argument: &oxc::ast::ast::Argument<'a>) {
        self.owner.push(SignatureOwner::Caller);
        walk::walk_argument(self, argument);
        self.owner.pop();
    }

    fn visit_object_property(&mut self, property: &oxc::ast::ast::ObjectProperty<'a>) {
        self.enclosing.push(property.key.static_name().map(|name| name.into_owned()));
        // The property's type comes from the object's declared type, which fixes the
        // arity of any function stored in it.
        self.owner.push(SignatureOwner::Caller);
        walk::walk_object_property(self, property);
        self.owner.pop();
        self.enclosing.pop();
    }

    fn visit_variable_declarator(&mut self, declarator: &oxc::ast::ast::VariableDeclarator<'a>) {
        let name = match &declarator.id {
            oxc::ast::ast::BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
            _ => None,
        };
        self.enclosing.push(name);
        // A binding with a declared type has its function shape imposed by that type.
        let annotated = declarator.type_annotation.is_some();
        self.owner
            .push(if annotated { SignatureOwner::Caller } else { SignatureOwner::Author });
        walk::walk_variable_declarator(self, declarator);
        self.owner.pop();
        self.enclosing.pop();
    }

    fn visit_module_declaration(&mut self, declaration: &ModuleDeclaration<'a>) {
        match declaration {
            ModuleDeclaration::ImportDeclaration(import) => {
                let mut names = Vec::new();
                let mut takes_everything = false;
                if let Some(specifiers) = &import.specifiers {
                    for specifier in specifiers {
                        match specifier {
                            oxc::ast::ast::ImportDeclarationSpecifier::ImportSpecifier(named) => {
                                names.push(named.imported.name().to_string());
                            }
                            oxc::ast::ast::ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {
                                names.push("default".to_owned());
                            }
                            oxc::ast::ast::ImportDeclarationSpecifier::ImportNamespaceSpecifier(
                                _,
                            ) => takes_everything = true,
                        }
                    }
                }
                self.facts.imports.push(ImportRequest {
                    specifier: import.source.value.to_string(),
                    names,
                    span: import.span,
                    takes_everything,
                });
            }
            ModuleDeclaration::ExportDeclaration(export) => {
                self.exported_depth += 1;
                self.export_declaration(&export.declaration);
                walk::walk_declaration(self, &export.declaration);
                self.exported_depth -= 1;
                return;
            }
            ModuleDeclaration::ExportNamedDeclaration(export) => {
                for specifier in &export.specifiers {
                    self.facts.exports.push(ExportedSymbol {
                        name: specifier.exported.name().to_string(),
                        span: specifier.span(),
                        kind: ExportKind::Variable,
                    });
                }
            }
            // A re-export is both: it uses the target's name and offers one of its own.
            ModuleDeclaration::ExportFromDeclaration(export) => {
                self.facts.imports.push(ImportRequest {
                    specifier: export.source.value.to_string(),
                    names: export
                        .specifiers
                        .iter()
                        .map(|specifier| specifier.local.name().to_string())
                        .collect(),
                    span: export.span,
                    takes_everything: false,
                });
                for specifier in &export.specifiers {
                    self.facts.exports.push(ExportedSymbol {
                        name: specifier.exported.name().to_string(),
                        span: specifier.span(),
                        kind: ExportKind::Variable,
                    });
                }
            }
            ModuleDeclaration::ExportAllDeclaration(export) => {
                self.facts.imports.push(ImportRequest {
                    specifier: export.source.value.to_string(),
                    names: Vec::new(),
                    span: export.span,
                    takes_everything: true,
                });
            }
            ModuleDeclaration::ExportDefaultDeclaration(export) => {
                self.facts.has_default_export = true;
                self.facts.exports.push(ExportedSymbol {
                    name: "default".to_owned(),
                    span: export.span,
                    kind: ExportKind::Default,
                });
                self.exported_depth += 1;
                walk::walk_export_default_declaration(self, export);
                self.exported_depth -= 1;
                return;
            }
            ModuleDeclaration::TSExportAssignment(_)
            | ModuleDeclaration::TSNamespaceExportDeclaration(_) => {}
        }
        walk::walk_module_declaration(self, declaration);
    }
}

pub fn collect(program: &Program<'_>, semantic: &Semantic<'_>) -> ModuleFacts {
    let mut collector = Collector {
        semantic,
        facts: ModuleFacts::default(),
        exported_depth: 0,
        enclosing: Vec::new(),
        owner: Vec::new(),
    };
    collector.visit_program(program);
    collector.facts
}

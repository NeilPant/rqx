use std::collections::{BTreeMap, BTreeSet, HashMap};

use oxc::ast::ast::{
    BindingPattern, Class, Expression, FormalParameters, Function, IfStatement, Statement,
    SwitchStatement, TSType, TSTypeAliasDeclaration,
};
use oxc::ast_visit::{Visit, walk};
use oxc::span::Span;
use serde::{Deserialize, Serialize};

use crate::span_io;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Clump {
    pub names: Vec<String>,
    pub sites: u32,
    #[serde(with = "span_io")]
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Discriminant {
    pub name: String,
    pub sites: u32,
    pub branches: u32,
    #[serde(with = "span_io")]
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Mention {
    pub name: String,
    #[serde(with = "span_io")]
    pub span: Span,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Patterns {
    pub clumps: Vec<Clump>,
    pub discriminants: Vec<Discriminant>,
    pub derived_references: Vec<Mention>,
    pub encoded_names: Vec<Mention>,
    pub type_codes: Vec<Mention>,
    /// Pairs of classes in this file that each reach for the other by name.
    pub intimate_pairs: Vec<Mention>,
}

pub struct PatternLimits {
    pub clump_size: u32,
    pub clump_sites: u32,
}

/// `strName`, `bEnabled`, `IOrder`: a type spelled into a name the compiler already
/// knows the type of.
fn encoding_of(name: &str) -> Option<&'static str> {
    const PREFIXES: [(&str, &str); 6] = [
        ("str", "string"),
        ("num", "number"),
        ("bln", "boolean"),
        ("arr", "array"),
        ("obj", "object"),
        ("fn", "function"),
    ];
    let mut characters = name.chars();
    let first = characters.next()?;
    let second = characters.next()?;
    // `IOrder` but not `Iterator`: a capital directly after the I is the convention.
    if first == 'I' && second.is_ascii_uppercase() {
        return Some("interface I-prefix");
    }
    if first == 'b' && second.is_ascii_uppercase() {
        return Some("boolean b-prefix");
    }
    PREFIXES.iter().find_map(|(prefix, label)| {
        let rest = name.strip_prefix(prefix)?;
        rest.chars().next().filter(char::is_ascii_uppercase).map(|_| *label)
    })
}

fn parameter_names(params: &FormalParameters<'_>) -> Vec<String> {
    params
        .items
        .iter()
        .filter_map(|parameter| match &parameter.pattern {
            BindingPattern::BindingIdentifier(binding) => Some(binding.name.to_string()),
            _ => None,
        })
        .collect()
}

fn discriminant_text(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::Identifier(identifier) => Some(identifier.name.to_string()),
        Expression::StaticMemberExpression(member) => {
            Some(format!("{}.{}", discriminant_text(&member.object)?, member.property.name))
        }
        Expression::ThisExpression(_) => Some("this".to_owned()),
        _ => None,
    }
}

/// The subject of an `if (x === "a") … else if (x === "b")` chain, which is a switch
/// wearing a different syntax.
fn comparison_subject(expression: &Expression<'_>) -> Option<String> {
    let Expression::BinaryExpression(binary) = expression else {
        return None;
    };
    use oxc::syntax::operator::BinaryOperator;
    if !matches!(binary.operator, BinaryOperator::StrictEquality | BinaryOperator::Equality) {
        return None;
    }
    matches!(binary.right, Expression::StringLiteral(_) | Expression::NumericLiteral(_))
        .then(|| discriminant_text(&binary.left))
        .flatten()
}

#[derive(Default)]
struct Collector {
    signatures: Vec<(Vec<String>, Span)>,
    switches: HashMap<String, (u32, u32, Span)>,
    class_stack: Vec<String>,
    subclasses: HashMap<String, String>,
    references: Vec<(String, String, Span)>,
    out: Patterns,
}

impl Collector {
    fn note_signature(&mut self, params: &FormalParameters<'_>, span: Span) {
        let names = parameter_names(params);
        if names.len() >= 2 {
            self.signatures.push((names, span));
        }
    }

    fn note_chain(&mut self, statement: &IfStatement<'_>) {
        let Some(subject) = comparison_subject(&statement.test) else {
            return;
        };
        let mut branches = 1;
        let mut alternate = statement.alternate.as_ref();
        while let Some(Statement::IfStatement(inner)) = alternate {
            if comparison_subject(&inner.test).as_deref() != Some(subject.as_str()) {
                break;
            }
            branches += 1;
            alternate = inner.alternate.as_ref();
        }
        if branches >= 2 {
            let entry = self.switches.entry(subject).or_insert((0, 0, statement.span));
            entry.0 += 1;
            entry.1 = entry.1.max(branches);
        }
    }
}

impl<'a> Visit<'a> for Collector {
    fn visit_function(&mut self, function: &Function<'a>, flags: oxc::syntax::scope::ScopeFlags) {
        self.note_signature(&function.params, function.span);
        walk::walk_function(self, function, flags);
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        let name = class.id.as_ref().map(|id| id.name.to_string());
        if let (Some(name), Some(heritage)) = (name.clone(), class.heritage.as_ref())
            && let Some(parent) = discriminant_text(&heritage.expression)
        {
            self.subclasses.insert(name, parent);
        }
        self.class_stack.push(name.unwrap_or_default());
        walk::walk_class(self, class);
        self.class_stack.pop();
    }

    fn visit_identifier_reference(&mut self, identifier: &oxc::ast::ast::IdentifierReference<'a>) {
        if let Some(owner) = self.class_stack.last()
            && !owner.is_empty()
        {
            self.references.push((owner.clone(), identifier.name.to_string(), identifier.span));
        }
    }

    fn visit_binding_identifier(&mut self, binding: &oxc::ast::ast::BindingIdentifier<'a>) {
        if let Some(encoding) = encoding_of(&binding.name) {
            self.out.encoded_names.push(Mention {
                name: format!("{} ({encoding})", binding.name),
                span: binding.span,
            });
        }
    }

    fn visit_ts_property_signature(&mut self, property: &oxc::ast::ast::TSPropertySignature<'a>) {
        if let Some(name) = property.key.static_name()
            && let Some(encoding) = encoding_of(&name)
        {
            self.out.encoded_names.push(Mention {
                name: format!("{name} ({encoding})"),
                span: property.span,
            });
        }
        walk::walk_ts_property_signature(self, property);
    }

    fn visit_switch_statement(&mut self, statement: &SwitchStatement<'a>) {
        if let Some(subject) = discriminant_text(&statement.discriminant) {
            let entry = self.switches.entry(subject).or_insert((0, 0, statement.span));
            entry.0 += 1;
            entry.1 = entry.1.max(statement.cases.len() as u32);
        }
        walk::walk_switch_statement(self, statement);
    }

    fn visit_if_statement(&mut self, statement: &IfStatement<'a>) {
        self.note_chain(statement);
        walk::walk_if_statement(self, statement);
    }

    /// A union of three or more string literals is a type code: a set of named cases
    /// with no type of its own to hang behaviour on.
    fn visit_ts_type_alias_declaration(&mut self, alias: &TSTypeAliasDeclaration<'a>) {
        if let TSType::TSUnionType(union) = &alias.type_annotation {
            let literals = union
                .types
                .iter()
                .filter(|member| {
                    matches!(member, TSType::TSLiteralType(literal)
                        if matches!(literal.literal, oxc::ast::ast::TSLiteral::StringLiteral(_)))
                })
                .count();
            if literals >= 3 && literals == union.types.len() {
                self.out.type_codes.push(Mention {
                    name: alias.id.name.to_string(),
                    span: alias.id.span,
                });
            }
        }
        walk::walk_ts_type_alias_declaration(self, alias);
    }
}

pub fn collect(program: &oxc::ast::ast::Program<'_>, limits: &PatternLimits) -> Patterns {
    let mut collector = Collector::default();
    collector.visit_program(program);

    let mut out = std::mem::take(&mut collector.out);

    // Data clumps: the same set of names travelling together through several
    // signatures is a concept the code has not named.
    let mut groups: BTreeMap<Vec<String>, (u32, Span)> = BTreeMap::new();
    for (names, span) in &collector.signatures {
        if names.len() < limits.clump_size as usize {
            continue;
        }
        let mut sorted: Vec<String> = names.clone();
        sorted.sort();
        sorted.dedup();
        let entry = groups.entry(sorted).or_insert((0, *span));
        entry.0 += 1;
    }
    for (names, (sites, span)) in groups {
        if sites >= limits.clump_sites {
            out.clumps.push(Clump { names, sites, span });
        }
    }

    for (name, (sites, branches, span)) in collector.switches {
        if sites >= 2 && branches >= 3 {
            out.discriminants.push(Discriminant { name, sites, branches, span });
        }
    }

    // A base class that names one of its own subclasses cannot be understood alone.
    let subclasses: BTreeSet<(&str, &str)> = collector
        .subclasses
        .iter()
        .map(|(child, parent)| (parent.as_str(), child.as_str()))
        .collect();
    for (owner, referenced, span) in &collector.references {
        if subclasses.contains(&(owner.as_str(), referenced.as_str())) {
            out.derived_references
                .push(Mention { name: referenced.clone(), span: *span });
        }
    }

    // Two classes that each name the other are coupled in both directions, which is
    // the shape Inappropriate Intimacy describes: neither can be read, tested or
    // moved without the other coming with it.
    let mut reaches: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut first_site: BTreeMap<(&str, &str), Span> = BTreeMap::new();
    for (owner, referenced, span) in &collector.references {
        if owner == referenced {
            continue;
        }
        reaches.entry(owner).or_default().insert(referenced);
        first_site.entry((owner.as_str(), referenced.as_str())).or_insert(*span);
    }
    // A base and its own subclass naming each other is inheritance, not intimacy, and
    // `base-depends-on-derived` already has an opinion about it.
    let related = |left: &str, right: &str| {
        collector.subclasses.get(left).is_some_and(|parent| parent == right)
            || collector.subclasses.get(right).is_some_and(|parent| parent == left)
    };
    let mut pairs: BTreeSet<(&str, &str)> = BTreeSet::new();
    for (owner, targets) in &reaches {
        for target in targets {
            if reaches.get(*target).is_some_and(|back| back.contains(owner))
                && !related(owner, target)
            {
                pairs.insert(if owner < target { (owner, target) } else { (target, owner) });
            }
        }
    }
    for (left, right) in pairs {
        if let Some(span) = first_site.get(&(left, right)).or_else(|| first_site.get(&(right, left))) {
            out.intimate_pairs.push(Mention { name: format!("{left} and {right}"), span: *span });
        }
    }
    out.intimate_pairs.sort_by_key(|item| item.span.start);

    out.clumps.sort_by_key(|clump| clump.span.start);
    out.discriminants.sort_by_key(|item| item.span.start);
    out.derived_references.sort_by_key(|item| item.span.start);
    out.encoded_names.sort_by_key(|item| item.span.start);
    out.type_codes.sort_by_key(|item| item.span.start);
    out
}

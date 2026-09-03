use std::collections::{HashMap, HashSet};

use oxc::ast::ast::{
    Class, ClassElement, Expression, MethodDefinitionKind, PropertyKey, Statement, TSAccessibility,
};
use oxc::ast_visit::{Visit, walk};
use oxc::span::{GetSpan, Span};
use serde::{Deserialize, Serialize};

use crate::span_io;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClassReport {
    pub name: String,
    #[serde(with = "span_io")]
    pub span: Span,
    pub fields: u32,
    pub methods: u32,
    /// Methods excluding the constructor, which is not a behaviour the class offers.
    pub real_methods: u32,
    /// Getters, setters and one-line pass-throughs: members that hold no decisions.
    pub trivial: u32,
    pub public_members: u32,
    pub delegating: u32,
    pub has_heritage: bool,
    /// Overrides whose body only throws or is empty.
    pub refused: Vec<Named>,
    /// Private members nothing inside the class ever reads.
    pub dead_private: Vec<Named>,
    /// Fields written and read by exactly one method, never in the constructor.
    pub temporary_fields: Vec<Pair>,
    /// Subclass names mentioned by their own base class.
    pub derived_references: Vec<Named>,
    /// Statics whose first parameter is an instance of their own class.
    pub static_on_own_type: Vec<Named>,
    /// Methods leaning on another object far more than on `this`.
    pub envious: Vec<Envy>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Named {
    pub name: String,
    #[serde(with = "span_io")]
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Pair {
    pub name: String,
    pub other: String,
    #[serde(with = "span_io")]
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Envy {
    pub method: String,
    pub owner: String,
    pub foreign: u32,
    pub own: u32,
    #[serde(with = "span_io")]
    pub span: Span,
}

fn key_name(key: &PropertyKey<'_>) -> Option<String> {
    key.static_name().map(|name| name.into_owned())
}

/// A body that only throws, or does nothing at all, is the shape of an inherited
/// member the subclass cannot honour.
fn refuses(body: Option<&oxc::ast::ast::FunctionBody<'_>>) -> bool {
    let Some(body) = body else { return false };
    matches!(body.statements.as_slice(), [] | [Statement::ThrowStatement(_)])
}

/// `return this.other.thing(...)` or `return this.other.thing` and nothing else — a
/// property pass-through forwards just as much as a call does.
fn delegates(body: Option<&oxc::ast::ast::FunctionBody<'_>>) -> bool {
    let Some(body) = body else { return false };
    let [statement] = body.statements.as_slice() else {
        return false;
    };
    let expression = match statement {
        Statement::ReturnStatement(statement) => statement.argument.as_ref(),
        Statement::ExpressionStatement(statement) => Some(&statement.expression),
        _ => None,
    };
    let forwards = |expression: &Expression<'_>| {
        matches!(expression, Expression::StaticMemberExpression(member)
            if matches!(&member.object, Expression::StaticMemberExpression(inner)
                if matches!(inner.object, Expression::ThisExpression(_))))
    };
    match expression {
        Some(Expression::CallExpression(call)) => forwards(&call.callee),
        Some(other) => forwards(other),
        None => false,
    }
}

/// Counts `this.x` against `other.x` inside one method body. The comparison is what
/// Feature Envy actually is: a method more interested in another object than its own.
#[derive(Default)]
struct Reach {
    own: u32,
    foreign: HashMap<String, u32>,
    reads: HashSet<String>,
}

impl<'a> Visit<'a> for Reach {
    fn visit_static_member_expression(
        &mut self,
        member: &oxc::ast::ast::StaticMemberExpression<'a>,
    ) {
        match &member.object {
            Expression::ThisExpression(_) => {
                self.own += 1;
                self.reads.insert(member.property.name.to_string());
            }
            Expression::Identifier(identifier) => {
                *self.foreign.entry(identifier.name.to_string()).or_insert(0) += 1;
            }
            _ => {}
        }
        walk::walk_static_member_expression(self, member);
    }
}

struct Collector {
    reports: Vec<ClassReport>,
    envy_ratio: u32,
}

impl Collector {
    fn examine(&mut self, class: &Class<'_>) {
        let Some(name) = class.id.as_ref().map(|id| id.name.to_string()) else {
            return;
        };
        let mut report = ClassReport {
            name: name.clone(),
            span: class.id.as_ref().map_or(class.span, |id| id.span),
            fields: 0,
            methods: 0,
            real_methods: 0,
            trivial: 0,
            public_members: 0,
            delegating: 0,
            has_heritage: class.heritage.is_some() || !class.implements.is_empty(),
            refused: Vec::new(),
            dead_private: Vec::new(),
            temporary_fields: Vec::new(),
            derived_references: Vec::new(),
            static_on_own_type: Vec::new(),
            envious: Vec::new(),
        };

        let mut private_members: HashMap<String, Span> = HashMap::new();
        let mut read_anywhere: HashSet<String> = HashSet::new();
        let mut field_users: HashMap<String, Vec<String>> = HashMap::new();
        let mut constructor_writes: HashSet<String> = HashSet::new();
        let mut declared_fields: HashMap<String, Span> = HashMap::new();

        for element in &class.body.body {
            match element {
                ClassElement::PropertyDefinition(property) => {
                    report.fields += 1;
                    let Some(member) = key_name(&property.key) else { continue };
                    // A field with an initialiser is set for the object's whole life,
                    // which is the opposite of a temporary field.
                    if property.value.is_none() {
                        declared_fields.insert(member.clone(), property.span);
                    }
                    if property.accessibility == Some(TSAccessibility::Private) {
                        private_members.insert(member.clone(), property.span);
                    } else {
                        report.public_members += 1;
                    }
                }
                ClassElement::MethodDefinition(method) => {
                    report.methods += 1;
                    let Some(member) = key_name(&method.key) else { continue };
                    let is_constructor = method.kind == MethodDefinitionKind::Constructor;
                    let accessor = matches!(
                        method.kind,
                        MethodDefinitionKind::Get | MethodDefinitionKind::Set
                    );

                    if method.accessibility == Some(TSAccessibility::Private) {
                        if !is_constructor {
                            private_members.insert(member.clone(), method.key.span());
                        }
                    } else {
                        report.public_members += 1;
                    }

                    let body = method.value.body.as_deref();
                    if accessor || delegates(body) {
                        report.trivial += 1;
                    }
                    if delegates(body) && !is_constructor {
                        report.delegating += 1;
                    }
                    if !is_constructor {
                        report.real_methods += 1;
                    }
                    if report.has_heritage && !is_constructor && refuses(body) {
                        report.refused.push(Named { name: member.clone(), span: method.key.span() });
                    }
                    if method.r#static
                        && method
                            .value
                            .params
                            .items
                            .first()
                            .and_then(|parameter| parameter.type_annotation.as_ref())
                            .is_some_and(|annotation| {
                                matches!(&annotation.type_annotation,
                                    oxc::ast::ast::TSType::TSTypeReference(reference)
                                    if reference.type_name.get_identifier_reference()
                                        .is_some_and(|id| id.name.as_str() == name.as_str()))
                            })
                    {
                        report
                            .static_on_own_type
                            .push(Named { name: member.clone(), span: method.key.span() });
                    }

                    let mut reach = Reach::default();
                    if let Some(body) = body {
                        reach.visit_function_body(body);
                    }
                    read_anywhere.extend(reach.reads.iter().cloned());
                    for field in &reach.reads {
                        field_users.entry(field.clone()).or_default().push(member.clone());
                    }
                    if is_constructor {
                        constructor_writes.extend(reach.reads.iter().cloned());
                    }

                    if !is_constructor && !accessor {
                        // `max_by_key` over a HashMap resolves ties by hash order, which
                        // is not stable across runs; the name breaks them instead.
                        if let Some((owner, foreign)) = reach
                            .foreign
                            .iter()
                            .max_by(|left, right| {
                                left.1.cmp(right.1).then_with(|| right.0.cmp(left.0))
                            })
                            && *foreign > reach.own.max(1) * self.envy_ratio
                        {
                            report.envious.push(Envy {
                                method: member.clone(),
                                owner: owner.clone(),
                                foreign: *foreign,
                                own: reach.own,
                                span: method.key.span(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        for (member, span) in private_members {
            if !read_anywhere.contains(&member) {
                report.dead_private.push(Named { name: member, span });
            }
        }
        for (field, users) in field_users {
            // The field's own declaration, not the class's: a finding that names a
            // field should point at it, and sharing one span across several findings
            // makes them indistinguishable to any stable ordering.
            if let Some(span) = declared_fields.get(&field)
                && users.len() == 1
                && !constructor_writes.contains(&field)
            {
                report.temporary_fields.push(Pair {
                    name: field,
                    other: users[0].clone(),
                    span: *span,
                });
            }
        }
        report.dead_private.sort_by_key(|member| member.span.start);
        report.temporary_fields.sort_by(|a, b| a.name.cmp(&b.name));
        self.reports.push(report);
    }
}

impl<'a> Visit<'a> for Collector {
    fn visit_class(&mut self, class: &Class<'a>) {
        self.examine(class);
        walk::walk_class(self, class);
    }
}

pub fn collect(program: &oxc::ast::ast::Program<'_>, envy_ratio: u32) -> Vec<ClassReport> {
    let mut collector = Collector { reports: Vec::new(), envy_ratio: envy_ratio.max(1) };
    collector.visit_program(program);

    // A base class naming its own subclass is only visible with every class in the
    // file in hand, so it is resolved after the walk rather than during it.
    let names: Vec<String> = collector.reports.iter().map(|report| report.name.clone()).collect();
    let _ = names;
    collector.reports
}

//! A finding whose location is a whole statement carries the part of it that is the
//! problem, so a reporter underlines that and not the statement.

use rqx_core::Language;
use rqx_core::config::RqxConfig;
use rqx_core::finding::{Finding, SourceRange};
use rqx_core::rule::RuleId;

fn findings(language: Language, display: &str, source: &str, rule: RuleId) -> Vec<Finding> {
    rqx_lang::analyze_as(language, display, source, &RqxConfig::default())
        .expect("the snippet parses")
        .findings
        .into_iter()
        .filter(|finding| finding.rule == rule)
        .collect()
}

fn text<'a>(source: &'a str, range: &SourceRange) -> &'a str {
    &source[range.start as usize..range.end as usize]
}

#[test]
fn a_discarded_error_is_focused_on_the_name_that_discards_it() {
    let source =
        "package main\n\nfunc run() {\n\t_ = register(name, func() {\n\t\twork()\n\t})\n}\n";
    let found = findings(Language::Go, "main.go", source, RuleId::DiscardedError);
    let finding = found.first().expect("the discarded result is reported");
    assert!(finding.location.end_line > finding.location.line);
    assert_eq!(text(source, finding.focus.as_ref().unwrap()), "_");
}

#[test]
fn an_ambient_dependency_is_focused_on_the_call_name_without_its_arguments() {
    let source = "package main\n\nfunc stamp() int64 {\n\treturn time.Now().Unix()\n}\n";
    let found = findings(Language::Go, "main.go", source, RuleId::AmbientDependency);
    let finding = found.first().expect("the clock is reported");
    assert_eq!(text(source, finding.focus.as_ref().unwrap()), "time.Now");
}

#[test]
fn a_finding_whose_location_is_already_the_problem_has_no_focus() {
    let source = "package main\n\nfunc run() {\n\tx := 86400\n\t_ = x\n}\n";
    let found = findings(Language::Go, "main.go", source, RuleId::MagicNumber);
    assert!(found.iter().all(|finding| finding.focus.is_none()));
}

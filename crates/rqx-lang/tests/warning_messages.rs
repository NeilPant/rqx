//! What a warning says about the code it found. A finding that is right but described
//! wrongly costs as much trust as a wrong one.

use rqx_core::Language;
use rqx_core::config::RqxConfig;
use rqx_core::finding::{Evidence, Finding};
use rqx_core::rule::RuleId;

fn findings(language: Language, display: &str, source: &str, rule: RuleId) -> Vec<Finding> {
    rqx_lang::analyze_as(language, display, source, &RqxConfig::default())
        .expect("the snippet parses")
        .findings
        .into_iter()
        .filter(|finding| finding.rule == rule)
        .collect()
}

fn shape(finding: &Finding) -> &str {
    match &finding.evidence {
        Evidence::BranchShape { shape } => shape,
        other => panic!("not a branch shape: {other:?}"),
    }
}

#[test]
fn a_redundant_else_names_how_the_branch_above_leaves() {
    let cases = [
        (
            "func f(rows []int) {\n\tfor _, row := range rows {\n\t\tif row > 0 {\n\t\t\tbreak\n\t\t} else {\n\t\t\tuse(row)\n\t\t}\n\t}\n}\n",
            "breaks",
        ),
        (
            "func f(rows []int) {\n\tfor _, row := range rows {\n\t\tif row > 0 {\n\t\t\tcontinue\n\t\t} else {\n\t\t\tuse(row)\n\t\t}\n\t}\n}\n",
            "continues",
        ),
        (
            "func f(row int) int {\n\tif row > 0 {\n\t\treturn 1\n\t} else {\n\t\tuse(row)\n\t}\n\treturn 0\n}\n",
            "returns",
        ),
    ];
    for (body, exit) in cases {
        let source = format!("package p\n\n{body}");
        let found = findings(Language::Go, "p.go", &source, RuleId::RedundantElse);
        let finding = found.first().expect("the else is reported");
        assert_eq!(
            shape(finding),
            format!("The branch above always {exit}, so this `else` is not a decision."),
            "{source}"
        );
    }
}

#[test]
fn a_discarded_last_result_is_not_called_an_error() {
    let source = "package p\n\nfunc f() {\n\tsv, _ := measurement.Scale(1, \"ms\")\n\tuse(sv)\n}\n";
    let found = findings(Language::Go, "p.go", source, RuleId::DiscardedError);
    let finding = found.first().expect("the discarded result is reported");
    match &finding.evidence {
        Evidence::Name { reason, .. } => assert_eq!(
            reason,
            "returns 2 results, and the line throws the last one away"
        ),
        other => panic!("not a name: {other:?}"),
    }
}

fn unused_parameter_names(language: Language, display: &str, source: &str) -> Vec<String> {
    findings(language, display, source, RuleId::UnusedParameter)
        .iter()
        .filter_map(|finding| match &finding.evidence {
            Evidence::UnusedParameter { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_unused_cpp_reference_parameter_is_named_and_a_used_one_is_not_reported() {
    let source = "template <typename T, typename U>\nbool equal(const T& lhs, const U& rhs, int* spare) {\n    return lhs == rhs;\n}\n";
    assert_eq!(
        unused_parameter_names(Language::Cpp, "a.cpp", source),
        ["spare"]
    );
    let unnamed = "bool always(const std::string&) {\n    return true;\n}\n";
    assert!(unused_parameter_names(Language::Cpp, "a.cpp", unnamed).is_empty());
}

#[test]
fn a_go_parameter_reassigned_by_a_short_declaration_is_used() {
    let source = "package p\n\nfunc compress(in []byte, s *Scratch) ([]byte, error) {\n\ts, err := s.prepare(in)\n\tif err != nil {\n\t\treturn nil, err\n\t}\n\treturn s.out, nil\n}\n";
    assert!(unused_parameter_names(Language::Go, "p.go", source).is_empty());
}

/// A file built for one platform keeps the signature its siblings for the others
/// declare, whether a comment or its name says which platform it is.
#[test]
fn a_go_platform_file_keeps_its_signatures() {
    let source = "package cpu\n\nfunc counts(logical bool) int {\n\treturn 1\n}\n";
    assert!(unused_parameter_names(Language::Go, "cpu_freebsd.go", source).is_empty());
    let constrained = format!("//go:build linux\n\n{source}");
    assert!(unused_parameter_names(Language::Go, "cpu.go", &constrained).is_empty());
    assert_eq!(
        unused_parameter_names(Language::Go, "cpu.go", source),
        ["logical"]
    );
}

#[test]
fn a_dead_store_points_at_the_name_written() {
    let source =
        "package p\n\nfunc f() {\n\tlocal := compute()\n\tlocal = 2\n\tprintln(local)\n}\n";
    let found = findings(Language::Go, "p.go", source, RuleId::DeadStore);
    let finding = found.first().expect("the first write is dead");
    let at = &finding.location;
    assert_eq!(&source[at.start as usize..at.end as usize], "local");
    assert_eq!(at.line, 4);
}

#[test]
fn a_cpp_handler_that_names_a_type_and_no_binding_takes_only_that_type() {
    let typed = "int f() {\n    try {\n        return g();\n    } catch (int) {\n        return -1;\n    }\n}\n";
    assert!(findings(Language::Cpp, "a.cpp", typed, RuleId::CatchAllHandler).is_empty());
    let everything = "int f() {\n    try {\n        return g();\n    } catch (...) {\n        return -1;\n    }\n}\n";
    assert_eq!(
        findings(Language::Cpp, "a.cpp", everything, RuleId::CatchAllHandler).len(),
        1
    );
}

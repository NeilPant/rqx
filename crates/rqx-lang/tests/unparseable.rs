//! A file the grammar cannot read is refused with that reason rather than graded on
//! the little that parsed: graded, it would come out pristine.

use rqx_core::Language;
use rqx_core::config::RqxConfig;

fn refusal(language: Language, display: &str, source: &str) -> Option<String> {
    rqx_lang::analyze_as(language, display, source, &RqxConfig::default())
        .err()
        .filter(rqx_lang::AnalysisError::unparseable)
        .map(|error| error.to_string())
}

#[test]
fn a_file_of_syntax_errors_is_refused_in_every_language() {
    for (language, display, source) in [
        (Language::TypeScript, "a.ts", "function (( {{{{ \n"),
        (Language::JavaScript, "a.js", "function (( {{{{ \n"),
        (Language::Python, "a.py", "def (:\n  x = \n"),
        (Language::Go, "a.go", "package\nfunc {{\n"),
        (Language::Java, "A.java", "class { \n"),
        (Language::Cpp, "a.cpp", "int main( {\n"),
    ] {
        let refused = refusal(language, display, source)
            .unwrap_or_else(|| panic!("{language:?}: {source:?} was analysed"));
        assert!(refused.contains("not parseable"), "{refused}");
    }
}

#[test]
fn a_file_with_merge_conflict_markers_is_refused() {
    let source = "export function f(a: number) {\n<<<<<<< HEAD\n  return a + 1;\n=======\n  \
                  return a + 2;\n>>>>>>> other\n}\n";
    let refused = refusal(Language::TypeScript, "a.ts", source).expect("refused");
    assert!(refused.contains("merge conflict"), "{refused}");
}

#[test]
fn a_file_with_one_syntax_error_is_still_analysed() {
    let source = "export function f(a: number) {\n  try { run(a); } catch (e) {}\n}\n\
                  export function g(a: number) {\n  return a +;\n}\n";
    let analysis =
        rqx_lang::analyze_as(Language::TypeScript, "a.ts", source, &RqxConfig::default())
            .expect("a file that mostly parses is analysed");
    assert!(
        analysis
            .findings
            .iter()
            .any(|finding| finding.rule == rqx_core::rule::RuleId::EmptyCatch),
        "the readable part was not analysed"
    );
}

#[test]
fn conflict_markers_in_a_file_that_parses_are_left_alone() {
    let source =
        "export const sample = `\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> other\n`;\n";
    assert!(
        rqx_lang::analyze_as(Language::TypeScript, "a.ts", source, &RqxConfig::default()).is_ok()
    );
}

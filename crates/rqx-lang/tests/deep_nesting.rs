//! Generated files nested far deeper than anyone writes by hand, in every language. A
//! file under the nesting limit is analysed in bounded time, and one over it is
//! refused with that reason; neither may take the process down. The test threads have
//! the 2 MB stack a scan's worker threads have, which every one of these overflowed
//! before the analysis kept its own stack.

use std::time::{Duration, Instant};

use rqx_core::Language;
use rqx_core::config::RqxConfig;

/// Far more than a release build needs, which is under a second for every one of
/// these: a debug build is many times slower, and a chain that cost the cube of its
/// length took minutes.
const BUDGET: Duration = Duration::from_secs(20);

/// Units of nesting for a file that stays under the limit, and for one that does not.
const UNDER: usize = 600;
const OVER: usize = 2_000;

#[derive(Clone, Copy, Debug)]
enum Shape {
    ElseIfChain,
    NestedBlocks,
    Concatenation,
    NestedCalls,
    MethodChain,
}

const SHAPES: [Shape; 5] = [
    Shape::ElseIfChain,
    Shape::NestedBlocks,
    Shape::Concatenation,
    Shape::NestedCalls,
    Shape::MethodChain,
];

const LANGUAGES: [(Language, &str); 6] = [
    (Language::TypeScript, "deep.ts"),
    (Language::JavaScript, "deep.js"),
    (Language::Python, "deep.py"),
    (Language::Go, "deep.go"),
    (Language::Java, "Deep.java"),
    (Language::Cpp, "deep.cpp"),
];

fn generated(language: Language, shape: Shape, units: usize) -> String {
    let python = language == Language::Python;
    let go = language == Language::Go;
    let body = match shape {
        Shape::ElseIfChain if python => {
            let mut body = String::from("if x == 0:\n    return 0\n");
            for unit in 1..units {
                body.push_str(&format!("elif x == {unit}:\n    return {unit}\n"));
            }
            body
        }
        Shape::ElseIfChain if go => {
            let mut body = String::from("if x == 0 {\nreturn 0\n}");
            for unit in 1..units {
                body.push_str(&format!(" else if x == {unit} {{\nreturn {unit}\n}}"));
            }
            body
        }
        Shape::ElseIfChain => {
            let mut body = String::from("if (x == 0) { return 0; }\n");
            for unit in 1..units {
                body.push_str(&format!("else if (x == {unit}) {{ return {unit}; }}\n"));
            }
            body
        }
        Shape::NestedBlocks if python => {
            let mut body = String::new();
            for level in 0..units {
                body.push_str(&format!("{}if x:\n", " ".repeat(level)));
            }
            body.push_str(&format!("{}return 1", " ".repeat(units)));
            body
        }
        Shape::NestedBlocks if go => {
            format!(
                "{}x = 1\n{}",
                "if x > 0 {\n".repeat(units),
                "}\n".repeat(units)
            )
        }
        Shape::NestedBlocks => {
            format!(
                "{}x = 1;\n{}",
                "if (x) {\n".repeat(units),
                "}\n".repeat(units)
            )
        }
        Shape::Concatenation => declaration(language, &vec!["1"; units].join(" + ")),
        Shape::NestedCalls => declaration(
            language,
            &format!("{}1{}", "g(".repeat(units), ")".repeat(units)),
        ),
        Shape::MethodChain if go => declaration(language, &format!("x{}", ".\nb()".repeat(units))),
        Shape::MethodChain if python => {
            declaration(language, &format!("(x{})", "\n.b()".repeat(units)))
        }
        Shape::MethodChain => declaration(language, &format!("x{}", "\n.b()".repeat(units))),
    };
    function_around(language, &body)
}

fn declaration(language: Language, value: &str) -> String {
    match language {
        Language::Python => format!("y = {value}\nreturn y"),
        Language::Go => format!("y := {value}\n_ = y"),
        Language::Cpp => format!("auto y = {value};"),
        Language::Java => format!("var y = {value};"),
        _ => format!("const y = {value};"),
    }
}

fn function_around(language: Language, body: &str) -> String {
    match language {
        Language::Python => {
            let indented: Vec<String> = body.lines().map(|line| format!("    {line}")).collect();
            format!("def f(x):\n{}\n", indented.join("\n"))
        }
        Language::Go => format!("package p\n\nfunc f(x int) int {{\n{body}\nreturn 0\n}}\n"),
        Language::Java => {
            format!("class Deep {{\n  int f(int x) {{\n{body}\nreturn 0;\n  }}\n}}\n")
        }
        Language::Cpp => format!("int f(int x) {{\n{body}\nreturn 0;\n}}\n"),
        _ => format!("export function f(x) {{\n{body}\n}}\n"),
    }
}

fn analysed(
    language: Language,
    display: &str,
    source: &str,
) -> (
    Result<rqx_lang::FileAnalysis, rqx_lang::AnalysisError>,
    Duration,
) {
    let started = Instant::now();
    let analysis = rqx_lang::analyze_as(language, display, source, &RqxConfig::default());
    (analysis, started.elapsed())
}

#[test]
fn a_file_under_the_nesting_limit_is_analysed_in_bounded_time() {
    for (language, display) in LANGUAGES {
        for shape in SHAPES {
            let source = generated(language, shape, UNDER);
            let (analysis, took) = analysed(language, display, &source);
            // Python's grammar gives up on indentation this deep and returns a tree of
            // errors, which is refused as unparseable rather than analysed.
            if let Err(error) = &analysis {
                assert!(
                    error.too_deep() || error.unparseable(),
                    "{language:?} {shape:?}: {error} rather than an analysis"
                );
            }
            assert!(
                took < BUDGET,
                "{language:?} {shape:?} took {took:?} at {UNDER} units"
            );
        }
    }
}

#[test]
fn a_long_else_if_chain_is_analysed_rather_than_refused() {
    for (language, display) in LANGUAGES {
        let source = generated(language, Shape::ElseIfChain, UNDER);
        let (analysis, _) = analysed(language, display, &source);
        assert!(
            analysis.is_ok(),
            "{language:?}: {}",
            analysis
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default()
        );
    }
}

#[test]
fn a_file_over_the_nesting_limit_is_refused_with_that_reason() {
    for (language, display) in LANGUAGES {
        for shape in [Shape::NestedBlocks, Shape::NestedCalls, Shape::MethodChain] {
            // Python's grammar gives up on indentation this deep and recovers a flat
            // tree, which is analysed like any other file with an error in it.
            if language == Language::Python && matches!(shape, Shape::NestedBlocks) {
                continue;
            }
            let source = generated(language, shape, OVER);
            let (analysis, took) = analysed(language, display, &source);
            let error = analysis
                .err()
                .unwrap_or_else(|| panic!("{language:?} {shape:?} at {OVER} units was analysed"));
            assert!(error.too_deep(), "{language:?} {shape:?}: {error}");
            assert!(
                error.to_string().contains("levels deep"),
                "{language:?} {shape:?}: {error}"
            );
            assert!(took < BUDGET, "{language:?} {shape:?} took {took:?}");
        }
    }
}

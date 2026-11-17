//! Plain JavaScript is read as TypeScript with the type syntax taken out: the same
//! node kinds, the same module surface and the same import resolution, under the
//! one grammar that parses both a `.js` file and the JSX inside it.

pub mod profile;

pub use profile::JAVASCRIPT;

#[cfg(test)]
mod tests {
    use rqx_core::Language;
    use rqx_core::config::RqxConfig;
    use rqx_core::rule::RuleId;

    use crate::{FileAnalysis, analyze};

    fn analysed(display: &str, source: &str) -> FileAnalysis {
        analyze(
            std::path::Path::new(display),
            display,
            source,
            &RqxConfig::default(),
        )
        .expect("a script that parses")
    }

    #[test]
    fn a_script_is_read_as_javascript() {
        let analysis = analysed("a.js", "export const a = 1\n");
        assert_eq!(analysis.language, Language::JavaScript);
    }

    #[test]
    fn jsx_parses_in_every_script_extension() {
        let source = "export const view = () => <div className=\"a\">hi</div>\n";
        for display in ["a.js", "a.jsx", "a.mjs", "a.cjs"] {
            let analysis = analysed(display, source);
            assert_eq!(analysis.language, Language::JavaScript, "{display}");
        }
    }

    #[test]
    fn a_script_importing_react_for_its_markup_has_not_left_it_unused() {
        let source = "import React from 'react'\nexport const view = () => <div>hi</div>\n";
        let analysis = analysed("view.js", source);
        assert!(
            analysis
                .findings
                .iter()
                .all(|finding| finding.rule != RuleId::UnusedImport),
            "{:?}",
            analysis.findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_same_unreachable_code_is_found_as_in_typescript() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n";
        for display in ["a.js", "a.ts"] {
            let analysis = analysed(display, source);
            assert!(
                analysis
                    .findings
                    .iter()
                    .any(|finding| finding.rule == RuleId::UnreachableCode),
                "{display}"
            );
        }
    }
}

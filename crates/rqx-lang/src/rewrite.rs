//! Turns a finding into the edit that resolves it, for the rules whose fix is
//! decided by the finding alone. A rule that is not here has no mechanical answer,
//! and `edit` says so by returning `None` rather than guessing at one.

use std::collections::BTreeSet;
use std::fmt;

use rqx_core::edit::{Edit, Resolution, apply, resolve};
use rqx_core::finding::Finding;
use rqx_core::rule::{Autofix, RuleId};
use rqx_core::span::Span;
use rqx_core::{Identity, Language, RqxConfig};
use tree_sitter::{Node, Tree};

use crate::profile::LanguageProfile;
use crate::{AnalysisError, support};

/// One parse of one file, reused across every finding in it. Parsing per finding
/// would re-read a large file once per fix, and the tree is the same either way.
pub struct Rewriter<'src> {
    language: Language,
    source: &'src str,
    profile: &'static LanguageProfile,
    tree: Tree,
}

/// Why a file's edits were not written. Every variant is a reason the reader gets
/// verbatim, because a silent refusal reads as a clean bill of health.
#[derive(Clone, Debug)]
pub enum Withheld {
    /// An edit reached past the source or split a code point.
    OutOfRange,
    DoesNotParse(ParseFault),
    Unanalysable(String),
    /// A finding the plan set out to remove is still reported after the edit.
    Survives(Vec<Finding>),
    /// A finding absent before the edit is reported after it.
    Introduces(Vec<Finding>),
}

impl fmt::Display for Withheld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange => f.write_str("an edit reached outside the file"),
            Self::DoesNotParse(fault) => write!(f, "the result does not parse: {fault}"),
            Self::Unanalysable(reason) => write!(f, "the result could not be analysed: {reason}"),
            Self::Survives(findings) => write!(
                f,
                "{} still reported after the edit",
                findings
                    .iter()
                    .map(|finding| finding.rule.slug())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Introduces(findings) => write!(
                f,
                "would introduce {}",
                findings
                    .iter()
                    .map(|finding| finding.rule.slug())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ParseFault {
    pub span: Span,
    pub missing: bool,
}

impl fmt::Display for ParseFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = if self.missing { "missing" } else { "error" };
        write!(
            f,
            "{what} node at bytes {}..{}",
            self.span.start, self.span.end
        )
    }
}

/// The edited file, proven: it parses, and re-analysis reports nothing it did not
/// before, minus what the edits set out to remove.
#[derive(Clone, Debug)]
pub struct Verified {
    pub source: String,
    pub findings: Vec<Finding>,
}

/// The first `ERROR` or `MISSING` node in a parse of `source`, if any. Tree-sitter
/// recovers from a bad edit by inventing one rather than failing, so a tree that
/// parsed is not yet a tree that parsed clean.
pub fn parses_clean(language: Language, source: &str) -> Result<(), ParseFault> {
    let support = support(language);
    let tree = crate::parse(language, support.profile, source).map_err(|_| ParseFault {
        span: Span::new(0, source.len() as u32),
        missing: false,
    })?;
    let root = tree.root_node();
    if !root.has_error() {
        return Ok(());
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            return Err(ParseFault {
                span: Span::new(node.start_byte() as u32, node.end_byte() as u32),
                missing: node.is_missing(),
            });
        }
        if node.has_error() {
            stack.extend(node.children(&mut cursor));
        }
    }
    Ok(())
}

impl<'src> Rewriter<'src> {
    pub fn new(language: Language, source: &'src str) -> Result<Self, AnalysisError> {
        let support = support(language);
        let tree = crate::parse(language, support.profile, source)?;
        Ok(Self {
            language,
            source,
            profile: support.profile,
            tree,
        })
    }

    /// Applies the plan in memory and proves the result before anyone writes it.
    /// `display` and `config` must be the ones the original analysis used: the
    /// display string becomes every finding's file, and the config decides which
    /// stages run, so a different one makes the two finding sets incomparable.
    ///
    /// Rules the whole project decides are left out of the comparison on both
    /// sides; one file's re-analysis cannot see them, and the caller's project pass
    /// is what answers for those. A new finding that is itself mechanically fixable
    /// is tolerated rather than withheld, because deleting the last use of a binding
    /// makes the binding unused, and that is the next round's edit, not a regression.
    pub fn verify(
        &self,
        display: &str,
        config: &RqxConfig,
        before: &[Finding],
        resolution: &Resolution,
    ) -> Result<Verified, Withheld> {
        let source = apply(self.source, resolution).ok_or(Withheld::OutOfRange)?;
        parses_clean(self.language, &source).map_err(Withheld::DoesNotParse)?;
        let after = crate::analyze_as(self.language, display, &source, config)
            .map_err(|error| Withheld::Unanalysable(error.to_string()))?
            .findings;

        let targeted: BTreeSet<Identity> =
            resolution.kept.iter().map(|edit| edit.answers).collect();
        let known: BTreeSet<Identity> = before
            .iter()
            .filter(|finding| !finding.rule.is_project_scoped())
            .map(|finding| Identity::resolved(finding, self.source))
            .collect();

        let mut survives = Vec::new();
        let mut introduced = Vec::new();
        for finding in after
            .iter()
            .filter(|finding| !finding.rule.is_project_scoped())
        {
            let identity = Identity::resolved(finding, &source);
            if targeted.contains(&identity) {
                survives.push(finding.clone());
            } else if !known.contains(&identity) && !finding.rule.spec().autofix.is_automatic() {
                introduced.push(finding.clone());
            }
        }
        if !survives.is_empty() {
            return Err(Withheld::Survives(survives));
        }
        if !introduced.is_empty() {
            return Err(Withheld::Introduces(introduced));
        }
        Ok(Verified {
            source,
            findings: after,
        })
    }

    pub fn profile(&self) -> &'static LanguageProfile {
        self.profile
    }

    /// Whether the file rqx analysed is still the file being edited. A span whose
    /// first byte starts no node came from a stale scan, and applying it would edit
    /// bytes no rule ever looked at. Anchored on the start alone because a span may
    /// deliberately cover a run of siblings, which no single node spans.
    fn anchored(&self, span: Span) -> Option<Node<'_>> {
        let node = self
            .tree
            .root_node()
            .descendant_for_byte_range(span.start as usize, span.start as usize)?;
        (node.start_byte() as u32 == span.start).then_some(node)
    }

    pub fn edit(&self, finding: &Finding) -> Option<Edit> {
        if !finding.rule.spec().autofix.is_automatic() {
            return None;
        }
        let span = Span::new(finding.location.start, finding.location.end);
        self.anchored(span)?;

        match finding.rule {
            RuleId::UnreachableCode
            | RuleId::JournalComment
            | RuleId::RedundantComment
            | RuleId::CommentedOutCode => Some(Edit::deletion(
                finding,
                Identity::resolved(finding, self.source),
                whole_lines(self.source, span),
            )),
            _ => None,
        }
    }

    /// Every edit this file's findings ask for, with the conflicts already settled.
    pub fn plan(&self, findings: &[Finding]) -> Resolution {
        resolve(
            findings
                .iter()
                .filter_map(|finding| self.edit(finding))
                .collect(),
        )
    }
}

/// Which rules `edit` can answer, for callers that want to say so before parsing.
pub fn is_fixable(rule: RuleId) -> bool {
    rule.spec().autofix.is_automatic()
}

pub fn kind(rule: RuleId) -> Autofix {
    rule.spec().autofix
}

fn line_start(source: &str, at: usize) -> usize {
    source[..at].rfind('\n').map_or(0, |index| index + 1)
}

fn line_end(source: &str, at: usize) -> usize {
    source[at..]
        .find('\n')
        .map_or(source.len(), |index| at + index)
}

/// Widens a deletion to the lines it sits on when it has them to itself, so removing
/// a statement does not leave its indentation and a blank line behind. A span sharing
/// a line with live code keeps that line and gives up only the whitespace leading
/// into it.
fn whole_lines(source: &str, span: Span) -> Span {
    let start = span.start as usize;
    let end = span.end as usize;
    let head = line_start(source, start);
    let tail = line_end(source, end);

    if source[head..start].trim().is_empty() && source[end..tail].trim().is_empty() {
        let past_newline = if tail < source.len() { tail + 1 } else { tail };
        return Span::new(head as u32, past_newline as u32);
    }

    let lead = &source[head..start];
    let trimmed = lead.trim_end().len();
    Span::new((start - (lead.len() - trimmed)) as u32, span.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rqx_core::RqxConfig;
    use rqx_core::edit::apply;

    fn fixed(language: Language, source: &str) -> String {
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(language, "fixture", source, &config).expect("analysis");
        let rewriter = Rewriter::new(language, source).expect("parse");
        let plan = rewriter.plan(&analysis.findings);
        apply(source, &plan).expect("edits within the source")
    }

    #[test]
    fn unreachable_code_loses_its_lines_and_its_indentation() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n";
        assert_eq!(
            fixed(Language::TypeScript, source),
            "function f() {\n  return 1\n}\n"
        );
    }

    #[test]
    fn a_commented_out_block_leaves_no_blank_lines() {
        let source = "\
const live = 1;
// const a = 1;
// const b = 2;
// const c = 3;
export const used = live;
";
        assert_eq!(
            fixed(Language::TypeScript, source),
            "const live = 1;\nexport const used = live;\n"
        );
    }

    #[test]
    fn a_trailing_comment_keeps_the_code_it_sits_beside() {
        let widened = whole_lines("call() // TODO: later\n", Span::new(7, 21));
        assert_eq!(widened, Span::new(6, 21));
    }

    #[test]
    fn the_last_line_of_a_file_without_a_newline_is_still_removable() {
        let source = "const a = 1\n// TODO: fix";
        let widened = whole_lines(source, Span::new(12, source.len() as u32));
        assert_eq!(widened, Span::new(12, source.len() as u32));
    }

    #[test]
    fn a_crlf_line_loses_both_of_its_endings() {
        let source = "const a = 1\r\n// TODO: fix\r\nconst b = 2\r\n";
        let widened = whole_lines(source, Span::new(13, 25));
        assert_eq!(
            &source[widened.start as usize..widened.end as usize],
            "// TODO: fix\r\n"
        );
    }

    fn verified(language: Language, source: &str) -> Result<Verified, Withheld> {
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(language, "fixture", source, &config).expect("analysis");
        let rewriter = Rewriter::new(language, source).expect("parse");
        let plan = rewriter.plan(&analysis.findings);
        rewriter.verify("fixture", &config, &analysis.findings, &plan)
    }

    #[test]
    fn a_clean_deletion_is_verified_and_its_finding_is_gone() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n";
        let verified = verified(Language::TypeScript, source).expect("verified");
        assert_eq!(verified.source, "function f() {\n  return 1\n}\n");
        assert!(
            verified
                .findings
                .iter()
                .all(|finding| finding.rule != RuleId::UnreachableCode),
            "{:?}",
            verified.findings
        );
    }

    #[test]
    fn a_deletion_that_makes_a_binding_unused_is_still_verified() {
        let source = "function f() {\n  const a = 1\n  return 1\n  console.log(a)\n}\n";
        let verified = verified(Language::TypeScript, source).expect("the next round's edit");
        assert!(
            verified
                .findings
                .iter()
                .any(|finding| finding.rule == RuleId::UnusedLocal),
            "{:?}",
            verified.findings
        );
    }

    #[test]
    fn an_edit_that_breaks_the_parse_is_withheld() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        let finding = analysis
            .findings
            .iter()
            .find(|finding| finding.rule == RuleId::UnreachableCode)
            .expect("an unreachable finding");
        let closing = source.rfind('}').unwrap() as u32;
        let broken = resolve(vec![Edit::deletion(
            finding,
            Identity::resolved(finding, source),
            Span::new(closing, closing + 1),
        )]);
        let rewriter = Rewriter::new(Language::TypeScript, source).unwrap();
        assert!(matches!(
            rewriter.verify("fixture", &config, &analysis.findings, &broken),
            Err(Withheld::DoesNotParse(_))
        ));
    }

    #[test]
    fn an_edit_that_leaves_its_finding_in_place_is_withheld() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        let finding = analysis
            .findings
            .iter()
            .find(|finding| finding.rule == RuleId::UnreachableCode)
            .expect("an unreachable finding");
        let last = source.len() as u32;
        let elsewhere = resolve(vec![Edit::deletion(
            finding,
            Identity::resolved(finding, source),
            Span::new(last - 1, last),
        )]);
        let rewriter = Rewriter::new(Language::TypeScript, source).unwrap();
        assert!(matches!(
            rewriter.verify("fixture", &config, &analysis.findings, &elsewhere),
            Err(Withheld::Survives(_))
        ));
    }

    /// One source per language carrying all four syntactic-span rules, and the file
    /// each should become. Fixing the result again must change nothing.
    const CORPUS: [(Language, &str, &str); 5] = [
        (
            Language::TypeScript,
            "// @author someone\n// get user name\nfunction getUserName() {\n  return 'x'\n  console.log('dead')\n}\n// const a = 1;\n// const b = 2;\n// const c = 3;\n",
            "function getUserName() {\n  return 'x'\n}\n",
        ),
        (
            Language::Python,
            "# @author someone\n# get user name\ndef get_user_name():\n    return 'x'\n    print('dead')\n# a = 1\n# b = 2\n# c = 3\n",
            "def get_user_name():\n    return 'x'\n",
        ),
        (
            Language::Go,
            "package main\n\n// @author someone\n// get user name\nfunc getUserName() string {\n\treturn \"x\"\n\tprintln(\"dead\")\n}\n\n// a := first()\n// b := second()\n// c := third()\n",
            "package main\n\nfunc getUserName() string {\n\treturn \"x\"\n}\n\n",
        ),
        (
            Language::Java,
            "// @author someone\nclass Hygiene {\n    // get user name\n    String getUserName() {\n        return \"x\";\n        System.out.println(\"dead\");\n    }\n    // int a = 1;\n    // int b = 2;\n    // int c = 3;\n}\n",
            "class Hygiene {\n    String getUserName() {\n        return \"x\";\n    }\n}\n",
        ),
        (
            Language::Cpp,
            "// @author someone\n// get user name\nint getUserName() {\n    return 1;\n    printf(\"dead\");\n}\n// int a = 1;\n// int b = 2;\n// int c = 3;\n",
            "int getUserName() {\n    return 1;\n}\n",
        ),
    ];

    #[test]
    fn every_language_fixes_the_four_syntactic_rules_and_converges() {
        for (language, source, expected) in CORPUS {
            let once = verified(language, source)
                .unwrap_or_else(|withheld| panic!("{language:?}: {withheld}"));
            assert_eq!(once.source, expected, "{language:?}");
            let again = verified(language, &once.source).expect("a fixed file verifies");
            assert_eq!(again.source, expected, "{language:?} did not converge");
        }
    }

    #[test]
    fn parses_clean_names_the_first_broken_node() {
        assert_eq!(parses_clean(Language::TypeScript, "const a = 1\n"), Ok(()));
        let fault = parses_clean(Language::TypeScript, "function f( {\n").unwrap_err();
        assert!(fault.span.end <= 14, "{fault}");
    }

    #[test]
    fn a_span_that_names_no_node_is_refused() {
        let source = "function f() {\n  return 1\n  console.log('dead')\n}\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        let mut stale = analysis
            .findings
            .into_iter()
            .find(|finding| finding.rule == RuleId::UnreachableCode)
            .expect("an unreachable finding");
        stale.location.start += 3;

        let rewriter = Rewriter::new(Language::TypeScript, source).unwrap();
        assert!(rewriter.edit(&stale).is_none());
    }

    #[test]
    fn a_rule_with_no_mechanical_answer_produces_nothing() {
        let source = "function f(a: number) {\n  if (a > 1) { if (a > 2) { if (a > 3) { return 1 } } }\n  return 0\n}\n";
        let config = RqxConfig::default();
        let analysis = crate::analyze_as(Language::TypeScript, "fixture", source, &config).unwrap();
        let rewriter = Rewriter::new(Language::TypeScript, source).unwrap();
        for finding in &analysis.findings {
            if !is_fixable(finding.rule) {
                assert!(rewriter.edit(finding).is_none(), "{}", finding.rule);
            }
        }
    }
}

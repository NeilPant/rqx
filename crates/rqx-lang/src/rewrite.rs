//! Turns a finding into the edit that resolves it, for the rules whose fix is
//! decided by the finding alone. A rule that is not here has no mechanical answer,
//! and `edit` says so by returning `None` rather than guessing at one.

use rqx_core::Language;
use rqx_core::edit::{Edit, Resolution, resolve};
use rqx_core::finding::Finding;
use rqx_core::rule::{Autofix, RuleId};
use rqx_core::span::Span;
use tree_sitter::{Node, Tree};

use crate::profile::LanguageProfile;
use crate::{AnalysisError, support};

/// One parse of one file, reused across every finding in it. Parsing per finding
/// would re-read a large file once per fix, and the tree is the same either way.
pub struct Rewriter<'src> {
    source: &'src str,
    profile: &'static LanguageProfile,
    tree: Tree,
}

impl<'src> Rewriter<'src> {
    pub fn new(language: Language, source: &'src str) -> Result<Self, AnalysisError> {
        let support = support(language);
        let tree = crate::parse(language, support.profile, source)?;
        Ok(Self {
            source,
            profile: support.profile,
            tree,
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
            | RuleId::CommentedOutCode => {
                Some(Edit::deletion(finding, whole_lines(self.source, span)))
            }
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

//! Comments no fix may remove: every comment in a block that carries licence or
//! copyright text, and every comment that holds a URL.

use rqx_core::span::Span;
use tree_sitter::Node;

use crate::tree::span_of;
use crate::vocabulary::{carries_url, licence_notice};

/// The spans of the protected comments among `comments`, in source order. A block is
/// a run of comments with nothing but whitespace between them, so a licence header
/// written as twenty `//` lines is protected as a whole when any one of them names
/// the licence.
pub fn protected_comments<'tree>(
    source: &str,
    comments: impl IntoIterator<Item = Node<'tree>>,
) -> Vec<Span> {
    let mut spans: Vec<Span> = comments.into_iter().map(span_of).collect();
    spans.sort();
    spans.dedup();

    let mut protected = Vec::new();
    let mut block: Vec<Span> = Vec::new();
    let mut flush = |block: &mut Vec<Span>| {
        let notice = block.iter().any(|span| licence_notice(span.text(source)));
        protected.extend(
            block
                .drain(..)
                .filter(|span| notice || carries_url(span.text(source))),
        );
    };
    for span in spans {
        let joined = block.last().is_some_and(|previous| {
            source
                .get(previous.end as usize..span.start as usize)
                .is_some_and(|between| between.trim().is_empty())
        });
        if !joined {
            flush(&mut block);
        }
        block.push(span);
    }
    flush(&mut block);
    protected
}

/// The protected comments of a parsed file.
pub fn protected_in(
    profile: &crate::profile::LanguageProfile,
    source: &str,
    root: Node<'_>,
) -> Vec<Span> {
    let comments = crate::tree::descendants(root)
        .into_iter()
        .filter(|node| profile.is_comment(*node));
    protected_comments(source, comments)
}

/// Whether removing `span` would remove any part of a protected comment.
pub fn touches_protected(protected: &[Span], span: Span) -> bool {
    protected
        .iter()
        .any(|kept| kept.start < span.end && span.start < kept.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn licence_words_and_urls_are_notices_and_dividers_are_not() {
        assert!(licence_notice(
            "// Licensed under the Apache License, Version 2.0"
        ));
        assert!(licence_notice("// SPDX-License-Identifier: MIT"));
        assert!(licence_notice(
            "# Copyright 2024 The Authors. All rights reserved."
        ));
        assert!(!licence_notice(
            "// commit the transaction before returning"
        ));
        assert!(carries_url(
            "//      http://www.apache.org/licenses/LICENSE-2.0"
        ));
        assert!(!carries_url("// ---------------- helpers ----------------"));
    }

    fn protected(language: rqx_core::Language, source: &str) -> Vec<&str> {
        let support = crate::support(language);
        let tree = crate::parse(language, support.profile, false, source).expect("parses");
        protected_in(support.profile, source, tree.root_node())
            .into_iter()
            .map(|span| span.text(source))
            .collect()
    }

    #[test]
    fn a_licence_header_is_protected_as_a_whole_block() {
        let source = "\
// Copyright 2013-2023 The Cobra Authors
//
// Licensed under the Apache License, Version 2.0 (the \"License\");
//      http://www.apache.org/licenses/LICENSE-2.0
// ======================================================

package cobra

// ======================================================
func f() {}
";
        let kept = protected(rqx_core::Language::Go, source);
        // The five header lines, and not the divider after `package`.
        assert_eq!(kept.len(), 5, "{kept:?}");
    }

    #[test]
    fn a_url_comment_is_protected_on_its_own() {
        let source = "\
package main

func f() {
\t// https://github.com/spf13/cobra/issues/1257
\t// ----------
}
";
        assert_eq!(
            protected(rqx_core::Language::Go, source),
            ["// https://github.com/spf13/cobra/issues/1257"]
        );
    }
}

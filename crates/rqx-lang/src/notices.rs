//! Comments no fix may remove: every comment in a block that carries licence or
//! copyright text, every comment that holds a URL, every comment a tool reads as a
//! directive, and a Go file's comments above its `package` clause.

use rqx_core::span::Span;
use tree_sitter::Node;

use crate::tree::{kind_of, span_of};
use crate::vocabulary::{carries_url, licence_notice, tool_directive};

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
                .filter(|span| notice || is_kept_alone(span.text(source))),
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

/// A comment protected on its own, whatever the comments around it say.
fn is_kept_alone(text: &str) -> bool {
    carries_url(text) || tool_directive(text).is_some()
}

/// The protected comments of a parsed file.
pub fn protected_in(
    profile: &crate::profile::LanguageProfile,
    source: &str,
    root: Node<'_>,
) -> Vec<Span> {
    let comments: Vec<Node<'_>> = crate::tree::descendants(root)
        .into_iter()
        .filter(|node| profile.is_comment(*node))
        .collect();
    let mut protected = protected_comments(source, comments.iter().copied());
    // Above `package` sit the build constraints and the package documentation, which
    // `go build` and `go doc` read.
    let mut cursor = root.walk();
    let package = root
        .named_children(&mut cursor)
        .find(|node| kind_of(*node) == "package_clause");
    if profile.language == rqx_core::Language::Go
        && let Some(package) = package
    {
        protected.extend(
            comments
                .iter()
                .filter(|comment| comment.end_byte() <= package.start_byte())
                .map(|comment| span_of(*comment)),
        );
        protected.sort();
        protected.dedup();
    }
    protected
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
    fn tool_directives_are_protected_and_a_plain_comment_is_not() {
        let go = "\
//go:build linux

// Package p does things.
package p

//go:generate stringer -type=Kind
func f() {
\t// nolint:errcheck
\t// ----------
}
";
        assert_eq!(
            protected(rqx_core::Language::Go, go),
            [
                "//go:build linux",
                "// Package p does things.",
                "//go:generate stringer -type=Kind",
                "// nolint:errcheck",
            ]
        );
        let python = "#!/usr/bin/env python\n# -*- coding: utf-8 -*-\nimport os  # noqa: F401\nx = []  # type: list\n# a note\n";
        assert_eq!(
            protected(rqx_core::Language::Python, python),
            [
                "#!/usr/bin/env python",
                "# -*- coding: utf-8 -*-",
                "# noqa: F401",
                "# type: list",
            ]
        );
        let script = "/* eslint-disable no-console */\n// @ts-expect-error\n/// <reference types=\"node\" />\n// prettier-ignore\n// a note\nconst a = 1;\n";
        assert_eq!(
            protected(rqx_core::Language::TypeScript, script),
            [
                "/* eslint-disable no-console */",
                "// @ts-expect-error",
                "/// <reference types=\"node\" />",
                "// prettier-ignore",
            ]
        );
        let cpp = "int f(int x) {\n  switch (x) {\n  case 1:\n    x++;\n    // fallthrough\n  case 2:\n    return x;  // NOLINT\n  }\n  // clang-format off\n  return 0;\n}\n";
        assert_eq!(
            protected(rqx_core::Language::Cpp, cpp),
            ["// fallthrough", "// NOLINT", "// clang-format off"]
        );
        let java = "class A {\n  //noinspection unchecked\n  // @SuppressWarnings(\"x\")\n  // a note\n}\n";
        assert_eq!(protected(rqx_core::Language::Java, java).len(), 2);
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

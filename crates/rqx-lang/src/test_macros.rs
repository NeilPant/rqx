//! Let grammars read named test-block macros as declarations. Only the argument
//! bytes change; delimiters, newlines and every body byte keep their positions.
//! Collectors still read labels and source ranges from the original source.

use std::borrow::Cow;
use std::ops::Range;

fn trivia(bytes: &[u8], mut at: usize) -> usize {
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at..at + 2) == Some(b"//") {
            at += bytes[at..]
                .iter()
                .position(|byte| *byte == b'\n')
                .unwrap_or(bytes.len() - at);
        } else if bytes.get(at..at + 2) == Some(b"/*") {
            at = bytes[at + 2..]
                .windows(2)
                .position(|pair| pair == b"*/")
                .map_or(bytes.len(), |end| at + end + 4);
        } else {
            return at;
        }
    }
}

// The end of a literal, and its contents when it is a string rather than a char.
fn literal(bytes: &[u8], at: usize) -> Option<(usize, Option<Range<usize>>)> {
    for prefix in [b"u8".as_slice(), b"u", b"U", b"L"] {
        if bytes.get(at..at + prefix.len()) == Some(prefix)
            && (bytes.get(at + prefix.len()) == Some(&b'"')
                || bytes.get(at + prefix.len()) == Some(&b'\'')
                || bytes.get(at + prefix.len()..at + prefix.len() + 2) == Some(b"R\""))
        {
            return literal(bytes, at + prefix.len());
        }
    }
    if bytes.get(at..at + 2) == Some(b"R\"") {
        let open = bytes[at + 2..]
            .iter()
            .take(17)
            .position(|byte| *byte == b'(')?
            + at
            + 2;
        let delimiter = &bytes[at + 2..open];
        let mut closing = vec![b')'];
        closing.extend_from_slice(delimiter);
        closing.push(b'"');
        let end = bytes[open + 1..]
            .windows(closing.len())
            .position(|part| part == closing)?
            + open
            + 1;
        return Some((end + closing.len(), Some(open + 1..end)));
    }
    let quote = *bytes.get(at)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    // A digit separator is not the start of a character literal.
    if quote == b'\''
        && at > 0
        && bytes[at - 1].is_ascii_hexdigit()
        && bytes.get(at + 1).is_some_and(u8::is_ascii_hexdigit)
        && !bytes[..at].ends_with(b"u8")
    {
        return None;
    }
    let mut end = at + 1;
    while let Some(byte) = bytes.get(end) {
        if *byte == b'\\' {
            end += 2;
        } else if *byte == quote {
            return Some((end + 1, (quote == b'"').then_some(at + 1..end)));
        } else {
            end += 1;
        }
    }
    None
}

fn close_call(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 1;
    let mut at = open + 1;
    while at < bytes.len() {
        at = trivia(bytes, at);
        if let Some((end, _)) = literal(bytes, at) {
            at = end;
            continue;
        }
        match bytes.get(at)? {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// The first string is the case label; fixture/type arguments come before it in
/// method macros, and tags and type lists come after it. Adjacent C++ string
/// literals form one label; a single literal is borrowed from the original text.
pub(crate) fn label(arguments: &str) -> Option<Cow<'_, str>> {
    let bytes = arguments.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        at = trivia(bytes, at);
        if let Some((end, content)) = literal(bytes, at) {
            if let Some(content) = content {
                let mut label = Cow::Borrowed(arguments.get(content)?);
                let mut next = trivia(bytes, end);
                while let Some((end, Some(content))) = literal(bytes, next) {
                    label.to_mut().push_str(arguments.get(content)?);
                    next = trivia(bytes, end);
                }
                return Some(label);
            }
            at = end;
        } else {
            at += 1;
        }
    }
    None
}

pub(crate) fn normalize<'a>(source: &'a str, macros: &[&str]) -> Cow<'a, [u8]> {
    let bytes = source.as_bytes();
    if macros.is_empty() {
        return Cow::Borrowed(bytes);
    }
    let mut rewritten: Option<Vec<u8>> = None;
    let mut at = 0;
    while at < bytes.len() {
        at = trivia(bytes, at);
        if let Some((end, _)) = literal(bytes, at) {
            at = end;
            continue;
        }
        let start = at;
        if !bytes
            .get(at)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        {
            at += 1;
            continue;
        }
        while bytes
            .get(at)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            at += 1;
        }
        let name = &source[start..at];
        if !macros.contains(&name) {
            continue;
        }
        let open = trivia(bytes, at);
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = close_call(bytes, open) else {
            continue;
        };
        if bytes.get(trivia(bytes, close + 1)) != Some(&b'{')
            || label(&source[open + 1..close]).is_none()
        {
            continue;
        }
        let first = trivia(bytes, open + 1);
        let output = rewritten.get_or_insert_with(|| bytes.to_vec());
        for byte in &mut output[open + 1..close] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
        // Comments may carry licences or tool directives. Keep them in the tree.
        let mut comment = open + 1;
        while comment < close {
            if let Some((end, _)) = literal(bytes, comment) {
                comment = end;
                continue;
            }
            if bytes
                .get(comment..comment + 2)
                .is_some_and(|pair| pair == b"//" || pair == b"/*")
            {
                let end = trivia(bytes, comment).min(close);
                output[comment..end].copy_from_slice(&bytes[comment..end]);
                comment = end;
            } else {
                comment += 1;
            }
        }
        // A synthetic identifier must end on an original UTF-8 boundary too.
        // Fixture types can start with a non-ASCII identifier.
        let width = source[first..].chars().next().unwrap().len_utf8();
        output[first..first + width].fill(b'X');
        at = close + 1;
    }
    rewritten.map_or(Cow::Borrowed(bytes), Cow::Owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_preserves_bytes_lines_comments_and_original_labels() {
        let source = "TEST_CASE( /* copyright example */\n u8\"déjà ) here\", \"[tag]\") {\n CHECK(true);\n}";
        let output = normalize(source, &["TEST_CASE"]);
        assert_eq!(output.len(), source.len());
        let text = std::str::from_utf8(&output).unwrap();
        assert!(text.contains("/* copyright example */"));
        assert!(text.ends_with("{\n CHECK(true);\n}"));
        assert_eq!(
            text.match_indices('\n').map(|(i, _)| i).collect::<Vec<_>>(),
            source
                .match_indices('\n')
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        );
        assert_eq!(label(source).as_deref(), Some("déjà ) here"));
    }

    #[test]
    fn literals_comments_and_prototypes_are_not_macro_definitions() {
        for source in [
            "// TEST_CASE(\"not code\") {}",
            "auto text = u8R\"tag(TEST_CASE(\"not code\") {})tag\";",
            "TEST_CASE(\"a call\"); {}",
            "OTHER(\"a block\") {}",
            "TEST_CASE(BareName) {}",
        ] {
            assert_eq!(normalize(source, &["TEST_CASE"]), source.as_bytes());
        }
    }

    #[test]
    fn raw_labels_and_digit_separators_leave_the_next_macro_visible() {
        let source = "auto n = 1'000; TEST_CASE(R\"tag(label ) here)tag\") {}";
        assert_ne!(normalize(source, &["TEST_CASE"]), source.as_bytes());
        assert_eq!(
            label("(R\"tag(label ) here)tag\")").as_deref(),
            Some("label ) here")
        );
    }

    #[test]
    fn adjacent_and_encoded_string_literals_form_one_label() {
        assert_eq!(
            label("(u8\"works\" /* join */ R\"( for positive values)\", \"[tag]\")").as_deref(),
            Some("works for positive values")
        );
    }

    #[test]
    fn every_synthetic_node_keeps_an_original_utf8_boundary() {
        let source = "TEST_CASE_METHOD(ΔFixture, \"a Unicode fixture\") { CHECK(true); }";
        let normalized = normalize(source, &["TEST_CASE_METHOD"]);
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(normalized.as_ref(), None).unwrap();
        assert!(!tree.root_node().has_error());
        for node in crate::tree::descendants(tree.root_node()) {
            assert!(
                source.get(node.byte_range()).is_some(),
                "{}: {:?}",
                node.kind(),
                node.byte_range()
            );
        }
    }
}

//! Paths written into URIs and read back out of them. One encoder and one decoder,
//! so what a report writes is what the reader of the same report decodes.
//!
//! A path is not a URI: a space, a `#` or a `%` all mean something else once it is
//! one. Everything outside RFC 3986's unreserved set is escaped, which is over-strict
//! and decodes back identically, whereas under-escaping writes a URI a consumer
//! rejects or resolves to another file.

/// An absolute path as the path of a `file://` URI or a link. `:` is kept, so a
/// Windows drive letter stays `C:`.
pub fn encode_path(path: &str) -> String {
    encode(path, b"/:")
}

/// A path relative to a base, as a URI reference. `:` is escaped too: in the first
/// segment of a relative reference it would read as the end of a scheme.
pub fn encode_relative(path: &str) -> String {
    encode(path, b"/")
}

fn encode(path: &str, kept: &[u8]) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) || kept.contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The text with every `%XX` escape turned back into its byte. A `%` that starts no
/// escape is kept as it is.
pub fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                at += 3;
            }
            None => {
                decoded.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWKWARD: [&str; 6] = [
        "dir with space/a b.ts",
        "q\"uote.ts",
        "d/c,d:e=f%.ts",
        "app/(auth)/[id]/page.tsx",
        "my dir/a b#c%d.ts",
        "ünï/日本語.ts",
    ];

    #[test]
    fn a_relative_path_is_a_valid_uri_reference_that_decodes_back() {
        for path in AWKWARD {
            let encoded = encode_relative(path);
            assert!(
                encoded
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-._~/%".contains(&byte)),
                "{encoded}"
            );
            assert_eq!(decode(&encoded), path);
        }
        assert_eq!(
            encode_relative("my dir/a b#c%d.ts"),
            "my%20dir/a%20b%23c%25d.ts"
        );
    }

    #[test]
    fn an_absolute_path_keeps_its_drive_letter() {
        assert_eq!(encode_path("/C:/work/a b.ts"), "/C:/work/a%20b.ts");
        assert_eq!(decode(&encode_path("/C:/work/a b.ts")), "/C:/work/a b.ts");
    }
}

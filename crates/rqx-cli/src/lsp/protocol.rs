use std::path::{Path, PathBuf};

use lsp_types::{
    ClientCapabilities, CodeActionKind, CodeActionOptions, CodeActionProviderCapability,
    ExecuteCommandOptions, HoverProviderCapability, OneOf, Position, PositionEncodingKind, Range,
    SaveOptions, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Uri, WorkDoneProgressOptions,
};
use rqx_core::LineIndex;
use rqx_core::finding::SourceRange;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    #[default]
    Utf16,
    Utf32,
}

impl Encoding {
    /// A client lists the encodings it accepts in preference order, and the server
    /// picks. UTF-8 avoids a conversion per column; UTF-16 is the protocol default and
    /// the only one VS Code speaks, so it is what an unstated capability means.
    pub fn negotiate(capabilities: &ClientCapabilities) -> Self {
        capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .and_then(|offered| {
                offered.iter().find_map(|kind| {
                    if *kind == PositionEncodingKind::UTF8 {
                        Some(Self::Utf8)
                    } else if *kind == PositionEncodingKind::UTF32 {
                        Some(Self::Utf32)
                    } else if *kind == PositionEncodingKind::UTF16 {
                        Some(Self::Utf16)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default()
    }

    pub fn kind(self) -> PositionEncodingKind {
        match self {
            Self::Utf8 => PositionEncodingKind::UTF8,
            Self::Utf16 => PositionEncodingKind::UTF16,
            Self::Utf32 => PositionEncodingKind::UTF32,
        }
    }

    fn width(self, text: &str) -> u32 {
        match self {
            Self::Utf8 => text.len() as u32,
            Self::Utf16 => text.encode_utf16().count() as u32,
            Self::Utf32 => text.chars().count() as u32,
        }
    }
}

/// A finding carries byte offsets and a character column, but LSP wants a column in
/// the encoding the client negotiated. Converting from the byte offset against the
/// real line is the only way to get all three encodings right; deriving it from the
/// stored column silently assumes UTF-32.
pub fn position_at(index: &LineIndex<'_>, encoding: Encoding, offset: u32) -> Position {
    let (line, _) = index.locate(offset);
    let text = index.line_text(line);
    let mut byte = (offset.saturating_sub(index.line_start(line)) as usize).min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    Position {
        line: line.saturating_sub(1),
        character: encoding.width(&text[..byte]),
    }
}

pub fn range_at(index: &LineIndex<'_>, encoding: Encoding, range: &SourceRange) -> Range {
    let start = position_at(index, encoding, range.start);
    let end = position_at(index, encoding, range.end.max(range.start));
    Range { start, end }
}

/// A path is not a URI: a space, a `#` or a `%` all mean something else once it is one.
/// Percent-encoding everything outside RFC 3986's unreserved set is over-strict and
/// decodes back identically, whereas under-encoding produces a URI the client rejects.
fn percent_encode(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn uri_for(root: &Path, relative: &str) -> Option<Uri> {
    uri_of(&root.join(relative))
}

pub fn uri_of(path: &Path) -> Option<Uri> {
    let mut text = path.display().to_string();
    if cfg!(windows) {
        text = text.replace('\\', "/");
    }
    // A Windows path starts at a drive letter rather than a slash, and `file://C:/…`
    // reads `C:` as the authority rather than as the start of the path.
    let absolute = if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    };
    format!("file://{}", percent_encode(&absolute)).parse().ok()
}

pub fn path_from_uri(uri: &Uri) -> Option<PathBuf> {
    let rest = uri.as_str().strip_prefix("file://")?;
    let decoded = percent_decode(rest);
    if cfg!(windows)
        && let Some(drive) = decoded.strip_prefix('/')
        && drive.as_bytes().get(1) == Some(&b':')
    {
        return Some(PathBuf::from(drive));
    }
    Some(PathBuf::from(decoded))
}

/// URIs arrive percent-encoded and a path with a space is common enough that ignoring
/// it would silently break diagnostics for those files.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn capabilities(encoding: Encoding, commands: Vec<String>) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(encoding.kind()),
        // Diagnostics are recomputed on open and save rather than on every keystroke:
        // a half-typed function is not a finding, it is a work in progress. The buffer
        // is still tracked on change, so a save analyses what the editor holds.
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
                ..TextDocumentSyncOptions::default()
            },
        )),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
            ..CodeActionOptions::default()
        })),
        code_lens_provider: Some(lsp_types::CodeLensOptions {
            resolve_provider: Some(false),
        }),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands,
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        document_link_provider: None,
        workspace: Some(lsp_types::WorkspaceServerCapabilities {
            workspace_folders: Some(lsp_types::WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(OneOf::Left(true)),
            }),
            file_operations: None,
        }),
        ..ServerCapabilities::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(source: &str) -> LineIndex<'_> {
        LineIndex::new(source)
    }

    #[test]
    fn lsp_positions_are_zero_based() {
        let source = "let a = 1;\nlet b = 2;\n";
        assert_eq!(
            position_at(&index(source), Encoding::Utf16, 0),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            position_at(&index(source), Encoding::Utf16, 15),
            Position {
                line: 1,
                character: 4
            }
        );
    }

    /// A synthetic span for a whole-file finding carries offset 0, and subtracting one
    /// from that would wrap a u32 into a position past the end of the universe.
    #[test]
    fn a_synthetic_position_does_not_underflow() {
        assert_eq!(
            position_at(&index(""), Encoding::Utf16, 0),
            Position {
                line: 0,
                character: 0
            }
        );
    }

    /// An emoji is one character, two UTF-16 code units and four bytes, so a column
    /// that ignores the negotiated encoding puts the squiggle in three different
    /// wrong places depending on the client.
    #[test]
    fn columns_follow_the_negotiated_encoding() {
        let source = "const 🎈 = value;";
        let offset = source.find("= value").unwrap() as u32;
        assert_eq!(
            position_at(&index(source), Encoding::Utf8, offset).character,
            11
        );
        assert_eq!(
            position_at(&index(source), Encoding::Utf16, offset).character,
            9
        );
        assert_eq!(
            position_at(&index(source), Encoding::Utf32, offset).character,
            8
        );
    }

    #[test]
    fn an_offset_inside_a_character_snaps_to_its_start() {
        let source = "const é = 1;";
        let inside = source.find('é').unwrap() as u32 + 1;
        assert_eq!(
            position_at(&index(source), Encoding::Utf16, inside).character,
            6
        );
    }

    #[test]
    fn file_uris_decode_back_into_paths() {
        let uri: Uri = "file:///tmp/my%20project/src/a.ts".parse().unwrap();
        assert_eq!(
            path_from_uri(&uri),
            Some(PathBuf::from("/tmp/my project/src/a.ts"))
        );
    }

    /// A path with a space produced an unparseable URI, which `publish` turned into a
    /// silent no-op: no diagnostics at all for the whole project, and no error.
    #[test]
    fn paths_round_trip_through_a_uri() {
        for path in [
            "/tmp/plain/a.ts",
            "/tmp/my project/a.ts",
            "/tmp/100%/a.ts",
            "/tmp/a#b/c.ts",
            "/tmp/ünïcode/a.ts",
            "/tmp/quote'd/a.ts",
        ] {
            let uri = uri_of(Path::new(path)).expect(path);
            assert_eq!(
                path_from_uri(&uri).as_deref(),
                Some(Path::new(path)),
                "{path}"
            );
        }
    }

    #[test]
    fn a_non_file_uri_is_ignored_rather_than_guessed_at() {
        let uri: Uri = "untitled:Untitled-1".parse().unwrap();
        assert_eq!(path_from_uri(&uri), None);
    }
}

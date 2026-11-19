//! OSC 8 hyperlinks, which let a terminal that understands them open `file:line` in an
//! editor on a click. Whether to emit them is decided once, by the caller; this only
//! knows how to write one.

use std::fmt::Display;
use std::path::Path;

/// Where a link points. By default the file itself; a template names an editor's own
/// scheme instead, with `{path}`, `{line}` and `{column}` filled in, such as
/// `vscode://file/{path}:{line}:{column}`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Links {
    template: Option<String>,
}

impl Links {
    pub const EDITOR_URL: &'static str = "RQX_EDITOR_URL";

    pub fn with_template(template: Option<String>) -> Self {
        Self {
            template: template.filter(|template| !template.is_empty()),
        }
    }

    pub fn from_environment() -> Self {
        Self::with_template(std::env::var(Self::EDITOR_URL).ok())
    }

    /// `text`, as a link to `path` at `line` and `column`. The path is made absolute
    /// because a link outlives the directory rqx was run from.
    pub fn wrap(&self, text: impl Display, path: &Path, line: u32, column: u32) -> String {
        let url = self.url(path, line, column);
        format!("\u{1b}]8;;{url}\u{1b}\\{text}\u{1b}]8;;\u{1b}\\")
    }

    fn url(&self, path: &Path, line: u32, column: u32) -> String {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let mut slashed = absolute.to_string_lossy().replace('\\', "/");
        if !slashed.starts_with('/') {
            slashed.insert(0, '/');
        }
        let encoded = crate::uri::encode_path(&slashed);
        match &self.template {
            Some(template) => template
                .replace("{path}", &encoded)
                .replace("{line}", &line.to_string())
                .replace("{column}", &column.to_string()),
            None => format!("file://{encoded}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_link_wraps_its_text_in_the_osc_8_open_and_close() {
        let link = Links::default().wrap("src/a.ts:3", Path::new("/work/src/a.ts"), 3, 1);

        assert_eq!(
            link,
            "\u{1b}]8;;file:///work/src/a.ts\u{1b}\\src/a.ts:3\u{1b}]8;;\u{1b}\\"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_path_is_escaped_so_a_space_cannot_end_the_link() {
        let link = Links::default().wrap("x", Path::new("/my work/a#1.ts"), 1, 1);

        assert!(link.contains("file:///my%20work/a%231.ts"), "{link:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_template_receives_the_path_the_line_and_the_column() {
        let links = Links::with_template(Some("vscode://file/{path}:{line}:{column}".into()));

        let link = links.wrap("x", Path::new("/work/a.ts"), 12, 5);

        assert!(
            link.contains("\u{1b}]8;;vscode://file//work/a.ts:12:5\u{1b}\\"),
            "{link:?}"
        );
    }

    #[test]
    fn an_empty_template_is_the_same_as_none() {
        assert_eq!(Links::with_template(Some(String::new())), Links::default());
    }

    #[test]
    fn a_relative_path_is_made_absolute() {
        let link = Links::default().wrap("x", Path::new("src/a.ts"), 1, 1);

        assert!(link.contains("]8;;file:///"), "{link:?}");
    }
}

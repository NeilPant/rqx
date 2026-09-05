use std::path::Path;
use std::process::Command;

use ratatui::DefaultTerminal;

/// Editors that take a line as `path:line` rather than as a `+line` argument. Every
/// terminal editor accepts `+line`, so the exceptions are the graphical ones.
const COLON: [&str; 4] = ["code", "codium", "cursor", "subl"];

/// Which editor to hand the file to. `$VISUAL` before `$EDITOR` is the convention:
/// the first names a full-screen editor and the second may be a line editor.
fn chosen() -> String {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_owned())
}

fn arguments(editor: &str, path: &Path, line: u32) -> Vec<String> {
    let name = Path::new(editor)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(editor);
    let file = path.display().to_string();
    if COLON.contains(&name) {
        // `-g` is what makes VS Code take a line at all rather than opening the file
        // and ignoring the suffix.
        return vec!["-g".to_owned(), format!("{file}:{line}")];
    }
    vec![format!("+{line}"), file]
}

/// Hands the terminal to an editor and takes it back afterwards.
///
/// The interface owns the alternate screen and raw mode, so an editor started under
/// it would draw into a buffer nobody sees and read keys the interface has already
/// swallowed.
pub fn open(terminal: &mut DefaultTerminal, path: &Path, line: u32) -> String {
    let editor = chosen();
    // A command line rather than a bare name: `EDITOR="code -w"` is common, and
    // running the whole string as one file name fails with a confusing error.
    let mut parts = editor.split_whitespace();
    let Some(program) = parts.next() else {
        return "no editor to open it with; set $EDITOR".to_owned();
    };
    let extra: Vec<String> = parts.map(ToOwned::to_owned).collect();

    ratatui::restore();
    let status = Command::new(program)
        .args(&extra)
        .args(arguments(program, path, line.max(1)))
        .status();
    let resumed = ratatui::try_init();

    match (status, resumed) {
        (Ok(_), Ok(fresh)) => {
            *terminal = fresh;
            String::new()
        }
        (Err(error), Ok(fresh)) => {
            *terminal = fresh;
            format!("could not run `{program}`: {error}")
        }
        // Without a terminal there is nothing left to report to, and the caller is
        // about to find the same error when it draws.
        (_, Err(error)) => format!("the terminal did not come back: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_editor_takes_the_line_as_a_plus_argument() {
        let args = arguments("nvim", Path::new("src/a.ts"), 42);
        assert_eq!(args, ["+42", "src/a.ts"]);
    }

    /// VS Code opens the file and ignores a `+line`, so the whole point of jumping
    /// there is lost without the other spelling.
    #[test]
    fn a_graphical_editor_takes_the_line_after_a_colon() {
        assert_eq!(
            arguments("code", Path::new("src/a.ts"), 42),
            ["-g", "src/a.ts:42"]
        );
        assert_eq!(
            arguments("/usr/local/bin/code", Path::new("a.ts"), 7),
            ["-g", "a.ts:7"]
        );
    }
}

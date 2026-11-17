//! `/open`: handing a finding's place to the editor the developer already uses.

use std::path::Path;

use super::commands::split_words;

/// A program and the arguments that open `file` at `line`.
#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub arguments: Vec<String>,
}

/// How an editor is told which line to start on.
enum Jump {
    /// `vim +42 file`
    PlusLine,
    /// `code -g file:42`
    GotoFlag,
    /// `hx file:42`
    Suffix,
    /// An editor rqx does not know: the file alone, which is always understood.
    None,
}

/// `$VISUAL` before `$EDITOR`, the order every tool that honours both uses.
pub fn configured() -> Option<String> {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.trim().is_empty())
}

/// The command for `editor` as the variable holds it, which may carry its own
/// arguments (`code --wait`).
pub fn launch(editor: &str, file: &str, line: u32) -> Option<Launch> {
    let mut words = split_words(editor).ok()?.into_iter();
    let program = words.next()?;
    let mut arguments: Vec<String> = words.collect();
    match jump_of(&program) {
        Jump::PlusLine => {
            arguments.push(format!("+{line}"));
            arguments.push(file.to_owned());
        }
        Jump::GotoFlag => {
            arguments.push("-g".to_owned());
            arguments.push(format!("{file}:{line}"));
        }
        Jump::Suffix => arguments.push(format!("{file}:{line}")),
        Jump::None => arguments.push(file.to_owned()),
    }
    Some(Launch { program, arguments })
}

fn jump_of(program: &str) -> Jump {
    let name = Path::new(program)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(program);
    match name {
        "vim" | "nvim" | "vi" | "view" | "ex" | "nano" | "pico" | "emacs" | "emacsclient" => {
            Jump::PlusLine
        }
        "code" | "code-insiders" | "codium" | "cursor" | "windsurf" => Jump::GotoFlag,
        "hx" | "helix" | "micro" | "zed" | "subl" => Jump::Suffix,
        _ => Jump::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built(editor: &str) -> (String, Vec<String>) {
        let launch = launch(editor, "src/a.ts", 42).expect("an editor");
        (launch.program, launch.arguments)
    }

    #[test]
    fn vim_takes_the_line_as_a_plus_argument() {
        assert_eq!(
            built("vim"),
            ("vim".into(), vec!["+42".into(), "src/a.ts".into()])
        );
        assert_eq!(built("/usr/bin/nvim").1, ["+42", "src/a.ts"]);
    }

    #[test]
    fn code_takes_a_goto_flag() {
        assert_eq!(
            built("code"),
            ("code".into(), vec!["-g".into(), "src/a.ts:42".into()])
        );
    }

    #[test]
    fn an_editor_with_arguments_of_its_own_keeps_them_first() {
        assert_eq!(built("code --wait").1, ["--wait", "-g", "src/a.ts:42"]);
    }

    #[test]
    fn helix_takes_the_line_after_a_colon() {
        assert_eq!(built("hx").1, ["src/a.ts:42"]);
    }

    #[test]
    fn an_editor_nobody_taught_rqx_gets_the_file_alone() {
        assert_eq!(built("ed"), ("ed".into(), vec!["src/a.ts".into()]));
    }

    #[test]
    fn an_empty_variable_is_no_editor() {
        assert!(launch("  ", "a", 1).is_none());
    }
}

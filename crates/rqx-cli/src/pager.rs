//! A report taller than the terminal goes through a pager, as git's output does:
//! `$RQX_PAGER`, else `$PAGER`, else `less -FRX`. A pipe, a file, a document reporter,
//! CI and `--no-pager` get the report printed as before, and so does a run whose pager
//! cannot be started.

use std::io::{self, Write};
use std::process::{Command, Stdio};

use unicode_width::UnicodeWidthStr;

/// `F` quits when the text fits after all, `R` keeps the colours, and `X` leaves the
/// text on the screen after quitting.
const DEFAULT: &str = "less -FRX";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pager {
    program: String,
    arguments: Vec<String>,
}

impl Pager {
    /// The pager the environment names, or `None` when it turns paging off.
    /// `RQX_PAGER` wins even when empty, so `RQX_PAGER=` turns paging off for rqx alone;
    /// an empty `PAGER` counts as unset. A pager of `cat` is no pager at all.
    fn from_environment(variable: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let named = variable("RQX_PAGER").unwrap_or_else(|| {
            variable("PAGER")
                .filter(|pager| !pager.trim().is_empty())
                .unwrap_or_else(|| DEFAULT.to_owned())
        });
        Self::parse(&named)
    }

    /// Split on whitespace: the program, then its arguments. Quotes are not read.
    fn parse(command: &str) -> Option<Self> {
        let mut words = command.split_whitespace().map(str::to_owned);
        let program = words.next()?;
        (program != "cat").then(|| Self {
            program,
            arguments: words.collect(),
        })
    }
}

/// What a run knows before it renders anything about whether a report may be paged.
#[derive(Copy, Clone, Debug, Default)]
pub struct Situation {
    pub stdout_terminal: bool,
    pub ci: bool,
    /// `--no-pager`, or a run that prints more than once, such as `--watch`.
    pub refused: bool,
}

impl Situation {
    pub fn of_this_run(refused: bool) -> Self {
        Self {
            stdout_terminal: crate::term::is_terminal(crate::term::Stream::Stdout),
            ci: crate::term::in_ci(),
            refused,
        }
    }
}

/// The pager a report may go through, or `None` when it is printed directly.
pub fn chosen(situation: Situation, variable: impl Fn(&str) -> Option<String>) -> Option<Pager> {
    if !situation.stdout_terminal || situation.ci || situation.refused {
        return None;
    }
    Pager::from_environment(variable)
}

/// `chosen` for this process's terminal and environment.
pub fn for_run(refused: bool) -> Option<Pager> {
    chosen(Situation::of_this_run(refused), |name| {
        std::env::var(name).ok()
    })
}

/// Whether `text` needs a pager on a window of `columns` by `rows`: when it needs more
/// rows than the window has, keeping one for the shell prompt that follows it.
pub fn taller_than(text: &[u8], (columns, rows): (u16, u16)) -> bool {
    rows_needed(text, usize::from(columns)) >= usize::from(rows)
}

/// Rows `text` takes on a terminal `columns` wide: each line, escapes left out, and
/// wrapped where it is wider than the window.
fn rows_needed(text: &[u8], columns: usize) -> usize {
    let plain: Vec<u8> = anstream::adapter::strip_bytes(text).into_vec();
    String::from_utf8_lossy(&plain)
        .lines()
        .map(|line| line.width().div_ceil(columns.max(1)).max(1))
        .sum()
}

/// Shows `text` through `pager` and waits for it to exit. `false` means the pager
/// could not be started and nothing was shown, so the caller prints `text` itself.
/// Quitting the pager before the end closes the pipe; that is the reader's choice, so
/// it is not an error and the run's exit code is unchanged.
pub fn page(pager: &Pager, text: &[u8]) -> bool {
    let mut command = Command::new(&pager.program);
    command.args(&pager.arguments).stdin(Stdio::piped());
    // What git sets, for a `PAGER=less` with no flags of its own.
    if std::env::var_os("LESS").is_none() {
        command.env("LESS", "FRX");
    }
    if std::env::var_os("LV").is_none() {
        command.env("LV", "-c");
    }
    let _ = io::stdout().flush();
    let _interrupts = interrupts::deferred();
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    if let Some(mut input) = child.stdin.take() {
        let _ = input.write_all(text);
    }
    let _ = child.wait();
    true
}

/// While the pager runs, ctrl-c reaches rqx as well as the pager, which uses it to
/// stop a search. Ending rqx there would hand the shell's prompt back under a pager
/// that is still drawing, so rqx ignores it until the pager has exited, as git does.
#[cfg(unix)]
mod interrupts {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use signal_hook::consts::SIGINT;

    pub struct Deferred {
        finished: Arc<AtomicBool>,
    }

    /// From here until the value is dropped, ctrl-c does not end rqx. After that it
    /// ends it as it would have.
    pub fn deferred() -> Deferred {
        let finished = Arc::new(AtomicBool::new(false));
        let _ = signal_hook::flag::register_conditional_default(SIGINT, Arc::clone(&finished));
        Deferred { finished }
    }

    impl Drop for Deferred {
        fn drop(&mut self) {
            self.finished.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(not(unix))]
mod interrupts {
    pub struct Deferred;

    pub fn deferred() -> Deferred {
        Deferred
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn terminal() -> Situation {
        Situation {
            stdout_terminal: true,
            ..Situation::default()
        }
    }

    fn pager(program: &str, arguments: &[&str]) -> Option<Pager> {
        Some(Pager {
            program: program.to_owned(),
            arguments: arguments.iter().map(|word| (*word).to_owned()).collect(),
        })
    }

    #[test]
    fn a_person_at_a_terminal_gets_less_by_default() {
        assert_eq!(
            chosen(terminal(), environment(&[])),
            pager("less", &["-FRX"])
        );
    }

    #[test]
    fn a_pipe_ci_and_no_pager_are_never_paged() {
        let none = environment(&[]);
        assert_eq!(chosen(Situation::default(), &none), None);
        let ci = Situation {
            ci: true,
            ..terminal()
        };
        assert_eq!(chosen(ci, &none), None);
        let refused = Situation {
            refused: true,
            ..terminal()
        };
        assert_eq!(chosen(refused, &none), None);
    }

    #[test]
    fn rqx_pager_comes_before_pager() {
        let both = environment(&[("RQX_PAGER", "most -s"), ("PAGER", "more")]);
        assert_eq!(chosen(terminal(), both), pager("most", &["-s"]));
        let only_pager = environment(&[("PAGER", "more")]);
        assert_eq!(chosen(terminal(), only_pager), pager("more", &[]));
    }

    #[test]
    fn an_empty_rqx_pager_or_cat_turns_paging_off() {
        for off in [
            &[("RQX_PAGER", "")][..],
            &[("RQX_PAGER", "  "), ("PAGER", "more")],
            &[("RQX_PAGER", "cat")],
            &[("PAGER", "cat")],
        ] {
            let off: &'static [(&'static str, &'static str)] = off;
            assert_eq!(chosen(terminal(), environment(off)), None, "{off:?}");
        }
    }

    #[test]
    fn an_empty_pager_counts_as_unset() {
        let empty = environment(&[("PAGER", "")]);
        assert_eq!(chosen(terminal(), empty), pager("less", &["-FRX"]));
    }

    #[test]
    fn height_counts_lines_without_their_escapes_and_wraps_wide_ones() {
        assert_eq!(rows_needed(b"one\ntwo\n\nfour\n", 80), 4);
        assert_eq!(rows_needed(b"\x1b[1m\x1b[31mred\x1b[0m\n", 3), 1);
        assert_eq!(rows_needed("x".repeat(200).as_bytes(), 80), 3);
        assert_eq!(rows_needed(b"", 80), 0);
    }

    #[test]
    fn a_report_is_paged_when_it_and_the_prompt_do_not_fit() {
        let lines = |count: usize| "line\n".repeat(count).into_bytes();
        assert!(!taller_than(&lines(23), (80, 24)));
        assert!(taller_than(&lines(24), (80, 24)));
        assert!(taller_than(&"x".repeat(80 * 30).into_bytes(), (80, 24)));
    }

    #[test]
    fn a_pager_that_cannot_start_shows_nothing_and_says_so() {
        let missing = Pager {
            program: "rqx-no-such-pager-anywhere".to_owned(),
            arguments: Vec::new(),
        };
        assert!(!page(&missing, b"text\n"));
    }
}

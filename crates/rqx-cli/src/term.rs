//! The one place that decides what kind of terminal rqx is talking to, so the
//! progress bar, the update notice and the interface cannot disagree about it.

use std::io::IsTerminal;

/// Every variable that means "this is a build, not a person". `CI` covers GitHub
/// Actions, GitLab, CircleCI, Travis, Buildkite and Bitbucket; Jenkins, TeamCity and
/// Azure Pipelines do not set it reliably and are named individually.
const A_PIPELINE: &[&str] = &[
    "CI",
    "CONTINUOUS_INTEGRATION",
    "BUILD_NUMBER",
    "RUN_ID",
    "JENKINS_URL",
    "TEAMCITY_VERSION",
    "TF_BUILD",
    "bamboo_buildKey",
    "GO_PIPELINE_NAME",
];

/// An output stream whose terminal-ness is asked about separately: stdout can be a
/// pipe while stderr is still a person's screen, and the reverse.
#[derive(Copy, Clone)]
pub enum Stream {
    Stdout,
    Stderr,
}

pub fn is_terminal(stream: Stream) -> bool {
    match stream {
        Stream::Stdout => std::io::stdout().is_terminal(),
        Stream::Stderr => std::io::stderr().is_terminal(),
    }
}

/// Whether a pipeline is running this rather than a person.
pub fn in_ci() -> bool {
    A_PIPELINE
        .iter()
        .any(|name| std::env::var_os(name).is_some())
}

/// Columns available on the stream. `COLUMNS` wins when it parses, because it is how
/// a caller says what width they want; otherwise the terminal is asked, and a stream
/// that is not one has no width.
pub fn width(stream: Stream) -> Option<usize> {
    columns_from(std::env::var("COLUMNS").ok().as_deref()).or_else(|| {
        if !is_terminal(stream) {
            return None;
        }
        crossterm::terminal::size()
            .ok()
            .map(|(columns, _)| usize::from(columns))
    })
}

/// Columns on a stream, but only when a person is looking at it. `width` also answers
/// for a pipe when `COLUMNS` is set, and prose is never re-wrapped for a file.
pub fn terminal_width(stream: Stream) -> Option<usize> {
    is_terminal(stream).then(|| width(stream)).flatten()
}

fn columns_from(variable: Option<&str>) -> Option<usize> {
    variable?.trim().parse().ok().filter(|columns| *columns > 0)
}

/// Why an interface declined to start. Every variant is a condition under which
/// taking over the terminal would corrupt output or hang a pipeline, so each one
/// falls back to the plain report rather than degrading the interface.
#[derive(Debug, PartialEq, Eq)]
pub enum Decline {
    NotATerminal,
    DumbTerminal,
    ContinuousIntegration,
    TooSmall {
        width: u16,
        height: u16,
        needs: (u16, u16),
    },
    SetupFailed(String),
}

impl std::fmt::Display for Decline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotATerminal => f.write_str("output is not a terminal"),
            Self::DumbTerminal => f.write_str("TERM does not support an interactive interface"),
            Self::ContinuousIntegration => f.write_str("running under CI"),
            Self::TooSmall {
                width,
                height,
                needs: (needs_width, needs_height),
            } => write!(
                f,
                "terminal is {width}x{height}; the interface needs at least {needs_width}x{needs_height}"
            ),
            Self::SetupFailed(error) => write!(f, "terminal setup failed ({error})"),
        }
    }
}

/// Whether an interface that draws on this terminal can start, given the smallest
/// window it can lay out in.
pub fn interactive(needs: (u16, u16)) -> Result<(), Decline> {
    if !is_terminal(Stream::Stdout) {
        return Err(Decline::NotATerminal);
    }
    if in_ci() {
        return Err(Decline::ContinuousIntegration);
    }
    match std::env::var("TERM") {
        Err(_) => return Err(Decline::DumbTerminal),
        Ok(term) if term.is_empty() || term == "dumb" => return Err(Decline::DumbTerminal),
        Ok(_) => {}
    }
    let (width, height) = crossterm::terminal::size().map_err(|_| Decline::NotATerminal)?;
    if width < needs.0 || height < needs.1 {
        return Err(Decline::TooSmall {
            width,
            height,
            needs,
        });
    }
    Ok(())
}

/// Installed once, and told how to give the terminal back. Ratatui installs a hook of
/// its own on every init and each wraps the last, so the interfaces install this one
/// instead: a panic printed into a terminal still in raw mode is unreadable.
pub fn install_panic_hook(restore: fn()) {
    use std::sync::Once;

    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            eprintln!("\nrqx hit a bug and stopped.\n");
            previous(info);
            eprintln!(
                "\nPlease report this at https://github.com/jadhamwi21/rqx/issues; \
                 run again with RUST_BACKTRACE=1 for more."
            );
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::columns_from;

    #[test]
    fn columns_variable_is_used_when_it_is_a_positive_number() {
        assert_eq!(columns_from(Some("132")), Some(132));
        assert_eq!(columns_from(Some(" 90 ")), Some(90));
    }

    #[test]
    fn columns_variable_is_ignored_when_it_is_not_usable() {
        assert_eq!(columns_from(None), None);
        assert_eq!(columns_from(Some("")), None);
        assert_eq!(columns_from(Some("wide")), None);
        assert_eq!(columns_from(Some("0")), None);
    }
}

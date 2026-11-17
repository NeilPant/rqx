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

fn columns_from(variable: Option<&str>) -> Option<usize> {
    variable?.trim().parse().ok().filter(|columns| *columns > 0)
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

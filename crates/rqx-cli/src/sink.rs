//! Where a command's output goes. Pairing a rendering with its destination in one
//! value is what lets a single run produce the console output and every file CI wants,
//! and it works the same for every command, whatever renderings that command offers.

use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;

use crate::brand::{ColorDepth, Palette};
use crate::report::{self, Reporter};

/// The renderings one command offers: a name on the command line, and what that
/// rendering is allowed to share a stream with.
pub trait Format: ValueEnum + Copy + PartialEq + Send + Sync + 'static {
    /// What prints when every sink names a file, so a run is never silent.
    const CONSOLE: Self;

    /// A document has to arrive whole to parse at all, so it cannot share a stream.
    fn is_document(self) -> bool;

    /// Whether the rendering is written for a person, and so may carry colour.
    fn is_coloured(self) -> bool;

    fn label(self) -> String {
        self.to_possible_value()
            .map(|value| value.get_name().to_owned())
            .unwrap_or_default()
    }
}

impl Format for Reporter {
    const CONSOLE: Self = Self::Terminal;

    fn is_document(self) -> bool {
        Self::is_document(self)
    }

    fn is_coloured(self) -> bool {
        Self::is_coloured(self)
    }
}

/// The two renderings of a command that prints its own view rather than a scan's
/// findings: what a person reads, and one JSON object per row.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Rendering {
    Terminal,
    Json,
}

impl Rendering {
    pub const fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

impl Format for Rendering {
    const CONSOLE: Self = Self::Terminal;

    fn is_document(self) -> bool {
        self.is_json()
    }

    fn is_coloured(self) -> bool {
        !self.is_json()
    }
}

#[derive(Clone, Debug)]
pub struct Sink<F = Reporter> {
    pub reporter: F,
    pub path: Option<PathBuf>,
}

impl<F: Format> FromStr for Sink<F> {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        // Split on the first `=` so a Windows path keeps its drive letter.
        let (name, path) = match text.split_once('=') {
            Some((name, path)) => (name, Some(PathBuf::from(path))),
            None => (text, None),
        };
        let reporter = F::from_str(name, true).map_err(|_| {
            let known: Vec<String> = F::value_variants().iter().map(|one| one.label()).collect();
            let known: Vec<&str> = known.iter().map(String::as_str).collect();
            report::unknown(
                "reporter",
                name,
                &known,
                &format!("Pick one of: {}.", known.join(", ")),
            )
            .to_string()
        })?;
        Ok(Self { reporter, path })
    }
}

/// The sinks a run will write: the ones asked for, plus the console when none of them
/// is the console. A document interleaved with anything else on one stream is silent
/// corruption, so it is rejected before a byte is written rather than discovered by
/// whatever tries to parse the result.
pub fn settle<F: Format>(mut sinks: Vec<Sink<F>>) -> Result<Vec<Sink<F>>> {
    // A run that writes only files would otherwise print nothing at all, which
    // reads as a hang in CI and as a failure locally.
    if sinks.iter().all(|sink| sink.path.is_some()) {
        sinks.push(Sink {
            reporter: F::CONSOLE,
            path: None,
        });
    }

    let to_stdout: Vec<F> = sinks
        .iter()
        .filter(|sink| sink.path.is_none())
        .map(|sink| sink.reporter)
        .collect();
    if let Some(document) = to_stdout.iter().find(|format| format.is_document())
        && to_stdout.len() > 1
    {
        let other = to_stdout
            .iter()
            .find(|format| *format != document)
            .expect("more than one sink writes to stdout");
        bail!(
            "`{}` is a document and cannot share stdout with `{}`; give one of them a path",
            document.label(),
            other.label()
        );
    }
    Ok(sinks)
}

/// Each sink's rendering to its file or to stdout, whichever the sink names. A file
/// never carries colour, and neither does a rendering meant for a machine.
pub fn deliver<F: Format>(
    sinks: &[Sink<F>],
    palette: Palette,
    mut render: impl FnMut(F, Palette, &mut dyn Write) -> Result<()>,
) -> Result<()> {
    let plain = Palette::with_depth(ColorDepth::Monochrome);
    for sink in sinks {
        match &sink.path {
            Some(path) => {
                let mut file = std::fs::File::create(path)
                    .with_context(|| format!("writing {}", path.display()))?;
                render(sink.reporter, plain, &mut file)?;
                file.flush()?;
            }
            None if sink.reporter.is_coloured() => {
                render(sink.reporter, palette, &mut anstream::stdout().lock())?;
            }
            None => render(sink.reporter, plain, &mut std::io::stdout().lock())?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(arguments: &[&str]) -> Result<Vec<Sink<Rendering>>> {
        let parsed = arguments
            .iter()
            .map(|text| text.parse::<Sink<Rendering>>().expect("a sink"))
            .collect();
        settle(parsed)
    }

    #[test]
    fn a_name_alone_is_stdout_and_a_name_with_a_path_is_a_file() {
        let sinks = settled(&["terminal", "json=out.json"]).expect("compatible");
        assert_eq!(sinks.len(), 2);
        assert!(sinks[0].path.is_none());
        assert_eq!(
            sinks[1].path.as_deref(),
            Some(std::path::Path::new("out.json"))
        );
    }

    #[test]
    fn only_files_still_leave_the_console_on() {
        let sinks = settled(&["json=out.json"]).expect("compatible");
        assert!(
            sinks
                .iter()
                .any(|sink| sink.path.is_none() && sink.reporter == Rendering::Terminal)
        );
    }

    #[test]
    fn a_document_cannot_share_stdout() {
        let refused = settled(&["terminal", "json"]).expect_err("two on stdout");
        assert!(
            format!("{refused}").contains("cannot share stdout"),
            "{refused}"
        );
    }

    #[test]
    fn an_unknown_name_offers_the_ones_this_command_has() {
        let refused = "sarif".parse::<Sink<Rendering>>().expect_err("unknown");
        assert!(refused.contains("terminal, json"), "{refused}");
    }
}

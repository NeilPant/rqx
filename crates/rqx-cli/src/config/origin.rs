//! Where a setting's value was written, for the `from` column and for deciding which
//! file a `set` may touch.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A config file the run read because of where it sits, or because it was named.
    File(PathBuf),
    /// A file pulled in by another file's `extends`. Shared with other projects as
    /// often as not, so nothing edits it by default.
    Extended(PathBuf),
    /// A built-in preset, or a level named like one.
    Preset(String),
    /// A command-line option.
    Flag(&'static str),
}

impl Origin {
    /// The file a `set` may write to: one the project owns, never a preset or a flag
    /// and never a file somebody else's config inherits from.
    pub fn file(&self) -> Option<&Path> {
        match self {
            Self::File(path) => Some(path),
            _ => None,
        }
    }

    /// The short form the `from` column prints.
    pub fn label(&self) -> String {
        match self {
            Self::File(path) => name_of(path),
            Self::Extended(path) => format!("{} (extends)", name_of(path)),
            Self::Preset(name) => format!("preset:{name}"),
            Self::Flag(flag) => (*flag).to_owned(),
        }
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) | Self::Extended(path) => write!(f, "{}", path.display()),
            Self::Preset(name) => write!(f, "the `{name}` preset"),
            Self::Flag(flag) => f.write_str(flag),
        }
    }
}

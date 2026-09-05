//! Running a tree-sitter query by hand, so a `[[rule]]` can be tried before it is
//! written down.

use std::io::Write;
use std::path::Path;

use anyhow::{Result, bail};
use rqx_core::Language;

use crate::brand::{Ink, Palette, paint};

pub fn run(
    root: &Path,
    language: Language,
    query: &str,
    settings: &crate::config::Resolved,
    captures: bool,
    palette: Palette,
    out: &mut dyn Write,
) -> Result<u32> {
    let dim = palette.style(Ink::Muted);
    let mut matched = 0;

    for path in crate::scan::files_of(root, language, &settings.config)? {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let markup = rqx_lang::holds_markup(&path.to_string_lossy());
        let found = rqx_lang::custom::probe(language, markup, query, &text)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        for one in found {
            matched += 1;
            writeln!(
                out,
                "  {}  {}",
                paint(
                    palette.style(Ink::Text),
                    format!(
                        "{}:{}:{}",
                        crate::scan::relative_to(root, &path),
                        one.line,
                        one.column
                    )
                ),
                paint(
                    dim,
                    one.captures
                        .first()
                        .map(|capture| capture.text.trim())
                        .unwrap_or_default()
                )
            )?;
            if captures {
                for capture in &one.captures {
                    writeln!(
                        out,
                        "      {}  {}  {}",
                        paint(palette.style(Ink::Accent), format!("@{}", capture.name)),
                        paint(dim, capture.kind),
                        capture.text.trim()
                    )?;
                }
            }
        }
    }

    writeln!(
        out,
        "\n  {}",
        paint(
            dim,
            match matched {
                0 => "nothing matched".to_owned(),
                1 => "1 match".to_owned(),
                many => format!("{many} matches"),
            }
        )
    )?;
    Ok(matched)
}

pub fn language_named(name: &str) -> Result<Language> {
    Language::ALL
        .iter()
        .copied()
        .find(|language| language.slug() == name)
        .map_or_else(
            || {
                bail!(
                    "there is no language called `{name}`. rqx reads: {}",
                    Language::ALL
                        .iter()
                        .map(|language| language.slug())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
            Ok,
        )
}

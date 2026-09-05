use std::io::Write;

use anyhow::{Context, Result, bail};

use crate::brand::{Ink, Palette, paint};
use crate::setup::{Client, Home};

/// The snippet for one editor, and where it goes. Printing rather than writing is the
/// default because an editor's config is the user's file, not rqx's.
pub fn render(client: &Client, palette: Palette, out: &mut dyn Write) -> Result<()> {
    let dim = palette.style(Ink::Muted);

    writeln!(
        out,
        "\n  {}",
        paint(palette.style(Ink::Text).bold(), client.name)
    )?;

    match client.home {
        Home::Extension(how) => {
            writeln!(out, "  {}\n", paint(palette.style(Ink::Accent), how))?;
        }
        Home::Append(path) | Home::Merge(path) => {
            writeln!(out, "  {}\n", paint(palette.style(Ink::Accent), path))?;
        }
    }

    for line in client.snippet.lines() {
        writeln!(out, "    {line}")?;
    }

    writeln!(out)?;
    match client.home {
        Home::Append(_) => writeln!(
            out,
            "  {}",
            paint(dim, "`rqx setup <editor> --write` appends this for you.")
        )?,
        Home::Merge(_) => writeln!(
            out,
            "  {}",
            paint(
                dim,
                "Merge this in by hand: the file is one document, and a second copy of \
                 a key is an error rather than an override."
            )
        )?,
        Home::Extension(_) => writeln!(
            out,
            "  {}",
            paint(dim, "The extension needs no configuration to work.")
        )?,
    }
    writeln!(out)?;
    Ok(())
}

/// Every editor rqx knows how to set up, and what each one gets.
pub fn matrix(palette: Palette, out: &mut dyn Write) -> Result<()> {
    let dim = palette.style(Ink::Muted);
    writeln!(
        out,
        "\n  {}\n",
        paint(palette.style(Ink::Text).bold(), "Editors")
    )?;
    for client in Client::ALL {
        writeln!(
            out,
            "    {}  {}",
            paint(palette.style(Ink::Accent), format!("{:11}", client.slug)),
            client.name
        )?;
    }
    writeln!(
        out,
        "\n  {}\n",
        paint(
            dim,
            "`rqx setup <editor>` prints the config; every one of them speaks to `rqx lsp`."
        )
    )?;
    Ok(())
}

/// Appends the snippet, after showing what will be added. Refuses a file whose format
/// cannot take a second copy of a key, and never writes one twice.
pub fn write(client: &Client, palette: Palette, out: &mut dyn Write) -> Result<()> {
    if !client.appendable() {
        bail!(
            "rqx cannot write {}'s config for you: it is one document, so appending \
             would produce a duplicate key rather than a setting. Run `rqx setup {}` \
             and merge it in.",
            client.name,
            client.slug
        );
    }
    let Some(path) = client.path() else {
        bail!("rqx does not know where {} keeps its config", client.name);
    };

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.contains("rqx") {
        writeln!(
            out,
            "\n  {} already mentions rqx; nothing written.\n",
            paint(palette.style(Ink::Accent), path.display().to_string())
        )?;
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let separator = if existing.is_empty() || existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    std::fs::write(&path, format!("{existing}{separator}{}\n", client.snippet))
        .with_context(|| format!("writing {}", path.display()))?;

    writeln!(
        out,
        "\n  {} {}\n",
        paint(palette.badge(Ink::Good), " ADDED "),
        paint(palette.style(Ink::Accent), path.display().to_string())
    )?;
    Ok(())
}

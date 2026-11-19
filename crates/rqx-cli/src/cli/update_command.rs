//! `rqx update`: what is newer, and installing it.

use anyhow::Result;

use crate::brand::Palette;
use crate::{brand, release, update};

use crate::Exit;

pub(super) struct UpdateRequest {
    pub(super) check: bool,
    pub(super) notes: bool,
    pub(super) version: Option<String>,
    pub(super) dry_run: bool,
}

/// `rqx update`: reports what is newer, and installs it the way this copy was installed.
pub(super) fn update_command(request: &UpdateRequest, palette: Palette) -> Result<Exit> {
    let dim = palette.style(brand::Ink::Muted);
    let version = match &request.version {
        Some(named) => named.trim_start_matches('v').to_owned(),
        None => match announce_newest(request, palette)? {
            Newest::Install(version) => version,
            Newest::Done => return Ok(Exit::Clean),
        },
    };

    let installation = update::detect()?;
    if request.dry_run {
        print_plan(
            &update::plan(&installation, &version, release::source_override())?,
            palette,
        );
        return Ok(Exit::Clean);
    }

    match installation.method {
        update::Method::Managed(manager) => {
            anstream::println!(
                "  {}\n",
                brand::paint(dim, update::managed_update(&manager))
            );
            return Ok(Exit::Clean);
        }
        update::Method::CargoBuild => {
            anstream::println!(
                "  {}\n",
                brand::paint(dim, "building it with cargo; this takes a minute")
            );
            update::install(&version)?;
        }
        update::Method::Download => {
            let release = release::Release::published(&version)?;
            anstream::println!(
                "  {}\n",
                brand::paint(dim, "downloading it and checking it against SHA256SUMS")
            );
            release::install(&release, &installation.executable)?;
        }
    }
    anstream::println!(
        "\n  {}\n",
        brand::paint(
            palette.bold(brand::Ink::Good),
            format!("installed rqx {version}")
        )
    );
    Ok(Exit::Clean)
}

enum Newest {
    Install(String),
    Done,
}

/// Looks up the newest release and says what it is, for an update that was not
/// told which version to install. `--check` and `--notes` end here.
fn announce_newest(request: &UpdateRequest, palette: Palette) -> Result<Newest> {
    let dim = palette.style(brand::Ink::Muted);
    // Asked for directly, so the cached answer is not good enough and the
    // environment's opinion about CI does not apply.
    let Some(newer) = update::fetch()? else {
        anstream::println!(
            "\n  rqx {} is the newest release.\n",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(Newest::Done);
    };

    anstream::println!(
        "\n  {}",
        brand::paint(
            palette.bold(brand::Ink::Accent),
            format!("rqx {} is out", newer.version)
        )
    );
    anstream::println!(
        "  {}\n",
        brand::paint(dim, format!("running {}", env!("CARGO_PKG_VERSION")))
    );
    if request.notes || request.check {
        for line in newer.notes.lines() {
            anstream::println!("  {line}");
        }
        anstream::println!();
        return Ok(Newest::Done);
    }
    Ok(Newest::Install(newer.version))
}

fn print_plan(lines: &[(&str, String)], palette: Palette) {
    let dim = palette.style(brand::Ink::Muted);
    anstream::println!(
        "\n  {}",
        brand::paint(dim, "dry run: nothing is downloaded or changed")
    );
    for (label, value) in lines {
        anstream::println!("  {label:<10}{value}");
    }
    anstream::println!();
}

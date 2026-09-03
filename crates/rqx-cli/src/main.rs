use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use rqx_core::config::{CONFIG_FILE_NAME, RqxConfig};
use rqx_core::rule::RuleId;

mod cache;
mod check;
mod history;
mod lsp;
mod report;
mod sarif;
mod scan;
mod tui;

#[derive(Parser)]
#[command(name = "rqx", version, about = "Finds dead weight in TypeScript and says why it matters")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Print help instead of opening the interface when run with no arguments
    #[arg(long, global = true)]
    no_tui: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Analyze a project and report every finding
    Scan {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Terminal)]
        format: Format,
        /// Display cap; 0 shows every finding. Never affects the health score.
        #[arg(long)]
        max_findings: Option<usize>,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print a rule's taxonomy entry: the smell it maps to, its metric, and its fix
    Explain {
        rule: String,
    },
    /// List every rule Rqx can report, grouped by smell family
    Rules {
        /// Show only one family
        category: Option<String>,
    },
    /// Analyze changed files against a base revision and fail if the gate is not met
    Check {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Revision to compare against
        #[arg(long, default_value = "origin/HEAD")]
        base: String,
        #[arg(long, value_enum, default_value_t = Format::Terminal)]
        format: Format,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Run as a Language Server over stdio, for editor integration
    Lsp {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Open the interactive interface explicitly
    Tui {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Copy, Clone, ValueEnum)]
enum Format {
    Terminal,
    Json,
    Sarif,
}

fn load_config(root: &Path, explicit: Option<&Path>) -> Result<RqxConfig> {
    let path = match explicit {
        Some(path) => path.to_owned(),
        None => root.join(CONFIG_FILE_NAME),
    };
    if !path.exists() {
        if explicit.is_some() {
            bail!("config file `{}` does not exist", path.display());
        }
        return Ok(RqxConfig::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Falls back to help rather than an error: the user asked for the interface and
/// the reason they cannot have it is environmental, so the useful response is the
/// thing the interface would have shown them anyway.
fn decline_to_help(reason: &tui::Decline) -> Result<()> {
    use clap::CommandFactory;

    print!("{}", tui::static_header());
    println!("  interactive interface unavailable: {reason}\n");
    Cli::command().print_help()?;
    println!();
    Ok(())
}

fn open_tui(path: &Path, config: Option<&Path>) -> Result<()> {
    if !path.exists() {
        bail!("`{}` does not exist", path.display());
    }
    let settings = load_config(path, config)?;
    if let Err(reason) = tui::gate().and_then(|()| tui::launch(path, &settings)) {
        return decline_to_help(&reason);
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        return if cli.no_tui {
            decline_to_help(&tui::Decline::NotATerminal)
        } else {
            open_tui(Path::new("."), None)
        };
    };
    match command {
        Command::Scan { path, format, max_findings, threads, config } => {
            if !path.exists() {
                bail!("`{}` does not exist", path.display());
            }
            let mut settings = load_config(&path, config.as_deref())?;
            if let Some(max) = max_findings {
                settings.scan.max_findings = max;
            }
            if let Some(count) = threads {
                settings.scan.threads = count;
            }
            let outcome = scan::scan(&path, &settings)?;
            match format {
                Format::Terminal => report::render_terminal(&outcome),
                Format::Json => report::render_json(&outcome),
                Format::Sarif => {
                    let findings: Vec<&_> = outcome.findings.iter().collect();
                    println!("{}", sarif::render(&findings)?);
                    Ok(())
                }
            }
        }
        Command::Explain { rule } => {
            let Some(rule) = RuleId::parse(&rule) else {
                bail!(
                    "unknown rule `{rule}`. Known rules: {}",
                    RuleId::ALL.iter().map(|rule| rule.slug()).collect::<Vec<_>>().join(", ")
                );
            };
            report::explain(rule)
        }
        Command::Check { path, base, format, config } => {
            if !path.exists() {
                bail!("`{}` does not exist", path.display());
            }
            let settings = load_config(&path, config.as_deref())?;
            let review = check::review(&path, &settings, &base)?;
            let verdict = check::judge(&review, &settings);
            match format {
                Format::Terminal => report::render_review(&review, &verdict)?,
                Format::Json => report::render_review_json(&review, &verdict)?,
                Format::Sarif => {
                    let findings: Vec<&_> = review.findings().collect();
                    println!("{}", sarif::render(&findings)?);
                }
            }
            // A failing gate has to be visible to CI, and CI only reads the exit code.
            if verdict.passed() { Ok(()) } else { std::process::exit(1) }
        }
        Command::Lsp { path, config } => {
            let settings = load_config(&path, config.as_deref())?;
            lsp::serve(&path, settings).map_err(|error| anyhow::anyhow!("{error}"))
        }
        Command::Tui { path } => open_tui(&path, None),
        Command::Rules { category } => report::render_rules(category.as_deref()),
    }
}

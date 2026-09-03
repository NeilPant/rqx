use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rqx_core::config::{CONFIG_FILE_NAME, GateSeverity, RqxConfig};
use rqx_core::finding::Severity;
use rqx_core::rule::RuleId;

mod brand;
mod cache;
mod check;
mod history;
mod lsp;
mod report;
mod scan;
mod tui;

use brand::{ColorDepth, Palette};
use report::{FindingFilter, Layout, Report, Reporter};

#[derive(Parser)]
#[command(
    name = "rqx",
    version,
    about = "Finds dead weight in TypeScript and says why it matters",
    after_help = "Exit codes: 0 clean, 1 gate failed, 2 rqx could not run."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Print help instead of opening the interface when run with no arguments
    #[arg(long, global = true)]
    no_tui: bool,
    /// When to colour output; `auto` follows NO_COLOR and whether stdout is a terminal
    #[arg(long, value_enum, default_value_t = Color::Auto, global = true)]
    color: Color,
}

#[derive(Subcommand)]
enum Command {
    /// Analyze a project and report every finding
    Scan {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        view: View,
        #[command(flatten)]
        gate: Gate,
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
        /// Print what Rqx does about every entry in both canonical catalogues
        #[arg(long)]
        coverage: bool,
    },
    /// List every refactoring both catalogues name, and which rules prescribe it
    Techniques,
    /// Analyze changed files against a base revision and fail if the gate is not met
    Check {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Revision to compare against
        #[arg(long, default_value = "origin/HEAD")]
        base: String,
        #[command(flatten)]
        view: View,
        #[command(flatten)]
        gate: Gate,
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
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

/// Every option here narrows the report and none of them touch the scan, the health
/// score or the gate: a display flag that changed the verdict would make a CI result
/// depend on how it was printed.
#[derive(Args, Default)]
struct View {
    /// Show only findings at or above this severity
    #[arg(long, value_enum)]
    severity: Option<SeverityArg>,
    /// Show only this rule; repeatable
    #[arg(long = "rule", value_name = "SLUG")]
    rules: Vec<String>,
    /// Show only this smell family; repeatable
    #[arg(long = "category", value_name = "FAMILY")]
    categories: Vec<String>,
    /// Show only findings in files matching this glob; repeatable
    #[arg(long = "path", value_name = "GLOB")]
    paths: Vec<String>,
    /// List files that have no findings as well
    #[arg(long)]
    show_passed: bool,
    /// Print the summary without the findings
    #[arg(short, long)]
    quiet: bool,
    /// Where a report goes: `<name>` for stdout, `<name>=<path>` for a file; repeatable
    #[arg(long = "reporter", value_name = "NAME[=PATH]")]
    reporters: Vec<Sink>,
    #[arg(long, value_enum, hide = true)]
    format: Option<Reporter>,
}

/// Overrides for `[gate]` in `rqx.toml`. The same repository often wants a strict
/// gate on its default branch and a lenient one everywhere else, which a committed
/// config file cannot express.
#[derive(Args)]
struct Gate {
    /// Lowest severity that fails the run
    #[arg(long, value_enum)]
    fail_on: Option<FailOn>,
    /// Findings allowed before the run fails
    #[arg(long)]
    allow_findings: Option<u32>,
    /// Health score below which the run fails; 0 disables
    #[arg(long)]
    min_health: Option<u32>,
}

impl Gate {
    fn is_set(&self) -> bool {
        self.fail_on.is_some() || self.allow_findings.is_some() || self.min_health.is_some()
    }

    fn apply(&self, config: &mut RqxConfig) {
        if let Some(fail_on) = self.fail_on {
            config.gate.fail_on = fail_on.into();
        }
        if let Some(allowed) = self.allow_findings {
            config.gate.max_findings = allowed;
        }
        if let Some(health) = self.min_health {
            config.gate.min_health = health;
        }
    }
}

#[derive(Copy, Clone, ValueEnum)]
enum SeverityArg {
    Info,
    Warning,
    Error,
}

impl From<SeverityArg> for Severity {
    fn from(value: SeverityArg) -> Self {
        match value {
            SeverityArg::Info => Self::Info,
            SeverityArg::Warning => Self::Warning,
            SeverityArg::Error => Self::Error,
        }
    }
}

#[derive(Copy, Clone, ValueEnum)]
enum FailOn {
    Info,
    Warning,
    Error,
    Never,
}

impl From<FailOn> for GateSeverity {
    fn from(value: FailOn) -> Self {
        match value {
            FailOn::Info => Self::Info,
            FailOn::Warning => Self::Warning,
            FailOn::Error => Self::Error,
            FailOn::Never => Self::Never,
        }
    }
}

#[derive(Copy, Clone, ValueEnum)]
enum Color {
    Auto,
    Always,
    Never,
}

impl Color {
    const fn choice(self) -> anstream::ColorChoice {
        match self {
            Self::Auto => anstream::ColorChoice::Auto,
            Self::Always => anstream::ColorChoice::Always,
            Self::Never => anstream::ColorChoice::Never,
        }
    }
}

/// Pairing a rendering with its destination in one value is what lets a single scan
/// produce the console output and every file CI wants.
#[derive(Clone)]
struct Sink {
    reporter: Reporter,
    path: Option<PathBuf>,
}

impl FromStr for Sink {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        // Split on the first `=` so a Windows path keeps its drive letter.
        let (name, path) = match text.split_once('=') {
            Some((name, path)) => (name, Some(PathBuf::from(path))),
            None => (text, None),
        };
        let reporter = Reporter::from_str(name, true)
            .map_err(|_| format!("unknown reporter `{name}`"))?;
        Ok(Self { reporter, path })
    }
}

/// A document interleaved with anything else on one stream is silent corruption, so
/// it is rejected before a byte is written rather than discovered by whatever tries
/// to parse the result.
fn sinks(view: &View) -> Result<Vec<Sink>> {
    let mut sinks = view.reporters.clone();
    if let Some(reporter) = view.format {
        sinks.push(Sink { reporter, path: None });
    }
    // A run that writes only files would otherwise print nothing at all, which
    // reads as a hang in CI and as a failure locally.
    if sinks.iter().all(|sink| sink.path.is_some()) {
        sinks.push(Sink { reporter: Reporter::Terminal, path: None });
    }

    let to_stdout: Vec<Reporter> =
        sinks.iter().filter(|sink| sink.path.is_none()).map(|sink| sink.reporter).collect();
    if let Some(document) = to_stdout.iter().find(|reporter| reporter.is_document())
        && to_stdout.len() > 1
    {
        let other = to_stdout
            .iter()
            .find(|reporter| *reporter != document)
            .expect("more than one sink writes to stdout");
        bail!(
            "`{}` is a document and cannot share stdout with `{}`; give one of them a path",
            document.name(),
            other.name()
        );
    }
    Ok(sinks)
}

fn emit(report: &Report<'_>, sinks: &[Sink], view: &View, palette: Palette) -> Result<()> {
    for sink in sinks {
        let plain = Layout {
            palette: Palette::with_depth(ColorDepth::Monochrome),
            show_passed: view.show_passed,
            quiet: view.quiet,
        };
        match &sink.path {
            Some(path) => {
                let mut file = std::fs::File::create(path)
                    .with_context(|| format!("writing {}", path.display()))?;
                report::emit(report, sink.reporter, &plain, &mut file)?;
                file.flush()?;
            }
            None if sink.reporter.is_coloured() => {
                let layout = Layout { palette, ..plain };
                let mut out = anstream::stdout().lock();
                report::emit(report, sink.reporter, &layout, &mut out)?;
            }
            None => {
                let mut out = std::io::stdout().lock();
                report::emit(report, sink.reporter, &plain, &mut out)?;
            }
        }
    }
    Ok(())
}

fn filter_of(view: &View) -> Result<FindingFilter> {
    FindingFilter::new(
        view.severity.map(Severity::from),
        &view.rules,
        &view.categories,
        &view.paths,
    )
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

fn open_tui(path: &Path, config: Option<&Path>) -> Result<i32> {
    if !path.exists() {
        bail!("`{}` does not exist", path.display());
    }
    let settings = load_config(path, config)?;
    match tui::gate().and_then(|()| tui::launch(path, &settings)) {
        Err(reason) => decline_to_help(&reason).map(|()| 0),
        Ok(None) => Ok(0),
        Ok(Some(tui::Chosen::Review)) => {
            let review = check::review(path, &settings, "origin/HEAD")?;
            let verdict = check::judge(&review, &settings);
            let filter = FindingFilter::default();
            let report = Report::review(&review, &verdict, &filter);
            let view = View::default();
            emit(&report, &[Sink { reporter: Reporter::Terminal, path: None }], &view, Palette::detect())?;
            Ok(i32::from(!verdict.passed()))
        }
        Ok(Some(tui::Chosen::Rules)) => report::rules(None, Palette::detect()).map(|()| 0),
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("Error: {error:#}");
            // Distinct from a failed gate: CI has to be able to tell "your code has
            // problems" from "rqx could not run".
            std::process::exit(2);
        }
    }
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    // Set before anything draws: `anstream::stdout()` and `Palette::detect()` both
    // read this global, so one flag governs every writer.
    cli.color.choice().write_global();
    let palette = Palette::detect();

    let Some(command) = cli.command else {
        return if cli.no_tui {
            decline_to_help(&tui::Decline::NotATerminal).map(|()| 0)
        } else {
            open_tui(Path::new("."), None)
        };
    };

    match command {
        Command::Scan { path, view, gate, max_findings, threads, config } => {
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
            gate.apply(&mut settings);

            let sinks = sinks(&view)?;
            let filter = filter_of(&view)?;
            let outcome = scan::scan(&path, &settings)?;
            let report = Report::scan(&outcome, &filter);
            emit(&report, &sinks, &view, palette)?;

            // A bare `rqx scan` reports; it only gates when asked to, so adding the
            // flag is the whole opt-in and no existing pipeline changes meaning.
            Ok(if gate.is_set() && !check::judge_scan(&outcome, &settings).passed() { 1 } else { 0 })
        }
        Command::Explain { rule } => {
            let Some(rule) = RuleId::parse(&rule) else {
                bail!(
                    "unknown rule `{rule}`. Known rules: {}",
                    RuleId::ALL.iter().map(|rule| rule.slug()).collect::<Vec<_>>().join(", ")
                );
            };
            report::explain(rule, palette).map(|()| 0)
        }
        Command::Check { path, base, view, gate, config } => {
            if !path.exists() {
                bail!("`{}` does not exist", path.display());
            }
            let mut settings = load_config(&path, config.as_deref())?;
            gate.apply(&mut settings);

            let sinks = sinks(&view)?;
            let filter = filter_of(&view)?;
            let review = check::review(&path, &settings, &base)?;
            let verdict = check::judge(&review, &settings);
            let report = Report::review(&review, &verdict, &filter);
            emit(&report, &sinks, &view, palette)?;
            Ok(i32::from(!verdict.passed()))
        }
        Command::Lsp { path, config } => {
            let settings = load_config(&path, config.as_deref())?;
            lsp::serve(&path, settings).map_err(|error| anyhow::anyhow!("{error}"))?;
            Ok(0)
        }
        Command::Tui { path, config } => open_tui(&path, config.as_deref()),
        Command::Rules { category, coverage } => {
            if coverage {
                report::coverage(palette).map(|()| 0)
            } else {
                report::rules(category.as_deref(), palette).map(|()| 0)
            }
        }
        Command::Techniques => report::techniques(palette).map(|()| 0),
    }
}

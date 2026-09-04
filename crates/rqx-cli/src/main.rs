use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rqx_core::config::{GateGrade, GateSeverity, RqxConfig};
use rqx_core::finding::Severity;
use rqx_core::rule::RuleId;

mod brand;
mod cache;
mod check;
mod config;
mod history;
mod lsp;
mod preview;
mod report;
mod scan;
mod tui;

use brand::{ColorDepth, Palette};
use report::{FindingFilter, Layout, Report, Reporter};

#[derive(Parser)]
#[command(
    name = "rqx",
    version,
    about = "Finds dead weight in TypeScript, Python, Go, Java and C++ and says why it matters",
    after_help = AFTER_HELP,
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
    /// Animation in the interactive interface
    #[arg(long, value_enum, default_value_t = Motion::On, global = true)]
    motion: Motion,
}

/// Run on its own, `rqx` opens the interface, so the help a reader reaches is more
/// often about what else it can do than about what it is.
const AFTER_HELP: &str = "\
Examples:
  rqx                              open the interface on the working directory
  rqx scan .                       report every finding, exit 0 either way
  rqx scan . --fail-on error       report, and exit 1 if anything is an error
  rqx check --base main            review only what changed, and gate on it
  rqx explain unused-export        what one rule measures and how to fix it
  rqx init                         write an RQX.toml you can edit

Exit codes:
  0   ran, nothing failed the gate
  1   ran, the gate failed
  2   rqx could not run: bad path, bad config, unreadable revision
  101 rqx hit a bug; please report it
";

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
        /// Append what the design-pattern catalogue makes of the same files
        #[arg(long)]
        advice: bool,
        /// Display cap; 0 shows every finding. Never affects the health score.
        #[arg(long)]
        max_findings: Option<usize>,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print a rule's or a pattern's taxonomy entry: what it maps to, what decides it,
    /// and what fixes it
    Explain { name: String },
    /// List every rule Rqx can report, grouped by smell family
    Rules {
        /// Show only one family
        category: Option<String>,
        /// Print what Rqx does about every entry in both canonical catalogues
        #[arg(long)]
        coverage: bool,
        /// Show only the rules that apply to one language
        #[arg(long)]
        language: Option<String>,
    },
    /// List every refactoring both catalogues name, and which rules prescribe it
    Techniques,
    /// Report which design patterns a project uses, which are half applied, and where
    /// one would fit
    Patterns {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Print what rqx does about every pattern in the book, without scanning
        #[arg(long)]
        coverage: bool,
        /// Emit one JSON object per advisory instead of the reader's report
        #[arg(long)]
        json: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
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
    /// Regenerate the checked-in rule reference under docs/
    #[command(hide = true)]
    Docs {
        #[arg(long, default_value = "docs")]
        out: PathBuf,
        #[arg(long, default_value = "fixtures")]
        fixtures: PathBuf,
    },
    /// Write a commented RQX.toml with every setting at its default
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Replace an RQX.toml that is already there
        #[arg(long)]
        force: bool,
    },
    /// Print the settings a run will use, and the file each one came from
    Config {
        #[arg(default_value = ".")]
        path: PathBuf,
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
    /// Lines of code shown either side of a finding
    #[arg(long, default_value_t = 2)]
    context: u32,
    /// Where a report goes: `<name>` for stdout, `<name>=<path>` for a file; repeatable
    #[arg(long = "reporter", value_name = "NAME[=PATH]")]
    reporters: Vec<Sink>,
    #[arg(long, value_enum, hide = true)]
    format: Option<Reporter>,
}

/// Overrides for `[gate]` in `RQX.toml`. The same repository often wants a strict
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
    /// Lowest grade any single file may carry
    #[arg(long, value_enum)]
    min_grade: Option<MinGrade>,
    /// Fail when a design pattern is half applied
    #[arg(long)]
    fail_on_incomplete: bool,
}

#[derive(Copy, Clone, ValueEnum)]
enum MinGrade {
    Pristine,
    Clean,
    Fair,
    Poor,
    Critical,
    Never,
}

impl From<MinGrade> for GateGrade {
    fn from(value: MinGrade) -> Self {
        match value {
            MinGrade::Pristine => Self::Pristine,
            MinGrade::Clean => Self::Clean,
            MinGrade::Fair => Self::Fair,
            MinGrade::Poor => Self::Poor,
            MinGrade::Critical => Self::Critical,
            MinGrade::Never => Self::Never,
        }
    }
}

impl Gate {
    fn is_set(&self) -> bool {
        self.fail_on.is_some()
            || self.allow_findings.is_some()
            || self.min_health.is_some()
            || self.min_grade.is_some()
            || self.fail_on_incomplete
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
        if let Some(grade) = self.min_grade {
            config.gate.min_grade = grade.into();
        }
        if self.fail_on_incomplete {
            config.gate.fail_on_incomplete = true;
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

/// Motion carries progress and state changes; a terminal over a slow link, or a
/// reader who does not want it, turns it off without losing anything the screen says.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Motion {
    On,
    Off,
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
        let reporter = Reporter::from_str(name, true).map_err(|_| {
            let known: Vec<&str> = Reporter::ALL
                .iter()
                .map(|reporter| reporter.name())
                .collect();
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

/// A document interleaved with anything else on one stream is silent corruption, so
/// it is rejected before a byte is written rather than discovered by whatever tries
/// to parse the result.
fn sinks(view: &View) -> Result<Vec<Sink>> {
    let mut sinks = view.reporters.clone();
    if let Some(reporter) = view.format {
        sinks.push(Sink {
            reporter,
            path: None,
        });
    }
    // A run that writes only files would otherwise print nothing at all, which
    // reads as a hang in CI and as a failure locally.
    if sinks.iter().all(|sink| sink.path.is_some()) {
        sinks.push(Sink {
            reporter: Reporter::Terminal,
            path: None,
        });
    }

    let to_stdout: Vec<Reporter> = sinks
        .iter()
        .filter(|sink| sink.path.is_none())
        .map(|sink| sink.reporter)
        .collect();
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
            context: view.context,
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

/// Falls back to help rather than an error: the user asked for the interface and
/// the reason they cannot have it is environmental, so the useful response is the
/// thing the interface would have shown them anyway.
fn decline_to_help(
    reason: &tui::Decline,
    root: &str,
    settings: &RqxConfig,
    source: Option<&str>,
) -> Result<()> {
    use clap::CommandFactory;

    print!("{}", tui::static_header(root, settings, source));
    println!("  interactive interface unavailable: {reason}\n");
    Cli::command().print_help()?;
    println!();
    Ok(())
}

fn open_tui(path: &Path, config: Option<&Path>, motion: Motion) -> Result<Exit> {
    if !path.exists() {
        bail!(
            "`{}` does not exist. Give rqx a directory to open, or run `rqx` on its own \
             to open the working directory.",
            path.display()
        );
    }
    let settings = config::resolve(path, config)?;
    let source = settings.display_source();
    let root = path.display().to_string();
    let options = tui::Launch {
        path,
        config: &settings,
        reduced_motion: matches!(motion, Motion::Off),
    };
    match tui::gate().and_then(|()| tui::launch(options)) {
        Err(reason) => decline_to_help(&reason, &root, &settings.config, source.as_deref())
            .map(|()| Exit::Clean),
        Ok(()) => Ok(Exit::Clean),
    }
}

fn write_template(root: &Path, force: bool) -> Result<()> {
    let path = root.join(rqx_core::config::CONFIG_FILE_NAME);
    if path.exists() && !force {
        bail!(
            "`{}` is already there. Pass --force to replace it, or `rqx config` to see what \
             it resolves to.",
            path.display()
        );
    }
    std::fs::write(&path, rqx_core::config::TEMPLATE)
        .with_context(|| format!("writing {}", path.display()))?;
    println!(
        "\n  wrote {}\n  every value is the default; delete what you agree with.\n",
        path.display()
    );
    Ok(())
}

/// The whole contract a pipeline branches on. Named rather than returned as bare
/// integers so a new command cannot invent a fourth meaning by accident.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Exit {
    Clean = 0,
    GateFailed = 1,
    Failed = 2,
}

impl Exit {
    const fn of(failed: bool) -> Self {
        if failed {
            Self::GateFailed
        } else {
            Self::Clean
        }
    }
}

/// `rqx scan . | head` closes the reader mid-report. That is the reader's decision,
/// not a failure of the scan, and reporting it as one fails a pipeline that did
/// exactly what it was asked to.
fn reader_went_away(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
    })
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code as i32),
        Err(error) if reader_went_away(&error) => std::process::exit(Exit::Clean as i32),
        Err(error) => {
            eprintln!("Error: {error:#}");
            // Distinct from a failed gate: CI has to be able to tell "your code has
            // problems" from "rqx could not run".
            std::process::exit(Exit::Failed as i32);
        }
    }
}

fn run() -> Result<Exit> {
    let cli = Cli::parse();
    // Set before anything draws: `anstream::stdout()` and `Palette::detect()` both
    // read this global, so one flag governs every writer.
    cli.color.choice().write_global();
    let palette = Palette::detect();

    let Some(command) = cli.command else {
        return if cli.no_tui {
            decline_to_help(
                &tui::Decline::NotATerminal,
                ".",
                &RqxConfig::default(),
                None,
            )
            .map(|()| Exit::Clean)
        } else {
            open_tui(Path::new("."), None, cli.motion)
        };
    };

    match command {
        Command::Scan {
            path,
            view,
            gate,
            advice,
            max_findings,
            threads,
            config,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx scans a directory or a single file; try `rqx scan .`",
                    path.display()
                );
            }
            let mut settings = config::resolve(&path, config.as_deref())?;
            if let Some(max) = max_findings {
                settings.config.scan.max_findings = max;
            }
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            gate.apply(&mut settings.config);

            let sinks = sinks(&view)?;
            let filter = filter_of(&view)?;
            let outcome = scan::scan(&path, &settings)?;

            // A bare `rqx scan` reports; it only gates when asked to, so adding a flag
            // is the whole opt-in and no existing pipeline changes meaning.
            let verdict = gate
                .is_set()
                .then(|| check::judge_scan(&outcome, &settings.config));
            let report = Report::scan(&outcome, &filter);
            let report = match &verdict {
                Some(verdict) => report.judged(verdict),
                None => report,
            };
            emit(&report, &sinks, &view, palette)?;
            if advice {
                let mut out = anstream::stdout().lock();
                report::advice(&outcome, palette, &mut out)?;
            }
            Ok(Exit::of(verdict.is_some_and(|verdict| !verdict.passed())))
        }
        Command::Explain { name } => {
            if let Some(rule) = RuleId::parse(&name) {
                return report::explain(rule, palette).map(|()| Exit::Clean);
            }
            // An advisory slug resolves to its pattern: `strategy-fits` and `strategy`
            // are the same question asked two ways.
            let pattern = rqx_core::Pattern::parse(&name)
                .or_else(|| rqx_core::AdviceId::parse(&name).map(rqx_core::AdviceId::pattern));
            let Some(pattern) = pattern else {
                let mut names: Vec<&str> = RuleId::ALL.iter().map(|rule| rule.slug()).collect();
                names.extend(rqx_core::Pattern::ALL.iter().map(|p| p.slug()));
                names.extend(rqx_core::AdviceId::ALL.iter().map(|a| a.slug()));
                return Err(report::unknown(
                    "rule or pattern",
                    &name,
                    &names,
                    "Run `rqx rules` for the rules and `rqx patterns --coverage` for the patterns.",
                ));
            };
            report::explain_pattern(pattern, palette).map(|()| Exit::Clean)
        }
        Command::Patterns {
            path,
            coverage,
            json,
            threads,
            config,
        } => {
            if coverage {
                return report::pattern_coverage(palette).map(|()| Exit::Clean);
            }
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx reads a directory or a single file; try                      `rqx patterns .`",
                    path.display()
                );
            }
            let mut settings = config::resolve(&path, config.as_deref())?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            let outcome = scan::scan(&path, &settings)?;
            let mut out = anstream::stdout().lock();
            if json {
                return report::advice_json(&outcome, &mut out).map(|()| Exit::Clean);
            }
            report::advice(&outcome, palette, &mut out).map(|()| Exit::Clean)
        }
        Command::Docs { out, fixtures } => report::docs(&out, &fixtures).map(|()| Exit::Clean),
        Command::Init { path, force } => write_template(&path, force).map(|()| Exit::Clean),
        Command::Config { path, config } => {
            let settings = config::resolve(&path, config.as_deref())?;
            report::config(&settings, palette).map(|()| Exit::Clean)
        }
        Command::Check {
            path,
            base,
            view,
            gate,
            config,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx checks a directory or a single file; try `rqx check .`",
                    path.display()
                );
            }
            let mut settings = config::resolve(&path, config.as_deref())?;
            gate.apply(&mut settings.config);

            let sinks = sinks(&view)?;
            let filter = filter_of(&view)?;
            let review = check::review(&path, &settings, &base)?;
            let verdict = check::judge(&review, &settings.config);
            let report = Report::review(&review, &verdict, &filter);
            emit(&report, &sinks, &view, palette)?;
            Ok(Exit::of(!verdict.passed()))
        }
        Command::Lsp { path, config } => {
            let settings = config::resolve(&path, config.as_deref())?;
            lsp::serve(&path, settings).map_err(|error| anyhow::anyhow!("{error}"))?;
            Ok(Exit::Clean)
        }
        Command::Tui { path, config } => open_tui(&path, config.as_deref(), cli.motion),
        Command::Rules {
            category,
            coverage,
            language,
        } => {
            if coverage {
                report::coverage(palette).map(|()| Exit::Clean)
            } else {
                report::rules(category.as_deref(), language.as_deref(), palette)
                    .map(|()| Exit::Clean)
            }
        }
        Command::Techniques => report::techniques(palette).map(|()| Exit::Clean),
    }
}

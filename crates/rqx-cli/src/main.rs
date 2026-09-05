use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rqx_core::config::{GateGrade, GateSeverity, RqxConfig};
use rqx_core::finding::Severity;
use rqx_core::rule::RuleId;

mod baseline;
mod brand;
mod cache;
mod check;
mod complexity;
mod config;
mod edit_config;
mod fix;
mod graph;
mod health;
mod history;
mod lsp;
mod mcp;
mod perf;
mod plan;
mod preview;
mod progress;
mod query;
mod recommend;
mod report;
mod scan;
mod serve;
mod setup;
mod slop;
mod tui;
mod verify;

use brand::{ColorDepth, Palette};
use report::{FindingFilter, Layout, Report, Reporter};

#[derive(Parser)]
#[command(
    name = "rqx",
    version,
    about = "Finds dead weight in TypeScript, Python, Go, Java and C++ and says why it matters",
    after_help = AFTER_HELP,
)]
pub struct Cli {
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
        standard: Standard,
        #[command(flatten)]
        view: View,
        #[command(flatten)]
        gate: Gate,
        /// Append what the design-pattern catalogue makes of the same files
        #[arg(long)]
        advice: bool,
        /// A coverage file, so a test run can contradict a dead-code finding.
        /// lcov, a Go cover profile and coverage.py JSON are read by content.
        #[arg(long)]
        coverage: Vec<PathBuf>,
        /// A CPU profile, so a run can contradict a performance finding. A V8
        /// .cpuprofile, a Go pprof and collapsed stacks are read by content.
        #[arg(long)]
        profile: Vec<PathBuf>,
        /// Display cap; 0 shows every finding. Never affects the health score.
        #[arg(long)]
        max_findings: Option<usize>,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Apply the fixes a finding decides on its own, proving each one before it is
    /// written. Previews by default.
    Fix {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Write the fixes. Without it nothing on disk changes.
        #[arg(long, short)]
        write: bool,
        /// One fix can make the next one possible, so fixing repeats until a round
        /// finds nothing. Overrides `[fix] max_rounds`.
        #[arg(long)]
        max_rounds: Option<u32>,
        /// Emit one JSON object per fixed file instead of the reader's preview
        #[arg(long)]
        json: bool,
        /// Report the findings `fix` will not touch, with the span, the source, the
        /// metric and the named refactoring, for a person or an agent to act on
        #[arg(long)]
        plan: bool,
        /// With --plan, emit markdown a person can paste rather than a report to read
        #[arg(long)]
        markdown: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Compare the working tree against a base revision and say what happened to
    /// each finding: resolved, improved, relocated, or introduced
    Verify {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Revision to compare against
        #[arg(long, default_value = "origin/HEAD")]
        base: String,
        /// Rules that must be resolved, as `rule` or `rule@file`. Repeatable.
        #[arg(long)]
        expect: Vec<String>,
        /// Emit one JSON object per change instead of the reader's report
        #[arg(long)]
        json: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print the configuration one editor needs to talk to rqx, or list them all
    Setup {
        /// Which editor. Omit to list every one rqx knows.
        editor: Option<String>,
        /// Append it to the editor's config, after showing what will be added
        #[arg(long, short)]
        write: bool,
    },
    /// Serve a live dashboard for this project in a browser
    Serve {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Address to bind. Loopback only, and port 0 lets the OS pick one.
        #[arg(long, default_value = "127.0.0.1:0")]
        addr: std::net::SocketAddr,
        /// Milliseconds between checks for a changed file
        #[arg(long, default_value_t = 500)]
        interval: u64,
        /// Open the dashboard in your browser
        #[arg(long)]
        open: bool,
        /// Where "open in editor" links go. `{path}`, `{line}` and `{column}` are
        /// filled in; the default opens VS Code.
        #[arg(long, default_value = "vscode://file/{path}:{line}:{column}")]
        editor_url: String,
        /// Serve the page without its fix, waive, disable and baseline buttons
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Rank the work by what it buys against what it costs, and say what the health
    /// score would be after it
    Plan {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Effort points to spend. Without it the plan is the top items by value.
        #[arg(long)]
        budget: Option<u32>,
        /// How many items to rank when no budget is given
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Also print what the plan does to the grade of every file
        #[arg(long)]
        simulate: bool,
        /// Emit one JSON object per item instead of the reader's plan
        #[arg(long)]
        json: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print a rule's or a pattern's taxonomy entry: what it maps to, what decides it,
    /// and what fixes it
    Explain { name: String },
    /// List every rule Rqx can report, grouped by family
    Rules {
        /// Show only one family
        category: Option<String>,
        /// Show only the rules that apply to one language
        #[arg(long)]
        language: Option<String>,
    },
    /// Record the findings this project already has, so a gate can judge only what a
    /// change adds
    Baseline {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Take it again over the current findings. Refuses to record more than the
        /// last one did unless --force.
        #[arg(long)]
        update: bool,
        /// Record a larger set than the last one held
        #[arg(long)]
        force: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// List every finding a comment in the source waives, with the reason given
    Suppressions {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Show only the waivers that carry an `until` date
        #[arg(long)]
        expiring: bool,
        /// Emit one JSON object per waiver instead of the reader's report
        #[arg(long)]
        json: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// What the analyser measured, as a distribution rather than only its tail
    Complexity {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// One row per function, per file, or per directory
        #[arg(long, value_enum, default_value_t = complexity::By::Function)]
        by: complexity::By,
        #[arg(long, value_enum, default_value_t = complexity::Sort::Complexity)]
        sort: complexity::Sort,
        /// How many rows
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Skip anything below this, on whatever `--sort` measures
        #[arg(long, default_value_t = 0)]
        min: u32,
        /// Draw the distribution as well as the worst of it
        #[arg(long)]
        histogram: bool,
        /// Emit one JSON object per row
        #[arg(long)]
        json: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Where the work grows faster than the data, ranked by reach
    Perf {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// One row per function, per file, or per directory
        #[arg(long, value_enum, default_value_t = perf::By::Function)]
        by: perf::By,
        #[arg(long, value_enum, default_value_t = perf::Sort::Weight)]
        sort: perf::Sort,
        /// How many rows
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Skip anything whose loops nest less deeply than this
        #[arg(long, default_value_t = 0)]
        min_depth: u32,
        /// A CPU profile, so a finding nothing ran stops counting. A V8
        /// .cpuprofile, a Go pprof and collapsed stacks are read by content.
        #[arg(long, value_name = "FILE")]
        profile: Vec<PathBuf>,
        /// Emit one JSON object per row
        #[arg(long)]
        json: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print a shell completion script
    Completions {
        /// bash, zsh, fish, elvish or powershell
        shell: clap_complete::Shell,
    },
    /// What to do about this project, as distinct from what is wrong with it
    Recommend {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Just one section
        #[arg(long, value_enum)]
        only: Option<recommend::Only>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Which files read as written rather than read
    Slop {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Skip anything scoring below this
        #[arg(long, default_value_t = 1)]
        min: u32,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Where the health score comes from, and what removing each part would buy
    Health {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        #[arg(long, value_enum, default_value_t = health::By::Family)]
        by: health::By,
        /// Emit one JSON object per bucket
        #[arg(long)]
        json: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print the import graph — as text, Graphviz, Mermaid or data
    Graph {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        #[arg(long, value_enum, default_value_t = graph::Shape::Text)]
        format: graph::Shape,
        /// Only the edges that close a cycle
        #[arg(long)]
        cycles: bool,
        /// Only what this file reaches, and what reaches it
        #[arg(long, value_name = "FILE")]
        focus: Option<String>,
        /// How far from `--focus` to follow
        #[arg(long, default_value_t = 2)]
        depth: u32,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Show what the analysis cache holds, or throw it away
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Run a tree-sitter query over a project, to try one out before writing it into
    /// a `[[rule]]`
    Query {
        /// typescript, python, go, java or cpp
        language: String,
        /// The query, in tree-sitter's s-expression syntax
        query: String,
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Show every capture of every match, not just where it starts
        #[arg(long)]
        captures: bool,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// List every refactoring Rqx prescribes, and which rules ask for it
    Techniques,
    /// Print what each cleanliness level changes, as the settings it would take to
    /// write it by hand
    Levels,
    /// Report which design patterns a project uses, which are half applied, and where
    /// one would fit
    Patterns {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
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
        #[command(flatten)]
        standard: Standard,
        /// Revision to compare against
        #[arg(long, default_value = "origin/HEAD")]
        base: String,
        /// Apply the mechanical fixes to what the branch touched before gating, so a
        /// branch whose only failures were mechanical passes
        #[arg(long)]
        fix: bool,
        /// With --fix, write the fixes. Without it nothing on disk changes.
        #[arg(long, short)]
        write: bool,
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
        #[command(flatten)]
        standard: Standard,
        #[arg(long)]
        config: Option<PathBuf>,
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Run as an MCP server, so an AI agent can ask what a project's problems are
    Mcp {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Serve Streamable HTTP on this address instead of speaking over stdio
        #[arg(long, value_name = "ADDR")]
        http: Option<std::net::SocketAddr>,
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
/// A named set of defaults, merged under whatever the project wrote. Declared here
/// rather than only in the config file because CI usually wants a stricter standard
/// on the default branch than a working copy does.
#[derive(Args, Default)]
struct Standard {
    /// relaxed, standard, strict or pristine
    #[arg(long, value_name = "LEVEL")]
    level: Option<String>,
}

impl Standard {
    fn chosen(&self) -> Result<Option<rqx_core::config::Level>> {
        let Some(name) = self.level.as_deref() else {
            return Ok(None);
        };
        rqx_core::config::Level::parse(name)
            .map(Some)
            .ok_or_else(|| {
                let names: Vec<&str> = rqx_core::config::Level::ALL
                    .iter()
                    .map(|level| level.slug())
                    .collect();
                report::unknown(
                    "level",
                    name,
                    &names,
                    "Levels set the defaults; anything you write wins over them.",
                )
            })
    }
}

#[derive(Copy, Clone, Default, ValueEnum)]
enum GroupArg {
    #[default]
    File,
    Rule,
    Family,
    Severity,
}

impl From<GroupArg> for report::Grouping {
    fn from(chosen: GroupArg) -> Self {
        match chosen {
            GroupArg::File => Self::File,
            GroupArg::Rule => Self::Rule,
            GroupArg::Family => Self::Family,
            GroupArg::Severity => Self::Severity,
        }
    }
}

#[derive(Copy, Clone, Default, ValueEnum)]
enum OrderArg {
    #[default]
    Line,
    Severity,
    File,
}

impl From<OrderArg> for report::Order {
    fn from(chosen: OrderArg) -> Self {
        match chosen {
            OrderArg::Line => Self::Line,
            OrderArg::Severity => Self::Severity,
            OrderArg::File => Self::File,
        }
    }
}

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
    /// Print how long each stage of the scan took
    #[arg(long)]
    timings: bool,
    /// Counts only: which rules and families fired, and how often
    #[arg(long)]
    stats: bool,
    /// What the listing is a list of
    #[arg(long = "group-by", value_enum, default_value_t = GroupArg::File)]
    grouping: GroupArg,
    /// What decides the order inside each group
    #[arg(long = "sort", value_enum, default_value_t = OrderArg::Line)]
    order: OrderArg,
    /// List only this many groups; the rest are counted
    #[arg(long, value_name = "N")]
    top: Option<usize>,
    /// Show only findings in files this revision changed
    #[arg(long, value_name = "REF")]
    since: Option<String>,
    /// Show only findings in files not yet committed
    #[arg(long)]
    changed: bool,
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
#[derive(Subcommand)]
enum ConfigAction {
    /// Write one setting, keeping the file's comments and layout
    Set {
        /// The setting, as `table.key` — `thresholds.max_params`, `gate.fail_on`
        key: String,
        value: String,
        /// Which file to write. Without it, the file that already holds the setting,
        /// else the nearest `RQX.toml`.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Choose the file from the ones this project reads
        #[arg(long, conflicts_with = "file")]
        pick: bool,
    },
    /// Remove one setting, so it falls back to the level or the default
    Unset {
        key: String,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        pick: bool,
    },
    /// List the files this project's settings come from, nearest last
    Files,
}

#[derive(Subcommand)]
enum CacheAction {
    /// What the cache holds, and whether the next scan can use it
    Status {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Throw the cache away, leaving everything else under `.rqx` alone
    Clear {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

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

impl From<ConfigAction> for edit_config::Change {
    fn from(action: ConfigAction) -> Self {
        match action {
            ConfigAction::Set {
                key,
                value,
                file,
                pick,
            } => Self::Set {
                key,
                value,
                file,
                pick,
            },
            ConfigAction::Unset { key, file, pick } => Self::Unset { key, file, pick },
            ConfigAction::Files => Self::Files,
        }
    }
}

fn emit(report: &Report<'_>, sinks: &[Sink], view: &View, palette: Palette) -> Result<()> {
    if view.timings {
        report::timings(report.outcome, palette, &mut anstream::stdout().lock())?;
    }
    for sink in sinks {
        let plain = Layout {
            grouping: view.grouping.into(),
            order: view.order.into(),
            top: view.top,
            palette: Palette::with_depth(ColorDepth::Monochrome),
            show_passed: view.show_passed,
            stats: view.stats,
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

fn filter_of(view: &View, root: &Path) -> Result<FindingFilter> {
    let filter = FindingFilter::new(
        view.severity.map(Severity::from),
        &view.rules,
        &view.categories,
        &view.paths,
    )?;
    let mut touched: Option<Vec<String>> = None;
    if let Some(base) = &view.since {
        touched = Some(history::changed_since(root, base).ok_or_else(|| {
            anyhow::anyhow!("`{base}` is not a revision this repository knows about")
        })?);
    }
    if view.changed {
        touched
            .get_or_insert_with(Vec::new)
            .extend(history::uncommitted(root));
    }
    Ok(match touched {
        Some(files) => filter.touching(files),
        None => filter,
    })
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
    let settings = config::resolve(path, config, None)?;
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
    progress::set_animated(!matches!(cli.motion, Motion::Off));
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
            standard,
            view,
            gate,
            advice,
            coverage,
            profile,
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
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(max) = max_findings {
                settings.config.scan.max_findings = max;
            }
            if !coverage.is_empty() {
                settings.config.scan.coverage = coverage
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
            }
            if !profile.is_empty() {
                settings.config.scan.profile = profile
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
            }
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            gate.apply(&mut settings.config);

            let sinks = sinks(&view)?;
            let filter = filter_of(&view, &path)?;
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
        Command::Fix {
            path,
            standard,
            write,
            max_rounds,
            json,
            plan,
            markdown,
            threads,
            config,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx fixes a directory or a single file; try `rqx fix .`",
                    path.display()
                );
            }
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            let mut out = anstream::stdout().lock();
            if plan {
                let scanned = scan::scan(&path, &settings)?;
                return if json {
                    report::brief_json(&scanned, &settings, &mut out)
                } else if markdown {
                    report::brief_markdown(&scanned, &settings, &mut out)
                } else {
                    report::brief(&scanned, &settings, palette, &mut out)
                }
                .map(|()| Exit::Clean);
            }
            let rounds = max_rounds.unwrap_or(settings.config.fix.max_rounds).max(1);
            let outcome = fix::run(
                &path,
                &settings,
                fix::Options {
                    write,
                    max_rounds: rounds,
                    ..fix::Options::default()
                },
            )?;
            if json {
                return report::fixes_json(&outcome, &mut out).map(|()| Exit::Clean);
            }
            report::fixes(&outcome, palette, "rqx fix --write", &mut out).map(|()| Exit::Clean)
        }
        Command::Verify {
            path,
            standard,
            base,
            expect,
            json,
            threads,
            config,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx verifies a directory; try `rqx verify .`",
                    path.display()
                );
            }
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            settings.config.scan.max_findings = 0;
            verify::ensure_repository(&path, &base)?;
            let verified = verify::verify(&path, &settings, &base, &expect)?;
            let mut out = anstream::stdout().lock();
            if json {
                report::verify_json(&verified, &mut out)?;
            } else {
                report::verify(&verified, palette, &mut out)?;
            }
            Ok(Exit::of(!verified.passed()))
        }
        Command::Setup { editor, write } => {
            let mut out = anstream::stdout().lock();
            let Some(name) = editor else {
                return report::editors(palette, &mut out).map(|()| Exit::Clean);
            };
            let Some(client) = setup::Client::parse(&name) else {
                let names: Vec<&str> = setup::Client::ALL.iter().map(|c| c.slug).collect();
                return Err(report::unknown(
                    "editor",
                    &name,
                    &names,
                    "Run `rqx setup` for the list. Anything speaking LSP can run `rqx lsp`.",
                ));
            };
            report::editor(client, palette, &mut out)?;
            if write {
                report::write_editor(client, palette, &mut out)?;
            }
            Ok(Exit::Clean)
        }
        Command::Serve {
            path,
            standard,
            addr,
            interval,
            open,
            editor_url,
            read_only,
            config,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx serves a directory; try `rqx serve .`",
                    path.display()
                );
            }
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            // The page filters in the browser, so a cap here would hide findings the
            // reader is about to filter for.
            settings.config.scan.max_findings = 0;
            let options = serve::Options {
                address: addr,
                interval,
                open,
                editor_url,
                read_only,
                explicit_config: config,
            };
            serve::serve(&path, settings, options).map(|()| Exit::Clean)
        }
        Command::Plan {
            path,
            standard,
            budget,
            limit,
            simulate,
            json,
            threads,
            config,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx plans a directory or a single file; try `rqx plan .`",
                    path.display()
                );
            }
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            // The display cap keeps the highest-ranked findings, and the plan is a
            // ranking of its own: capping first would hide the cheap work entirely.
            settings.config.scan.max_findings = 0;
            let outcome = scan::scan(&path, &settings)?;
            let ranked = plan::rank(&outcome, budget, limit);
            let mut out = anstream::stdout().lock();
            if json {
                return report::plan_json(&outcome, &ranked, &mut out).map(|()| Exit::Clean);
            }
            report::plan(&outcome, &ranked, budget, simulate, palette, &mut out)
                .map(|()| Exit::Clean)
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
            standard,
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
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
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
        Command::Baseline {
            path,
            standard,
            update,
            force,
            threads,
            config,
        } => {
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            let taken = baseline::take(&path, &settings, update, force)?;
            println!(
                "  recorded {} findings in {}{}",
                taken.recorded,
                taken.path.display(),
                match taken.replaced {
                    Some(was) if was != taken.recorded => format!(" ({was} before)"),
                    _ => String::new(),
                }
            );
            if !taken.configured {
                println!(
                    "  set `[gate] baseline = \"{}\"` in RQX.toml to gate on new findings only",
                    baseline::DEFAULT_PATH
                );
            }
            Ok(Exit::Clean)
        }
        Command::Suppressions {
            path,
            standard,
            expiring,
            json,
            threads,
            config,
        } => {
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            // A waiver is only visible when the finding it covers was found, and the
            // display cap drops findings before anything can read them.
            settings.config.scan.max_findings = 0;
            let outcome = scan::scan(&path, &settings)?;
            let mut out = anstream::stdout().lock();
            if json {
                return report::waivers_json(&path, &outcome, &mut out).map(|()| Exit::Clean);
            }
            report::waivers(&path, &outcome, expiring, palette, &mut out).map(|()| Exit::Clean)
        }
        Command::Complexity {
            path,
            standard,
            by,
            sort,
            top,
            min,
            histogram,
            json,
            threads,
            config,
        } => {
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            let threshold = settings.config.thresholds.cognitive_complexity;
            let outcome = scan::scan(&path, &settings)?;
            let view = complexity::View {
                by,
                sort,
                top,
                min,
                histogram,
            };
            let mut out = anstream::stdout().lock();
            if json {
                return complexity::json(&outcome, &view, &mut out).map(|()| Exit::Clean);
            }
            complexity::render(&outcome, &view, threshold, palette, &mut out).map(|()| Exit::Clean)
        }
        Command::Perf {
            path,
            standard,
            by,
            sort,
            top,
            min_depth,
            profile,
            json,
            threads,
            config,
        } => {
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            // The display cap would drop findings this ranking has to weigh.
            settings.config.scan.max_findings = 0;
            if !profile.is_empty() {
                settings.config.scan.profile = profile
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
            }
            let threshold = settings.config.thresholds.loop_nesting;
            let outcome = scan::scan(&path, &settings)?;
            let view = perf::View {
                by,
                sort,
                top,
                min_depth,
            };
            let mut out = anstream::stdout().lock();
            if json {
                return perf::json(&outcome, &view, &mut out).map(|()| Exit::Clean);
            }
            perf::render(&outcome, &view, threshold, palette, &mut out).map(|()| Exit::Clean)
        }
        Command::Completions { shell } => {
            // Generated from the parser itself, so a flag added anywhere is
            // completable the moment it exists.
            let mut command = <Cli as clap::CommandFactory>::command();
            let name = command.get_name().to_owned();
            clap_complete::generate(shell, &mut command, name, &mut std::io::stdout());
            Ok(Exit::Clean)
        }
        Command::Recommend {
            path,
            standard,
            only,
            json,
            threads,
            config,
        } => {
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            settings.config.scan.max_findings = 0;
            let outcome = scan::scan(&path, &settings)?;
            let mut out = anstream::stdout().lock();
            if json {
                return recommend::json(&outcome, &settings, only, &mut out).map(|()| Exit::Clean);
            }
            recommend::render(&outcome, &settings, only, palette, &mut out).map(|()| Exit::Clean)
        }
        Command::Slop {
            path,
            standard,
            top,
            min,
            json,
            threads,
            config,
        } => {
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            settings.config.scan.max_findings = 0;
            // The signals are Info rules and several ship off by default, so the
            // command turns its own family on rather than reporting nothing.
            for (rule, _) in rqx_core::slop::SIGNALS {
                settings.config.rules.enabled.insert(rule.slug().to_owned());
            }
            let outcome = scan::scan(&path, &settings)?;
            let mut out = anstream::stdout().lock();
            if json {
                return slop::json(&outcome, top, min, &mut out).map(|()| Exit::Clean);
            }
            slop::render(&outcome, top, min, palette, &mut out).map(|()| Exit::Clean)
        }
        Command::Health {
            path,
            standard,
            by,
            json,
            threads,
            config,
        } => {
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            // Every finding, not the first five hundred: a bucket judged on a capped
            // list would understate whichever rule fires most.
            settings.config.scan.max_findings = 0;
            let outcome = scan::scan(&path, &settings)?;
            let mut out = anstream::stdout().lock();
            if json {
                return health::json(&outcome, by, &mut out).map(|()| Exit::Clean);
            }
            health::render(&outcome, by, palette, &mut out).map(|()| Exit::Clean)
        }
        Command::Graph {
            path,
            standard,
            format,
            cycles,
            focus,
            depth,
            threads,
            config,
        } => {
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            let outcome = scan::scan(&path, &settings)?;
            let view = graph::View {
                shape: format,
                cycles_only: cycles,
                focus: focus.as_deref(),
                depth,
            };
            let mut out = anstream::stdout().lock();
            graph::render(&outcome, &view, palette, &mut out).map(|()| Exit::Clean)
        }
        Command::Cache { action } => {
            match action {
                CacheAction::Status { path, config } => {
                    let settings = config::resolve(&path, config.as_deref(), None)?;
                    match cache::stored(&path, &settings) {
                        Some(stored) => {
                            println!("  {} files", stored.entries);
                            println!("  {} KB in {}", stored.bytes / 1024, stored.path.display());
                            if !stored.current {
                                println!(
                                    "  written under different settings, so the next scan rebuilds it"
                                );
                            }
                        }
                        None => println!("  nothing cached yet"),
                    }
                    if !settings.config.scan.cache {
                        println!("  the cache is off; `[scan] cache = true` turns it on");
                    }
                }
                CacheAction::Clear { path } => {
                    if cache::clear(&path)? {
                        println!("  cleared");
                    } else {
                        println!("  nothing to clear");
                    }
                }
            }
            Ok(Exit::Clean)
        }
        Command::Query {
            language,
            query,
            path,
            captures,
            config,
        } => {
            let settings = config::resolve(&path, config.as_deref(), None)?;
            let language = query::language_named(&language)?;
            let mut out = anstream::stdout().lock();
            query::run(
                &path, language, &query, &settings, captures, palette, &mut out,
            )
            .map(|_| Exit::Clean)
        }
        Command::Docs { out, fixtures } => report::docs(&out, &fixtures).map(|()| Exit::Clean),
        Command::Init { path, force } => write_template(&path, force).map(|()| Exit::Clean),
        Command::Config {
            path,
            standard,
            config,
            action,
        } => {
            let settings = config::resolve(&path, config.as_deref(), standard.chosen()?);
            match action {
                None => report::config(&settings?, palette).map(|()| Exit::Clean),
                Some(action) => {
                    let change = edit_config::Change::from(action);
                    let mut out = anstream::stdout().lock();
                    // A config that breaks its own policy will not resolve, and the
                    // command that removes the offending key is the way out of that —
                    // so a removal naming its file does not need the config to load.
                    match settings {
                        Ok(settings) => {
                            edit_config::run(&path, &settings, change, palette, &mut out)?;
                        }
                        Err(refused) => {
                            edit_config::force(&path, change, palette, &mut out)
                                .map_err(|_| refused)?;
                        }
                    }
                    Ok(Exit::Clean)
                }
            }
        }
        Command::Check {
            path,
            standard,
            base,
            fix,
            write,
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
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            gate.apply(&mut settings.config);

            let sinks = sinks(&view)?;
            let filter = filter_of(&view, &path)?;
            let mut review = check::review(&path, &settings, &base)?;

            // The gate is judged after the fixes, so a branch whose only failures were
            // mechanical passes rather than failing on work rqx could have done.
            if fix {
                let fixed = fix::run(
                    &path,
                    &settings,
                    fix::Options {
                        write,
                        max_rounds: settings.config.fix.max_rounds.max(1),
                        only: review.reviewed.clone(),
                    },
                )?;
                let mut out = anstream::stdout().lock();
                report::fixes(&fixed, palette, "rqx check --fix --write", &mut out)?;
                drop(out);
                if write && fixed.applied() > 0 {
                    review = check::review(&path, &settings, &base)?;
                }
            }
            let verdict = check::judge(&review, &settings.config);
            let report = Report::review(&review, &verdict, &filter);
            emit(&report, &sinks, &view, palette)?;
            Ok(Exit::of(!verdict.passed()))
        }
        Command::Mcp { path, http, config } => {
            let settings = config::resolve(&path, config.as_deref(), None)?;
            mcp::serve(&path, settings, http)?;
            Ok(Exit::Clean)
        }
        Command::Lsp { path, config } => {
            let settings = config::resolve(&path, config.as_deref(), None)?;
            lsp::serve(&path, settings).map_err(|error| anyhow::anyhow!("{error}"))?;
            Ok(Exit::Clean)
        }
        Command::Tui { path, config } => open_tui(&path, config.as_deref(), cli.motion),
        Command::Rules { category, language } => {
            report::rules(category.as_deref(), language.as_deref(), palette).map(|()| Exit::Clean)
        }
        Command::Techniques => report::techniques(palette).map(|()| Exit::Clean),
        Command::Levels => report::levels(palette).map(|()| Exit::Clean),
    }
}

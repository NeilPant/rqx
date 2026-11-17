use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use clap::{Args, FromArgMatches, Parser, Subcommand, ValueEnum, ValueHint};
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
mod forge;
mod graph;
mod health;
mod history;
mod lsp;
mod mcp;
mod perf;
mod plan;
mod preview;
mod progress;
mod project;
mod query;
mod recommend;
mod render;
mod report;
mod scan;
mod session;
mod setup;
mod slop;
mod stats;
mod term;
// The full-screen interface is no longer opened by anything; a later change deletes it.
#[allow(dead_code)]
mod tui;
mod update;
mod verify;
mod watch;

use brand::{ColorDepth, Palette};
use report::{FindingFilter, Layout, Report, Reporter};

#[derive(Parser)]
#[command(
    name = "rqx",
    version,
    about = "Finds dead weight in TypeScript, JavaScript, Python, Go, Java and C++ and says why it matters",
    // `{subcommands}` is left out and the grouped list put in `after_help` instead:
    // clap has no heading for a subcommand, and twenty-seven of them in one block is
    // a wall that gets worse with every one added. They stay visible to the parser
    // rather than hidden, because hiding them would group the help and silently
    // break shell completion, which reads the same flag.
    help_template = "\
{about}

{usage-heading} {usage}

{options}
{after-help}",
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Never open the interactive interface; bare `rqx` prints the report instead
    #[arg(long, global = true)]
    no_tui: bool,
    /// Leave the mouse to the terminal. The session never captures it; the flag is
    /// still accepted so an existing invocation keeps working.
    #[arg(long, global = true)]
    #[allow(dead_code)]
    no_mouse: bool,
    /// When to colour output; `auto` follows NO_COLOR and whether stdout is a terminal
    #[arg(long, value_enum, default_value_t = Color::Auto, global = true)]
    color: Color,
    /// Animation in the interactive interface
    #[arg(long, value_enum, default_value_t = Motion::On, global = true)]
    motion: Motion,
}

/// The commands, in five groups. Order inside a group is the order a reader meets
/// them, not alphabetical: `scan` before `check` before `fix` is the loop itself.
///
/// A test asserts every subcommand is in exactly one group and every name here is a
/// real subcommand, so a twenty-eighth command fails the build until it is placed.
const GROUPS: [(&str, &[&str]); 5] = [
    ("The loop", &["scan", "check", "fix", "verify", "comment"]),
    (
        "Look closer",
        &[
            "complexity",
            "perf",
            "health",
            "slop",
            "graph",
            "plan",
            "patterns",
            "recommend",
        ],
    ),
    ("Reference", &["rules", "explain", "techniques", "levels"]),
    (
        "Project",
        &["init", "config", "baseline", "suppressions", "cache"],
    ),
    (
        "Integrations",
        &["lsp", "mcp", "setup", "completions", "query", "update"],
    ),
];

/// The parser, with the grouped list attached. Attached here rather than declared
/// on the derive because building the list reads the parser's own descriptions, and
/// an attribute that asks for the command while the command is being built never
/// finishes.
pub fn parser() -> clap::Command {
    use clap::CommandFactory;

    let base = Cli::command();
    let grouped = grouped_help(&base);
    base.after_help(grouped)
}

/// Built from the parser rather than written out, so a description here cannot drift
/// from the one `rqx <command> --help` gives.
fn grouped_help(command: &clap::Command) -> String {
    let described: std::collections::HashMap<String, (String, String)> = command
        .get_subcommands()
        .map(|sub| {
            let alias = sub
                .get_visible_aliases()
                .next()
                .map(|alias| format!(", {alias}"))
                .unwrap_or_default();
            (
                sub.get_name().to_owned(),
                (
                    alias,
                    sub.get_about().map(ToString::to_string).unwrap_or_default(),
                ),
            )
        })
        .collect();

    let mut out = String::new();
    for (title, names) in GROUPS {
        out.push_str(title);
        out.push('\n');
        for name in names {
            let (alias, about) = described
                .get(*name)
                .map(|(alias, about)| (alias.as_str(), about.as_str()))
                .unwrap_or_default();
            // One line each: the first sentence is what the command is for, and the
            // rest is what `--help` on it is for.
            let first = about.split_once(". ").map_or(about, |(head, _)| head);
            let named = format!("{name}{alias}");
            out.push_str(&format!("  {named:<14}{}\n", first.trim_end_matches('.')));
        }
        out.push('\n');
    }
    out.push_str(
        "\
Run on its own in a terminal, `rqx` opens the interactive session on the working
directory; anywhere else it scans the working directory, as `rqx scan` does.
`scan` already reports import cycles, layer violations against a declared
[architecture], dead code, duplication and churn; the commands under Look closer
narrow that same scan to one question.

Examples:
  rqx                              the session in a terminal, the scan elsewhere
  rqx scan .                       print the whole report
  rqx scan . --fail-on error       report, and exit 1 if anything is an error
  rqx check --base main            review only what changed, and gate on it
  rqx explain unused-export        what one rule measures and how to fix it

Exit codes:
  0   ran, nothing failed the gate
  1   ran, the gate failed
  2   rqx could not run: bad path, bad config, unreadable revision
  101 rqx hit a bug; please report it
",
    );
    out
}

#[derive(Subcommand)]
enum Command {
    /// Analyze a project and report every finding
    #[command(visible_alias = "s")]
    #[command(
        after_help = "Examples:\n  rqx scan .                          the whole report\n  rqx scan . --rule empty-catch       only one rule\n  rqx scan . --not-path 'test/**'     everything except the tests\n  rqx scan . --fixable                only what `rqx fix` can act on\n  rqx scan . --stats                  which rules fired, and how often"
    )]
    Scan {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
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
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        coverage: Vec<PathBuf>,
        /// A CPU profile, so a run can contradict a performance finding. A V8
        /// .cpuprofile, a Go pprof and collapsed stacks are read by content.
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        profile: Vec<PathBuf>,
        /// Display cap; 0 shows every finding. Never affects the health score.
        #[arg(long)]
        max_findings: Option<usize>,
        /// Scan again whenever a file changes, until interrupted
        #[arg(long)]
        watch: bool,
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Apply the fixes a finding decides on its own, proving each one before it is
    /// written. Previews by default.
    #[command(visible_alias = "f")]
    #[command(
        after_help = "Examples:\n  rqx fix                             show what would change\n  rqx fix -w                          write it\n  rqx fix --plan                      the edits as a list, not a diff"
    )]
    Fix {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
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
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Compare the working tree against a base revision and say what happened to
    /// each finding: resolved, improved, relocated, or introduced
    #[command(visible_alias = "v")]
    Verify {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
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
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
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
    /// Rank the work by what it buys against what it costs, and say what the health
    /// score would be after it
    #[command(
        after_help = "Examples:\n  rqx plan --budget 20                the best twenty points of work\n  rqx plan --simulate                 the score each step would leave"
    )]
    Plan {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
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
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
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
        /// One slug per line and nothing else, for a script or a shell completion
        #[arg(long)]
        slugs: bool,
    },
    /// Record the findings this project already has, so a gate can judge only what a
    /// change adds
    #[command(
        after_help = "Examples:\n  rqx baseline                        snapshot today's debt\n  rqx baseline --update               after a cleanup, record the new floor"
    )]
    Baseline {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
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
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// List every finding a comment in the source waives, with the reason given
    Suppressions {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Show only the waivers that carry an `until` date
        #[arg(long)]
        expiring: bool,
        /// Emit one JSON object per waiver instead of the reader's report
        #[arg(long)]
        json: bool,
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// What the analyser measured, as a distribution rather than only its tail
    #[command(
        after_help = "Examples:\n  rqx complexity --top 20             the twenty worst functions\n  rqx complexity --by dir             the worst of each directory\n  rqx complexity --histogram          where the codebase actually sits"
    )]
    Complexity {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
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
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Where the work grows faster than the data, ranked by reach
    Perf {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
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
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        profile: Vec<PathBuf>,
        /// Emit one JSON object per row
        #[arg(long)]
        json: bool,
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Install the newest release, or say what it would change
    #[command(
        after_help = "Examples:\n  rqx update                          install the newest release\n  rqx update --check                  say whether one exists, install nothing\n  rqx update --notes                  read what changed"
    )]
    Update {
        /// Report whether a newer release exists and stop
        #[arg(long)]
        check: bool,
        /// Print the release notes for the newest release
        #[arg(long)]
        notes: bool,
    },
    /// Print a shell completion script
    Completions {
        /// bash, zsh, fish, elvish or powershell
        shell: clap_complete::Shell,
    },
    /// What to do about this project, as distinct from what is wrong with it
    #[command(
        after_help = "Examples:\n  rqx recommend                       all four answers\n  rqx recommend --only config         thresholds this codebase would fit"
    )]
    Recommend {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Just one section
        #[arg(long, value_enum)]
        only: Option<recommend::Only>,
        #[arg(long)]
        json: bool,
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Which files read as written rather than read
    Slop {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
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
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Where the health score comes from, and what removing each part would buy
    Health {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        #[arg(long, value_enum, default_value_t = health::By::Family)]
        by: health::By,
        /// Emit one JSON object per bucket
        #[arg(long)]
        json: bool,
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Print the import graph — as text, Graphviz, Mermaid or data
    #[command(
        after_help = "Examples:\n  rqx graph --cycles                  only the rings\n  rqx graph --format mermaid          paste it into a design doc\n  rqx graph --focus src/api.ts        what it reaches, and what reaches it"
    )]
    Graph {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        #[arg(long, value_enum, default_value_t = graph::Shape::Text)]
        format: graph::Shape,
        /// Only the edges that close a cycle
        #[arg(long)]
        cycles: bool,
        /// Only what this file reaches, and what reaches it
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        focus: Option<String>,
        /// How far from `--focus` to follow
        #[arg(long, default_value_t = 2)]
        depth: u32,
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
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
        /// typescript, javascript, python, go, java or cpp
        language: String,
        /// The query, in tree-sitter's s-expression syntax
        query: String,
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Show every capture of every match, not just where it starts
        #[arg(long)]
        captures: bool,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
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
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Print what rqx does about every pattern in the book, without scanning
        #[arg(long)]
        coverage: bool,
        /// Emit one JSON object per advisory instead of the reader's report
        #[arg(long)]
        json: bool,
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Analyze changed files against a base revision and fail if the gate is not met
    #[command(visible_alias = "c")]
    #[command(
        after_help = "Examples:\n  rqx check --base main               review the branch against main\n  rqx check --base main --fail-on error\n                                      and fail CI on any error\n  rqx check --fix -w                  apply the proven fixes first"
    )]
    Check {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
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
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Regenerate the checked-in rule reference under docs/
    #[command(hide = true)]
    Docs {
        #[arg(long, default_value = "docs", value_hint = ValueHint::DirPath)]
        out: PathBuf,
        #[arg(long, default_value = "fixtures", value_hint = ValueHint::DirPath)]
        fixtures: PathBuf,
    },
    /// Post the review onto the merge request it is reviewing, and grade the commit
    #[command(after_help = "Examples:
  rqx comment --base main             one sticky comment, edited on every re-run
  rqx comment --dry-run               print what would be posted, post nothing
  rqx comment --marker api            a second comment for a second job

GitLab needs RQX_TOKEN or GITLAB_TOKEN with `api` scope: CI_JOB_TOKEN cannot write
merge-request notes. GitHub needs `permissions: pull-requests: write`, and
`statuses: write` for the commit grade.")]
    Comment {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Review against this revision. Defaults to the merge request's own base,
        /// which the pipeline already knows
        #[arg(long, value_name = "REF")]
        base: Option<String>,
        /// Print the comment and where it would go, and post nothing
        #[arg(long)]
        dry_run: bool,
        /// Which comment to keep. Two jobs on one merge request each keep their own
        #[arg(long, default_value = "results", value_name = "ID")]
        marker: String,
        /// Comment, but leave the commit ungraded
        #[arg(long)]
        no_status: bool,
        #[command(flatten)]
        gate: Gate,
        /// Worker threads. Defaults to the machine's parallelism
        #[arg(long, value_name = "N", env = "RQX_THREADS")]
        threads: Option<usize>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Write a commented RQX.toml with every setting at its default
    Init {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Replace an RQX.toml that is already there
        #[arg(long)]
        force: bool,
    },
    /// Print the settings a run will use, and the file each one came from
    Config {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Run as an MCP server, so an AI agent can ask what a project's problems are
    Mcp {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Serve Streamable HTTP on this address instead of speaking over stdio
        #[arg(long, value_name = "ADDR")]
        http: Option<std::net::SocketAddr>,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Run as a Language Server over stdio, for editor integration
    Lsp {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
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
    #[arg(long, value_name = "LEVEL", env = "RQX_LEVEL")]
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

/// Display only: a filter narrows what is printed, never what is scanned. The gate,
/// the health score and the counts all see the whole set, because a score that moved
/// with a display flag would not mean anything.
#[derive(Args, Default)]
struct View {
    /// Show only findings at or above this severity
    #[arg(long, value_enum, help_heading = "Filtering")]
    severity: Option<SeverityArg>,
    /// Show only this rule; repeatable
    #[arg(long = "rule", value_name = "SLUG", help_heading = "Filtering")]
    rules: Vec<String>,
    /// Show only this smell family; repeatable
    #[arg(long = "category", value_name = "FAMILY", help_heading = "Filtering")]
    categories: Vec<String>,
    /// Show only findings in files matching this glob; repeatable
    #[arg(long = "path", value_name = "GLOB", help_heading = "Filtering")]
    paths: Vec<String>,
    /// Hide this rule; repeatable
    #[arg(long = "not-rule", value_name = "SLUG", help_heading = "Filtering")]
    not_rules: Vec<String>,
    /// Hide this smell family; repeatable
    #[arg(
        long = "not-category",
        value_name = "FAMILY",
        help_heading = "Filtering"
    )]
    not_categories: Vec<String>,
    /// Hide findings in files matching this glob; repeatable
    #[arg(long = "not-path", value_name = "GLOB", help_heading = "Filtering")]
    not_paths: Vec<String>,
    /// Show only findings rqx is certain of, or only the ones it is guessing at
    #[arg(long, value_enum, help_heading = "Filtering")]
    confidence: Option<ConfidenceArg>,
    /// Show only findings `rqx fix` can act on
    #[arg(long, help_heading = "Filtering")]
    fixable: bool,
    /// Show only findings whose rule, file or evidence contains this text
    #[arg(long, value_name = "TEXT", help_heading = "Filtering")]
    search: Option<String>,
    /// Show only findings in files this revision changed
    #[arg(long, value_name = "REF", help_heading = "Filtering")]
    since: Option<String>,
    /// Show only findings in files not yet committed
    #[arg(long, help_heading = "Filtering")]
    changed: bool,

    /// List files that have no findings as well
    #[arg(long, help_heading = "Display")]
    show_passed: bool,
    /// Print the summary without the findings
    #[arg(short, long, help_heading = "Display")]
    quiet: bool,
    /// Print how long each stage of the scan took, and what it did
    #[arg(long, help_heading = "Display")]
    timings: bool,
    /// Counts only: which rules and families fired, and how often
    #[arg(long, help_heading = "Display")]
    stats: bool,
    /// What the listing is a list of
    #[arg(long = "group-by", value_enum, default_value_t = GroupArg::File, help_heading = "Display")]
    grouping: GroupArg,
    /// What decides the order inside each group
    #[arg(long = "sort", value_enum, default_value_t = OrderArg::Line, help_heading = "Display")]
    order: OrderArg,
    /// List only this many groups; the rest are counted
    #[arg(long, value_name = "N", help_heading = "Display")]
    top: Option<usize>,
    /// List the findings a comment waives, alongside the ones it does not
    #[arg(long, help_heading = "Display")]
    show_suppressed: bool,
    /// Lines of code shown either side of a finding. Two under `--detail full`,
    /// none otherwise, unless this says
    #[arg(long, value_name = "N", help_heading = "Display")]
    context: Option<u32>,
    /// How much of each finding to print
    #[arg(long, value_enum, default_value_t = DetailArg::Normal, help_heading = "Display")]
    detail: DetailArg,
    /// Where a report goes: `<name>` for stdout, `<name>=<path>` for a file; repeatable
    #[arg(
        long = "reporter",
        value_name = "NAME[=PATH]",
        help_heading = "Display"
    )]
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
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Read settings from this file, layered over any `RQX.toml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Throw the cache away, leaving everything else under `.rqx` alone
    Clear {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
    },
}

#[derive(Args)]
struct Gate {
    /// Lowest severity that fails the run
    #[arg(long, value_enum, help_heading = "Gate")]
    fail_on: Option<FailOn>,
    /// Findings allowed before the run fails
    #[arg(long, help_heading = "Gate")]
    allow_findings: Option<u32>,
    /// Health score below which the run fails; 0 disables
    #[arg(long, help_heading = "Gate")]
    min_health: Option<u32>,
    /// Lowest grade any single file may carry
    #[arg(long, value_enum, help_heading = "Gate")]
    min_grade: Option<MinGrade>,
    /// Fail when a design pattern is half applied
    #[arg(long, help_heading = "Gate")]
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

/// `normal` prints a rule's reason once and a file's churn once, because both read
/// the same on every finding that repeats them.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
enum DetailArg {
    /// One line per finding
    Terse,
    /// The reason once per rule, the cost once per file
    #[default]
    Normal,
    /// Every line on every finding
    Full,
}

impl From<DetailArg> for report::Detail {
    fn from(value: DetailArg) -> Self {
        match value {
            DetailArg::Terse => Self::Terse,
            DetailArg::Normal => Self::Normal,
            DetailArg::Full => Self::Full,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum ConfidenceArg {
    Verified,
    Heuristic,
}

impl From<ConfidenceArg> for rqx_core::finding::Confidence {
    fn from(value: ConfidenceArg) -> Self {
        match value {
            ConfidenceArg::Verified => Self::Verified,
            ConfidenceArg::Heuristic => Self::Heuristic,
        }
    }
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
    if !view.timings {
        for sink in sinks {
            write_sink(report, sink, view, palette, &mut stdout_for(sink))?;
        }
        return Ok(());
    }
    // Held back so rendering can be timed and still printed beneath the stages it
    // is one of, which come above the report.
    let mut held = Vec::new();
    let rendering = std::time::Instant::now();
    for sink in sinks {
        write_sink(report, sink, view, palette, &mut held)?;
    }
    let render = rendering.elapsed().as_millis();
    let mut out = anstream::stdout().lock();
    report::timings(report.outcome, &[("render", render)], palette, &mut out)?;
    out.write_all(&held)?;
    Ok(())
}

fn stdout_for(sink: &Sink) -> Box<dyn Write> {
    if sink.reporter.is_coloured() {
        Box::new(anstream::stdout().lock())
    } else {
        Box::new(std::io::stdout().lock())
    }
}

/// One sink's report, to its file or to `stdout`, whichever the sink names.
fn write_sink(
    report: &Report<'_>,
    sink: &Sink,
    view: &View,
    palette: Palette,
    stdout: &mut dyn Write,
) -> Result<()> {
    let plain = Layout {
        detail: view.detail.into(),
        grouping: view.grouping.into(),
        show_suppressed: view.show_suppressed,
        order: view.order.into(),
        top: view.top,
        palette: Palette::with_depth(ColorDepth::Monochrome),
        show_passed: view.show_passed,
        stats: view.stats,
        quiet: view.quiet,
        // Full is the level that shows everything, so it is the one that pays
        // for surrounding lines; at 285 findings they are most of the scroll.
        context: view.context.unwrap_or(match view.detail {
            DetailArg::Full => 2,
            DetailArg::Terse | DetailArg::Normal => 0,
        }),
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
            report::emit(report, sink.reporter, &layout, stdout)?;
        }
        None => report::emit(report, sink.reporter, &plain, stdout)?,
    }
    Ok(())
}

/// One dim line under a report when a newer release is already known about, and a
/// check on a thread nobody waits for so the next run knows. Never on a report going
/// somewhere other than a terminal: a line about updates in a JSON document or a
/// JUnit file is corruption, not news.
fn update_notice(view: &View, sinks: &[Sink], palette: Palette, enabled: bool) {
    // `is_coloured` is about the reporter's kind, not about where it lands, so it is
    // true for the default reporter piped into a build log. `update::wanted` is what
    // actually tests the terminal.
    let readable = sinks
        .iter()
        .any(|sink| sink.path.is_none() && sink.reporter.is_coloured());
    if view.quiet || !readable || !update::wanted(enabled) {
        return;
    }
    // A quarter of a second at the very end, for a check that started before the
    // scan did. Long enough to catch an answer that is all but arrived, short enough
    // that nobody attributes it to the scan.
    if let Some(newer) = update::settle(std::time::Duration::from_millis(250)) {
        // Through anstream so a redirected stderr gets the words without the escape
        // sequences, which is the whole reason `paint` exists.
        anstream::eprintln!(
            "\n  {}",
            brand::paint(palette.style(brand::Ink::Muted), newer.line())
        );
    }
}

fn filter_of(view: &View, root: &Path) -> Result<FindingFilter> {
    let filter = FindingFilter::new(
        view.severity.map(Severity::from),
        &view.rules,
        &view.categories,
        &view.paths,
    )?
    .without(&view.not_rules, &view.not_categories, &view.not_paths)?
    .at_confidence(view.confidence.map(Into::into))
    .fixable(view.fixable)
    .with_text(view.search.as_deref().unwrap_or_default());
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

/// Opens the session on the working directory. `None` means the terminal could not
/// host it after all, and the caller prints the report instead.
fn open_session(motion: Motion) -> Result<Option<Exit>> {
    let Command::Scan {
        standard,
        threads,
        config,
        ..
    } = bare_scan()?
    else {
        bail!("bare `rqx` did not parse as a scan");
    };
    let path = Path::new(".");
    let mut settings = config::resolve(path, config.as_deref(), standard.chosen()?)?;
    if let Some(count) = threads {
        settings.config.scan.threads = count;
    }
    let options = session::Launch {
        path,
        config: &settings,
        reduced_motion: matches!(motion, Motion::Off),
    };
    match session::launch(options) {
        Ok(()) => Ok(Some(Exit::Clean)),
        Err(reason) => {
            eprintln!("  interactive session unavailable: {reason}");
            Ok(None)
        }
    }
}

/// What bare `rqx` runs when it does not open the interface: exactly `rqx scan`,
/// parsed the way a typed one is so the defaults that come from the environment, such
/// as `RQX_LEVEL` and `RQX_THREADS`, apply to it as well.
fn bare_scan() -> Result<Command> {
    let matches = parser().get_matches_from(["rqx", "scan"]);
    Ok(Command::from_arg_matches(&matches)?)
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
    let cli = Cli::from_arg_matches(&parser().get_matches()).unwrap_or_else(|error| error.exit());
    // Set before anything draws: `anstream::stdout()` and `Palette::detect()` both
    // read this global, so one flag governs every writer.
    cli.color.choice().write_global();
    progress::set_animated(!matches!(cli.motion, Motion::Off));
    let palette = Palette::detect();

    let command = match cli.command {
        Some(command) => command,
        None => {
            // The gate says no for a pipe, CI and a window too small to draw in, and
            // for all of them the answer is the report, so a pipeline running `rqx`
            // gets the scan's exit code rather than a help page that exits 0.
            if !cli.no_tui
                && session::gate().is_ok()
                && let Some(exit) = open_session(cli.motion)?
            {
                return Ok(exit);
            }
            bare_scan()?
        }
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
            watch,
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
            // Started before the scan rather than after it: the answer needs the length
            // of the work to arrive in, and `process::exit` does not wait for a thread.
            update::start(settings.config.update.check);
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
            let run = |settings: &config::Resolved| -> Result<Exit> {
                let outcome = scan::scan(&path, settings)?;

                // A bare `rqx scan` reports; it only gates when asked to, so adding a
                // flag is the whole opt-in and no existing pipeline changes meaning.
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
                update_notice(&view, &sinks, palette, settings.config.update.check);
                Ok(Exit::of(verdict.is_some_and(|verdict| !verdict.passed())))
            };

            if watch {
                return watch::watch(&path, &settings, run);
            }
            run(&settings)
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
            let request = render::Plan {
                budget,
                limit,
                simulate,
                json,
            };
            render::plan(&outcome, &request, palette, &mut anstream::stdout().lock())
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
            render::patterns(&outcome, json, palette, &mut anstream::stdout().lock())
                .map(|()| Exit::Clean)
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
            render::suppressions(
                &path,
                &outcome,
                expiring,
                json,
                palette,
                &mut anstream::stdout().lock(),
            )
            .map(|()| Exit::Clean)
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
            let outcome = scan::scan(&path, &settings)?;
            let view = complexity::View {
                by,
                sort,
                top,
                min,
                histogram,
            };
            render::complexity(
                &outcome,
                &view,
                json,
                &settings,
                palette,
                &mut anstream::stdout().lock(),
            )
            .map(|()| Exit::Clean)
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
            let outcome = scan::scan(&path, &settings)?;
            let view = perf::View {
                by,
                sort,
                top,
                min_depth,
            };
            render::perf(
                &outcome,
                &view,
                json,
                &settings,
                palette,
                &mut anstream::stdout().lock(),
            )
            .map(|()| Exit::Clean)
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
            render::recommend(
                &outcome,
                &settings,
                only,
                json,
                palette,
                &mut anstream::stdout().lock(),
            )
            .map(|()| Exit::Clean)
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
            render::slop_settings(&mut settings);
            let outcome = scan::scan(&path, &settings)?;
            let request = render::Slop { top, min, json };
            render::slop(&outcome, &request, palette, &mut anstream::stdout().lock())
                .map(|()| Exit::Clean)
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
            render::health(&outcome, by, json, palette, &mut anstream::stdout().lock())
                .map(|()| Exit::Clean)
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
        Command::Comment {
            path,
            standard,
            base,
            dry_run,
            marker,
            no_status,
            gate,
            threads,
            config,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx comments on a project; try `rqx comment .`",
                    path.display()
                );
            }
            let mut settings = config::resolve(&path, config.as_deref(), standard.chosen()?)?;
            if let Some(count) = threads {
                settings.config.scan.threads = count;
            }
            gate.apply(&mut settings.config);
            // The whole review, not the first five hundred: a display cap is for a
            // terminal, and this is a document somebody scrolls.
            settings.config.scan.max_findings = 0;

            // A CI clone is `git init` plus a fetch, so `origin/HEAD` points at
            // nothing and a branch name is not a local ref; both forges hand over the
            // merge base as a SHA, which is a revision that exists.
            let base = base
                .or_else(forge::base_revision)
                .unwrap_or_else(|| "origin/HEAD".to_owned());
            let review = check::review(&path, &settings, &base)?;
            let verdict = check::judge(&review, &settings.config);
            let report = Report::reviewed(&review, &verdict);

            // Detected before posting but after the scan, so a dry run still gets its
            // comment where there is nowhere to put one.
            let forge = forge::Forge::detect();
            let body = report::comment(&report, forge.as_ref().ok().and_then(forge::Forge::job))?;
            if dry_run {
                println!("{body}");
                eprintln!(
                    "\n  would post to {}\n",
                    forge.as_ref().map_or_else(
                        |error| format!("nowhere: {error:#}"),
                        |forge| format!("{} {}", forge.name(), forge.target())
                    )
                );
                return Ok(Exit::of(!verdict.passed()));
            }

            let forge = forge?;
            let said = match forge.comment(&marker, &body)? {
                forge::Posted::Created => "commented on",
                forge::Posted::Updated => "updated the comment on",
            };
            eprintln!("  {said} {} {}", forge.name(), forge.target());
            if !no_status {
                let classification = report::classification(&report);
                // A status that cannot be set is worth saying and not worth failing
                // the build over: the comment is already there.
                match forge.classify(verdict.passed(), &classification) {
                    Ok(()) => eprintln!("  graded the commit: {classification}"),
                    Err(error) => eprintln!("  could not grade the commit: {error:#}"),
                }
            }
            Ok(Exit::of(!verdict.passed()))
        }
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
            // Started before the scan rather than after it: the answer needs the length
            // of the work to arrive in, and `process::exit` does not wait for a thread.
            update::start(settings.config.update.check);
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
            update_notice(&view, &sinks, palette, settings.config.update.check);
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
        Command::Rules {
            category,
            language,
            slugs,
        } => {
            if slugs {
                // The same narrowing the printed listing does. Without this a script
                // asking for one family's slugs got all 185 and no sign it had been
                // ignored.
                let only = category
                    .as_deref()
                    .map(|name| {
                        rqx_core::rule::Category::parse(name).ok_or_else(|| {
                            report::unknown(
                                "family",
                                name,
                                &rqx_core::rule::Category::ALL.map(rqx_core::rule::Category::slug),
                                &format!(
                                    "The families are: {}.",
                                    rqx_core::rule::Category::ALL
                                        .map(rqx_core::rule::Category::slug)
                                        .join(", ")
                                ),
                            )
                        })
                    })
                    .transpose()?;
                let spoken = language
                    .as_deref()
                    .map(|name| {
                        rqx_core::Language::parse(name).ok_or_else(|| {
                            report::unknown(
                                "language",
                                name,
                                &rqx_core::Language::ALL.map(rqx_core::Language::slug),
                                &format!(
                                    "rqx reads: {}.",
                                    rqx_core::Language::ALL
                                        .map(rqx_core::Language::slug)
                                        .join(", ")
                                ),
                            )
                        })
                    })
                    .transpose()?;
                // `writeln!` rather than `println!`, which panics when the reader
                // closes the pipe. `rqx rules --slugs | head` is the shape a shell
                // completion uses, and `reader_went_away` can only forgive an error
                // that is returned to it.
                use std::io::Write;
                let mut out = std::io::stdout().lock();
                for rule in rqx_core::rule::RuleId::ALL {
                    let spec = rule.spec();
                    if only.is_some_and(|family| spec.category != family) {
                        continue;
                    }
                    if spoken.is_some_and(|language| !rule.applies_to(language)) {
                        continue;
                    }
                    writeln!(out, "{}", spec.slug)?;
                }
                out.flush()?;
                return Ok(Exit::Clean);
            }
            report::rules(category.as_deref(), language.as_deref(), palette).map(|()| Exit::Clean)
        }
        Command::Update { check, notes } => {
            let dim = palette.style(brand::Ink::Muted);
            // Asked for directly, so the cached answer is not good enough and the
            // environment's opinion about CI does not apply.
            let found = update::fetch()?;
            let Some(newer) = found else {
                anstream::println!(
                    "\n  rqx {} is the newest release.\n",
                    env!("CARGO_PKG_VERSION")
                );
                return Ok(Exit::Clean);
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
            if notes || check {
                for line in newer.notes.lines() {
                    anstream::println!("  {line}");
                }
                anstream::println!();
            }
            if check || notes {
                return Ok(Exit::Clean);
            }

            if let Some(manager) = update::managed() {
                anstream::println!(
                    "  {}\n",
                    brand::paint(dim, update::managed_update(&manager))
                );
                return Ok(Exit::Clean);
            }

            anstream::println!(
                "  {}\n",
                brand::paint(dim, "building it with cargo; this takes a minute")
            );
            update::install(&newer.version)?;
            anstream::println!(
                "\n  {}\n",
                brand::paint(
                    palette.bold(brand::Ink::Good),
                    format!("installed rqx {}", newer.version)
                )
            );
            Ok(Exit::Clean)
        }
        Command::Techniques => {
            report::techniques(palette, &mut anstream::stdout().lock()).map(|()| Exit::Clean)
        }
        Command::Levels => {
            report::levels(palette, &mut anstream::stdout().lock()).map(|()| Exit::Clean)
        }
    }
}

#[cfg(test)]
mod grouping {
    use std::collections::HashSet;

    #[test]
    fn every_subcommand_is_in_exactly_one_group() {
        let parser = super::parser();
        let real: HashSet<&str> = parser
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
            .map(clap::Command::get_name)
            .collect();

        let mut placed: Vec<&str> = Vec::new();
        for (_, names) in super::GROUPS {
            placed.extend_from_slice(names);
        }

        let twice: Vec<&&str> = placed
            .iter()
            .filter(|name| placed.iter().filter(|other| other == name).count() > 1)
            .collect();
        assert!(twice.is_empty(), "in more than one group: {twice:?}");

        let listed: HashSet<&str> = placed.iter().copied().collect();
        let missing: Vec<&&str> = real.difference(&listed).collect();
        assert!(missing.is_empty(), "in no help group: {missing:?}");

        let invented: Vec<&&str> = listed.difference(&real).collect();
        assert!(invented.is_empty(), "not a real subcommand: {invented:?}");
    }
}

/// Every flag, alias and subcommand a script can type today, checked against
/// `tests/contract/flags.txt`. Removing or renaming one breaks whoever wrote it into
/// a pipeline; adding one is free, so the file may grow but no line may disappear.
/// `RQX_BLESS=1 cargo test -p rqx flags` rewrites the file.
#[cfg(test)]
mod flags {
    use std::collections::BTreeSet;

    fn surface(command: &clap::Command, path: &str, lines: &mut BTreeSet<String>) {
        for argument in command.get_arguments() {
            argument_lines(argument, path, lines);
        }
        for sub in command.get_subcommands() {
            let sub_path = if path.is_empty() {
                sub.get_name().to_owned()
            } else {
                format!("{path} {}", sub.get_name())
            };
            lines.insert(sub_path.clone());
            for alias in sub.get_all_aliases() {
                lines.insert(format!("alias {alias} -> {sub_path}"));
            }
            surface(sub, &sub_path, lines);
        }
    }

    fn argument_lines(argument: &clap::Arg, path: &str, lines: &mut BTreeSet<String>) {
        let mut spellings: Vec<String> = Vec::new();
        spellings.extend(argument.get_long().map(|long| format!("--{long}")));
        spellings.extend(argument.get_short().map(|short| format!("-{short}")));
        spellings.extend(
            argument
                .get_all_aliases()
                .into_iter()
                .flatten()
                .map(|alias| format!("--{alias}")),
        );
        spellings.extend(
            argument
                .get_all_short_aliases()
                .into_iter()
                .flatten()
                .map(|alias| format!("-{alias}")),
        );
        if spellings.is_empty() {
            lines.insert(format!("{path} <{}>", argument.get_id()));
            return;
        }
        for spelling in spellings {
            lines.insert(format!("{path} {spelling}"));
            if let Some(variable) = argument.get_env() {
                lines.insert(format!(
                    "{path} {spelling} [env {}]",
                    variable.to_string_lossy()
                ));
            }
            for value in argument.get_possible_values() {
                lines.insert(format!("{path} {spelling} ={}", value.get_name()));
            }
        }
    }

    fn current_surface() -> BTreeSet<String> {
        let mut parser = super::parser();
        parser.build();
        let mut lines = BTreeSet::new();
        surface(&parser, "", &mut lines);
        lines
            .into_iter()
            .map(|line| line.trim_start().to_owned())
            .collect()
    }

    #[test]
    fn no_flag_alias_or_subcommand_is_ever_removed() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/contract/flags.txt");
        let current = current_surface();

        if std::env::var_os("RQX_BLESS").is_some() {
            let mut text = current.iter().cloned().collect::<Vec<_>>().join("\n");
            text.push('\n');
            std::fs::write(path, text).expect("the flags file");
            return;
        }

        let recorded = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("no {path}; generate it with RQX_BLESS=1"));
        let removed: Vec<&str> = recorded
            .lines()
            .filter(|line| !line.is_empty() && !current.contains(*line))
            .collect();
        assert!(
            removed.is_empty(),
            "these were part of the command line and are gone:\n{}",
            removed.join("\n")
        );
    }
}

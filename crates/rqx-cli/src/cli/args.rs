//! The command line as clap sees it: the commands and their flags.

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::builder::{EnumValueParser, FalseyValueParser};
use clap::{Args, Parser, Subcommand, ValueEnum, ValueHint, value_parser};
use rqx_core::config::{GateGrade, GateSeverity};
use rqx_core::finding::Severity;

use crate::report::Reporter;
use crate::setting::{OrUnset, Setting};
use crate::sink::{Rendering, Sink};
use crate::{
    agent_setup, check, complexity, config, edit_config, graph, health, pager, perf, progress,
    recommend, report, sink, term, trend,
};

#[derive(Parser)]
#[command(
    name = "rqx",
    version,
    about = "Reports dead code, duplication, complexity and architectural drift in TypeScript, JavaScript, Python, Go, Java and C++, with the reason each finding matters",
    // `{subcommands}` is left out and the grouped list put in `before_help` instead:
    // clap has no heading for a subcommand, and twenty-seven of them in one block is
    // a wall that gets worse with every one added. They stay visible to the parser
    // rather than hidden, because hiding them would group the help and silently
    // break shell completion, which reads the same flag. The commands come before the
    // options because what a reader came for is a command, and the global options
    // are the same for all of them.
    help_template = "\
{about}

{usage-heading} {usage}

{before-help}Options:
{options}
{after-help}",
)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
    /// Do not open the interactive interface; bare `rqx` prints the report instead
    #[arg(long, global = true)]
    pub(super) no_tui: bool,
    /// Keeps the session from capturing the mouse wheel for its command menu, so the
    /// terminal keeps the mouse throughout. Without it the session captures the wheel
    /// only while the menu is open.
    #[arg(long, global = true)]
    pub(super) no_mouse: bool,
    /// When to colour output; `auto` follows NO_COLOR and whether stdout is a terminal
    #[arg(long, value_enum, default_value_t = Color::Auto, global = true)]
    pub(super) color: Color,
    /// Animation in the interactive interface
    #[arg(long, value_enum, default_value_t = Motion::On, global = true)]
    pub(super) motion: Motion,
    /// Do not draw the progress bar on stderr; `--quiet` implies it
    #[arg(long, global = true, env = "RQX_NO_PROGRESS")]
    pub(super) no_progress: bool,
    /// How a scan reports progress on stderr; `plain` prints a line every two seconds for a job log
    #[arg(
        long,
        value_enum,
        default_value_t = progress::Style::Auto,
        global = true,
        env = "RQX_PROGRESS"
    )]
    pub(super) progress: progress::Style,
    /// Increase output: `-v` adds the configuration, the cache and every skipped file to the report; `-vv` also prints stage timings and git commands on stderr
    #[arg(short = 'v', long, action = clap::ArgAction::Count, global = true)]
    pub(super) verbose: u8,
    /// Render `file:line` in the report as a hyperlink that opens in an editor; `auto` enables it on terminals known to support hyperlinks
    #[arg(
        long,
        value_enum,
        default_value_t = term::LinkChoice::Auto,
        global = true,
        env = "RQX_HYPERLINKS"
    )]
    pub(super) hyperlinks: term::LinkChoice,
    /// Print a report taller than the terminal directly instead of through the pager: `$RQX_PAGER`, then `$PAGER`, then `less -FRX`
    #[arg(long, global = true)]
    pub(super) no_pager: bool,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Analyze a project and report every finding
    #[command(visible_alias = "s")]
    #[command(
        after_help = "Examples:\n  rqx scan .                          the full report\n  rqx scan . --rule empty-catch       one rule only\n  rqx scan . --not-path 'test/**'     all files except tests\n  rqx scan . --fixable                only findings `rqx fix` can apply\n  rqx scan . --stats                  finding counts per rule"
    )]
    Scan {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        view: View,
        #[command(flatten)]
        gate: Gate,
        /// Append design pattern advice for the same files
        #[arg(long)]
        advice: bool,
        #[command(flatten)]
        evidence: Evidence,
        #[command(flatten)]
        cap: DisplayCap,
        /// Rescan whenever a file changes, until interrupted
        #[arg(long)]
        watch: bool,
        #[command(flatten)]
        trending: trend::Options,
        #[command(flatten)]
        run: Run,
    },
    /// Apply the fixes that have a single correct result, proving each one before it
    /// is written. Previews by default.
    #[command(visible_alias = "f")]
    #[command(
        after_help = "Examples:\n  rqx fix                             show what would change\n  rqx fix -w                          write the fixes\n  rqx fix --plan                      list the edits instead of a diff"
    )]
    Fix {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Write the fixes. Without it no file changes.
        #[arg(long, short)]
        write: bool,
        /// Maximum number of rounds. One fix can enable another, so fixing repeats
        /// until a round finds nothing. Overrides `fix.max_rounds`.
        #[arg(long)]
        max_rounds: Option<u32>,
        /// Emit one JSON object per fixed file instead of the text preview
        #[arg(long)]
        json: bool,
        /// Report the findings `fix` does not change, with the span, the source, the
        /// metric and the named refactoring
        #[arg(long)]
        plan: bool,
        /// With --plan, emit Markdown for pasting instead of the terminal report
        #[arg(long)]
        markdown: bool,
        /// Also run `go vet` on a temporary copy of each Go module with the fixes
        /// applied, and withhold a module's fixes if it fails. Needs `go` on PATH
        #[arg(long)]
        verify_build: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Compare the working tree against a base revision and classify each finding
    /// as resolved, improved, relocated or introduced
    #[command(visible_alias = "v")]
    Verify {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Revision to compare against
        #[arg(long, default_value = "origin/HEAD")]
        base: String,
        /// Rules that must be resolved, as `rule` or `rule@file`. Repeatable.
        #[arg(long)]
        expect: Vec<String>,
        /// Emit one JSON object per change instead of the text report
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Print the configuration an editor or a coding agent needs to use rqx, or list the supported editors
    #[command(args_conflicts_with_subcommands = true)]
    Setup {
        /// Editor name. Omit to list the supported editors.
        editor: Option<String>,
        /// Append the configuration to the editor's configuration file, after showing what will be added
        #[arg(long, short)]
        write: bool,
        #[command(subcommand)]
        target: Option<SetupTarget>,
    },
    /// Rank the work by value against effort, and report the health score after it
    #[command(
        after_help = "Examples:\n  rqx plan --budget 20                the highest-value work within 20 effort points\n  rqx plan --simulate                 the health score after each step"
    )]
    Plan {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Effort points to spend. Without it the plan lists the top items by value.
        #[arg(long)]
        budget: Option<u32>,
        /// How many items to rank when no budget is given
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Also print the effect of the plan on the grade of each file
        #[arg(long)]
        simulate: bool,
        /// Emit one JSON object per item instead of the text plan
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Print the catalogue entry for a rule or a pattern: what it maps to, what decides
    /// it and what fixes it. A performance rule the precision corpus scores also says how
    /// often it was right there
    Explain { name: String },
    /// List every rule rqx can report, grouped by family
    Rules {
        /// Show only one family
        category: Option<String>,
        /// Show only the rules that apply to one language
        #[arg(long)]
        language: Option<String>,
        /// Print one slug per line and nothing else, for scripts and shell completion
        #[arg(long)]
        slugs: bool,
    },
    /// Record the findings this project already has, so the gate evaluates only what a
    /// change adds
    #[command(
        after_help = "Examples:\n  rqx baseline                        record the current findings\n  rqx baseline --update               record the reduced set after a cleanup"
    )]
    Baseline {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Record the baseline again from the current findings. Refuses to record more
        /// findings than the previous baseline held unless --force is given.
        #[arg(long)]
        update: bool,
        /// Record a larger set than the previous baseline held
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        run: Run,
    },
    /// List every finding waived by a comment in the source, with its reason
    Suppressions {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Show only the waivers that carry an `until` date
        #[arg(long)]
        expiring: bool,
        /// Emit one JSON object per waiver instead of the text report
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Waive one finding with a comment in the source that states a reason
    #[command(
        after_help = "Examples:\n  rqx waive src/api.ts:42 unused-export --reason \"called from the plugin host\"\n  rqx waive src/api.ts:42 unused-export --reason kept --until 2027-03-01\n  rqx waive src/gen.ts unused-export --reason generated --whole-file\n  rqx waive src/api.ts:42 unused-export --reason kept --dry-run"
    )]
    Waive {
        /// Where the finding is, as FILE:LINE; FILE alone with --whole-file
        #[arg(value_name = "FILE:LINE", value_hint = ValueHint::FilePath)]
        at: String,
        /// The rule to waive, as `rqx rules` lists it
        rule: String,
        /// Why the finding is kept. A waiver without a reason is not honoured, so this
        /// option is required; the interactive session prompts for it when omitted
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,
        /// Date after which the waiver is no longer honoured
        #[arg(long, value_name = "YYYY-MM-DD")]
        until: Option<String>,
        /// Waive the rule for the whole file instead of one line
        #[arg(long)]
        whole_file: bool,
        /// Print the diff the waiver would make, and change no file
        #[arg(long)]
        dry_run: bool,
        /// Read settings from this file, layered over any `rqx.yaml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Report the complexity distribution, with the highest values
    #[command(
        after_help = "Examples:\n  rqx complexity --top 20             the 20 highest-scoring functions\n  rqx complexity --by dir             the highest-scoring entry of each directory\n  rqx complexity --histogram          the distribution across the codebase"
    )]
    Complexity {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// One row per function, per file, or per directory
        #[arg(long, value_enum, default_value_t = complexity::By::Function)]
        by: complexity::By,
        #[arg(long, value_enum, default_value_t = complexity::Sort::Complexity)]
        sort: complexity::Sort,
        /// How many rows
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Skip rows below this value of the measure `--sort` selects
        #[arg(long, default_value_t = 0)]
        min: u32,
        /// Draw the distribution as well as the highest values
        #[arg(long)]
        histogram: bool,
        /// Emit one JSON object per row
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Report code whose work grows faster than its input, ranked by reach
    Perf {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// One row per function, per file, or per directory
        #[arg(long, value_enum, default_value_t = perf::By::Function)]
        by: perf::By,
        #[arg(long, value_enum, default_value_t = perf::Sort::Weight)]
        sort: perf::Sort,
        /// How many rows
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Skip entries whose loops nest less deeply than this
        #[arg(long, default_value_t = 0)]
        min_depth: u32,
        /// Show how one function's cost was derived: its loops, where each bound came
        /// from, what each callee costs and why the analysis is as sure as it is. Name
        /// it as `name`, `Class.method` or `file:line`
        #[arg(
            long,
            value_name = "FUNCTION",
            conflicts_with_all = ["by", "sort", "top", "min_depth"]
        )]
        explain: Option<String>,
        /// A CPU profile; a finding in code that nothing ran stops counting. A V8
        /// .cpuprofile, a Go pprof, speedscope JSON, JFR text and collapsed stacks are
        /// detected by content.
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        profile: Vec<PathBuf>,
        /// Emit one JSON object per row
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Install the newest release, or report what it would change
    #[command(
        after_help = "Examples:\n  rqx update                          install the newest release\n  rqx update --check                  report whether a newer release exists; install nothing\n  rqx update --notes                  print the release notes\n  rqx update --version 1.2.3          install that release, newer or older\n  rqx update --dry-run                show the method and source; change nothing"
    )]
    Update {
        /// Report whether a newer release exists and stop
        #[arg(long, conflicts_with_all = ["version", "dry_run"])]
        check: bool,
        /// Print the release notes for the newest release
        #[arg(long, conflicts_with_all = ["version", "dry_run"])]
        notes: bool,
        /// Install this release, such as 1.2.3, instead of the newest
        #[arg(long, value_name = "X.Y.Z")]
        version: Option<String>,
        /// Print the method, the URLs and the file that would be replaced, and change nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Print a shell completion script
    Completions {
        /// bash, zsh, fish, elvish or powershell
        shell: clap_complete::Shell,
    },
    /// Recommend actions for the project: configuration thresholds, refactorings, files to read and performance work
    #[command(
        after_help = "Examples:\n  rqx recommend                       all four sections\n  rqx recommend --only config         thresholds that fit this codebase"
    )]
    Recommend {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Only this section
        #[arg(long, value_enum)]
        only: Option<recommend::Only>,
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Score files that show signs of not having been reviewed after they were written
    Slop {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Skip anything scoring below this
        #[arg(long, default_value_t = 1)]
        min: u32,
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Rank files by how often they change and how much is wrong with them
    #[command(
        after_help = "Examples:\n  rqx hotspots                        the ten files that are both churned and flagged\n  rqx hotspots --top 25 --since 180   a longer list over half a year of history"
    )]
    Hotspots {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// How many files to list
        #[arg(long, default_value_t = 10)]
        top: usize,
        /// Days of git history to count commits over. Defaults to `scan.churn_days`
        #[arg(long, value_name = "DAYS")]
        since: Option<u32>,
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Write the health score as a small SVG badge
    #[command(
        after_help = "Examples:\n  rqx badge --output badge.svg        write the badge for this project\n  rqx badge > badge.svg               the same, through the shell"
    )]
    Badge {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Write the SVG to this file. Without it, the SVG goes to standard output
        #[arg(long, short, value_name = "PATH", value_hint = ValueHint::FilePath)]
        output: Option<PathBuf>,
        #[command(flatten)]
        run: Run,
    },
    /// Break the health score down by component and report what removing each would gain
    Health {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = health::By::Family)]
        by: health::By,
        /// Emit one JSON object per bucket
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Rank the owners named in CODEOWNERS by the health of the files they own
    Owners {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Emit one JSON object per owner
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Show how health, findings and the rules or owners that moved most have changed
    /// across the snapshots `--record` wrote
    Trend {
        /// The trend file `rqx scan --record` appends to
        #[arg(default_value = trend::DEFAULT_FILE, value_hint = ValueHint::FilePath)]
        path: PathBuf,
        /// Only the newest N snapshots
        #[arg(long, value_name = "N")]
        last: Option<usize>,
        /// What to list the biggest movers of
        #[arg(long, value_enum, default_value_t = trend::By::Rule)]
        by: trend::By,
        /// Emit one JSON object per series
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
    },
    /// Print the import graph as text, Graphviz, Mermaid or JSON
    #[command(
        after_help = "Examples:\n  rqx graph --cycles                  only the edges that close a cycle\n  rqx graph --format mermaid          Mermaid output for documentation\n  rqx graph --focus src/api.ts        files it reaches and files that reach it"
    )]
    Graph {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// text, dot, mermaid or json. Defaults to text unless a `--reporter` is given
        #[arg(long, value_enum)]
        format: Option<graph::Shape>,
        /// Output destination: `<name>` for stdout, `<name>=<path>` for a file;
        /// repeatable. The names are the formats, and `terminal` is `text`
        #[arg(long = "reporter", value_name = "NAME[=PATH]")]
        reporters: Vec<Sink<graph::Shape>>,
        /// Only the edges that close a cycle
        #[arg(long)]
        cycles: bool,
        /// Only the files this file reaches and the files that reach it
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        focus: Option<String>,
        /// Distance from `--focus` to follow
        #[arg(long, default_value_t = 2)]
        depth: u32,
        #[command(flatten)]
        run: Run,
    },
    /// Show what the analysis cache holds, or delete it
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Run a tree-sitter query over a project, to test it before adding it to a `rule`
    /// entry in `rqx.yaml`
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
        /// Read settings from this file, layered over any `rqx.yaml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// List every refactoring rqx prescribes and the rules that prescribe it
    Techniques,
    /// Print what each cleanliness level sets, as the equivalent configuration
    /// settings
    Levels,
    /// Report which design patterns a project uses, which are half applied, and where
    /// one would fit
    Patterns {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Print how rqx handles each pattern in its catalogue, without scanning
        #[arg(long)]
        coverage: bool,
        /// Emit one JSON object per advisory instead of the text report
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(flatten)]
        run: Run,
    },
    /// Analyze changed files against a base revision and fail if the gate is not met
    #[command(visible_alias = "c")]
    #[command(
        after_help = "Examples:\n  rqx check --base main               review the branch against main\n  rqx check --base main --fail-on error\n                                      and exit 1 on any error\n  rqx check --fix -w                  apply the proven fixes first\n  git diff -z --name-only main... | rqx check --files-from -\n                                      review from a file list, without git history"
    )]
    Check {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Revision to compare against
        #[arg(long, default_value = "origin/HEAD", conflicts_with = "files_from")]
        base: String,
        /// Review what `git commit` would record: the staged contents, read from the
        /// index. Unstaged edits do not affect the verdict. The rest of the project is
        /// read from the working tree
        #[arg(
            long,
            conflicts_with_all = ["base", "files_from", "fix", "write"]
        )]
        staged: bool,
        /// Apply the mechanical fixes to the files the branch changed before gating, so
        /// a branch whose only failures are mechanical passes
        #[arg(long)]
        fix: bool,
        /// With --fix, write the fixes. Without it no file changes.
        #[arg(long, short)]
        write: bool,
        #[command(flatten)]
        view: View,
        #[command(flatten)]
        gate: Gate,
        #[command(flatten)]
        evidence: Evidence,
        #[command(flatten)]
        cap: DisplayCap,
        #[command(flatten)]
        trending: trend::Options,
        #[command(flatten)]
        complexity: Complexity,
        #[command(flatten)]
        run: Run,
    },
    /// Regenerate the checked-in rule reference under docs/
    #[command(hide = true)]
    Docs {
        #[arg(long, default_value = "docs", value_hint = ValueHint::DirPath)]
        out: PathBuf,
        #[arg(long, default_value = "fixtures", value_hint = ValueHint::DirPath)]
        fixtures: PathBuf,
    },
    /// Post the review as a comment on its merge request, and grade the commit
    #[command(after_help = "Examples:
  rqx comment --base main             one comment, edited on every re-run
  rqx comment --dry-run               print the comment and its destination; post nothing
  rqx comment --marker api            a separate comment for a second job

GitLab requires RQX_TOKEN or GITLAB_TOKEN with `api` scope: CI_JOB_TOKEN cannot write
merge request notes. GitHub requires `permissions: pull-requests: write`, and
`statuses: write` for the commit grade.")]
    Comment {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Review against this revision. Defaults to the merge request's own base, as
        /// reported by the pipeline
        #[arg(long, value_name = "REF")]
        base: Option<String>,
        /// Print the comment and its destination, and post nothing
        #[arg(long)]
        dry_run: bool,
        /// Identifier of the comment to update. Jobs with different markers keep separate comments on one merge request
        #[arg(long, default_value = "results", value_name = "ID")]
        marker: String,
        /// Post the comment and leave the commit ungraded
        #[arg(long)]
        no_status: bool,
        #[command(flatten)]
        gate: Gate,
        #[command(flatten)]
        evidence: Evidence,
        #[command(flatten)]
        complexity: Complexity,
        #[command(flatten)]
        run: Run,
    },
    /// Write a commented rqx.yaml with every setting at its default
    Init {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Replace an rqx.yaml that is already there
        #[arg(long)]
        force: bool,
    },
    /// Print the settings a run will use, and the file each one came from
    Config {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Read settings from this file, layered over any `rqx.yaml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
        /// One JSON object per setting, then a summary, instead of the table
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Run as an MCP server, so an AI agent can query a project's findings
    Mcp {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Serve Streamable HTTP on this address instead of stdio
        #[arg(long, value_name = "ADDR")]
        http: Option<std::net::SocketAddr>,
        /// Read settings from this file, layered over any `rqx.yaml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Run as a Language Server over stdio, for editor integration
    Lsp {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Read settings from this file, layered over any `rqx.yaml` found
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
pub(crate) struct Standard {
    /// relaxed, standard, strict or pristine
    #[arg(long, value_name = "LEVEL", env = "RQX_LEVEL")]
    pub(crate) level: Option<String>,
}

impl Standard {
    pub(super) fn chosen(&self) -> Result<Option<rqx_core::config::Level>> {
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

/// What every command that scans accepts, declared once so a new scanning command
/// cannot forget one and an existing one cannot word it differently.
#[derive(Args, Default)]
pub(crate) struct Run {
    #[command(flatten)]
    pub(crate) standard: Standard,
    /// Worker threads. Defaults to the machine's parallelism
    #[arg(long, value_name = "N", env = "RQX_THREADS")]
    threads: Option<usize>,
    /// Read settings from this file, layered over any `rqx.yaml` found
    #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
    pub(crate) config: Option<PathBuf>,
}

/// Where a command that prints its own view writes it. `--json` stays as the short
/// way to say `--reporter json`.
#[derive(Args, Default)]
pub(crate) struct Printing {
    /// Where the output goes: `<name>` for stdout, `<name>=<path>` for a file;
    /// repeatable. The names are `terminal` and `json`
    #[arg(long = "reporter", value_name = "NAME[=PATH]")]
    reporters: Vec<Sink<Rendering>>,
}

impl Printing {
    pub(super) fn sinks(&self, json: bool) -> Result<Vec<Sink<Rendering>>> {
        let mut sinks = self.reporters.clone();
        if json {
            sinks.push(Sink {
                reporter: Rendering::Json,
                path: None,
            });
        }
        sink::settle(sinks)
    }

    pub(crate) fn is_given(&self) -> bool {
        !self.reporters.is_empty()
    }
}

/// What a test run or a profile saw, so a finding about code nothing ran can be
/// contradicted by the run that says otherwise.
#[derive(Args, Default)]
pub(crate) struct Evidence {
    /// A coverage file that can contradict a dead-code finding.
    /// lcov, a Go cover profile and coverage.py JSON are detected by content.
    #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
    coverage: Vec<PathBuf>,
    /// A CPU profile that can contradict a performance finding. A V8
    /// .cpuprofile, a Go pprof, speedscope JSON, JFR text and collapsed stacks are
    /// detected by content.
    #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
    profile: Vec<PathBuf>,
    /// A SARIF 2.1.0 file another tool wrote (ruff, ESLint, Semgrep), read as findings
    /// in the `external` family. They count for the gate and the reports and not for
    /// the health score
    #[arg(long = "sarif-in", value_name = "FILE", value_hint = ValueHint::FilePath)]
    sarif_in: Vec<PathBuf>,
}

impl Evidence {
    pub(crate) fn is_empty(&self) -> bool {
        self.coverage.is_empty() && self.profile.is_empty() && self.sarif_in.is_empty()
    }

    pub(super) fn apply(&self, settings: &mut config::Resolved) -> Result<()> {
        if !self.coverage.is_empty() {
            settings.set_by_flag("scan.coverage", "--coverage", |config| {
                config.scan.coverage = display_all(&self.coverage);
            })?;
        }
        if !self.profile.is_empty() {
            settings.set_by_flag("scan.profile", "--profile", |config| {
                config.scan.profile = display_all(&self.profile);
            })?;
        }
        if !self.sarif_in.is_empty() {
            settings.set_by_flag("scan.sarif_in", "--sarif-in", |config| {
                config.scan.sarif_in = display_all(&self.sarif_in);
            })?;
        }
        Ok(())
    }
}

pub(super) fn display_all(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect()
}

/// What a review is asked to say about complexity.
#[derive(Args, Default)]
pub(crate) struct Complexity {
    /// List the functions whose time order grew against the base revision, even when
    /// `gate.complexity_regression` is `off`. They are reported and do not change the
    /// exit code; the setting does that
    #[arg(long)]
    pub(crate) regressions: bool,
}

impl Complexity {
    pub(super) fn options(&self) -> check::Options {
        check::Options {
            regressions: self.regressions,
        }
    }
}

/// How many findings a report lists. The gate and the health score count every
/// finding whatever this says.
#[derive(Args, Default)]
pub(crate) struct DisplayCap {
    /// Maximum number of findings listed; 0 lists every finding. Does not affect the health score.
    #[arg(long)]
    max_findings: Option<usize>,
}

impl DisplayCap {
    pub(crate) fn is_set(&self) -> bool {
        self.max_findings.is_some()
    }

    pub(super) fn apply(&self, settings: &mut config::Resolved) -> Result<()> {
        if let Some(max) = self.max_findings {
            settings.set_by_flag("scan.max_findings", "--max-findings", |config| {
                config.scan.max_findings = max;
            })?;
        }
        Ok(())
    }
}

impl Run {
    /// The settings for a run over `path`, which has to exist: a run over a mistyped
    /// path would read no files and report a perfect score.
    pub(super) fn resolve(&self, path: &Path) -> Result<config::Resolved> {
        ensure_exists(path)?;
        let mut settings = config::resolve(path, self.config.as_deref(), self.standard.chosen()?)?;
        if let Some(count) = self.threads {
            settings.set_by_flag(
                "scan.threads",
                &given_by("--threads", Some("RQX_THREADS")),
                |config| config.scan.threads = count,
            )?;
        }
        Ok(settings)
    }
}

/// Exit 2 on a path that is not there, rather than a run that reads nothing and
/// reports a clean project.
pub(super) fn ensure_exists(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    anyhow::bail!(
        "`{}` does not exist. rqx reads a directory or a single file; try `.`",
        path.display()
    )
}

#[derive(Copy, Clone, Default, ValueEnum)]
pub(crate) enum GroupArg {
    #[default]
    File,
    Rule,
    Family,
    Severity,
    Owner,
}

impl From<GroupArg> for report::Grouping {
    fn from(chosen: GroupArg) -> Self {
        match chosen {
            GroupArg::File => Self::File,
            GroupArg::Rule => Self::Rule,
            GroupArg::Family => Self::Family,
            GroupArg::Severity => Self::Severity,
            GroupArg::Owner => Self::Owner,
        }
    }
}

#[derive(Copy, Clone, Default, ValueEnum)]
pub(crate) enum OrderArg {
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
pub(crate) struct View {
    /// Set from the global `-v` after parsing, so the report can see it without every
    /// command that flattens a view declaring the flag again.
    #[arg(skip)]
    pub(super) verbose: u8,
    /// Columns to wrap prose to; set when stdout is a terminal, so a pipe is never wrapped
    #[arg(skip)]
    pub(super) width: Option<usize>,
    /// Set when the terminal opens links and `--hyperlinks` allows them
    #[arg(skip)]
    pub(super) links: Option<report::Links>,
    /// Set when a terminal report taller than the window may go through a pager
    #[arg(skip)]
    pub(super) pager: Option<pager::Pager>,
    /// Show only findings at or above this severity
    #[arg(long, value_enum, help_heading = "Filtering")]
    pub(crate) severity: Option<SeverityArg>,
    /// Show only this rule; repeatable
    #[arg(long = "rule", value_name = "SLUG", help_heading = "Filtering")]
    pub(crate) rules: Vec<String>,
    /// Show only this smell family; repeatable
    #[arg(long = "category", value_name = "FAMILY", help_heading = "Filtering")]
    pub(super) categories: Vec<String>,
    /// Show only findings in files matching this glob; repeatable
    #[arg(long = "path", value_name = "GLOB", help_heading = "Filtering")]
    pub(super) paths: Vec<String>,
    /// Hide this rule; repeatable
    #[arg(long = "not-rule", value_name = "SLUG", help_heading = "Filtering")]
    pub(super) not_rules: Vec<String>,
    /// Hide this smell family; repeatable
    #[arg(
        long = "not-category",
        value_name = "FAMILY",
        help_heading = "Filtering"
    )]
    pub(super) not_categories: Vec<String>,
    /// Hide findings in files matching this glob; repeatable
    #[arg(long = "not-path", value_name = "GLOB", help_heading = "Filtering")]
    pub(super) not_paths: Vec<String>,
    /// Show only verified findings, or only heuristic ones
    #[arg(long, value_enum, help_heading = "Filtering")]
    pub(super) confidence: Option<ConfidenceArg>,
    /// Show only findings `rqx fix` can act on
    #[arg(long, help_heading = "Filtering")]
    pub(super) fixable: bool,
    /// Show only findings whose rule, file or evidence contains this text
    #[arg(long, value_name = "TEXT", help_heading = "Filtering")]
    pub(crate) search: Option<String>,
    /// Show only findings in files this revision changed
    #[arg(long, value_name = "REF", help_heading = "Filtering")]
    pub(super) since: Option<String>,
    /// Show only findings in files not yet committed
    #[arg(long, help_heading = "Filtering")]
    pub(super) changed: bool,
    /// Show only findings in the files this list names, one path per line, or
    /// separated by NUL when the input holds one (`git diff -z --name-only`); `-` reads
    /// standard input. Paths are relative to the scanned directory, or absolute. The
    /// whole project is still analysed, so cross-file findings remain correct. On `check`
    /// the list replaces the git diff as the files under review
    #[arg(
        long = "files-from",
        value_name = "FILE",
        value_hint = ValueHint::FilePath,
        help_heading = "Filtering"
    )]
    pub(crate) files_from: Option<String>,

    /// Also list files with no findings
    #[arg(long, help_heading = "Display")]
    pub(super) show_passed: bool,
    /// Print the summary without the findings
    #[arg(short, long, help_heading = "Display")]
    pub(super) quiet: bool,
    /// Print how long each stage of the scan took, and what it did
    #[arg(long, help_heading = "Display")]
    pub(crate) timings: bool,
    /// Counts only: the number of findings per rule and per family
    #[arg(long, help_heading = "Display")]
    pub(super) stats: bool,
    /// What findings are grouped by
    #[arg(long = "group-by", value_enum, default_value_t = GroupArg::File, help_heading = "Display")]
    pub(super) grouping: GroupArg,
    /// Sort order inside each group
    #[arg(long = "sort", value_enum, default_value_t = OrderArg::Line, help_heading = "Display")]
    pub(super) order: OrderArg,
    /// List only this many groups; the rest are counted
    #[arg(long, value_name = "N", help_heading = "Display")]
    pub(super) top: Option<usize>,
    /// Also list the findings a source comment waives
    #[arg(long, help_heading = "Display")]
    pub(super) show_suppressed: bool,
    /// Lines of code shown on each side of a finding. Defaults to 2 under `--detail
    /// full` and 0 otherwise
    #[arg(long, value_name = "N", help_heading = "Display")]
    pub(super) context: Option<u32>,
    /// How much of each finding to print
    #[arg(long, value_enum, default_value_t = DetailArg::Normal, help_heading = "Display")]
    pub(super) detail: DetailArg,
    /// Where a report goes: `<name>` for stdout, `<name>=<path>` for a file; repeatable.
    /// `html` is deprecated and will be removed in 0.6.0
    #[arg(
        long = "reporter",
        value_name = "NAME[=PATH]",
        help_heading = "Display"
    )]
    pub(super) reporters: Vec<Sink>,
    #[arg(long, value_enum, hide = true)]
    pub(super) format: Option<Reporter>,
}

/// Overrides for `gate` in `rqx.yaml`. The same repository often wants a strict
/// gate on its default branch and a lenient one everywhere else, which a committed
/// config file cannot express.
#[derive(Subcommand)]
pub(crate) enum ConfigAction {
    /// Write one setting, keeping the file's comments and layout
    Set {
        /// The setting, as `table.key`, such as `thresholds.max_params` or `gate.fail_on`
        key: String,
        value: String,
        /// File to write. Defaults to the file that already holds the setting,
        /// otherwise the nearest `rqx.yaml`.
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
    /// Print the JSON Schema of a config file, for an editor or a validator
    Schema,
    /// Describe one setting: its meaning, its default, its current value and where that value comes from
    Explain {
        /// The setting, dotted: `thresholds.max_params`, `gate.fail_on`,
        /// `rules.long-function.severity`
        key: String,
    },
    /// Check that the configuration loads, and report where it does not
    #[command(after_help = "Examples:
  rqx config validate                 exit 2 on an error, 0 otherwise
  rqx config validate --strict        exit 1 when there are warnings too
  rqx config validate --json          one line per problem, then a summary

The configuration is resolved the way a scan resolves it: presets and `extends`, the
user config, the policy, every `rqx.yaml` up to the repository root, RQX_CONFIG and
--config. An error is anything that stops a run. A warning is a setting that loads but
has no effect: a rule name not in the catalogue, a threshold no enabled rule reads, an
`overrides` entry with no `paths`.")]
    Validate {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        #[command(flatten)]
        standard: Standard,
        /// Exit 1 when the config has warnings, not only when it has errors
        #[arg(long)]
        strict: bool,
        /// One JSON object per problem, then a summary, instead of the text report
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        printing: Printing,
        /// Read settings from this file, layered over any `rqx.yaml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Convert RQX.toml files, the policies they point at and the user config to YAML
    Migrate {
        /// A directory to search for RQX.toml, or one TOML file to convert
        #[arg(value_hint = ValueHint::AnyPath)]
        path: Option<PathBuf>,
        /// Say what would change and write nothing
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum SetupTarget {
    /// Print the instructions that make a coding agent run rqx before it finishes
    Agent {
        /// The agent the instructions are for. Claude Code also gets a Stop hook
        #[arg(long = "for", value_enum, default_value_t = agent_setup::Agent::Generic)]
        agent: agent_setup::Agent,
        /// Add the instructions to the project's files, keeping what they already hold.
        /// Without it nothing on disk changes
        #[arg(long, short)]
        write: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum CacheAction {
    /// Show what the cache holds and whether the next scan can use it
    Status {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
        /// Read settings from this file, layered over any `rqx.yaml` found
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Delete the cache. Other files under `.rqx` are not changed
    Clear {
        #[arg(default_value = ".", value_hint = ValueHint::AnyPath)]
        path: PathBuf,
    },
}

/// Each setting can also come from the environment, so a pipeline sets its standard
/// once for every job. A flag wins over the variable, and the variable over
/// `rqx.yaml`; setting any of them opts a scan into gating, as a flag does. A variable
/// set to an empty string is not set, because a template that has nothing to say
/// still names it. A key the policy locks keeps its value: asking for another is
/// refused.
#[derive(Args)]
pub(crate) struct Gate {
    /// Lowest severity that fails the run
    #[arg(
        long,
        env = "RQX_FAIL_ON",
        value_parser = OrUnset(EnumValueParser::<FailOn>::new()),
        help_heading = "Gate"
    )]
    fail_on: Option<Setting<FailOn>>,
    /// Findings allowed before the run fails
    #[arg(
        long,
        env = "RQX_ALLOW_FINDINGS",
        value_parser = OrUnset(value_parser!(u32)),
        help_heading = "Gate"
    )]
    allow_findings: Option<Setting<u32>>,
    /// Health score below which the run fails; 0 disables
    #[arg(
        long,
        env = "RQX_MIN_HEALTH",
        value_parser = OrUnset(value_parser!(u32)),
        help_heading = "Gate"
    )]
    min_health: Option<Setting<u32>>,
    /// Lowest grade any single file may carry
    #[arg(
        long,
        env = "RQX_MIN_GRADE",
        value_parser = OrUnset(EnumValueParser::<MinGrade>::new()),
        help_heading = "Gate"
    )]
    min_grade: Option<Setting<MinGrade>>,
    /// Fail when a design pattern is half applied
    #[arg(
        long,
        env = "RQX_FAIL_ON_INCOMPLETE",
        value_parser = FalseyValueParser::new(),
        help_heading = "Gate"
    )]
    fail_on_incomplete: bool,
}

#[derive(Copy, Clone, ValueEnum)]
pub(crate) enum MinGrade {
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
    pub(crate) fn is_set(&self) -> bool {
        self.fail_on().is_some()
            || self.allow_findings().is_some()
            || self.min_health().is_some()
            || self.min_grade().is_some()
            || self.fail_on_incomplete
    }

    fn fail_on(&self) -> Option<FailOn> {
        Setting::of(self.fail_on)
    }

    fn allow_findings(&self) -> Option<u32> {
        Setting::of(self.allow_findings)
    }

    fn min_health(&self) -> Option<u32> {
        Setting::of(self.min_health)
    }

    fn min_grade(&self) -> Option<MinGrade> {
        Setting::of(self.min_grade)
    }

    /// Refuses a value for a key the policy locks, the way a config file is refused.
    pub(crate) fn apply(&self, settings: &mut config::Resolved) -> Result<()> {
        if let Some(fail_on) = self.fail_on() {
            settings.set_by_flag(
                "gate.fail_on",
                &given_by("--fail-on", Some("RQX_FAIL_ON")),
                |config| config.gate.fail_on = fail_on.into(),
            )?;
        }
        if let Some(allowed) = self.allow_findings() {
            settings.set_by_flag(
                "gate.max_findings",
                &given_by("--allow-findings", Some("RQX_ALLOW_FINDINGS")),
                |config| config.gate.max_findings = allowed,
            )?;
        }
        if let Some(health) = self.min_health() {
            settings.set_by_flag(
                "gate.min_health",
                &given_by("--min-health", Some("RQX_MIN_HEALTH")),
                |config| config.gate.min_health = health,
            )?;
        }
        if let Some(grade) = self.min_grade() {
            settings.set_by_flag(
                "gate.min_grade",
                &given_by("--min-grade", Some("RQX_MIN_GRADE")),
                |config| config.gate.min_grade = grade.into(),
            )?;
        }
        if self.fail_on_incomplete {
            settings.set_by_flag(
                "gate.fail_on_incomplete",
                &given_by("--fail-on-incomplete", Some("RQX_FAIL_ON_INCOMPLETE")),
                |config| config.gate.fail_on_incomplete = true,
            )?;
        }
        Ok(())
    }
}

/// The variable when it is set and the command line does not carry the flag, and the
/// flag otherwise: clap takes a flag over its variable. The interface parses commands
/// typed into it, which are not on the process's command line, so a variable that is
/// not set never answers.
fn given_by(long: &str, env: Option<&str>) -> String {
    let on_command_line = std::env::args_os().skip(1).any(|arg| {
        arg.to_str().is_some_and(|arg| {
            arg == long
                || arg
                    .strip_prefix(long)
                    .is_some_and(|rest| rest.starts_with('='))
        })
    });
    let variable_set =
        |variable: &str| std::env::var_os(variable).is_some_and(|value| !value.is_empty());
    match env {
        Some(variable) if !on_command_line && variable_set(variable) => variable.to_owned(),
        _ => long.to_owned(),
    }
}

#[derive(Copy, Clone, ValueEnum)]
pub(crate) enum SeverityArg {
    Info,
    Warning,
    Error,
}

/// `normal` prints a rule's reason once and a file's churn once, because both read
/// the same on every finding that repeats them.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum DetailArg {
    /// One line per finding
    Terse,
    /// The reason once per rule and the cost once per file
    #[default]
    Normal,
    /// Every line for every finding
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
pub(crate) enum ConfidenceArg {
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
pub(crate) enum FailOn {
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
pub(crate) enum Motion {
    On,
    Off,
}

#[derive(Copy, Clone, ValueEnum)]
pub(crate) enum Color {
    Auto,
    Always,
    Never,
}

impl Color {
    pub(super) const fn choice(self) -> anstream::ColorChoice {
        match self {
            Self::Auto => anstream::ColorChoice::Auto,
            Self::Always => anstream::ColorChoice::Always,
            Self::Never => anstream::ColorChoice::Never,
        }
    }
}

impl View {
    /// What the run itself knows and a flag cannot say: how chatty it was asked to be
    /// and how wide the screen is. Only the command line sets these; a view built any
    /// other way keeps the plain, unwrapped report.
    pub(super) fn on_terminal(self, verbose: u8, hyperlinks: term::LinkChoice) -> Self {
        let on_screen = term::is_terminal(term::Stream::Stdout);
        Self {
            verbose,
            width: term::terminal_width(term::Stream::Stdout),
            links: hyperlinks
                .wanted(on_screen)
                .then(report::Links::from_environment),
            ..self
        }
    }

    /// The pager a long report goes through; `None` prints it directly.
    pub(super) fn paged(self, pager: Option<pager::Pager>) -> Self {
        Self { pager, ..self }
    }
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
            ConfigAction::Migrate { .. }
            | ConfigAction::Validate { .. }
            | ConfigAction::Schema
            | ConfigAction::Explain { .. } => {
                unreachable!("answered before settings load")
            }
        }
    }
}

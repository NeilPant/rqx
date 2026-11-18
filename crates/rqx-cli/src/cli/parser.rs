//! Builds the parser: the grouped help and the flags each command shows first.

use crate::progress;

use super::args::Cli;

/// `--no-progress` is the older spelling of `--progress none`, and a request for
/// quiet output is a request not to be drawn at either.
pub(super) fn progress_style(
    requested: progress::Style,
    no_progress: bool,
    quiet: bool,
) -> progress::Style {
    if no_progress || quiet {
        progress::Style::None
    } else {
        requested
    }
}

/// The commands, in five groups. Order inside a group is the order a reader meets
/// them, not alphabetical: `scan` before `check` before `fix` is the loop itself.
///
/// A test asserts every subcommand is in exactly one group and every name here is a
/// real subcommand, so a thirtieth command fails the build until it is placed.
const GROUPS: [(&str, &[&str]); 5] = [
    ("The loop", &["scan", "check", "fix", "verify", "comment"]),
    (
        "Look closer",
        &[
            "complexity",
            "perf",
            "health",
            "owners",
            "trend",
            "slop",
            "hotspots",
            "graph",
            "plan",
            "patterns",
            "recommend",
        ],
    ),
    ("Reference", &["rules", "explain", "techniques", "levels"]),
    (
        "Project",
        &[
            "init",
            "config",
            "baseline",
            "suppressions",
            "waive",
            "cache",
        ],
    ),
    (
        "Integrations",
        &[
            "lsp",
            "mcp",
            "setup",
            "completions",
            "badge",
            "query",
            "update",
        ],
    ),
];

/// The parser, with the grouped list attached. Attached here rather than declared
/// on the derive because building the list reads the parser's own descriptions, and
/// an attribute that asks for the command while the command is being built never
/// finishes.
pub fn parser() -> clap::Command {
    use clap::CommandFactory;

    let base = with_common_flags(Cli::command());
    let grouped = grouped_help(&base);
    base.before_help(grouped).after_help(HELP_NOTES)
}

/// The flags each command's `-h` lists and the session's `/` menu offers first, the
/// ones people reach for. The rest stay accepted and stay in `--help`. Most commands
/// flatten the same structs (`View`, `Gate`, `Run`), so which of them is common
/// depends on the command, and this is the one place that says. A command left out
/// has no flag set apart: all of its flags are common.
const COMMON_FLAGS: &[(&str, &[&str])] = &[
    (
        "scan",
        &[
            "rule", "severity", "path", "not-path", "changed", "fixable", "group-by", "detail",
            "fail-on", "reporter",
        ],
    ),
    (
        "check",
        &[
            "base",
            "staged",
            "fix",
            "write",
            "files-from",
            "fail-on",
            "severity",
            "rule",
            "reporter",
        ],
    ),
    ("fix", &["write", "plan", "json", "markdown", "reporter"]),
    ("verify", &["base", "expect", "json", "reporter"]),
    (
        "comment",
        &["base", "dry-run", "marker", "no-status", "fail-on"],
    ),
    (
        "complexity",
        &["by", "top", "sort", "min", "histogram", "json", "reporter"],
    ),
    (
        "perf",
        &[
            "by",
            "top",
            "sort",
            "min-depth",
            "explain",
            "json",
            "reporter",
        ],
    ),
    ("health", &["by", "json", "reporter"]),
    ("owners", &["json", "reporter"]),
    ("slop", &["top", "min", "json", "reporter"]),
    ("hotspots", &["top", "since", "json", "reporter"]),
    ("graph", &["format", "reporter", "cycles", "focus", "depth"]),
    ("plan", &["budget", "limit", "simulate", "json", "reporter"]),
    ("patterns", &["coverage", "json", "reporter"]),
    ("recommend", &["only", "json", "reporter"]),
    ("baseline", &["update", "force"]),
    ("suppressions", &["expiring", "json", "reporter"]),
    ("waive", &["reason", "until", "whole-file", "dry-run"]),
    ("badge", &["output"]),
];

/// The global flags `-h` lists. They are repeated under every command, so the ones
/// that matter only now and then stay out of its short help.
const COMMON_GLOBAL_FLAGS: &[&str] = &["no-tui", "color", "verbose"];

/// Sets `hide_short_help` on every flag a command has that `COMMON_FLAGS` does not
/// name, and has its help flag say that `--help` lists the rest. Nothing is hidden
/// from `--help`, the parser or the completion scripts.
fn with_common_flags(command: clap::Command) -> clap::Command {
    let command = command.mut_args(|argument| match argument.get_long() {
        Some(long) if argument.is_global_set() && !COMMON_GLOBAL_FLAGS.contains(&long) => {
            argument.hide_short_help(true)
        }
        _ => argument,
    });
    command.mut_subcommands(|subcommand| {
        let Some((_, common)) = COMMON_FLAGS
            .iter()
            .find(|(name, _)| *name == subcommand.get_name())
        else {
            return subcommand;
        };
        with_help_hint(subcommand.mut_args(|argument| match argument.get_long() {
            Some(long) if !common.contains(&long) => argument.hide_short_help(true),
            _ => argument,
        }))
    })
}

/// Replaces clap's own help flag, whose hint reads "see more", with one that says
/// what `-h` leaves out. Only a subcommand with a flag left out of `-h` needs it: the
/// setting that turns clap's flag off is passed down to every subcommand, so the root
/// keeps clap's own.
fn with_help_hint(command: clap::Command) -> clap::Command {
    if !command
        .get_arguments()
        .any(clap::Arg::is_hide_short_help_set)
    {
        return command;
    }
    command.disable_help_flag(true).arg(
        clap::Arg::new("help")
            .short('h')
            .long("help")
            .action(clap::ArgAction::Help)
            .help("Print help; `--help` lists every option")
            .long_help("Print help; `-h` lists only the common options"),
    )
}

/// What follows the options: how a bare `rqx` behaves, the examples and the exit codes.
const HELP_NOTES: &str = "\
Run without a subcommand in a terminal, `rqx` opens the interactive session on the
working directory. In any other context it scans the working directory, as `rqx scan`
does. `rqx scan` reports import cycles, layer violations against a configured
`architecture`, dead code, duplication and churn. The commands under Look closer
present the same scan for one question.

Examples:
  rqx                              the session in a terminal, the scan elsewhere
  rqx scan .                       print the full report
  rqx scan . --fail-on error       report, and exit 1 if any finding is an error
  rqx check --base main            review the changed files and apply the gate
  rqx explain unused-export        what one rule measures and how to fix it

Exit codes:
  0   completed; no gate failed
  1   completed; the gate failed
  2   rqx could not run: bad path, invalid configuration, unresolvable revision
  101 internal error; please report it
";

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
    out.trim_end().to_owned()
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

#[cfg(test)]
mod common_flags {
    #[test]
    fn every_name_in_the_table_is_a_flag_of_that_command() {
        let parser = super::parser();
        for (name, common) in super::COMMON_FLAGS {
            let command = parser
                .find_subcommand(name)
                .unwrap_or_else(|| panic!("{name} is not a command"));
            for long in *common {
                assert!(
                    command.get_arguments().any(|a| a.get_long() == Some(long)),
                    "{name} has no --{long}"
                );
            }
        }
    }

    #[test]
    fn what_the_table_leaves_out_is_left_out_of_short_help_only() {
        let parser = super::parser();
        let scan = parser.find_subcommand("scan").expect("scan");
        let of = |long: &str| {
            scan.get_arguments()
                .find(|a| a.get_long() == Some(long))
                .unwrap_or_else(|| panic!("no --{long}"))
        };
        assert!(!of("rule").is_hide_short_help_set());
        assert!(of("timings").is_hide_short_help_set());
        assert!(!of("timings").is_hide_set());
        assert!(!of("timings").is_hide_long_help_set());
    }

    #[test]
    fn a_short_help_that_leaves_flags_out_says_where_they_are() {
        let mut parser = super::parser();
        let short = parser
            .find_subcommand_mut("scan")
            .expect("scan")
            .render_help()
            .to_string();
        let long = parser
            .find_subcommand_mut("scan")
            .expect("scan")
            .render_long_help()
            .to_string();
        assert!(short.contains("`--help` lists every option"), "{short}");
        assert!(!short.contains("--timings"), "{short}");
        assert!(long.contains("--timings"), "{long}");
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

    fn cli_of(arguments: &[&str]) -> super::Cli {
        let matches = super::parser()
            .try_get_matches_from(arguments)
            .unwrap_or_else(|error| panic!("{arguments:?}: {error}"));
        <super::Cli as clap::FromArgMatches>::from_arg_matches(&matches).unwrap()
    }

    #[test]
    fn quiet_and_no_progress_both_mean_no_progress() {
        use crate::progress::Style;

        assert_eq!(
            super::progress_style(Style::Plain, false, false),
            Style::Plain
        );
        assert_eq!(
            super::progress_style(Style::Plain, false, true),
            Style::None
        );
        assert_eq!(
            super::progress_style(Style::Plain, true, false),
            Style::None
        );
    }

    #[test]
    fn progress_is_accepted_before_and_after_the_command() {
        for arguments in [
            ["rqx", "--progress", "plain", "scan"],
            ["rqx", "scan", "--progress", "plain"],
        ] {
            assert_eq!(
                cli_of(&arguments).progress,
                crate::progress::Style::Plain,
                "{arguments:?}"
            );
        }
    }

    #[test]
    fn no_progress_is_accepted_before_and_after_the_command() {
        for arguments in [
            ["rqx", "--no-progress", "scan"],
            ["rqx", "scan", "--no-progress"],
        ] {
            let matches = super::parser()
                .try_get_matches_from(arguments)
                .unwrap_or_else(|error| panic!("{arguments:?}: {error}"));
            let cli = <super::Cli as clap::FromArgMatches>::from_arg_matches(&matches).unwrap();
            assert!(cli.no_progress, "{arguments:?}");
        }
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

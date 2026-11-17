//! Slash commands, which are the command line's own commands.
//!
//! `/plan --budget 20` is parsed by the same clap definition as `rqx plan --budget 20`,
//! then run against the scan the session already holds and printed by the same
//! functions the command line calls. Only the commands that need something a scan
//! does not hold (a base revision, a rescan with other rules on, a write to disk) go
//! back to the files.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::FromArgMatches;

use crate::brand::Palette;
use crate::config::Resolved;
use crate::report::{self, Report};
use crate::scan::{self, ScanOutcome};
use crate::{
    Cli, Command, DisplayCap, Evidence, Standard, check, complexity, fix, graph, parser, perf,
    render, verify,
};

/// What a typed line turned out to be.
pub enum Parsed {
    Nothing,
    Help,
    Clear,
    Exit,
    Rescan,
    Watch(WatchMode),
    /// Hand a finding's place to the editor.
    Open(Target),
    /// Text a command's own `--help` printed, ready to show as it is.
    Printed(Vec<u8>),
    Command(Box<Invocation>),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WatchMode {
    On,
    Off,
    Toggle,
}

/// Where `/open` goes: a place typed out, or a finding from the last listing, counted
/// from one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Place { file: String, line: u32 },
    Listed(usize),
}

/// A command line command, with the name it was typed under for the messages that
/// have to say it.
pub struct Invocation {
    pub name: String,
    pub command: Command,
}

/// Commands that only exist in the session.
pub const SESSION_ONLY: [(&str, &str); 6] = [
    ("rescan", "scan the project again"),
    (
        "watch",
        "re-analyse changed files as they are saved: on or off",
    ),
    (
        "open",
        "open a finding in your editor: file:line, or its number in the list",
    ),
    ("clear", "clear the screen"),
    ("help", "list the commands"),
    ("exit", "leave the session"),
];

/// The command line commands the session runs against its own scan. `findings` is
/// `scan`, named for what it shows.
pub const TWINS: [&str; 20] = [
    "findings",
    "plan",
    "health",
    "owners",
    "complexity",
    "perf",
    "graph",
    "recommend",
    "slop",
    "hotspots",
    "patterns",
    "suppressions",
    "waive",
    "explain",
    "rules",
    "techniques",
    "levels",
    "fix",
    "check",
    "verify",
];

/// The clap subcommand a slash name stands for.
pub fn subcommand_of(name: &str) -> &str {
    if name == "findings" { "scan" } else { name }
}

/// Every command with a line on what it is for, in the order `/help` lists them. The
/// sentence comes from the command line's own help so the two cannot drift.
pub fn help_rows() -> Vec<(String, String)> {
    let parser = parser();
    let twins = TWINS.iter().map(|name| {
        let about = parser
            .find_subcommand(subcommand_of(name))
            .map(about_of)
            .unwrap_or_default();
        ((*name).to_owned(), about)
    });
    let own = SESSION_ONLY
        .iter()
        .map(|(name, about)| ((*name).to_owned(), (*about).to_owned()));
    twins.chain(own).collect()
}

/// The first sentence of what a command is for, on one line.
pub fn about_of(command: &clap::Command) -> String {
    let about = command
        .get_about()
        .map(ToString::to_string)
        .unwrap_or_default();
    let about = about.split_whitespace().collect::<Vec<_>>().join(" ");
    let first = about
        .split_once(". ")
        .map_or(about.as_str(), |(head, _)| head);
    first.trim_end_matches('.').to_owned()
}

pub fn parse(line: &str) -> std::result::Result<Parsed, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(Parsed::Nothing);
    }
    let Some(typed) = line.strip_prefix('/') else {
        return invoke("scan", &[format!("--search={line}")]);
    };
    let words = split_words(typed)?;
    let Some((name, rest)) = words.split_first() else {
        return Ok(Parsed::Help);
    };
    match name.as_str() {
        "help" | "?" => Ok(Parsed::Help),
        "clear" => Ok(Parsed::Clear),
        "exit" | "quit" => Ok(Parsed::Exit),
        "rescan" => Ok(Parsed::Rescan),
        "watch" => watch(rest),
        "open" => open(rest),
        other => invoke(subcommand_of(other), rest),
    }
}

fn watch(arguments: &[String]) -> std::result::Result<Parsed, String> {
    match arguments {
        [] => Ok(Parsed::Watch(WatchMode::Toggle)),
        [state] if state == "on" => Ok(Parsed::Watch(WatchMode::On)),
        [state] if state == "off" => Ok(Parsed::Watch(WatchMode::Off)),
        _ => Err("/watch takes `on` or `off`".to_owned()),
    }
}

/// `/open` is the first finding of the list, `/open 3` the third, and `/open
/// src/a.ts:42` that place.
fn open(arguments: &[String]) -> std::result::Result<Parsed, String> {
    let usage = || "/open takes file:line, or a finding's number from the list".to_owned();
    match arguments {
        [] => Ok(Parsed::Open(Target::Listed(1))),
        [word] => match word.parse::<usize>() {
            Ok(0) => Err(usage()),
            Ok(number) => Ok(Parsed::Open(Target::Listed(number))),
            Err(_) => {
                let (file, line) = crate::waive::parse_target(word, false).map_err(|_| usage())?;
                Ok(Parsed::Open(Target::Place {
                    file: file.display().to_string(),
                    line,
                }))
            }
        },
        _ => Err(usage()),
    }
}

fn invoke(name: &str, arguments: &[String]) -> std::result::Result<Parsed, String> {
    let mut words = vec!["rqx".to_owned(), name.to_owned()];
    words.extend(arguments.iter().cloned());
    match parser().try_get_matches_from(words) {
        Ok(matches) => {
            let cli = Cli::from_arg_matches(&matches).map_err(|error| short(&error))?;
            let command = cli.command.ok_or_else(|| "nothing to run".to_owned())?;
            Ok(Parsed::Command(Box::new(Invocation {
                name: name.to_owned(),
                command,
            })))
        }
        Err(error) if error.use_stderr() => Err(short(&error)),
        Err(help) => Ok(Parsed::Printed(
            help.render().ansi().to_string().into_bytes(),
        )),
    }
}

/// The first line of what clap said, without its `error:` label, which is all a
/// prompt has room for.
fn short(error: &clap::Error) -> String {
    let text = error.render().to_string();
    let first = text.lines().next().unwrap_or_default();
    first.trim_start_matches("error: ").to_owned()
}

/// Shell-style words: whitespace separates, quotes group, a backslash escapes.
pub fn split_words(line: &str) -> std::result::Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        match (character, quote) {
            ('\\', _) => {
                word.push(characters.next().unwrap_or('\\'));
                started = true;
            }
            (open, Some(close)) if open == close => quote = None,
            (other, Some(_)) => word.push(other),
            ('\'' | '"', None) => {
                quote = Some(character);
                started = true;
            }
            (space, None) if space.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (other, None) => {
                word.push(other);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("a quote was never closed".to_owned());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

/// Everything a command may read from the session.
pub struct Context<'a> {
    pub outcome: Option<&'a ScanOutcome>,
    pub settings: &'a Resolved,
    pub palette: Palette,
    /// The level the session's scan ran under, so a command that asks for another one
    /// can be told it needs its own scan.
    pub level: Option<&'a str>,
}

impl<'a> Context<'a> {
    fn scanned(&self) -> Result<&'a ScanOutcome> {
        self.outcome
            .ok_or_else(|| anyhow::anyhow!("there is no scan yet; /rescan runs one"))
    }

    /// The session reads one directory with one set of settings. A flag that would
    /// change either belongs to a scan of its own, which is `rqx` run in a shell.
    fn same_scan(&self, path: &Path, standard: &Standard, config: Option<&PathBuf>) -> Result<()> {
        if path != Path::new(".") {
            bail!(
                "the session reads the working directory; run `rqx` on {} from a shell",
                path.display()
            );
        }
        if config.is_some() {
            bail!("`--config` changes the scan; run it from a shell");
        }
        if standard.level.as_deref() != self.level {
            bail!("`--level` changes the scan; run it from a shell");
        }
        Ok(())
    }
}

fn refuse_reporter(given: bool) -> Result<()> {
    if given {
        bail!("`--reporter` writes outside the session; `--json` prints here");
    }
    Ok(())
}

fn refuse_recording(trending: &crate::trend::Options) -> Result<()> {
    if trending.is_set() {
        bail!("`--record` writes a file; run it from a shell");
    }
    Ok(())
}

fn refuse_scan_changes(evidence: &Evidence, cap: &DisplayCap) -> Result<()> {
    if evidence.is_empty() && !cap.is_set() {
        return Ok(());
    }
    bail!("`--coverage`, `--profile` and `--max-findings` change the scan; run them from a shell")
}

/// What running a command asks the session to do.
pub enum Action {
    Print(Vec<u8>),
    /// A waiver that is missing the one thing the command line would refuse it for.
    /// The session asks for the reason on the prompt and then runs it.
    AskReason(crate::waive::Order),
    /// Work that scans or writes, so it runs off the input thread. `rescan` says the
    /// scan the session holds is out of date once it is done.
    Background {
        work: Box<dyn FnOnce() -> Result<Vec<u8>> + Send>,
        rescan: bool,
    },
}

fn print(render: impl FnOnce(&mut Vec<u8>) -> Result<()>) -> Result<Action> {
    let mut out = Vec::new();
    render(&mut out)?;
    Ok(Action::Print(out))
}

pub fn run(invocation: Invocation, context: &Context<'_>) -> Result<Action> {
    let palette = context.palette;
    match invocation.command {
        Command::Scan {
            path,
            view,
            gate,
            advice,
            evidence,
            cap,
            watch,
            trending,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_scan_changes(&evidence, &cap)?;
            refuse_recording(&trending)?;
            if watch {
                bail!("use /watch on");
            }
            let outcome = context.scanned()?;
            let mut settings = context.settings.clone();
            gate.apply(&mut settings.config);
            let verdict = gate.is_set().then(|| {
                crate::gate::judge(outcome, crate::gate::Scope::Project, &settings.config)
            });
            let sinks = crate::sinks(&view)?;
            let filter = crate::filter_of(&view, &path, listed(&view, &path)?.as_ref())?;
            let report = Report::scan(outcome, &filter);
            let report = match &verdict {
                Some(verdict) => report.judged(verdict),
                None => report,
            };
            print(|out| {
                if view.timings {
                    crate::emit_timed(&report, &sinks, &view, palette, out)?;
                } else {
                    for sink in &sinks {
                        crate::write_sink(&report, sink, &view, palette, out)?;
                    }
                }
                if advice {
                    report::advice(outcome, palette, out)?;
                }
                Ok(())
            })
        }
        Command::Plan {
            path,
            budget,
            limit,
            simulate,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            let request = render::Plan {
                budget,
                limit,
                simulate,
                json,
            };
            let outcome = context.scanned()?;
            print(|out| render::plan(outcome, &request, palette, out))
        }
        Command::Health {
            path,
            by,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            let outcome = context.scanned()?;
            print(|out| render::health(outcome, by, json, palette, out))
        }
        Command::Owners {
            path,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            let outcome = context.scanned()?;
            print(|out| render::owners(outcome, json, palette, out))
        }
        Command::Complexity {
            path,
            by,
            sort,
            top,
            min,
            histogram,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            let view = complexity::View {
                by,
                sort,
                top,
                min,
                histogram,
            };
            let outcome = context.scanned()?;
            print(|out| render::complexity(outcome, &view, json, context.settings, palette, out))
        }
        Command::Perf {
            path,
            by,
            sort,
            top,
            min_depth,
            profile,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            if !profile.is_empty() {
                bail!("`--profile` changes the scan; run it from a shell");
            }
            let view = perf::View {
                by,
                sort,
                top,
                min_depth,
            };
            let outcome = context.scanned()?;
            print(|out| render::perf(outcome, &view, json, context.settings, palette, out))
        }
        Command::Graph {
            path,
            format,
            cycles,
            focus,
            depth,
            reporters,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(!reporters.is_empty())?;
            let view = graph::View {
                shape: format.unwrap_or_default(),
                cycles_only: cycles,
                focus: focus.as_deref(),
                depth,
            };
            let outcome = context.scanned()?;
            print(|out| graph::render(outcome, &view, palette, out))
        }
        Command::Recommend {
            path,
            only,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            let outcome = context.scanned()?;
            print(|out| render::recommend(outcome, context.settings, only, json, palette, out))
        }
        Command::Patterns {
            path,
            coverage,
            json,
            printing,
            run,
        } => {
            if coverage {
                return print(|out| report::pattern_coverage(palette, out));
            }
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            let outcome = context.scanned()?;
            print(|out| render::patterns(outcome, json, palette, out))
        }
        Command::Suppressions {
            path,
            expiring,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            let outcome = context.scanned()?;
            print(|out| render::suppressions(&path, outcome, expiring, json, palette, out))
        }
        Command::Waive {
            at,
            rule,
            reason,
            until,
            whole_file,
            dry_run,
            config,
        } => {
            let order = crate::waive::Order {
                at,
                rule,
                reason: reason.unwrap_or_default(),
                until,
                whole_file,
                dry_run,
                config,
            };
            if order.reason.trim().is_empty() {
                return Ok(Action::AskReason(order));
            }
            Ok(waiving(order))
        }
        Command::Explain { name } => print(|out| render::explain(&name, palette, out)),
        Command::Rules {
            category,
            language,
            slugs,
        } => print(|out| {
            if slugs {
                render::rule_slugs(category.as_deref(), language.as_deref(), out)
            } else {
                render::rules(category.as_deref(), language.as_deref(), palette, out)
            }
        }),
        Command::Techniques => print(|out| report::techniques(palette, out)),
        Command::Levels => print(|out| report::levels(palette, out)),
        Command::Slop {
            path,
            top,
            min,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            // The signals are Info rules that ship off, which the session's scan did not
            // run, so this one scans again with its own family on.
            let mut settings = context.settings.clone();
            render::slop_settings(&mut settings);
            let request = render::Slop { top, min, json };
            background(false, move || {
                let outcome = scan::scan(&path, &settings)?;
                let mut out = Vec::new();
                render::slop(&outcome, &request, palette, &mut out)?;
                Ok(out)
            })
        }
        Command::Hotspots {
            path,
            top,
            since,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            // The session's scan reads the churn window of its own settings and caps its
            // findings, so this one scans again with the window and the cap it needs.
            let mut settings = context.settings.clone();
            settings.config.scan.max_findings = 0;
            let request = render::Hotspots::over(&mut settings, top, since).printed_as(json);
            background(false, move || {
                let outcome = scan::scan(&path, &settings)?;
                let mut out = Vec::new();
                render::hotspots(&outcome, &request, palette, &mut out)?;
                Ok(out)
            })
        }
        Command::Fix {
            path,
            write,
            max_rounds,
            json,
            plan,
            markdown,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            if plan {
                let outcome = context.scanned()?;
                return print(|out| {
                    if json {
                        report::brief_json(outcome, context.settings, out)
                    } else if markdown {
                        report::brief_markdown(outcome, context.settings, out)
                    } else {
                        report::brief(outcome, context.settings, palette, out)
                    }
                });
            }
            let settings = context.settings.clone();
            let rounds = max_rounds.unwrap_or(settings.config.fix.max_rounds).max(1);
            background(write, move || {
                let options = fix::Options {
                    write,
                    max_rounds: rounds,
                    ..fix::Options::default()
                };
                let outcome = fix::run(&path, &settings, options)?;
                let mut out = Vec::new();
                if json {
                    report::fixes_json(&outcome, &mut out)?;
                } else {
                    report::fixes(&outcome, palette, "/fix --write", &mut out)?;
                }
                Ok(out)
            })
        }
        Command::Check {
            path,
            base,
            staged,
            fix,
            write: _,
            view,
            gate,
            evidence,
            cap,
            trending,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_scan_changes(&evidence, &cap)?;
            if staged {
                bail!("`--staged` reads the index; run `rqx check --staged` from a shell");
            }
            refuse_recording(&trending)?;
            if fix {
                bail!("`--fix` writes files; run `rqx check --fix` from a shell, or /fix");
            }
            let mut settings = context.settings.clone();
            gate.apply(&mut settings.config);
            let sinks = crate::sinks(&view)?;
            background(false, move || {
                let filter = crate::filter_of(&view, &path, listed(&view, &path)?.as_ref())?;
                let review = check::review(&path, &settings, &base)?;
                let verdict = review.judge(&settings.config);
                let report = Report::review(&review, &verdict, &filter);
                let mut out = Vec::new();
                for sink in &sinks {
                    crate::write_sink(&report, sink, &view, palette, &mut out)?;
                }
                Ok(out)
            })
        }
        Command::Verify {
            path,
            base,
            expect,
            json,
            printing,
            run,
        } => {
            context.same_scan(&path, &run.standard, run.config.as_ref())?;
            refuse_reporter(printing.is_given())?;
            let settings = context.settings.clone();
            background(false, move || {
                verify::ensure_repository(&path, &base)?;
                let verified = verify::verify(&path, &settings, &base, &expect)?;
                let mut out = Vec::new();
                if json {
                    report::verify_json(&verified, &mut out)?;
                } else {
                    report::verify(&verified, palette, &mut out)?;
                }
                Ok(out)
            })
        }
        _ => bail!(
            "`/{}` is not part of the session; run `rqx {}` from a shell",
            invocation.name,
            invocation.name
        ),
    }
}

/// A waiver edits a file, so the scan the session holds is out of date once it is
/// written. A dry run writes nothing and leaves the scan alone.
pub fn waiving(order: crate::waive::Order) -> Action {
    let rescan = !order.dry_run;
    Action::Background {
        work: Box::new(move || {
            let mut out = Vec::new();
            crate::waive::run(&order, &mut out)?;
            Ok(out)
        }),
        rescan,
    }
}

fn background(
    rescan: bool,
    work: impl FnOnce() -> Result<Vec<u8>> + Send + 'static,
) -> Result<Action> {
    Ok(Action::Background {
        work: Box::new(work),
        rescan,
    })
}

/// `--files-from` in the session, which can read a named list but not standard input:
/// the session's standard input is the terminal the prompt is reading.
fn listed(
    view: &crate::View,
    root: &std::path::Path,
) -> anyhow::Result<Option<crate::listed::Listed>> {
    if view.files_from.as_deref() == Some("-") {
        anyhow::bail!(
            "`--files-from -` reads standard input, which the session is using; name a file instead"
        );
    }
    crate::listed_files(view, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(line: &str) -> Command {
        match parse(line) {
            Ok(Parsed::Command(invocation)) => invocation.command,
            Ok(_) => panic!("{line} did not parse to a command"),
            Err(message) => panic!("{line}: {message}"),
        }
    }

    #[test]
    fn a_slash_command_is_parsed_by_the_command_lines_own_definition() {
        let Command::Plan { budget, limit, .. } = command("/plan --budget 20") else {
            panic!("not a plan");
        };
        assert_eq!(budget, Some(20));
        assert_eq!(limit, 20, "an unspoken flag keeps its default");
    }

    #[test]
    fn findings_is_scan_and_its_flags_come_with_it() {
        let Command::Scan { view, .. } = command("/findings --severity error --rule empty-catch")
        else {
            panic!("not a scan");
        };
        assert_eq!(view.rules, ["empty-catch"]);
        assert!(view.severity.is_some());
    }

    #[test]
    fn text_without_a_slash_searches_the_findings() {
        let Command::Scan { view, .. } = command("dead export") else {
            panic!("not a scan");
        };
        assert_eq!(view.search.as_deref(), Some("dead export"));
    }

    #[test]
    fn text_that_looks_like_a_flag_is_still_only_text() {
        let Command::Scan { view, .. } = command("--weird") else {
            panic!("not a scan");
        };
        assert_eq!(view.search.as_deref(), Some("--weird"));
    }

    #[test]
    fn quotes_group_words_and_a_backslash_escapes() {
        assert_eq!(
            split_words(r#"graph --focus "src/a b.ts" x\ y 'z'"#).unwrap(),
            ["graph", "--focus", "src/a b.ts", "x y", "z"]
        );
        assert!(split_words("graph --focus 'oops").is_err());
        assert_eq!(split_words("a ''").unwrap(), ["a", ""]);
    }

    #[test]
    fn the_session_only_commands_are_recognised() {
        assert!(matches!(parse("/exit"), Ok(Parsed::Exit)));
        assert!(matches!(parse("/quit"), Ok(Parsed::Exit)));
        assert!(matches!(parse("/rescan"), Ok(Parsed::Rescan)));
        assert!(matches!(parse("/clear"), Ok(Parsed::Clear)));
        assert!(matches!(parse("/help"), Ok(Parsed::Help)));
        assert!(matches!(parse("/"), Ok(Parsed::Help)));
        assert!(matches!(parse("  "), Ok(Parsed::Nothing)));
        assert!(matches!(
            parse("/watch on"),
            Ok(Parsed::Watch(WatchMode::On))
        ));
        assert!(matches!(
            parse("/watch off"),
            Ok(Parsed::Watch(WatchMode::Off))
        ));
        assert!(matches!(
            parse("/watch"),
            Ok(Parsed::Watch(WatchMode::Toggle))
        ));
        assert!(parse("/watch sideways").is_err());
    }

    #[test]
    fn open_takes_a_place_or_a_number_from_the_list() {
        let Ok(Parsed::Open(place)) = parse("/open src/a.ts:42") else {
            panic!("not an open");
        };
        assert_eq!(
            place,
            Target::Place {
                file: "src/a.ts".into(),
                line: 42
            }
        );
        assert!(matches!(
            parse("/open 3"),
            Ok(Parsed::Open(Target::Listed(3)))
        ));
        assert!(matches!(
            parse("/open"),
            Ok(Parsed::Open(Target::Listed(1)))
        ));
        assert!(parse("/open 0").is_err());
        assert!(parse("/open src/a.ts").is_err());
        assert!(parse("/open a b").is_err());
    }

    #[test]
    fn waive_is_the_command_lines_and_a_missing_reason_is_asked_for() {
        let Command::Waive {
            at,
            rule,
            reason,
            whole_file,
            ..
        } = command("/waive src/a.ts:7 empty-catch --whole-file")
        else {
            panic!("not a waive");
        };
        assert_eq!((at.as_str(), rule.as_str()), ("src/a.ts:7", "empty-catch"));
        assert!(whole_file);
        assert_eq!(reason, None);

        let Command::Waive { reason, .. } =
            command("/waive src/a.ts:7 empty-catch --reason \"called by the host\"")
        else {
            panic!("not a waive");
        };
        assert_eq!(reason.as_deref(), Some("called by the host"));
    }

    #[test]
    fn a_waiver_without_a_reason_asks_instead_of_writing() {
        let settings = Resolved::default();
        let Ok(Parsed::Command(invocation)) = parse("/waive src/a.ts:7 empty-catch") else {
            panic!("not a command");
        };
        let context = Context {
            outcome: None,
            settings: &settings,
            palette: Palette::with_depth(crate::brand::ColorDepth::Monochrome),
            level: None,
        };
        assert!(matches!(
            run(*invocation, &context),
            Ok(Action::AskReason(_))
        ));
    }

    #[test]
    fn a_clap_error_is_one_short_line() {
        let Err(message) = parse("/plan --nonsense") else {
            panic!("an unknown flag parsed");
        };
        assert!(message.contains("--nonsense"), "{message}");
        assert!(!message.contains('\n'));
        assert!(!message.starts_with("error"));
        let Err(unknown) = parse("/nothing-like-this") else {
            panic!("an unknown command parsed");
        };
        assert!(unknown.contains("nothing-like-this"), "{unknown}");
    }

    #[test]
    fn help_for_a_command_is_printed_rather_than_refused() {
        let Ok(Parsed::Printed(text)) = parse("/plan --help") else {
            panic!("--help was not printed");
        };
        assert!(String::from_utf8_lossy(&text).contains("--budget"));
    }

    /// The point of slash commands being the command line's own: the same arguments,
    /// run against the same scan, print the same bytes by either route.
    mod parity {
        use super::*;
        use crate::View;
        use crate::brand::ColorDepth;
        use crate::report::FindingFilter;
        use crate::report::tests::outcome_of;
        use rqx_core::rule::RuleId;

        type Direct = fn(&ScanOutcome, &Resolved, Palette, &mut Vec<u8>) -> Result<()>;

        fn fixture() -> ScanOutcome {
            outcome_of(&[
                (RuleId::LongFunction, "src/api/client.ts"),
                (RuleId::LongFunction, "src/api/client.ts"),
                (RuleId::LongFunction, "src/billing/invoice.ts"),
                (RuleId::UnusedExport, "src/billing/totals.ts"),
            ])
        }

        fn via_session(line: &str, outcome: &ScanOutcome, settings: &Resolved) -> Vec<u8> {
            let palette = Palette::with_depth(ColorDepth::Truecolor);
            let Ok(Parsed::Command(invocation)) = parse(line) else {
                panic!("{line} is not a command");
            };
            let context = Context {
                outcome: Some(outcome),
                settings,
                palette,
                level: None,
            };
            match run(*invocation, &context) {
                Ok(Action::Print(bytes)) => bytes,
                Ok(Action::Background { .. } | Action::AskReason(_)) => {
                    panic!("{line} did not print")
                }
                Err(error) => panic!("{line}: {error:#}"),
            }
        }

        fn terminal(
            outcome: &ScanOutcome,
            view: &View,
            palette: Palette,
            out: &mut Vec<u8>,
        ) -> Result<()> {
            let report = Report::scan(outcome, &FindingFilter::default());
            let sink = crate::Sink {
                reporter: report::Reporter::Terminal,
                path: None,
            };
            crate::write_sink(&report, &sink, view, palette, out)
        }

        #[test]
        fn each_slash_command_prints_what_its_command_line_twin_prints() {
            let outcome = fixture();
            let settings = Resolved::default();
            let palette = Palette::with_depth(ColorDepth::Truecolor);
            let cases: [(&str, Direct); 10] = [
                ("/findings", |outcome, _, palette, out| {
                    terminal(outcome, &View::default(), palette, out)
                }),
                ("/plan --budget 20", |outcome, _, palette, out| {
                    let request = render::Plan {
                        budget: Some(20),
                        limit: 20,
                        simulate: false,
                        json: false,
                    };
                    render::plan(outcome, &request, palette, out)
                }),
                ("/plan --simulate --json", |outcome, _, palette, out| {
                    let request = render::Plan {
                        budget: None,
                        limit: 20,
                        simulate: true,
                        json: true,
                    };
                    render::plan(outcome, &request, palette, out)
                }),
                ("/health", |outcome, _, palette, out| {
                    render::health(outcome, crate::health::By::Family, false, palette, out)
                }),
                ("/owners", |outcome, _, palette, out| {
                    render::owners(outcome, false, palette, out)
                }),
                ("/complexity --top 5", |outcome, settings, palette, out| {
                    let view = complexity::View {
                        by: complexity::By::Function,
                        sort: complexity::Sort::Complexity,
                        top: 5,
                        min: 0,
                        histogram: false,
                    };
                    render::complexity(outcome, &view, false, settings, palette, out)
                }),
                ("/perf", |outcome, settings, palette, out| {
                    let view = perf::View {
                        by: perf::By::Function,
                        sort: perf::Sort::Weight,
                        top: 20,
                        min_depth: 0,
                    };
                    render::perf(outcome, &view, false, settings, palette, out)
                }),
                ("/patterns", |outcome, _, palette, out| {
                    render::patterns(outcome, false, palette, out)
                }),
                ("/suppressions --expiring", |outcome, _, palette, out| {
                    render::suppressions(Path::new("."), outcome, true, false, palette, out)
                }),
                ("/explain long-function", |_, _, palette, out| {
                    render::explain("long-function", palette, out)
                }),
            ];
            for (line, direct) in cases {
                let mut expected = Vec::new();
                direct(&outcome, &settings, palette, &mut expected).expect(line);
                assert!(!expected.is_empty(), "{line} printed nothing");
                assert_eq!(
                    String::from_utf8_lossy(&via_session(line, &outcome, &settings)),
                    String::from_utf8_lossy(&expected),
                    "{line} differs from its command line twin"
                );
            }
        }

        #[test]
        fn a_command_that_needs_a_scan_says_so_when_there_is_none() {
            let settings = Resolved::default();
            let Ok(Parsed::Command(invocation)) = parse("/plan") else {
                panic!("not a command");
            };
            let context = Context {
                outcome: None,
                settings: &settings,
                palette: Palette::with_depth(ColorDepth::Monochrome),
                level: None,
            };
            let Err(error) = run(*invocation, &context) else {
                panic!("ran without a scan");
            };
            assert!(error.to_string().contains("no scan yet"));
        }

        #[test]
        fn a_flag_that_would_change_the_scan_is_refused_rather_than_ignored() {
            let outcome = fixture();
            let settings = Resolved::default();
            for line in [
                "/plan src",
                "/health --level strict",
                "/findings --coverage x.lcov",
            ] {
                let Ok(Parsed::Command(invocation)) = parse(line) else {
                    panic!("{line} is not a command");
                };
                let context = Context {
                    outcome: Some(&outcome),
                    settings: &settings,
                    palette: Palette::with_depth(ColorDepth::Monochrome),
                    level: None,
                };
                assert!(run(*invocation, &context).is_err(), "{line} was accepted");
            }
        }

        #[test]
        fn a_command_the_session_does_not_host_names_the_one_to_run() {
            let outcome = fixture();
            let settings = Resolved::default();
            let Ok(Parsed::Command(invocation)) = parse("/init") else {
                panic!("not a command");
            };
            let context = Context {
                outcome: Some(&outcome),
                settings: &settings,
                palette: Palette::with_depth(ColorDepth::Monochrome),
                level: None,
            };
            let Err(error) = run(*invocation, &context) else {
                panic!("init ran");
            };
            assert!(error.to_string().contains("rqx init"));
        }
    }

    #[test]
    fn help_lists_every_command_on_one_line_each() {
        let rows = help_rows();
        assert_eq!(rows.len(), TWINS.len() + SESSION_ONLY.len());
        for (name, about) in rows {
            assert!(!about.is_empty(), "/{name} has no description");
            assert!(!about.contains('\n'), "/{name}: {about}");
        }
    }

    #[test]
    fn every_twin_names_a_subcommand_the_parser_has() {
        let parser = parser();
        for name in TWINS {
            assert!(
                parser.find_subcommand(subcommand_of(name)).is_some(),
                "/{name} has no command behind it"
            );
        }
    }
}

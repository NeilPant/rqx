//! Runs the parsed command and turns its result into an exit status.

use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::FromArgMatches;
use rqx_core::finding::Severity;

use crate::brand::Palette;
use crate::gate::{Scope, judge};
use crate::report::{FindingFilter, Report};
use crate::sink::{Rendering, Sink};
use crate::{
    agent_setup, badge, baseline, cache, check, complexity, config, edit_config, explain, fix,
    forge, graph, history, listed, lsp, mcp, migrate, pager, perf, progress, query, render, report,
    scan, session, setup, sink, staged, update, verify, waive, watch,
};

use super::args::{
    CacheAction, Cli, Command, ConfigAction, Motion, SetupTarget, Standard, View, display_all,
};
use super::emit::{
    companion_stream, comparison_for, emit, sinks, update_notice, warn_of_deprecated,
};
use super::parser::{parser, progress_style};
use super::update_command::{UpdateRequest, update_command};
use crate::Exit;

/// The list `--files-from` names, read once because standard input cannot be read
/// twice, and said once when it named things that are not there.
pub(crate) fn listed_files(view: &View, root: &Path) -> Result<Option<listed::Listed>> {
    let Some(source) = view.files_from.as_deref() else {
        return Ok(None);
    };
    let directory = if root.is_file() {
        root.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    } else {
        root
    };
    let listed = listed::Listed::read(source, directory)?;
    if let Some(note) = listed.note_on_missing() {
        eprintln!("  {note}");
    }
    Ok(Some(listed))
}

pub(crate) fn filter_of(
    view: &View,
    root: &Path,
    listed: Option<&listed::Listed>,
) -> Result<FindingFilter> {
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
        let base = history::revision(base).map_err(|refused| anyhow::anyhow!("{refused}"))?;
        touched = Some(history::changed_files(root, base).map_err(|git_said| {
            anyhow::anyhow!("`{base}` is not a revision this repository knows about: {git_said}")
        })?);
    }
    if view.changed {
        touched
            .get_or_insert_with(Vec::new)
            .extend(history::uncommitted(root));
    }
    if let Some(listed) = listed {
        touched
            .get_or_insert_with(Vec::new)
            .extend(listed.files.iter().cloned());
    }
    Ok(match touched {
        Some(files) => filter.touching(files),
        None => filter,
    })
}

/// A config that does not load is a config rqx cannot run with, which is exit 2 like
/// any other run that could not start; one that loads and has warnings is only a
/// failure when `--strict` says warnings are.
fn validate_config(
    path: &Path,
    explicit: Option<&Path>,
    standard: &Standard,
    strict: bool,
    sinks: &[Sink<Rendering>],
    palette: Palette,
) -> Result<Exit> {
    let validation = config::validate(path, explicit, standard.chosen()?);
    let passes = validation.passes(strict);
    sink::deliver(sinks, palette, |rendering, palette, out| match rendering {
        Rendering::Json => report::validation_json(&validation, passes, out),
        Rendering::Terminal => report::validation(&validation, strict, palette, out),
    })?;
    Ok(if validation.count(config::Severity::Error) > 0 {
        Exit::Failed
    } else {
        Exit::of(!passes)
    })
}

/// Opens the session on the working directory. `None` means the terminal could not
/// host it after all, and the caller prints the report instead.
fn open_session(motion: Motion, wheel: bool) -> Result<Option<Exit>> {
    let Command::Scan { run, .. } = bare_scan()? else {
        bail!("bare `rqx` did not parse as a scan");
    };
    let path = Path::new(".");
    let settings = run.resolve(path)?;
    let options = session::Launch {
        path,
        config: &settings,
        level: run.standard.level.as_deref(),
        reduced_motion: matches!(motion, Motion::Off),
        wheel,
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
    let path = config::init(root, force)?;
    println!(
        "\n  wrote {}\n  every value is the default; delete what you agree with.\n",
        path.display()
    );
    Ok(())
}

pub(crate) fn run() -> Result<Exit> {
    let cli = Cli::from_arg_matches(&parser().get_matches()).unwrap_or_else(|error| error.exit());
    // Set before anything draws: `anstream::stdout()` and `Palette::detect()` both
    // read this global, so one flag governs every writer.
    cli.color.choice().write_global();
    let animated = !matches!(cli.motion, Motion::Off);
    let (requested, no_progress, verbosity) = (cli.progress, cli.no_progress, cli.verbose);
    let hyperlinks = cli.hyperlinks;
    let no_pager = cli.no_pager;
    let progress_for =
        |quiet| progress::Mode::new(progress_style(requested, no_progress, quiet), animated);
    let progress = progress_for(false);
    let palette = Palette::detect();

    let command = match cli.command {
        Some(command) => command,
        None => {
            // The gate says no for a pipe, CI and a window too small to draw in, and
            // for all of them the answer is the report, so a pipeline running `rqx`
            // gets the scan's exit code rather than a help page that exits 0.
            if !cli.no_tui
                && session::gate().is_ok()
                && let Some(exit) = open_session(cli.motion, !cli.no_mouse)?
            {
                return Ok(exit);
            }
            bare_scan()?
        }
    };

    match command {
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
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx scans a directory or a single file; try `rqx scan .`",
                    path.display()
                );
            }
            let mut settings = run.resolve(&path)?;
            // Started before the scan rather than after it: the answer needs the length
            // of the work to arrive in, and `process::exit` does not wait for a thread.
            update::start(settings.config.update.check);
            cap.apply(&mut settings)?;
            evidence.apply(&mut settings)?;
            gate.apply(&mut settings)?;

            let progress = progress_for(view.quiet);
            // A watch prints a report on every save, and a pager would hold the loop.
            let view = view
                .on_terminal(verbosity, hyperlinks)
                .paged(pager::for_run(no_pager || watch));
            let sinks = sinks(&view)?;
            warn_of_deprecated(&sinks);
            let listed = listed_files(&view, &path)?;
            let filter = filter_of(&view, &path, listed.as_ref())?;
            let run = |settings: &config::Resolved| -> Result<Exit> {
                let outcome =
                    scan::scan_drawn_against(&path, settings, progress, comparison_for(&sinks))?;
                let history = trending.apply(&outcome)?;

                // A bare `rqx scan` reports; it only gates when asked to, so adding a
                // flag is the whole opt-in and no existing pipeline changes meaning.
                let verdict = gate
                    .is_set()
                    .then(|| judge(&outcome, Scope::Project, &settings.config, &[]));
                let report = Report::scan(&outcome, &filter).trended(history.as_deref());
                let report = match &verdict {
                    Some(verdict) => report.judged(verdict),
                    None => report,
                };
                emit(&report, &sinks, &view, palette)?;
                if advice {
                    let mut out = companion_stream(&sinks);
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
            write,
            max_rounds,
            json,
            plan,
            markdown,
            verify_build,
            printing,
            run,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx fixes a directory or a single file; try `rqx fix .`",
                    path.display()
                );
            }
            let settings = run.resolve(&path)?;
            let sinks = printing.sinks(json)?;
            if plan {
                let scanned = scan::scan_drawn(&path, &settings, progress)?;
                return sink::deliver(&sinks, palette, |rendering, palette, out| match rendering {
                    Rendering::Json => report::brief_json(&scanned, &settings, out),
                    Rendering::Terminal if markdown => {
                        report::brief_markdown(&scanned, &settings, out)
                    }
                    Rendering::Terminal => report::brief(&scanned, &settings, palette, out),
                })
                .map(|()| Exit::Clean);
            }
            let rounds = max_rounds.unwrap_or(settings.config.fix.max_rounds).max(1);
            let outcome = fix::run_drawn(
                &path,
                &settings,
                fix::Options {
                    write,
                    max_rounds: rounds,
                    verify_build,
                    ..fix::Options::default()
                },
                progress,
            )?;
            sink::deliver(&sinks, palette, |rendering, palette, out| match rendering {
                Rendering::Json => report::fixes_json(&outcome, out),
                Rendering::Terminal => report::fixes(&outcome, palette, "rqx fix --write", out),
            })
            .map(|()| Exit::Clean)
        }
        Command::Verify {
            path,
            base,
            expect,
            json,
            printing,
            run,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx verifies a directory; try `rqx verify .`",
                    path.display()
                );
            }
            let mut settings = run.resolve(&path)?;
            settings.config.scan.max_findings = 0;
            let sinks = printing.sinks(json)?;
            verify::ensure_repository(&path, &base)?;
            let verified = verify::verify_drawn(&path, &settings, &base, &expect, progress)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| match rendering {
                Rendering::Json => report::verify_json(&verified, out),
                Rendering::Terminal => report::verify(&verified, palette, out),
            })?;
            Ok(Exit::of(!verified.passed()))
        }
        Command::Setup {
            target: Some(SetupTarget::Agent { agent, write }),
            ..
        } => {
            let mut out = anstream::stdout().lock();
            let outputs = agent_setup::outputs(agent);
            if !write {
                return report::agent(&outputs, palette, &mut out).map(|()| Exit::Clean);
            }
            for output in &outputs {
                let change = output.apply(Path::new("."))?;
                report::agent_change(output.path, change, palette, &mut out)?;
            }
            Ok(Exit::Clean)
        }
        Command::Setup { editor, write, .. } => {
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
            budget,
            limit,
            simulate,
            json,
            printing,
            run,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx plans a directory or a single file; try `rqx plan .`",
                    path.display()
                );
            }
            let mut settings = run.resolve(&path)?;
            // The display cap keeps the highest-ranked findings, and the plan is a
            // ranking of its own: capping first would hide the cheap work entirely.
            settings.config.scan.max_findings = 0;
            let sinks = printing.sinks(json)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                let request = render::Plan {
                    budget,
                    limit,
                    simulate,
                    json: rendering.is_json(),
                };
                render::plan(&outcome, &request, palette, out)
            })
            .map(|()| Exit::Clean)
        }
        Command::Explain { name } => {
            render::explain(&name, palette, &mut anstream::stdout().lock()).map(|()| Exit::Clean)
        }
        Command::Patterns {
            path,
            coverage,
            json,
            printing,
            run,
        } => {
            if coverage {
                return report::pattern_coverage(palette, &mut anstream::stdout().lock())
                    .map(|()| Exit::Clean);
            }
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx reads a directory or a single file; try                      `rqx patterns .`",
                    path.display()
                );
            }
            let settings = run.resolve(&path)?;
            let sinks = printing.sinks(json)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                render::patterns(&outcome, rendering.is_json(), palette, out)
            })
            .map(|()| Exit::Clean)
        }
        Command::Baseline {
            path,
            update,
            force,
            run,
        } => {
            let settings = run.resolve(&path)?;
            let taken = baseline::take_drawn(&path, &settings, update, force, progress)?;
            println!(
                "  recorded {} findings in {}{}",
                taken.findings,
                taken.path.display(),
                match taken.replaced_findings {
                    Some(was) if was != taken.findings => format!(" ({was} before)"),
                    _ => String::new(),
                }
            );
            if !taken.configured {
                println!(
                    "  set `gate.baseline: {}` in rqx.yaml to gate on new findings only",
                    baseline::DEFAULT_PATH
                );
            }
            Ok(Exit::Clean)
        }
        Command::Suppressions {
            path,
            expiring,
            json,
            printing,
            run,
        } => {
            let mut settings = run.resolve(&path)?;
            // A waiver is only visible when the finding it covers was found, and the
            // display cap drops findings before anything can read them.
            settings.config.scan.max_findings = 0;
            let sinks = printing.sinks(json)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                render::suppressions(&path, &outcome, expiring, rendering.is_json(), palette, out)
            })
            .map(|()| Exit::Clean)
        }
        Command::Waive {
            at,
            rule,
            reason,
            until,
            whole_file,
            dry_run,
            config,
        } => waive::run(
            &waive::Order {
                at,
                rule,
                reason: reason.unwrap_or_default(),
                until,
                whole_file,
                dry_run,
                config,
            },
            &mut anstream::stdout().lock(),
        )
        .map(|()| Exit::Clean),
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
            let settings = run.resolve(&path)?;
            let sinks = printing.sinks(json)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            let view = complexity::View {
                by,
                sort,
                top,
                min,
                histogram,
            };
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                render::complexity(
                    &outcome,
                    &view,
                    rendering.is_json(),
                    &settings,
                    palette,
                    out,
                )
            })
            .map(|()| Exit::Clean)
        }
        Command::Perf {
            path,
            by,
            sort,
            top,
            min_depth,
            explain,
            profile,
            json,
            printing,
            run,
        } => {
            let mut settings = run.resolve(&path)?;
            // The display cap would drop findings this ranking has to weigh.
            settings.config.scan.max_findings = 0;
            if !profile.is_empty() {
                settings.set_by_flag("scan.profile", "--profile", |config| {
                    config.scan.profile = display_all(&profile);
                })?;
            }
            let sinks = printing.sinks(json)?;
            let outcome =
                scan::scan_drawn_against(&path, &settings, progress, scan::Comparison::costed())?;
            let view = perf::View {
                by,
                sort,
                top,
                min_depth,
            };
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                let json = rendering.is_json();
                match &explain {
                    Some(target) => render::perf_explain(&outcome, target, json, palette, out),
                    None => render::perf(&outcome, &view, json, &settings, palette, out),
                }
            })
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
            only,
            json,
            printing,
            run,
        } => {
            let mut settings = run.resolve(&path)?;
            settings.config.scan.max_findings = 0;
            let sinks = printing.sinks(json)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                render::recommend(&outcome, &settings, only, rendering.is_json(), palette, out)
            })
            .map(|()| Exit::Clean)
        }
        Command::Slop {
            path,
            top,
            min,
            json,
            printing,
            run,
        } => {
            let mut settings = run.resolve(&path)?;
            settings.config.scan.max_findings = 0;
            render::slop_settings(&mut settings);
            let sinks = printing.sinks(json)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                let request = render::Slop {
                    top,
                    min,
                    json: rendering.is_json(),
                };
                render::slop(&outcome, &request, palette, out)
            })
            .map(|()| Exit::Clean)
        }
        Command::Hotspots {
            path,
            top,
            since,
            json,
            printing,
            run,
        } => {
            let mut settings = run.resolve(&path)?;
            settings.config.scan.max_findings = 0;
            let request = render::Hotspots::over(&mut settings, top, since)?;
            let sinks = printing.sinks(json)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                render::hotspots(
                    &outcome,
                    &request.printed_as(rendering.is_json()),
                    palette,
                    out,
                )
            })
            .map(|()| Exit::Clean)
        }
        Command::Badge { path, output, run } => {
            let settings = run.resolve(&path)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            let health = Report::everything(&outcome).health();
            let badge = badge::svg(health, outcome.counts.total);
            match output {
                Some(file) => {
                    std::fs::write(&file, badge)
                        .with_context(|| format!("writing {}", file.display()))?;
                    eprintln!("  wrote the badge to {}", file.display());
                }
                None => std::io::Write::write_all(&mut std::io::stdout().lock(), badge.as_bytes())?,
            }
            Ok(Exit::Clean)
        }
        Command::Health {
            path,
            by,
            json,
            printing,
            run,
        } => {
            let mut settings = run.resolve(&path)?;
            // Every finding, not the first five hundred: a bucket judged on a capped
            // list would understate whichever rule fires most.
            settings.config.scan.max_findings = 0;
            let sinks = printing.sinks(json)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                render::health(&outcome, by, rendering.is_json(), palette, out)
            })
            .map(|()| Exit::Clean)
        }
        Command::Owners {
            path,
            json,
            printing,
            run,
        } => {
            let mut settings = run.resolve(&path)?;
            settings.config.scan.max_findings = 0;
            let sinks = printing.sinks(json)?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                render::owners(&outcome, rendering.is_json(), palette, out)
            })
            .map(|()| Exit::Clean)
        }
        Command::Trend {
            path,
            last,
            by,
            json,
            printing,
        } => {
            let sinks = printing.sinks(json)?;
            sink::deliver(&sinks, palette, |rendering, palette, out| {
                let request = render::Trend {
                    path: path.clone(),
                    last,
                    by,
                    json: rendering.is_json(),
                };
                render::trend(&request, palette, out)
            })
            .map(|()| Exit::Clean)
        }
        Command::Graph {
            path,
            format,
            reporters,
            cycles,
            focus,
            depth,
            run,
        } => {
            let settings = run.resolve(&path)?;
            let sinks = sink::settle(
                reporters
                    .into_iter()
                    .chain(format.map(|reporter| Sink {
                        reporter,
                        path: None,
                    }))
                    .collect(),
            )?;
            let outcome = scan::scan_drawn(&path, &settings, progress)?;
            sink::deliver(&sinks, palette, |shape, palette, out| {
                let view = graph::View {
                    shape,
                    cycles_only: cycles,
                    focus: focus.as_deref(),
                    depth,
                };
                graph::render(&outcome, &view, palette, out)
            })
            .map(|()| Exit::Clean)
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
                        println!("  the cache is off; setting `scan.cache: true` turns it on");
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
            base,
            dry_run,
            marker,
            no_status,
            gate,
            evidence,
            complexity,
            run,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx comments on a project; try `rqx comment .`",
                    path.display()
                );
            }
            let mut settings = run.resolve(&path)?;
            gate.apply(&mut settings)?;
            evidence.apply(&mut settings)?;
            // The whole review, not the first five hundred: a display cap is for a
            // terminal, and this is a document somebody scrolls.
            settings.config.scan.max_findings = 0;

            // A CI clone is `git init` plus a fetch, so `origin/HEAD` points at
            // nothing and a branch name is not a local ref; both forges hand over the
            // merge base as a SHA, which is a revision that exists.
            let base = base
                .or_else(forge::base_revision)
                .unwrap_or_else(|| "origin/HEAD".to_owned());
            let review = check::review_drawn(
                &path,
                &settings,
                check::Reviewed::Since(&base),
                progress,
                complexity.options(),
            )?;
            let verdict = review.judge(&settings.config);
            let report = Report::reviewed(&review, &verdict);

            // Detected before posting but after the scan, so a dry run still gets its
            // comment where there is nowhere to put one.
            let forge = forge::Forge::detect();
            let body = report::comment(
                &report,
                forge.as_ref().ok().and_then(forge::Forge::job),
                settings.config.comment.mention_owners,
            )?;
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
            json,
            printing,
            action,
        } => {
            if (json || printing.is_given()) && action.is_some() {
                bail!(
                    "`--json` and `--reporter` go after `rqx config` to print the settings, \
                     or after `rqx config validate` to print the problems; the other \
                     subcommands have no JSON form"
                );
            }
            // A TOML file stops the config from loading, and converting it is the way
            // out, so this has to run before anything reads the configuration.
            match action {
                Some(ConfigAction::Migrate {
                    path: named,
                    dry_run,
                }) => {
                    let mut out = anstream::stdout().lock();
                    migrate::run(named.as_ref().unwrap_or(&path), dry_run, palette, &mut out)?;
                    return Ok(Exit::Clean);
                }
                Some(ConfigAction::Schema) => {
                    println!("{}", report::config_schema());
                    return Ok(Exit::Clean);
                }
                Some(ConfigAction::Validate {
                    path,
                    standard,
                    strict,
                    json,
                    printing,
                    config,
                }) => {
                    let sinks = printing.sinks(json)?;
                    return validate_config(
                        &path,
                        config.as_deref(),
                        &standard,
                        strict,
                        &sinks,
                        palette,
                    );
                }
                _ => {}
            }
            let settings = config::resolve(&path, config.as_deref(), standard.chosen()?);
            match action {
                None => {
                    let sinks = printing.sinks(json)?;
                    let settings = settings?;
                    sink::deliver(&sinks, palette, |rendering, palette, out| match rendering {
                        Rendering::Json => report::config_json(&settings, out),
                        Rendering::Terminal => report::config(&settings, palette, out),
                    })
                    .map(|()| Exit::Clean)
                }
                Some(ConfigAction::Explain { key }) => {
                    let mut out = anstream::stdout().lock();
                    explain::run(&key, &settings?, palette, &mut out).map(|()| Exit::Clean)
                }
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
            base,
            staged,
            fix,
            write,
            view,
            gate,
            evidence,
            cap,
            trending,
            complexity,
            run,
        } => {
            if !path.exists() {
                bail!(
                    "`{}` does not exist. rqx checks a directory or a single file; try `rqx check .`",
                    path.display()
                );
            }
            // Built before the settings, because a scope in the configuration names the
            // directory it was resolved for, and the scan reads the overlay.
            let overlay = staged.then(|| staged::Overlay::build(&path)).transpose()?;
            let scan_root = overlay
                .as_ref()
                .map_or(path.as_path(), staged::Overlay::root);
            let mut settings = run.resolve(scan_root)?;
            // Started before the scan rather than after it: the answer needs the length
            // of the work to arrive in, and `process::exit` does not wait for a thread.
            update::start(settings.config.update.check);
            gate.apply(&mut settings)?;
            cap.apply(&mut settings)?;
            evidence.apply(&mut settings)?;

            let progress = progress_for(view.quiet);
            let view = view
                .on_terminal(verbosity, hyperlinks)
                .paged(pager::for_run(no_pager));
            let sinks = sinks(&view)?;
            warn_of_deprecated(&sinks);
            // The list is what is reviewed here, so it does not also narrow the report:
            // that would hide the findings in the files that import what it names.
            let listed = listed_files(&view, &path)?;
            let source = match (&listed, &overlay) {
                (Some(listed), _) => check::Reviewed::Listed(&listed.files),
                (None, Some(overlay)) => check::Reviewed::Staged(&overlay.files),
                (None, None) => check::Reviewed::Since(&base),
            };
            let filter = filter_of(&view, &path, None)?;
            let mut review =
                check::review_drawn(scan_root, &settings, source, progress, complexity.options())?;

            // The gate is judged after the fixes, so a branch whose only failures were
            // mechanical passes rather than failing on work rqx could have done.
            if fix {
                let fixed = fix::run_drawn(
                    scan_root,
                    &settings,
                    fix::Options {
                        write,
                        max_rounds: settings.config.fix.max_rounds.max(1),
                        only: review.reviewed.clone(),
                        ..fix::Options::default()
                    },
                    progress,
                )?;
                let mut out = companion_stream(&sinks);
                report::fixes(&fixed, palette, "rqx check --fix --write", &mut out)?;
                drop(out);
                if write && fixed.applied() > 0 {
                    review = check::review_drawn(
                        scan_root,
                        &settings,
                        source,
                        progress,
                        complexity.options(),
                    )?;
                }
            }
            let history = trending.apply(&review.outcome)?;
            let verdict = review.judge(&settings.config);
            let report = Report::review(&review, &verdict, &filter).trended(history.as_deref());
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
            let mut out = anstream::stdout().lock();
            if slugs {
                render::rule_slugs(category.as_deref(), language.as_deref(), &mut out)
            } else {
                render::rules(category.as_deref(), language.as_deref(), palette, &mut out)
            }
            .map(|()| Exit::Clean)
        }
        Command::Update {
            check,
            notes,
            version,
            dry_run,
        } => update_command(
            &UpdateRequest {
                check,
                notes,
                version,
                dry_run,
            },
            palette,
        ),
        Command::Techniques => {
            report::techniques(palette, &mut anstream::stdout().lock()).map(|()| Exit::Clean)
        }
        Command::Levels => {
            report::levels(palette, &mut anstream::stdout().lock()).map(|()| Exit::Clean)
        }
    }
}

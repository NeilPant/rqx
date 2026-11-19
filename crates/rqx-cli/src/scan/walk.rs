//! Finds the files a scan reads: the walker and what it leaves out.

use std::path::{Path, PathBuf};
use std::sync::mpsc::SyncSender;

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;
use rqx_core::config::RqxConfig;

use super::skip::{SkipReason, Skipped};
use super::{gitattributes, submodules};

use super::analyze::skipped;
use super::{ScanMessage, Watch};

/// Every file of one language the scan would look at. Collected rather than streamed
/// because the one caller wants them in a stable order and there are no findings to
/// race.
pub(crate) fn files_of(
    root: &Path,
    language: rqx_core::Language,
    config: &RqxConfig,
) -> Result<Vec<PathBuf>> {
    let found = std::sync::Mutex::new(Vec::new());
    build_walker(root, config)?.run(|| {
        Box::new(|entry| {
            if let Ok(entry) = entry
                && entry.file_type().is_some_and(|kind| kind.is_file())
                && rqx_core::Language::of_path(entry.path()) == Some(language)
            {
                found
                    .lock()
                    .expect("the walk holds no lock")
                    .push(entry.into_path());
            }
            ignore::WalkState::Continue
        })
    });
    let mut found = found.into_inner().expect("the walk is done");
    found.sort();
    Ok(found)
}

pub(crate) fn build_walker(root: &Path, config: &RqxConfig) -> Result<ignore::WalkParallel> {
    walker_noting(root, config, |_| {})
}

/// The scan's walker, which also tells `on_submodule` about each submodule it leaves
/// out. Told from inside the walk, so a submodule under a pruned directory or an
/// ignored path, which the walk never reaches, is never reported.
pub(super) fn walker_noting(
    root: &Path,
    config: &RqxConfig,
    on_left_out: impl Fn(LeftOut<'_>) + Send + Sync + 'static,
) -> Result<ignore::WalkParallel> {
    let submodules = submodules::left_out(root, config.scan.skip_submodules);
    let marked = config
        .scan
        .respect_gitattributes
        .then(|| gitattributes::Marked::load(root))
        .flatten();
    let pruned: Vec<String> = config.scan.prune_dirs.clone();
    let start = root.to_path_buf();
    let mut builder = source_walk(root, root, config)?;
    builder.filter_entry(move |entry| {
        if is_pruned(entry, &pruned) {
            return false;
        }
        if let Some(submodule) = submodules
            .iter()
            .find(|submodule| submodule.path == entry.path())
        {
            on_left_out(LeftOut::Submodule(submodule));
            return false;
        }
        if let Some(attribute) = marked.as_ref().and_then(|marked| {
            let relative = entry.path().strip_prefix(&start).ok()?;
            entry
                .file_type()
                .is_some_and(|kind| kind.is_file())
                .then(|| marked.marking(relative))
                .flatten()
        }) {
            on_left_out(LeftOut::Marked(entry.path(), attribute));
            return false;
        }
        true
    });
    Ok(builder.build_parallel())
}

/// What the walk leaves out on the project's own say, told to whoever reports it.
pub(super) enum LeftOut<'a> {
    Submodule(&'a submodules::Submodule),
    /// A file and the `.gitattributes` attribute that marks it.
    Marked(&'a Path, &'static str),
}

/// A walk from `start` that yields what a scan of `root` would read below it: the
/// enabled languages, minus the ignore patterns, which match relative to `root`.
fn source_walk(root: &Path, start: &Path, config: &RqxConfig) -> Result<WalkBuilder> {
    let mut types = TypesBuilder::new();
    for language in config.languages.enabled() {
        for extension in language.extensions() {
            types
                .add(language.slug(), &format!("*.{extension}"))
                .with_context(|| format!("registering a {} file glob", language.title()))?;
        }
        types.select(language.slug());
    }

    let mut overrides = OverrideBuilder::new(root);
    for pattern in &config.ignore.patterns {
        overrides
            .add(&format!("!{pattern}"))
            .with_context(|| format!("ignore pattern `{pattern}` is not a valid glob"))?;
    }

    let mut builder = WalkBuilder::new(start);
    builder
        .types(types.build().context("building the source file matcher")?)
        .overrides(overrides.build().context("building ignore overrides")?)
        // `ignore` reads 0 as "pick for me" and then caps its own choice at 12, so a
        // machine with more cores than that leaves them idle with no way to ask.
        .threads(if config.scan.threads == 0 {
            std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
        } else {
            config.scan.threads
        });
    Ok(builder)
}

fn is_pruned(entry: &ignore::DirEntry, pruned: &[String]) -> bool {
    entry.file_type().is_some_and(|kind| kind.is_dir())
        && pruned
            .iter()
            .any(|dir| *dir == entry.file_name().to_string_lossy())
}

/// The source files a scan would read in `directory` if it were not left out, so a
/// skipped submodule can say how much it held. Only directory entries are read.
fn source_files_in(root: &Path, directory: &Path, config: &RqxConfig) -> u32 {
    let Ok(mut builder) = source_walk(root, directory, config) else {
        return 0;
    };
    let pruned = config.scan.prune_dirs.clone();
    builder.filter_entry(move |entry| !is_pruned(entry, &pruned));
    let files = builder
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .count();
    u32::try_from(files).unwrap_or(u32::MAX)
}

/// Reports what the walk left out as skipped entries.
pub(super) fn report_left_out(
    root: PathBuf,
    config: RqxConfig,
    sender: SyncSender<ScanMessage>,
) -> impl Fn(LeftOut<'_>) + Send + Sync + 'static {
    move |left_out| match left_out {
        LeftOut::Submodule(submodule) => report_submodule(&root, &config, &sender, submodule),
        LeftOut::Marked(path, attribute) => {
            let _ = sender.send(skipped(
                path,
                SkipReason::Gitattributes,
                format!(
                    "marked {attribute} in .gitattributes \
                     (set `scan.respect_gitattributes: false` to include it)"
                ),
            ));
        }
    }
}

/// A submodule the walk left out, as one skipped entry carrying the number of source
/// files in it. One with no source files left nothing out, so it says nothing.
fn report_submodule(
    root: &Path,
    config: &RqxConfig,
    sender: &SyncSender<ScanMessage>,
    submodule: &submodules::Submodule,
) {
    let files = source_files_in(root, &submodule.path, config);
    if files == 0 {
        return;
    }
    let holding = crate::report::plural(files, "source file", "source files");
    let detail = match &submodule.foreign {
        Some(hosted) => format!(
            "a git submodule hosted at {hosted}, with {holding} \
             (set `scan.skip_submodules: none` to include it)"
        ),
        None => format!(
            "a git submodule with {holding} \
             (set `scan.skip_submodules: auto` or `none` to include it)"
        ),
    };
    let _ = sender.send(ScanMessage::Skipped(Skipped {
        path: submodule.path.clone(),
        reason: SkipReason::Submodule,
        detail,
        files,
        foreign: submodule.foreign.clone(),
    }));
}

/// The same walker the scan uses, so the total it reports cannot disagree with what is
/// then scanned. Only directory entries are touched: nothing is opened or parsed.
pub(super) fn count_files(walker: ignore::WalkParallel, watch: Option<&Watch>) -> Option<u32> {
    let counted = std::sync::atomic::AtomicU32::new(0);
    walker.run(|| {
        Box::new(|entry| {
            if watch.is_some_and(Watch::stopped) {
                return ignore::WalkState::Quit;
            }
            if entry.is_ok_and(|entry| entry.file_type().is_some_and(|kind| kind.is_file())) {
                counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            ignore::WalkState::Continue
        })
    });
    Some(counted.into_inner()).filter(|total| *total > 0)
}

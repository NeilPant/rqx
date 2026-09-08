//! Whether a newer rqx has been tagged, and installing it.
//!
//! A tag is the release: it carries the binaries, the crates and the npm packages,
//! so there is one version and one changelog section behind all of them. What this
//! module updates is a `cargo install --git` build, because that is the only install
//! nothing else owns — an npm or distribution install marks itself with
//! `RQX_MANAGED` and is handed back to whatever put it there.
//!
//! A version check is a network request, and a scan is not the place for one: the
//! answer is cached for a day, fetched on a thread nobody waits for, and never
//! attempted at all in CI or when the output is being piped somewhere. What reaches
//! the reader is one dim line after a report that was going to be printed anyway.
//!
//! Requests go through `curl` for the same reason `forge.rs` does — rqx carries no
//! TLS stack, and certificates and proxies stay the system's problem.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const REPOSITORY: &str = "https://github.com/jadhamwi21/rqx";
const TAGS: &str = "https://api.github.com/repos/jadhamwi21/rqx/tags";
const CHANGELOG: &str = "https://raw.githubusercontent.com/jadhamwi21/rqx";

/// How long an answer stands before it is asked for again. A day: a release nobody
/// hears about for a few hours has cost nobody anything, and a check on every scan
/// would be a request per keystroke in a watch loop.
const GOOD_FOR: Duration = Duration::from_secs(60 * 60 * 24);

/// What the last check found, kept between runs so the common case costs no network
/// at all.
#[derive(Serialize, Deserialize, Default)]
struct Remembered {
    /// Seconds since the epoch, so a clock that moves does not make this unreadable.
    checked_at: u64,
    version: String,
    notes: String,
}

/// A release newer than the one running.
pub struct Newer {
    pub version: String,
    pub notes: String,
}

impl Newer {
    /// The line a report ends with. Deliberately one line: an update notice that
    /// takes a paragraph is an advert.
    pub fn line(&self) -> String {
        format!(
            "rqx {} is out (running {}). `rqx update` builds it, `rqx update --notes` reads it.",
            self.version,
            env!("CARGO_PKG_VERSION")
        )
    }
}

fn cache_file() -> Option<PathBuf> {
    let directory = match std::env::var_os("XDG_CACHE_HOME") {
        Some(xdg) => PathBuf::from(xdg),
        None => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(".cache"),
            None => PathBuf::from(std::env::var_os("LOCALAPPDATA")?),
        },
    };
    Some(directory.join("rqx").join("update.json"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn remembered() -> Option<Remembered> {
    let text = std::fs::read_to_string(cache_file()?).ok()?;
    serde_json::from_str(&text).ok()
}

fn remember(found: &Remembered) {
    let Some(path) = cache_file() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(found) {
        let _ = std::fs::write(path, text);
    }
}

/// Every variable that means "this is a build, not a person". `CI` covers GitHub
/// Actions, GitLab, CircleCI, Travis, Buildkite and Bitbucket; Jenkins, TeamCity and
/// Azure Pipelines do not set it reliably and are named individually.
const A_PIPELINE: &[&str] = &[
    "CI",
    "CONTINUOUS_INTEGRATION",
    "BUILD_NUMBER",
    "RUN_ID",
    "JENKINS_URL",
    "TEAMCITY_VERSION",
    "TF_BUILD",
    "bamboo_buildKey",
    "GO_PIPELINE_NAME",
];

/// Whether asking is appropriate at all. A pipeline that reached out to GitHub on
/// every run would be a pipeline that fails when GitHub does, so the check is off
/// wherever the output is not going to a person who could act on it.
pub fn wanted(enabled: bool) -> bool {
    use std::io::IsTerminal;

    enabled
        && std::env::var_os("RQX_NO_UPDATE_CHECK").is_none()
        // A version a package manager installed is not one rqx should offer to
        // replace: the manager owns that file and will put it back.
        && managed().is_none()
        && !A_PIPELINE
            .iter()
            .any(|name| std::env::var_os(name).is_some())
        // The decisive test rather than the environment's word for it: a notice is
        // only worth printing to something a person is watching, and a redirected
        // stderr is a log file whatever the variables say.
        && std::io::stderr().is_terminal()
        && std::io::stdout().is_terminal()
}

/// Set by the npm wrapper, and expected of a distribution packager, because a
/// binary a manager owns must not be replaced behind its back.
pub fn managed() -> Option<String> {
    let manager = std::env::var_os("RQX_MANAGED")?;
    (!manager.is_empty()).then(|| manager.to_string_lossy().into_owned())
}

/// Installing over a managed binary leaves a second copy on the path and the
/// manager puts its own back on the next install, so `rqx update` hands the job
/// over rather than doing it badly.
pub fn managed_update(manager: &str) -> String {
    match manager {
        "npm" => {
            "npm installed rqx. `npm install --save-dev @rqx/cli@latest` updates it.".to_owned()
        }
        other => format!("{other} installed rqx, so updating it is {other}'s job."),
    }
}

/// Compares two dotted versions numerically, so `0.10.0` is newer than `0.9.0` and a
/// string comparison's answer of "no" does not strand everyone on a nine.
fn newer_than(candidate: &str, running: &str) -> bool {
    let parts = |version: &str| -> Vec<u32> {
        version
            .trim_start_matches('v')
            .split(['.', '-'])
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    };
    let (candidate, running) = (parts(candidate), parts(running));
    for at in 0..candidate.len().max(running.len()) {
        let (left, right) = (
            candidate.get(at).copied().unwrap_or(0),
            running.get(at).copied().unwrap_or(0),
        );
        if left != right {
            return left > right;
        }
    }
    false
}

/// The cached answer, if it is still fresh. `None` means nothing is known, not that
/// the running version is current.
pub fn known() -> Option<Newer> {
    let found = remembered()?;
    if now().saturating_sub(found.checked_at) > GOOD_FOR.as_secs() {
        return None;
    }
    newer_than(&found.version, env!("CARGO_PKG_VERSION")).then_some(Newer {
        version: found.version,
        notes: found.notes,
    })
}

/// Asks GitHub for the newest tag, and reads that tag's changelog section. Slow, so
/// callers put it on a thread.
pub fn fetch() -> Result<Option<Newer>> {
    let body = request(TAGS)?;
    let tags: serde_json::Value =
        serde_json::from_str(&body).context("GitHub's answer was not JSON")?;
    let Some(tags) = tags.as_array() else {
        bail!("GitHub listed no tags");
    };

    // Highest by number rather than first in the list: GitHub orders tags by the
    // commit graph, which puts a patch on an old branch above the newest minor.
    let newest = tags
        .iter()
        .filter_map(|tag| tag["name"].as_str())
        .filter(|name| name.starts_with('v'))
        .max_by(|left, right| {
            if newer_than(left, right) {
                std::cmp::Ordering::Greater
            } else if newer_than(right, left) {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        });
    let Some(tag) = newest else {
        bail!(
            "rqx has no tagged version yet, so there is nothing to update to. \
             `cargo install --git {REPOSITORY} rqx` builds the newest commit."
        );
    };
    let version = tag.trim_start_matches('v').to_owned();
    let notes = notes_for(tag, &version).unwrap_or_default();

    remember(&Remembered {
        checked_at: now(),
        version: version.clone(),
        notes: notes.clone(),
    });

    Ok(newer_than(&version, env!("CARGO_PKG_VERSION")).then_some(Newer { version, notes }))
}

/// The changelog section a tag carries, so the prompt can say what changed rather
/// than only that something did.
fn notes_for(tag: &str, version: &str) -> Option<String> {
    section_for(
        &request(&format!("{CHANGELOG}/{tag}/CHANGELOG.md")).ok()?,
        version,
    )
}

/// One version's entries out of a changelog. Split from the request so the parsing
/// is testable without a network.
fn section_for(changelog: &str, version: &str) -> Option<String> {
    let heading = format!("## {version} ");
    let section: Vec<&str> = changelog
        .lines()
        .skip_while(|line| !line.starts_with(&heading))
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect();
    let trimmed = section.join("\n").trim().to_owned();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// The running check, so the report at the end of a scan can pick up an answer the
/// beginning of it asked for.
/// A `Receiver` is not `Sync`, so it travels in a mutex rather than bare. Only one
/// thread ever reads it; the lock is what makes it a legal static, not contention.
static ASKED: std::sync::Mutex<Option<std::sync::mpsc::Receiver<Option<Newer>>>> =
    std::sync::Mutex::new(None);

/// Starts the check before the work does, so it has the length of a scan to finish
/// in. Spawning it at the end and abandoning it was the first shape of this, and it
/// never completed once: `main` leaves through `process::exit`, which does not wait
/// for a thread, and a scan is over before `curl` has finished its handshake.
pub fn start(enabled: bool) {
    if !wanted(enabled) || fresh() {
        return;
    }
    let (told, answer) = std::sync::mpsc::channel();
    let Ok(mut asked) = ASKED.lock() else {
        return;
    };
    if asked.is_some() {
        return;
    }
    *asked = Some(answer);
    std::thread::spawn(move || {
        let _ = told.send(fetch().ok().flatten());
    });
}

/// Whether the last answer is recent enough to stand, whatever it said.
fn fresh() -> bool {
    remembered().is_some_and(|found| now().saturating_sub(found.checked_at) <= GOOD_FOR.as_secs())
}

/// The answer, if one is already known or arrives in the next moment. The wait is
/// short and at the very end of a run: a scan that blocked on GitHub would be a scan
/// that hangs when GitHub is slow, and a notice is worth no delay a reader notices.
pub fn settle(within: Duration) -> Option<Newer> {
    if let Some(newer) = known() {
        return Some(newer);
    }
    ASKED
        .lock()
        .ok()?
        .as_ref()?
        .recv_timeout(within)
        .ok()
        .flatten()
}

/// Builds and installs the tagged version, the same way rqx was installed.
///
/// `--force` because cargo refuses to overwrite a binary it already put there, and
/// `--locked` so the build uses the versions the tag was tested with rather than
/// whatever has been published since.
pub fn install(version: &str) -> Result<()> {
    let mut cargo = Command::new("cargo");
    cargo
        .args(["install", "--git", REPOSITORY])
        .args(["--tag", &format!("v{version}")])
        .args(["--locked", "--force", "rqx"]);

    let status = cargo.status().context(
        "could not run `cargo`. rqx updates by building the tagged version, so the toolchain \
         that installed it has to still be there; otherwise install from source by hand.",
    )?;
    if !status.success() {
        bail!("`cargo install` did not finish; rqx is unchanged");
    }
    Ok(())
}

fn request(url: &str) -> Result<String> {
    let output = Command::new("curl")
        .args(["--silent", "--show-error", "--location", "--fail"])
        .args(["--max-time", "20"])
        .args(["--header", "Accept: application/vnd.github+json"])
        .args([
            "--header",
            &format!("User-Agent: rqx/{}", env!("CARGO_PKG_VERSION")),
        ])
        .arg(url)
        .output()
        .context("could not run `curl`, which rqx asks GitHub through")?;
    if !output.status.success() {
        bail!(
            "could not reach GitHub: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_compared_by_number_rather_than_by_spelling() {
        assert!(newer_than("0.10.0", "0.9.0"), "0.10 must beat 0.9");
        assert!(newer_than("1.0.0", "0.99.99"));
        assert!(newer_than("0.1.1", "0.1.0"));
        assert!(!newer_than("0.1.0", "0.1.0"));
        assert!(!newer_than("0.1.0", "0.1.1"));
        assert!(!newer_than("v0.1.0", "0.1.0"), "a `v` is not a difference");
    }

    /// The prompt's whole value is saying what changed, and the changelog is where
    /// that is already written. An `## Unreleased` heading sits above every release,
    /// so the reader has to stop at the next heading rather than run into it.
    #[test]
    fn the_notes_are_the_changelog_section_for_that_version() {
        let changelog = "\
# Changelog

## Unreleased

## 0.2.0 \u{2014} 2026-09-07

### Added

- A thing.

## 0.1.0 \u{2014} 2026-09-06

### Added

- The first thing.
";
        let newest = section_for(changelog, "0.2.0").expect("0.2.0 has entries");
        assert!(newest.contains("A thing."), "{newest}");
        assert!(
            !newest.contains("The first thing."),
            "ran into 0.1.0: {newest}"
        );
        assert!(
            !newest.lines().any(|line| line.starts_with("## ")),
            "swallowed a release heading: {newest}"
        );

        let older = section_for(changelog, "0.1.0").expect("0.1.0 has entries");
        assert!(older.contains("The first thing."), "{older}");

        assert!(
            section_for(changelog, "9.9.9").is_none(),
            "invented a section"
        );
        // The heading exists but holds nothing, which is every freshly cut release.
        assert!(
            section_for(
                "## 0.3.0 \u{2014} 2026-09-08\n\n## 0.2.0 \u{2014} x\n",
                "0.3.0"
            )
            .is_none()
        );
    }

    /// The first shape of this spawned the check at the end of a run and abandoned
    /// it, and it never once completed: `process::exit` does not wait for a thread.
    /// What matters is that an answer sent by the worker actually reaches the notice.
    #[test]
    fn an_answer_that_arrives_in_time_reaches_the_notice() {
        let (told, answer) = std::sync::mpsc::channel();
        told.send(Some(Newer {
            version: "9.9.9".to_owned(),
            notes: "- something".to_owned(),
        }))
        .expect("the worker's answer");
        *ASKED.lock().expect("the slot") = Some(answer);

        let found = settle(Duration::from_millis(50)).expect("an answer was waiting");
        assert_eq!(found.version, "9.9.9");
        assert!(found.line().contains("9.9.9"), "{}", found.line());

        // And a run where nothing arrives waits its moment and gives up rather than
        // blocking, which is what keeps a slow GitHub off the end of a scan.
        let (idle, quiet) = std::sync::mpsc::channel::<Option<Newer>>();
        *ASKED.lock().expect("the slot") = Some(quiet);
        let began = std::time::Instant::now();
        assert!(settle(Duration::from_millis(60)).is_none());
        assert!(began.elapsed() < Duration::from_millis(600), "it blocked");
        drop(idle);
    }

    #[test]
    fn ci_is_never_asked_to_check_for_updates() {
        // The environment is process-wide, so this asserts the predicate rather than
        // setting variables another test could be reading at the same time.
        assert!(!wanted(false), "the setting must be able to turn it off");
    }
}

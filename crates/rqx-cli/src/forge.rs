//! Where the change is being reviewed, and how to say something there.
//!
//! A review that only exists in a build log is a review nobody reads. This posts the
//! verdict onto the merge request itself: one comment that is edited rather than
//! repeated, and a commit status so the branch carries its own grade.
//!
//! Requests go through `curl`. rqx already requires `git` and shells out to it on
//! every scan, every CI image has `curl`, and the alternative is a TLS stack and
//! forty crates in a binary whose whole claim is that it has no runtime
//! dependencies. Certificates, proxies and corporate roots stay the system's
//! problem, which on a self-hosted GitLab is where they belong.

use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

/// The hidden line that makes a comment findable again on the next run. Invisible in
/// both GitLab's and GitHub's markdown, which is what lets the same comment be
/// edited instead of a new one piling up under it every pipeline.
fn marker_line(marker: &str) -> String {
    format!("<!-- rqx:{marker} -->")
}

pub enum Forge {
    GitLab {
        api: String,
        project: String,
        merge_request: String,
        sha: String,
        token: String,
        job: Option<String>,
    },
    GitHub {
        api: String,
        repository: String,
        pull_request: String,
        sha: String,
        token: String,
        job: Option<String>,
    },
}

/// What a post did, so the caller can say so rather than guess.
pub enum Posted {
    Created,
    Updated,
}

impl Forge {
    /// Read from the environment the pipeline already sets. Nothing is guessed: a
    /// missing merge-request number means the run is not on a merge request, which
    /// is a fact to report rather than an error to raise.
    pub fn detect() -> Result<Self> {
        let token = |names: &[&str]| {
            names
                .iter()
                .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        };

        if let Ok(merge_request) = std::env::var("CI_MERGE_REQUEST_IID") {
            let Some(token) = token(&["RQX_TOKEN", "GITLAB_TOKEN"]) else {
                bail!(
                    "no token. GitLab's CI_JOB_TOKEN cannot write merge-request notes, so set \
                     RQX_TOKEN or GITLAB_TOKEN to a token with `api` scope."
                );
            };
            return Ok(Self::GitLab {
                api: std::env::var("CI_API_V4_URL")
                    .unwrap_or_else(|_| "https://gitlab.com/api/v4".to_owned()),
                project: std::env::var("CI_PROJECT_ID").context("CI_PROJECT_ID is not set")?,
                merge_request,
                sha: std::env::var("CI_MERGE_REQUEST_SOURCE_BRANCH_SHA")
                    .ok()
                    .filter(|sha| !sha.is_empty())
                    .or_else(|| std::env::var("CI_COMMIT_SHA").ok())
                    .unwrap_or_default(),
                token,
                job: std::env::var("CI_JOB_URL").ok(),
            });
        }

        if let Ok(repository) = std::env::var("GITHUB_REPOSITORY") {
            let Some(pull_request) = github_pull_request() else {
                bail!(
                    "this run is not on a pull request. GITHUB_REF is `{}`.",
                    std::env::var("GITHUB_REF").unwrap_or_default()
                );
            };
            let Some(token) = token(&["RQX_TOKEN", "GITHUB_TOKEN"]) else {
                bail!(
                    "no token. Set RQX_TOKEN, or pass GITHUB_TOKEN through with \
                     `permissions: pull-requests: write`."
                );
            };
            let job = match (
                std::env::var("GITHUB_SERVER_URL"),
                std::env::var("GITHUB_RUN_ID"),
            ) {
                (Ok(server), Ok(run)) => Some(format!("{server}/{repository}/actions/runs/{run}")),
                _ => None,
            };
            return Ok(Self::GitHub {
                api: std::env::var("GITHUB_API_URL")
                    .unwrap_or_else(|_| "https://api.github.com".to_owned()),
                repository,
                pull_request,
                sha: github_head_sha().unwrap_or_default(),
                token,
                job,
            });
        }

        bail!(
            "no merge request to comment on. rqx reads GitLab's CI_MERGE_REQUEST_IID or \
             GitHub's GITHUB_REPOSITORY and event payload; neither is set."
        )
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::GitLab { .. } => "GitLab",
            Self::GitHub { .. } => "GitHub",
        }
    }

    /// Where the comment will land, for a dry run to name.
    pub fn target(&self) -> String {
        match self {
            Self::GitLab {
                project,
                merge_request,
                ..
            } => format!("merge request !{merge_request} of project {project}"),
            Self::GitHub {
                repository,
                pull_request,
                ..
            } => format!("pull request #{pull_request} of {repository}"),
        }
    }

    pub fn job(&self) -> Option<&str> {
        match self {
            Self::GitLab { job, .. } | Self::GitHub { job, .. } => job.as_deref(),
        }
    }

    fn comments_url(&self) -> String {
        match self {
            Self::GitLab {
                api,
                project,
                merge_request,
                ..
            } => format!("{api}/projects/{project}/merge_requests/{merge_request}/notes"),
            Self::GitHub {
                api,
                repository,
                pull_request,
                ..
            } => format!("{api}/repos/{repository}/issues/{pull_request}/comments"),
        }
    }

    fn headers(&self) -> Vec<String> {
        match self {
            Self::GitLab { token, .. } => vec![format!("PRIVATE-TOKEN: {token}")],
            Self::GitHub { token, .. } => vec![
                format!("Authorization: Bearer {token}"),
                "Accept: application/vnd.github+json".to_owned(),
                "X-GitHub-Api-Version: 2022-11-28".to_owned(),
            ],
        }
    }

    /// The comment this run already left, if it left one. Matched on the hidden
    /// marker rather than on the author, so two rqx jobs in the same pipeline can
    /// each keep their own comment by passing a different one.
    fn existing(&self, marker: &str) -> Result<Option<String>> {
        let wanted = marker_line(marker);
        let mut page = 1;
        // Bounded: a merge request with more than a thousand comments has bigger
        // problems, and an unbounded loop against a paging API is how a job hangs.
        while page <= 10 {
            let url = format!("{}?per_page=100&page={page}", self.comments_url());
            let body = self.request("GET", &url, None)?;
            let Value::Array(notes) = serde_json::from_str::<Value>(&body)? else {
                return Ok(None);
            };
            if notes.is_empty() {
                return Ok(None);
            }
            for note in &notes {
                if note["body"]
                    .as_str()
                    .is_some_and(|text| text.contains(&wanted))
                {
                    return Ok(note["id"].as_i64().map(|id| id.to_string()));
                }
            }
            page += 1;
        }
        Ok(None)
    }

    /// One comment per marker, edited in place. A pipeline that runs ten times leaves
    /// one comment, not ten.
    pub fn comment(&self, marker: &str, body: &str) -> Result<Posted> {
        let body = format!("{body}\n\n{}", marker_line(marker));
        let payload = json!({ "body": body });
        match self.existing(marker)? {
            Some(id) => {
                let url = match self {
                    Self::GitLab { .. } => format!("{}/{id}", self.comments_url()),
                    Self::GitHub {
                        api, repository, ..
                    } => format!("{api}/repos/{repository}/issues/comments/{id}"),
                };
                let verb = match self {
                    Self::GitLab { .. } => "PUT",
                    Self::GitHub { .. } => "PATCH",
                };
                self.request(verb, &url, Some(&payload))?;
                Ok(Posted::Updated)
            }
            None => {
                self.request("POST", &self.comments_url(), Some(&payload))?;
                Ok(Posted::Created)
            }
        }
    }

    /// The grade, on the commit, so the merge request carries it without anyone
    /// opening a log. `state` is the forge's own vocabulary for pass and fail.
    pub fn classify(&self, passed: bool, description: &str) -> Result<()> {
        if self.sha().is_empty() {
            bail!("no commit to attach a status to");
        }
        match self {
            Self::GitLab { api, project, .. } => {
                let payload = json!({
                    "state": if passed { "success" } else { "failed" },
                    "name": "rqx",
                    "description": description,
                    "target_url": self.job(),
                });
                let url = format!("{api}/projects/{project}/statuses/{}", self.sha());
                self.request("POST", &url, Some(&payload))?;
            }
            Self::GitHub {
                api, repository, ..
            } => {
                let payload = json!({
                    "state": if passed { "success" } else { "failure" },
                    "context": "rqx",
                    "description": description,
                    "target_url": self.job(),
                });
                let url = format!("{api}/repos/{repository}/statuses/{}", self.sha());
                self.request("POST", &url, Some(&payload))?;
            }
        }
        Ok(())
    }

    fn sha(&self) -> &str {
        match self {
            Self::GitLab { sha, .. } | Self::GitHub { sha, .. } => sha,
        }
    }

    /// Neither the token nor the payload goes on the command line. `ps` is readable
    /// by every other job on a shared runner and the token is the one secret here;
    /// a finding's text is arbitrary source, and `ARG_MAX` is real.
    ///
    /// The status code comes back through `--write-out` rather than through
    /// `--fail-with-body`, which landed in curl 7.76: Ubuntu 20.04 and Debian
    /// bullseye ship older ones, where that option is an unknown-option failure
    /// rather than a request.
    fn request(&self, verb: &str, url: &str, payload: Option<&Value>) -> Result<String> {
        let mut settings = self
            .headers()
            .iter()
            .map(|header| format!("header = \"{}\"\n", escaped(header)))
            .collect::<String>();

        let body = payload
            .map(|payload| tempfile::Temp::holding(&serde_json::to_vec(payload)?))
            .transpose()?;
        if let Some(body) = &body {
            settings.push_str("header = \"Content-Type: application/json\"\n");
            settings.push_str(&format!(
                "data-binary = \"@{}\"\n",
                escaped(&body.path().display().to_string())
            ));
        }
        let settings = tempfile::Temp::holding(settings.as_bytes())?;
        let answer = tempfile::Temp::named();

        let output = Command::new("curl")
            .args(["--silent", "--show-error"])
            .args(["--retry", "2", "--retry-connrefused"])
            .args(["--max-time", "30"])
            .args(["--request", verb])
            .arg("--config")
            .arg(settings.path())
            .arg("--output")
            .arg(answer.path())
            .args(["--write-out", "%{http_code}"])
            .arg(url)
            .output()
            .context(
                "could not run `curl`. rqx posts through it rather than carrying a TLS stack; \
                 every CI image has it, and alpine needs `apk add curl`.",
            )?;

        if !output.status.success() {
            let said = String::from_utf8_lossy(&output.stderr);
            bail!("curl could not reach {}: {}", self.name(), said.trim());
        }
        let code: u16 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap_or_default();
        let answered = answer.read();
        if !(200..300).contains(&code) {
            bail!(
                "{} refused a {verb} with {code}{}",
                self.name(),
                if answered.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", answered.trim())
                }
            );
        }
        Ok(answered)
    }
}

/// The event the runner wrote to disk. Everything GitHub does not put in a variable
/// is in here, and two of the three things this needs are among them.
fn github_event() -> Option<Value> {
    let path = std::env::var("GITHUB_EVENT_PATH").ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// The pull-request number. `GITHUB_REF` is `refs/pull/N/merge` on a pull request,
/// which is the fallback when the payload is unreadable.
fn github_pull_request() -> Option<String> {
    if let Some(number) = github_event().and_then(|event| event["pull_request"]["number"].as_i64())
    {
        return Some(number.to_string());
    }
    let reference = std::env::var("GITHUB_REF").ok()?;
    reference
        .strip_prefix("refs/pull/")?
        .split('/')
        .next()
        .map(str::to_owned)
}

/// The commit a status has to name. On a `pull_request` event `GITHUB_SHA` is the
/// *merge* commit the runner built, which belongs to no branch — a status posted
/// there shows on nothing. The head is only in the event payload.
fn github_head_sha() -> Option<String> {
    github_event()
        .and_then(|event| event["pull_request"]["head"]["sha"].as_str().map(str::to_owned))
        .or_else(|| std::env::var("GITHUB_SHA").ok())
}

/// What the review should compare against, when the pipeline already knows. Both
/// forges hand over the merge base, and both clone in a way that leaves `origin/HEAD`
/// pointing at nothing, so guessing a ref name is worse than asking.
pub fn base_revision() -> Option<String> {
    if let Ok(base) = std::env::var("CI_MERGE_REQUEST_DIFF_BASE_SHA")
        && !base.is_empty()
    {
        return Some(base);
    }
    if let Some(base) =
        github_event().and_then(|event| event["pull_request"]["base"]["sha"].as_str().map(str::to_owned))
    {
        return Some(base);
    }
    None
}

/// curl's config-file syntax quotes a value and reads backslash escapes inside it,
/// so a token only survives one after these two.
fn escaped(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A file on disk for the length of one request, removed afterwards even when the
/// request fails. Written `0600`: one of these holds the token, and the temp
/// directory on a shared runner is shared.
mod tempfile {
    use std::path::{Path, PathBuf};

    use anyhow::{Context, Result};

    pub struct Temp(PathBuf);

    impl Temp {
        pub fn named() -> Self {
            Self(std::env::temp_dir().join(format!(
                "rqx-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| since.as_nanos())
            )))
        }

        pub fn holding(bytes: &[u8]) -> Result<Self> {
            let file = Self::named();
            file.write(bytes)
                .with_context(|| format!("writing {}", file.0.display()))?;
            Ok(file)
        }

        #[cfg(unix)]
        fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;

            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&self.0)?
                .write_all(bytes)
        }

        #[cfg(not(unix))]
        fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
            std::fs::write(&self.0, bytes)
        }

        pub fn read(&self) -> String {
            std::fs::read_to_string(&self.0).unwrap_or_default()
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_is_invisible_and_unique_per_id() {
        assert_eq!(marker_line("results"), "<!-- rqx:results -->");
        assert_ne!(marker_line("api"), marker_line("web"));
    }

    /// Two jobs in one pipeline each keep their own comment, and neither finds the
    /// other's: that is the whole reason the id is part of the marker.
    #[test]
    fn one_markers_line_does_not_contain_anothers() {
        let body = format!("a comment\n\n{}", marker_line("api"));
        assert!(body.contains(&marker_line("api")));
        assert!(!body.contains(&marker_line("web")));
    }
}

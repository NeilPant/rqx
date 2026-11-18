//! Which git submodules a scan leaves out. A submodule hosted under another
//! organisation is someone else's code pinned at a commit; one under the project's own
//! organisation is usually the team's code split into repositories, and is scanned.

use std::path::{Path, PathBuf};

use rqx_core::config::SubmodulePolicy;

/// A submodule the walk leaves out.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Submodule {
    /// The path the walker yields for it.
    pub path: PathBuf,
    /// `host/organisation` when it is hosted elsewhere than the project's `origin`.
    pub foreign: Option<String>,
}

/// The submodules a walk from `root` leaves out under `policy`. `.gitmodules` is read
/// at `root` and at the repository root above it, whose entries are relative to that
/// root. A submodule that holds `root` is never one: a scan started inside a submodule
/// is a scan of that submodule.
pub fn left_out(root: &Path, policy: SubmodulePolicy) -> Vec<Submodule> {
    if policy == SubmodulePolicy::None {
        return Vec::new();
    }
    let Ok(start) = rqx_core::paths::canonicalize(root) else {
        return Vec::new();
    };
    let repository = start
        .ancestors()
        .find(|directory| directory.join(".git").exists());
    let origin = repository
        .and_then(origin_url)
        .and_then(|url| Remote::parse(&url));
    let mut found: Vec<Submodule> = std::iter::once(start.as_path())
        .chain(repository.filter(|repository| *repository != start))
        .flat_map(|listed_at| {
            let listing =
                std::fs::read_to_string(listed_at.join(".gitmodules")).unwrap_or_default();
            entries(&listing)
                .into_iter()
                .filter_map(|entry| {
                    let absolute = entry
                        .path
                        .split('/')
                        .fold(listed_at.to_path_buf(), |at, part| at.join(part));
                    let inside = absolute.strip_prefix(&start).ok()?;
                    if inside.as_os_str().is_empty() {
                        return None;
                    }
                    let foreign = entry.url.and_then(|url| foreign_to(origin.as_ref(), url));
                    (policy == SubmodulePolicy::All || foreign.is_some()).then(|| Submodule {
                        path: root.join(inside),
                        foreign,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    found.sort();
    found.dedup_by(|a, b| a.path == b.path);
    found
}

/// `host/organisation` of `url` when it is hosted elsewhere than `origin`: another
/// host, or another first namespace segment (the GitHub organisation, the top GitLab
/// group). Anything that cannot be compared, such as a relative URL or a project with
/// no `origin`, counts as the project's own.
fn foreign_to(origin: Option<&Remote>, url: &str) -> Option<String> {
    let origin = origin?;
    let remote = Remote::parse(url)?;
    (remote.host != origin.host || remote.organisation() != origin.organisation())
        .then(|| remote.label())
}

/// One `[submodule]` section of a `.gitmodules` file.
struct Entry<'a> {
    path: &'a str,
    url: Option<&'a str>,
}

/// The sections of a `.gitmodules` file, which is in git's config format. A section
/// without a `path` names nothing on disk and is dropped.
fn entries(listing: &str) -> Vec<Entry<'_>> {
    let mut sections: Vec<(Option<&str>, Option<&str>)> = Vec::new();
    for line in listing.lines().map(str::trim) {
        if line.starts_with('[') {
            sections.push((None, None));
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        let Some(section) = sections.last_mut() else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "path" => section.0 = Some(value.trim_end_matches('/')),
            "url" => section.1 = Some(value),
            _ => {}
        }
    }
    sections
        .into_iter()
        .filter_map(|(path, url)| {
            path.filter(|path| !path.is_empty())
                .map(|path| Entry { path, url })
        })
        .collect()
}

/// The `origin` remote's URL from the repository's own config file. A `.git` file, as
/// in a worktree or a submodule checkout, points at the real git directory; a worktree's
/// shared config sits in the directory its `commondir` names.
fn origin_url(repository: &Path) -> Option<String> {
    let dot_git = repository.join(".git");
    let git_directory = if dot_git.is_dir() {
        dot_git
    } else {
        let pointer = std::fs::read_to_string(&dot_git).ok()?;
        repository.join(pointer.trim().strip_prefix("gitdir:")?.trim())
    };
    let common = std::fs::read_to_string(git_directory.join("commondir"))
        .map_or(git_directory.clone(), |common| {
            git_directory.join(common.trim())
        });
    let config = std::fs::read_to_string(common.join("config")).ok()?;
    remote_url(&config, "origin").map(str::to_owned)
}

/// The `url` of `[remote "<name>"]` in a git config file.
fn remote_url<'a>(config: &'a str, name: &str) -> Option<&'a str> {
    let header = format!("[remote \"{name}\"]");
    let mut inside = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
            continue;
        }
        if inside
            && let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
        {
            return Some(value.trim().trim_matches('"'));
        }
    }
    None
}

/// Where a repository is hosted, reduced to what decides whose it is: the host and the
/// namespace above the repository, lower-cased. Credentials and ports are dropped, so
/// neither ever reaches a report.
#[derive(Debug, PartialEq, Eq)]
struct Remote {
    host: String,
    namespace: Vec<String>,
}

impl Remote {
    /// Reads `https://`, `ssh://`, `git://` and scp-style `user@host:path` URLs. A
    /// relative or local URL is `None`: it names a repository beside the project's own.
    fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        if url.starts_with('.') || url.starts_with('/') || url.starts_with('\\') {
            return None;
        }
        let (authority, path) = match url.split_once("://") {
            Some((scheme, _)) if scheme.eq_ignore_ascii_case("file") => return None,
            Some((_, rest)) => rest.split_once('/')?,
            None => {
                let (authority, path) = url.split_once(':')?;
                // `C:\work\repo` is a Windows path, not a host called `C`.
                if authority.contains('/') || authority.len() == 1 {
                    return None;
                }
                (authority, path)
            }
        };
        let host_and_port = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let host = host_and_port
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mut segments: Vec<String> = path
            .split(['/', '\\'])
            .filter(|segment| !segment.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
        segments.pop()?;
        (!host.is_empty()).then_some(Self {
            host,
            namespace: segments,
        })
    }

    fn organisation(&self) -> &str {
        self.namespace.first().map_or("", String::as_str)
    }

    fn label(&self) -> String {
        match self.organisation() {
            "" => self.host.clone(),
            organisation => format!("{}/{organisation}", self.host),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(host: &str, namespace: &[&str]) -> Option<Remote> {
        Some(Remote {
            host: host.to_owned(),
            namespace: namespace.iter().map(|part| (*part).to_owned()).collect(),
        })
    }

    #[test]
    fn urls_are_reduced_to_host_and_namespace() {
        let cases = [
            (
                "git@github.com:Org/Repo.git",
                remote("github.com", &["org"]),
            ),
            (
                "https://github.com/org/repo",
                remote("github.com", &["org"]),
            ),
            (
                "https://github.com/org/repo.git/",
                remote("github.com", &["org"]),
            ),
            ("http://GitHub.COM/Org/repo", remote("github.com", &["org"])),
            (
                "ssh://git@github.com/org/repo.git",
                remote("github.com", &["org"]),
            ),
            (
                "ssh://git@github.com:22/org/repo.git",
                remote("github.com", &["org"]),
            ),
            (
                "git+ssh://git@host.example:2222/org/repo",
                remote("host.example", &["org"]),
            ),
            (
                "git://github.com/org/repo.git",
                remote("github.com", &["org"]),
            ),
            (
                "https://git.example.com:8443/org/repo",
                remote("git.example.com", &["org"]),
            ),
            (
                "https://gitlab.com/group/sub/repo.git",
                remote("gitlab.com", &["group", "sub"]),
            ),
            (
                "git@gitlab.example.com:Group/Sub/Deeper/repo.git",
                remote("gitlab.example.com", &["group", "sub", "deeper"]),
            ),
            ("git@host.example:repo.git", remote("host.example", &[])),
            (
                "  https://github.com/org/repo  ",
                remote("github.com", &["org"]),
            ),
            ("../design.git", None),
            ("./design", None),
            ("/srv/git/design.git", None),
            ("file:///srv/git/design.git", None),
            ("C:\\work\\design.git", None),
            ("https://github.com", None),
            ("", None),
        ];
        for (url, expected) in cases {
            assert_eq!(Remote::parse(url), expected, "{url}");
        }
    }

    #[test]
    fn credentials_in_a_url_never_reach_the_label() {
        let parsed = Remote::parse("https://someone:s3cret-token@github.com/Org/repo.git")
            .expect("a remote");
        assert_eq!(parsed.label(), "github.com/org");
        assert!(!format!("{parsed:?}").contains("s3cret"));
    }

    #[test]
    fn a_submodule_is_foreign_only_on_another_host_or_organisation() {
        let origin = Remote::parse("git@github.com:acme/app.git");
        let origin = origin.as_ref();
        let cases = [
            ("https://github.com/Acme/design.git", None),
            ("git@github.com:acme/tools/design.git", None),
            ("../design.git", None),
            (
                "https://github.com/other-org/design.git",
                Some("github.com/other-org"),
            ),
            ("git@gitlab.com:acme/design.git", Some("gitlab.com/acme")),
        ];
        for (url, expected) in cases {
            assert_eq!(foreign_to(origin, url).as_deref(), expected, "{url}");
        }
        assert_eq!(
            foreign_to(None, "https://github.com/other-org/design.git"),
            None,
            "with no origin to compare against, a submodule is the project's own"
        );
    }

    #[test]
    fn subgroups_under_the_same_top_group_are_the_projects_own() {
        let origin = Remote::parse("https://gitlab.example.com/company/platform/app.git");
        assert_eq!(
            foreign_to(origin.as_ref(), "git@gitlab.example.com:company/web/ui.git"),
            None
        );
        assert_eq!(
            foreign_to(origin.as_ref(), "git@gitlab.example.com:vendor/web/ui.git").as_deref(),
            Some("gitlab.example.com/vendor")
        );
    }

    #[test]
    fn gitmodules_sections_keep_their_path_and_url_together() {
        let listing = "[submodule \"design\"]\n\tpath = design\n\turl = ../design.git\n\
                       [submodule \"lib\"]\n\turl = \"https://github.com/x/lib\"\n\
                       \tpath = \"vendor/lib/\"\n[submodule \"orphan\"]\n\turl = x\n";
        let found: Vec<(&str, Option<&str>)> = entries(listing)
            .iter()
            .map(|entry| (entry.path, entry.url))
            .collect();
        assert_eq!(
            found,
            [
                ("design", Some("../design.git")),
                ("vendor/lib", Some("https://github.com/x/lib"))
            ]
        );
    }

    #[test]
    fn the_origin_url_is_read_from_the_named_remote_only() {
        let config = "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = git@a.example:up/x.git\n\
                      [remote \"origin\"]\n\turl = git@b.example:mine/x.git\n\tfetch = +refs/*\n";
        assert_eq!(
            remote_url(config, "origin"),
            Some("git@b.example:mine/x.git")
        );
        assert_eq!(remote_url("[core]\n", "origin"), None);
    }

    fn scratch(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("rqx-submodules-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        rqx_core::paths::canonicalize(&root).unwrap()
    }

    fn project(name: &str) -> PathBuf {
        let root = scratch(name);
        std::fs::write(
            root.join(".git/config"),
            "[remote \"origin\"]\n\turl = https://github.com/acme/app.git\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("app/design/src")).unwrap();
        std::fs::create_dir_all(root.join("app/shared")).unwrap();
        std::fs::write(
            root.join(".gitmodules"),
            "[submodule \"design\"]\n\tpath = app/design\n\turl = git@github.com:other-org/design.git\n\
             [submodule \"shared\"]\n\tpath = app/shared\n\turl = ../shared.git\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn auto_leaves_out_only_the_foreign_submodule_and_all_leaves_out_both() {
        let root = project("policies");
        let auto = left_out(&root, SubmodulePolicy::Auto);
        let all = left_out(&root, SubmodulePolicy::All);
        let none = left_out(&root, SubmodulePolicy::None);
        let _ = std::fs::remove_dir_all(&root);

        let design = root.join("app").join("design");
        assert_eq!(
            auto,
            [Submodule {
                path: design.clone(),
                foreign: Some("github.com/other-org".to_owned())
            }]
        );
        assert_eq!(
            all.iter().map(|one| one.path.clone()).collect::<Vec<_>>(),
            [design, root.join("app").join("shared")]
        );
        assert!(none.is_empty());
    }

    #[test]
    fn submodules_are_found_from_a_subdirectory_but_not_from_inside_one() {
        let root = project("from-where");
        let from_app = left_out(&root.join("app"), SubmodulePolicy::All);
        let from_inside = left_out(&root.join("app/design/src"), SubmodulePolicy::All);
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(from_app.len(), 2, "{from_app:?}");
        assert!(from_inside.is_empty(), "{from_inside:?}");
    }

    #[test]
    fn a_project_without_an_origin_scans_every_submodule_under_auto() {
        let root = project("no-origin");
        std::fs::write(root.join(".git/config"), "[core]\n\tbare = false\n").unwrap();
        let auto = left_out(&root, SubmodulePolicy::Auto);
        let _ = std::fs::remove_dir_all(&root);
        assert!(auto.is_empty(), "{auto:?}");
    }

    #[test]
    fn a_worktree_reads_the_origin_through_its_git_file() {
        let main = scratch("worktree-main");
        std::fs::write(
            main.join(".git/config"),
            "[remote \"origin\"]\n\turl = git@github.com:acme/app.git\n",
        )
        .unwrap();
        std::fs::create_dir_all(main.join(".git/worktrees/side")).unwrap();
        std::fs::write(main.join(".git/worktrees/side/commondir"), "../..\n").unwrap();
        let side = main.join("side");
        std::fs::create_dir_all(&side).unwrap();
        std::fs::write(
            side.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/side").display()),
        )
        .unwrap();

        let url = origin_url(&side);
        let _ = std::fs::remove_dir_all(&main);
        assert_eq!(url.as_deref(), Some("git@github.com:acme/app.git"));
    }
}

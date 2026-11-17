//! Installing a published release binary over the one that is running.
//!
//! The release carries `rqx-<version>-<platform>.tar.gz` for six platforms and a
//! `SHA256SUMS` beside them, the same files `install.sh` and `install.ps1` read. This
//! does what they do, in the order they do it: download both, check the archive
//! against the checksum, and only then touch the installed file. Anything that does
//! not match, or is missing, stops before that point, so a failed update leaves the
//! running binary exactly as it was.
//!
//! Downloads go through `curl` and extraction through the system `tar`, because rqx
//! carries no TLS or archive stack and both ship with every supported platform
//! (`curl.exe` and `tar.exe` since Windows 10).

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

const DOWNLOADS: &str = "https://github.com/jadhamwi21/rqx/releases/download";

/// Points downloads somewhere else, so a test can serve a release from a directory.
/// It is the only way a non-HTTPS source is ever accepted.
const SOURCE_OVERRIDE: &str = "RQX_RELEASE_URL";

/// The source a test pointed downloads at, if any.
pub fn source_override() -> Option<String> {
    std::env::var(SOURCE_OVERRIDE).ok()
}

/// One published version for this platform, and where its files are.
pub struct Release {
    version: String,
    platform: String,
    base: String,
    overridden: bool,
}

impl Release {
    /// The release at GitHub, unless the environment names another source.
    pub fn published(version: &str) -> Result<Self> {
        Self::new(version, source_override())
    }

    pub fn new(version: &str, source: Option<String>) -> Result<Self> {
        check_version(version)?;
        let overridden = source.is_some();
        Ok(Self {
            version: version.to_owned(),
            platform: platform()?,
            base: source.unwrap_or_else(|| DOWNLOADS.to_owned()),
            overridden,
        })
    }

    pub fn archive_name(&self) -> String {
        format!("rqx-{}-{}.tar.gz", self.version, self.platform)
    }

    pub fn archive_url(&self) -> String {
        format!("{}/v{}/{}", self.base, self.version, self.archive_name())
    }

    pub fn checksums_url(&self) -> String {
        format!("{}/v{}/SHA256SUMS", self.base, self.version)
    }

    /// What curl may fetch from. HTTPS only, unless a test pointed the source at a
    /// local directory.
    fn protocols(&self) -> &'static str {
        if self.overridden {
            "=https,file"
        } else {
            "=https"
        }
    }

    fn download(&self, url: &str, destination: &Path) -> Result<()> {
        let program = if cfg!(windows) { "curl.exe" } else { "curl" };
        let output = Command::new(program)
            .args(["--proto", self.protocols(), "--proto-redir", "=https"])
            .args(["--tlsv1.2", "-fsSL", "--max-time", "300", "-o"])
            .arg(destination)
            .arg(url)
            .output()
            .with_context(|| format!("could not run `{program}`, which rqx downloads with"))?;
        if !output.status.success() {
            bail!(
                "could not download {url}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }
}

/// Downloads, verifies and installs `release` over `target`, which is the running
/// executable. Nothing is written to `target` until the archive has been verified.
pub fn install(release: &Release, target: &Path) -> Result<()> {
    let scratch = Scratch::beside(target)?;
    let archive = scratch.path().join(release.archive_name());
    let checksums = scratch.path().join("SHA256SUMS");

    release.download(&release.archive_url(), &archive)?;
    release.download(&release.checksums_url(), &checksums)?;
    verify(&archive, &checksums, &release.archive_name())?;

    let binary = extract(&archive, scratch.path())?;
    replace_executable(&binary, target)
}

/// The platform name releases are published under, which is the npm package's.
fn platform() -> Result<String> {
    let system = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "win32",
        other => bail!("no rqx binary is published for {other}"),
    };
    let architecture = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => bail!("no rqx binary is published for the {other} architecture"),
    };
    Ok(format!("{system}-{architecture}"))
}

/// A version goes into a URL, so it is held to the shape of one before it does.
fn check_version(version: &str) -> Result<()> {
    let numbered = version.split('.').count() >= 3
        && version.starts_with(|first: char| first.is_ascii_digit())
        && version
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'));
    if !numbered {
        bail!("`{version}` is not a version like 1.2.3");
    }
    Ok(())
}

/// The checksum `SHA256SUMS` lists for one archive, in `sha256sum`'s format.
fn expected_checksum<'a>(checksums: &'a str, archive_name: &str) -> Option<&'a str> {
    checksums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        let name = name.trim_start().trim_start_matches('*');
        let valid = hash.len() == 64 && hash.chars().all(|digit| digit.is_ascii_hexdigit());
        (valid && name == archive_name).then_some(hash)
    })
}

fn sha256_of(path: &Path) -> Result<String> {
    let mut file =
        File::open(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn verify(archive: &Path, checksums: &Path, archive_name: &str) -> Result<()> {
    let listed = fs::read_to_string(checksums).context("could not read SHA256SUMS")?;
    let Some(expected) = expected_checksum(&listed, archive_name) else {
        bail!("SHA256SUMS has no valid checksum for {archive_name}; nothing was installed");
    };
    let actual = sha256_of(archive)?;
    if !actual.eq_ignore_ascii_case(expected) {
        bail!(
            "{archive_name} hashes to {actual} but SHA256SUMS says {expected}; nothing was installed"
        );
    }
    Ok(())
}

/// The system `tar`. On Windows that is the one in System32 by full path: a GNU tar
/// earlier on `PATH` (Git for Windows ships one) reads `C:\...` as a remote host.
fn tar_program() -> PathBuf {
    if cfg!(windows)
        && let Some(root) = std::env::var_os("SystemRoot")
    {
        let system_tar = Path::new(&root).join("System32").join("tar.exe");
        if system_tar.exists() {
            return system_tar;
        }
    }
    PathBuf::from("tar")
}

fn extract(archive: &Path, scratch: &Path) -> Result<PathBuf> {
    let destination = scratch.join("extracted");
    fs::create_dir(&destination)?;
    let name = format!("rqx{}", std::env::consts::EXE_SUFFIX);

    let output = Command::new(tar_program())
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&destination)
        .arg(&name)
        .output()
        .context("could not run `tar`, which rqx unpacks the release with")?;
    let binary = destination.join(&name);
    if !output.status.success() || !binary.is_file() {
        bail!("the release archive does not contain {name}; nothing was installed");
    }
    Ok(binary)
}

/// The new file is already on the target's filesystem, so the rename is atomic: a
/// reader sees the old binary or the new one, never half of either. The old file's
/// permissions carry over, so an install someone made group-writable stays so.
#[cfg(not(windows))]
fn replace_executable(binary: &Path, target: &Path) -> Result<()> {
    let permissions = fs::metadata(target)
        .with_context(|| format!("could not read {}", target.display()))?
        .permissions();
    fs::set_permissions(binary, permissions)?;
    fs::rename(binary, target).with_context(|| format!("could not replace {}", target.display()))
}

/// A running `.exe` cannot be overwritten or renamed over on Windows, which is the
/// problem `self-replace` exists to solve: it moves the running file aside and
/// removes it once the process has exited.
#[cfg(windows)]
fn replace_executable(binary: &Path, target: &Path) -> Result<()> {
    self_replace::self_replace(binary)
        .with_context(|| format!("could not replace {}", target.display()))
}

/// A directory the download and extraction happen in, removed when it goes out of
/// scope whether the install worked or not.
///
/// It sits beside the executable rather than in the system temp directory for two
/// reasons: the final rename has to stay on one filesystem to be atomic, and a
/// directory only the owner of the install can write to cannot be swapped under a
/// verified archive by someone else.
struct Scratch(PathBuf);

impl Scratch {
    fn beside(target: &Path) -> Result<Self> {
        let parent = target
            .parent()
            .with_context(|| format!("{} has no directory", target.display()))?;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let path = parent.join(format!(".rqx-update-{}-{unique}", std::process::id()));
        fs::create_dir(&path).with_context(|| {
            format!(
                "cannot write to {}, so rqx cannot replace itself there; nothing was installed",
                parent.display()
            )
        })?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn a_checksum_is_read_for_the_named_archive_only() {
        let sums = format!("{HASH}  rqx-1.0.0-linux-x64.tar.gz\n{HASH}  other.tar.gz\n");
        assert_eq!(
            expected_checksum(&sums, "rqx-1.0.0-linux-x64.tar.gz"),
            Some(HASH)
        );
        assert_eq!(
            expected_checksum(&sums, "rqx-1.0.0-darwin-x64.tar.gz"),
            None
        );
        assert_eq!(
            expected_checksum(&format!("{HASH} *a.tar.gz"), "a.tar.gz"),
            Some(HASH),
            "sha256sum's binary-mode marker"
        );
        assert_eq!(expected_checksum("not-a-hash  a.tar.gz", "a.tar.gz"), None);
    }

    #[test]
    fn a_version_must_look_like_one_before_it_reaches_a_url() {
        assert!(check_version("0.2.1").is_ok());
        assert!(check_version("0.3.0-rc.1").is_ok());
        for bad in ["latest", "1.2", "../1.2.3", "1.2.3/x", "v1.2.3", ""] {
            assert!(check_version(bad).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn only_the_test_override_lets_curl_read_a_file() {
        let published = Release::new("1.2.3", None).expect("a release");
        assert_eq!(published.protocols(), "=https");
        assert!(published.archive_url().starts_with("https://github.com/"));

        let local = Release::new("1.2.3", Some("file:///releases".to_owned())).expect("a release");
        assert_eq!(local.protocols(), "=https,file");
        assert_eq!(local.checksums_url(), "file:///releases/v1.2.3/SHA256SUMS");
    }
}

#[cfg(all(test, unix))]
mod installing {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const VERSION: &str = "9.9.9";
    const OLD: &str = "#!/bin/sh\necho old\n";
    const NEW: &str = "#!/bin/sh\necho new\n";

    fn workspace() -> Scratch {
        Scratch::beside(&std::env::temp_dir().join("rqx-release-test")).expect("a workspace")
    }

    /// A release directory laid out as GitHub serves one, holding a binary that
    /// prints `new`. `listed` is what SHA256SUMS claims the archive hashes to.
    fn fake_release(root: &Path, listed: impl Fn(&Path) -> String) -> Release {
        let staged = root.join("staged");
        fs::create_dir_all(&staged).expect("staging directory");
        fs::write(staged.join("rqx"), NEW).expect("the binary");

        let source = format!("file://{}", root.display());
        let release = Release::new(VERSION, Some(source)).expect("a release");
        let published = root.join(format!("v{VERSION}"));
        fs::create_dir_all(&published).expect("release directory");
        let archive = published.join(release.archive_name());
        let packed = Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&staged)
            .arg("rqx")
            .status()
            .expect("tar");
        assert!(packed.success());
        fs::write(
            published.join("SHA256SUMS"),
            format!("{}  {}\n", listed(&archive), release.archive_name()),
        )
        .expect("checksums");
        release
    }

    /// An installed binary in its own directory, so what an update leaves beside it
    /// can be counted.
    fn installed_copy(work: &Scratch) -> PathBuf {
        let bin = work.path().join("bin");
        fs::create_dir(&bin).expect("bin");
        let target = bin.join("rqx");
        fs::write(&target, OLD).expect("the installed binary");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).expect("permissions");
        target
    }

    fn contents(path: &Path) -> String {
        fs::read_to_string(path).expect("readable")
    }

    #[test]
    fn a_verified_release_replaces_the_binary_and_keeps_its_permissions() {
        let work = workspace();
        let served = work.path().join("served");
        let release = fake_release(&served, |archive| sha256_of(archive).expect("a hash"));
        let target = installed_copy(&work);

        install(&release, &target).expect("the update");

        assert_eq!(contents(&target), NEW);
        let mode = fs::metadata(&target)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o750);
        let beside = fs::read_dir(target.parent().expect("bin"))
            .expect("bin")
            .count();
        assert_eq!(beside, 1, "scratch files were left behind");
    }

    #[test]
    fn a_checksum_mismatch_installs_nothing() {
        let work = workspace();
        let release = fake_release(&work.path().join("served"), |_| "0".repeat(64));
        let target = installed_copy(&work);

        let error = install(&release, &target).expect_err("a mismatch must fail");

        assert!(
            format!("{error:#}").contains("nothing was installed"),
            "{error:#}"
        );
        assert_eq!(contents(&target), OLD);
    }

    #[test]
    fn a_missing_checksum_installs_nothing() {
        let work = workspace();
        let served = work.path().join("served");
        let release = fake_release(&served, |archive| sha256_of(archive).expect("a hash"));
        let sums = served.join(format!("v{VERSION}/SHA256SUMS"));
        fs::write(
            &sums,
            format!("{}  some-other-archive.tar.gz\n", "a".repeat(64)),
        )
        .expect("checksums");
        let target = installed_copy(&work);

        let error = install(&release, &target).expect_err("no checksum must fail");

        assert!(
            format!("{error:#}").contains("no valid checksum"),
            "{error:#}"
        );
        assert_eq!(contents(&target), OLD);
    }

    #[test]
    fn a_release_that_was_never_published_installs_nothing() {
        let work = workspace();
        let source = format!("file://{}/empty", work.path().display());
        let release = Release::new(VERSION, Some(source)).expect("a release");
        let target = installed_copy(&work);

        install(&release, &target).expect_err("nothing to download");

        assert_eq!(contents(&target), OLD);
    }
}

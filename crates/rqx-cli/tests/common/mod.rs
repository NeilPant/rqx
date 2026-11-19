//! Helpers shared by the integration tests, so a platform difference is handled in one
//! place instead of once per test file.

// Every test file is its own crate and compiles this module separately, so each one
// leaves the helpers it does not need unused.
#![allow(dead_code)]

mod scratch;
pub use scratch::Scratch;

use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

/// Kills a child process that outlives its deadline. Reading a pipe blocks with no
/// timeout, so without this a server that stops answering hangs the suite forever
/// rather than failing it. Dropping the watchdog disarms it, which keeps it from
/// killing a recycled process id after the test has finished.
pub struct Watchdog {
    _disarm: Sender<()>,
}

impl Watchdog {
    pub fn arm(pid: u32, deadline: Duration) -> Self {
        let (disarm, disarmed) = mpsc::channel();
        std::thread::spawn(move || {
            if disarmed.recv_timeout(deadline) == Err(RecvTimeoutError::Timeout) {
                kill(pid);
            }
        });
        Self { _disarm: disarm }
    }
}

/// The process is owned by a `Child` that the test is blocked reading from, so it is
/// killed by id. `kill` does not exist on Windows and `taskkill` does not exist
/// elsewhere.
fn kill(pid: u32) {
    let (program, flags): (&str, &[&str]) = if cfg!(windows) {
        ("taskkill", &["/F", "/PID"])
    } else {
        ("kill", &["-9"])
    };
    let _ = Command::new(program)
        .args(flags)
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Runs the command, reads the first few bytes of its output and closes the pipe while
/// it may still be writing: what `rqx scan . | head` does to it. The shell and `head`
/// are not available on every platform, and a broken pipe is an error kind that std
/// reports on Windows as well, so no platform needs to be excluded.
pub fn run_with_a_reader_that_walks_away(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the command starts");
    let mut reader = child.stdout.take().expect("a piped stdout");
    let read = reader
        .read(&mut [0u8; 16])
        .expect("a first chunk of output");
    assert!(read > 0, "the command wrote nothing before closing");
    drop(reader);
    child.wait_with_output().expect("the command finishes")
}

/// `file://` followed by the path, the way an editor and git both write it: forward
/// slashes, a leading slash before a Windows drive letter, and spaces escaped.
pub fn file_uri(path: &Path) -> String {
    let mut text = path.display().to_string();
    if cfg!(windows) {
        text = text.replace('\\', "/");
    }
    if !text.starts_with('/') {
        text.insert(0, '/');
    }
    format!("file://{}", text.replace(' ', "%20"))
}

/// A SARIF fixture with each `{root}` replaced by the path of `root` as a file URI
/// carries it. The text goes through a JSON value, so the path is escaped as JSON and
/// a Windows backslash or quote cannot corrupt the document.
pub fn sarif_for(fixture: &str, root: &Path) -> String {
    fn fill(value: &mut serde_json::Value, root: &str) {
        match value {
            serde_json::Value::String(text) => *text = text.replace("{root}", root),
            serde_json::Value::Array(items) => items.iter_mut().for_each(|item| fill(item, root)),
            serde_json::Value::Object(map) => map.values_mut().for_each(|item| fill(item, root)),
            _ => {}
        }
    }
    let mut document: serde_json::Value = serde_json::from_str(fixture).expect("a SARIF fixture");
    let uri = file_uri(root);
    fill(&mut document, uri.trim_start_matches("file://"));
    document.to_string()
}

/// Where a configuration file that does not exist lives, so git finds none.
const NO_CONFIGURATION: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

/// Cuts a git process off from the machine it runs on: no global or system
/// configuration, a fixed author and committer, and none of the variables a git hook
/// exports. A contributor's `commit.gpgsign`, `core.hooksPath`, `init.defaultBranch`,
/// commit template or `safe.directory` then cannot change what a test sees. Apply it to
/// every process a test starts that runs git, `rqx` included.
pub fn isolate_git(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", NO_CONFIGURATION)
        .env("GIT_CONFIG_SYSTEM", NO_CONFIGURATION)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
}

/// `git`, isolated from the machine's configuration, with signing and hooks off.
pub fn git() -> Command {
    let mut command = Command::new("git");
    isolate_git(&mut command).args([
        "-c",
        "commit.gpgsign=false",
        "-c",
        "tag.gpgsign=false",
        "-c",
        "core.autocrlf=false",
        "-c",
        &format!("core.hooksPath={NO_CONFIGURATION}"),
    ]);
    command
}

/// `git -C directory`, isolated as `git()` is.
pub fn git_in(directory: &Path) -> Command {
    let mut command = git();
    command.arg("-C").arg(directory);
    command
}

/// Runs git in `directory`, fails the test with git's message when it exits non-zero,
/// and returns what it printed.
pub fn run_git(directory: &Path, args: &[&str]) -> String {
    let output = git_in(directory).args(args).output().expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A throwaway git repository that commits the files it is given, so a test can build
/// the history a review compares against instead of mocking git. The directory is
/// removed when the repository is dropped.
pub struct Repository {
    directory: Scratch,
    pub root: std::path::PathBuf,
}

impl Repository {
    pub fn new(name: &str) -> Self {
        let directory = Scratch::new(name);
        let root = directory.to_path_buf();
        let repository = Self { directory, root };
        repository.git(&["init", "-q"]);
        repository
    }

    pub fn git(&self, args: &[&str]) {
        run_git(&self.root, args);
    }

    /// Writes the files, removes the ones named with no text, and commits everything.
    pub fn commit(&self, message: &str, files: &[(&str, Option<&str>)]) {
        for (path, text) in files {
            let file = self.root.join(path);
            match text {
                Some(text) => {
                    std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
                    std::fs::write(file, text).expect("a file");
                }
                None => {
                    let _ = std::fs::remove_file(file);
                }
            }
        }
        self.git(&["add", "-A"]);
        self.git(&["commit", "-qm", message]);
    }

    /// Runs rqx in the repository and returns what it printed and how it exited.
    pub fn rqx_output(&self, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rqx"));
        isolate_git(&mut command);
        command
            .args(args)
            .current_dir(&self.root)
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", &self.root)
            .env_remove("RQX_CONFIG")
            .env_remove("RQX_POLICY")
            .env_remove("RQX_LEVEL")
            .output()
            .expect("rqx runs")
    }

    /// Runs rqx in the repository and returns its parsed JSON lines.
    pub fn rqx_lines(&self, args: &[&str]) -> Vec<serde_json::Value> {
        let output = self.rqx_output(args);
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

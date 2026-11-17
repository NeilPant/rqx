//! Helpers shared by the integration tests, so a platform difference is handled in one
//! place instead of once per test file.

// Every test file is its own crate and compiles this module separately, so each one
// leaves the helpers it does not need unused.
#![allow(dead_code)]

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

/// A throwaway git repository that commits the files it is given, so a test can build
/// the history a review compares against instead of mocking git.
pub struct Repository {
    pub root: std::path::PathBuf,
}

impl Repository {
    pub fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "rqx-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temp repository");
        let repository = Self { root };
        repository.git(&["init", "-q"]);
        repository.git(&["config", "core.autocrlf", "false"]);
        repository.git(&["config", "user.email", "test@example.com"]);
        repository.git(&["config", "user.name", "test"]);
        repository
    }

    pub fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
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
        Command::new(env!("CARGO_BIN_EXE_rqx"))
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

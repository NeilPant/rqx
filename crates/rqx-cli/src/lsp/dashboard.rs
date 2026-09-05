use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use lsp_server::{Message, RequestId, Response};
use serde_json::json;

use super::diagnostics::Publisher;

/// The one `rqx serve` this session owns. Every editor that asks gets the same page,
/// and closing the editor closes the page's server with it instead of leaving one
/// process per request bound to one port each.
#[derive(Default)]
pub struct Dashboard {
    child: Option<Child>,
    started: Arc<Mutex<Started>>,
}

/// What the reader thread learns once the child prints its address, and who is
/// waiting to hear it. A second request that lands while the first is still starting
/// joins the queue rather than starting a second server.
#[derive(Default)]
struct Started {
    url: Option<String>,
    waiting: Vec<RequestId>,
}

impl Dashboard {
    /// Answers `reply_to` with `{ url }` or `{ error }` — now if the server is already
    /// up, otherwise once it prints its address. The first line arrives only after the
    /// child's initial scan, so the wait is off the message loop.
    pub fn request(
        &mut self,
        root: &Path,
        config: Option<&Path>,
        publisher: &Arc<Publisher>,
        reply_to: RequestId,
    ) {
        if self.is_running() {
            let mut started = lock(&self.started);
            match started.url.clone() {
                Some(url) => publisher.send(answer(reply_to, Ok(url))),
                None => started.waiting.push(reply_to),
            }
            return;
        }

        self.child = None;
        *lock(&self.started) = Started {
            url: None,
            waiting: vec![reply_to],
        };
        match spawn(root, config) {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                self.child = Some(child);
                let started = Arc::clone(&self.started);
                let publisher = Arc::clone(publisher);
                thread::spawn(move || {
                    let outcome = stdout
                        .ok_or_else(|| "the dashboard said nothing".to_owned())
                        .and_then(read_address);
                    settle(&started, &publisher, outcome);
                });
            }
            Err(reason) => settle(&self.started, publisher, Err(reason)),
        }
    }

    fn is_running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn(root: &Path, config: Option<&Path>) -> Result<Child, String> {
    let binary = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = Command::new(binary);
    command
        .arg("serve")
        .arg(root)
        .args(["--addr", "127.0.0.1:0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Without this the page and the editor could resolve two different RQX.toml files
    // and disagree about what the project scores.
    if let Some(config) = config {
        command.arg("--config").arg(config);
    }
    command
        .spawn()
        .map_err(|error| format!("cannot start the dashboard: {error}"))
}

fn read_address(stdout: std::process::ChildStdout) -> Result<String, String> {
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    line.split_whitespace()
        .find(|word| word.starts_with("http://"))
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("the dashboard did not name an address: {}", line.trim()))
}

fn settle(started: &Mutex<Started>, publisher: &Publisher, outcome: Result<String, String>) {
    let mut started = lock(started);
    started.url = outcome.as_ref().ok().cloned();
    for id in started.waiting.drain(..) {
        publisher.send(answer(id, outcome.clone()));
    }
}

fn answer(id: RequestId, outcome: Result<String, String>) -> Message {
    let value = match outcome {
        Ok(url) => json!({ "url": url }),
        Err(reason) => json!({ "error": reason }),
    };
    Message::Response(Response::new_ok(id, value))
}

fn lock(started: &Mutex<Started>) -> std::sync::MutexGuard<'_, Started> {
    started
        .lock()
        .expect("the dashboard state is only ever held to read or set the address")
}

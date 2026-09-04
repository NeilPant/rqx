mod project;
mod routes;
mod theme;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::sync::broadcast;

use crate::config::Resolved;
use crate::scan::ScanOutcome;

pub use project::{Project, Tick};

/// Everything a request reads. The project itself stays on the polling thread: a
/// request never blocks behind an analysis, it reads the last answer.
pub struct Live {
    pub root: std::path::PathBuf,
    pub settings: Resolved,
    pub outcome: RwLock<Arc<ScanOutcome>>,
    pub trend: RwLock<Vec<Tick>>,
    pub events: broadcast::Sender<String>,
    /// Proves a write request came from the page this process served rather than from
    /// another page in the same browser.
    pub nonce: String,
}

impl Live {
    pub fn outcome(&self) -> Arc<ScanOutcome> {
        Arc::clone(&self.outcome.read().expect("the outcome lock"))
    }
}

pub fn serve(
    path: &Path,
    settings: Resolved,
    address: SocketAddr,
    interval: u64,
    open: bool,
) -> Result<()> {
    let root = path
        .canonicalize()
        .with_context(|| format!("`{}` cannot be read", path.display()))?;

    // No token path: a browser cannot carry a bearer token to a page you opened, so
    // the only honest answer for a non-loopback bind is to refuse it.
    anyhow::ensure!(
        address.ip().is_loopback(),
        "`--addr {address}` is not a loopback address. `rqx serve` renders your source \
         in a browser and has no way to authenticate one, so it binds locally only."
    );

    let mut project = Project::read(&root, settings.clone())?;
    let (events, _) = broadcast::channel(64);
    let live = Arc::new(Live {
        root: root.clone(),
        settings,
        outcome: RwLock::new(project.outcome()),
        trend: RwLock::new(project.trend()),
        events: events.clone(),
        nonce: nonce(),
    });

    let watcher = Arc::clone(&live);
    let interval = Duration::from_millis(interval.max(100));
    std::thread::spawn(move || poll(&mut project, &watcher, interval));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let app = routes::router(Arc::clone(&live));
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .with_context(|| format!("cannot listen on {address}"))?;
        let bound = listener.local_addr()?;
        let url = format!("http://{bound}");
        println!("rqx serve {url}");
        if open {
            launch(&url);
        }
        axum::serve(listener, app).await?;
        Ok(())
    })
}

/// One `stat` per file on an interval, rather than a filesystem watcher. It costs
/// tens of milliseconds on a twelve-thousand-file tree and needs no dependency; a
/// repository large enough for that to hurt should raise `--interval`.
fn poll(project: &mut Project, live: &Live, interval: Duration) {
    loop {
        std::thread::sleep(interval);
        let Ok(seen) = project.walk() else {
            continue;
        };
        let started = Instant::now();
        let Some(changed) = project.refresh(&seen) else {
            continue;
        };
        let outcome = project.outcome();
        let health = crate::report::Report::everything(&outcome).health();

        *live.outcome.write().expect("the outcome lock") = outcome;
        *live.trend.write().expect("the trend lock") = project.trend();

        let _ = live.events.send(
            serde_json::json!({
                "kind": "refresh",
                "health": health,
                "elapsedMs": started.elapsed().as_millis(),
                "changed": changed,
            })
            .to_string(),
        );
    }
}

fn nonce() -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn launch(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(url).spawn();
}

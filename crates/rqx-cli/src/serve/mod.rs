mod project;
mod routes;
pub mod theme;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use tokio::sync::broadcast;

use crate::config::Resolved;
use crate::scan::ScanOutcome;

pub use project::{Project, Tick};

pub struct Options {
    pub address: SocketAddr,
    pub interval: u64,
    pub open: bool,
    /// Template for "open in editor" links, with `{path}`, `{line}` and `{column}`
    /// filled in by the page.
    pub editor_url: String,
    pub read_only: bool,
    /// The `--config` the caller passed, so a reload after `RQX.toml` changes resolves
    /// the same way the first read did.
    pub explicit_config: Option<PathBuf>,
}

/// Everything a request reads. The project itself stays on the polling thread: a
/// request never blocks behind an analysis, it reads the last answer.
pub struct Live {
    pub root: PathBuf,
    settings: RwLock<Resolved>,
    pub outcome: RwLock<Arc<ScanOutcome>>,
    pub trend: RwLock<Vec<Tick>>,
    pub events: broadcast::Sender<String>,
    /// Proves a write request came from the page this process served rather than from
    /// another page in the same browser.
    pub nonce: String,
    pub editor_url: String,
    pub read_only: bool,
}

impl Live {
    pub fn outcome(&self) -> Arc<ScanOutcome> {
        Arc::clone(&self.outcome.read().expect("the outcome lock"))
    }

    /// A copy rather than a guard, so a handler never holds the lock across an await
    /// or a scan.
    pub fn settings(&self) -> Resolved {
        self.settings.read().expect("the settings lock").clone()
    }
}

pub fn serve(path: &Path, settings: Resolved, options: Options) -> Result<()> {
    let root = path
        .canonicalize()
        .with_context(|| format!("`{}` cannot be read", path.display()))?;

    // No token path: a browser cannot carry a bearer token to a page you opened, so
    // the only honest answer for a non-loopback bind is to refuse it.
    anyhow::ensure!(
        options.address.ip().is_loopback(),
        "`--addr {}` is not a loopback address. `rqx serve` renders your source \
         in a browser and has no way to authenticate one, so it binds locally only.",
        options.address
    );

    let mut project = Project::read(&root, settings.clone())?;
    let (events, _) = broadcast::channel(64);
    let live = Arc::new(Live {
        root: root.clone(),
        settings: RwLock::new(settings),
        outcome: RwLock::new(project.outcome()),
        trend: RwLock::new(project.trend()),
        events: events.clone(),
        nonce: nonce(),
        editor_url: options.editor_url,
        read_only: options.read_only,
    });

    let watcher = Arc::clone(&live);
    let interval = Duration::from_millis(options.interval.max(100));
    let explicit_config = options.explicit_config;
    std::thread::spawn(move || poll(&mut project, &watcher, interval, explicit_config.as_deref()));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let app = routes::router(Arc::clone(&live));
        let listener = tokio::net::TcpListener::bind(options.address)
            .await
            .with_context(|| format!("cannot listen on {}", options.address))?;
        let bound = listener.local_addr()?;
        let url = format!("http://{bound}");
        println!("rqx serve {url}");
        if options.open {
            launch(&url);
        }
        axum::serve(listener, app).await?;
        Ok(())
    })
}

/// One `stat` per file on an interval, rather than a filesystem watcher. It costs
/// tens of milliseconds on a twelve-thousand-file tree and needs no dependency; a
/// repository large enough for that to hurt should raise `--interval`.
fn poll(project: &mut Project, live: &Live, interval: Duration, explicit_config: Option<&Path>) {
    let mut config_seen = config_stamps(&live.root, &live.settings());
    loop {
        std::thread::sleep(interval);

        // The walker reads source files only, so a config edit — the page's own
        // "disable this rule" included — has to be noticed here or it never lands.
        let config_now = config_stamps(&live.root, &live.settings());
        if config_now != config_seen {
            config_seen = config_now;
            if let Ok(mut settings) = crate::config::resolve(&live.root, explicit_config, None) {
                settings.config.scan.max_findings = 0;
                *live.settings.write().expect("the settings lock") = settings.clone();
                project.reload(settings);
                publish(project, live, Instant::now(), Vec::new());
            }
            continue;
        }

        let Ok(seen) = project.walk() else {
            continue;
        };
        let started = Instant::now();
        let Some(changed) = project.refresh(&seen) else {
            continue;
        };
        publish(project, live, started, changed);
    }
}

fn publish(project: &Project, live: &Live, started: Instant, changed: Vec<project::Change>) {
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

/// The project's own `RQX.toml` is watched even when it does not exist yet, because
/// the first "disable this rule" from the page is what creates it.
fn config_stamps(root: &Path, settings: &Resolved) -> BTreeMap<PathBuf, Option<SystemTime>> {
    settings
        .sources
        .iter()
        .cloned()
        .chain(std::iter::once(
            root.join(rqx_core::config::CONFIG_FILE_NAME),
        ))
        .map(|path| {
            let modified = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .ok();
            (path, modified)
        })
        .collect()
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

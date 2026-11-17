pub(crate) mod root;
mod tools;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::transport::io::stdio;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt, tool_handler};

use crate::config::Resolved;
use crate::scan::{self, ScanOutcome};

/// A token in `argv` is readable by every process on the machine, so it is only ever
/// taken from the environment.
const TOKEN_ENV: &str = "RQX_MCP_TOKEN";

const ENDPOINT: &str = "/mcp";

const INSTRUCTIONS: &str = "\
rqx finds dead code, duplication and architectural drift, and names the refactoring \
that fixes each one. Call `scan` to analyse the project; every other project tool \
reads that scan, running one first if none has happened yet. `scan` answers with a \
summary — health, grades, what to open first — and never with the findings \
themselves: page those out of `findings`, narrowed by rule, severity or path. Every \
tool is read-only; rqx never edits your source, and the fix it names is a refactoring \
for you to make.";

/// Kept so the drill-down tools do not each pay for a scan of their own, and numbered
/// because a cursor into one is meaningless once the project has been scanned again.
struct Snapshot {
    id: u64,
    scanned: PathBuf,
    outcome: ScanOutcome,
}

#[derive(Clone)]
pub struct Rqx {
    root: Arc<PathBuf>,
    settings: Arc<Resolved>,
    // One scan at a time for the whole server: a second lock would only earn its keep
    // if one process were ever asked to serve several projects.
    last: Arc<tokio::sync::Mutex<Option<Arc<Snapshot>>>>,
    scans: Arc<AtomicU64>,
    tool_router: ToolRouter<Self>,
}

impl Rqx {
    fn new(root: PathBuf, settings: Resolved) -> Self {
        Self {
            root: Arc::new(root),
            settings: Arc::new(settings),
            last: Arc::new(tokio::sync::Mutex::new(None)),
            scans: Arc::new(AtomicU64::new(1)),
            tool_router: Self::tool_router(),
        }
    }

    /// A path is reported relative to the root rather than as it sits on disk: an agent
    /// has no use for the server's home directory, and over HTTP it is not the caller's
    /// to know.
    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(self.root.as_path())
            .ok()
            .map(rqx_core::paths::display_path)
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| ".".to_owned())
    }

    /// `None` reads whatever was scanned last rather than the root, because a
    /// drill-down after a scan of a subdirectory belongs to that scan — reading the root
    /// would answer a cursor out of findings it was never issued against.
    ///
    /// The lock is the coalescing: a caller arriving mid-scan waits and then finds the
    /// answer already there, rather than starting a second scan of the same tree.
    async fn snapshot(&self, scanned: Option<PathBuf>) -> Result<Arc<Snapshot>, McpError> {
        let mut slot = self.last.lock().await;
        if scanned.is_none()
            && let Some(kept) = slot.clone()
        {
            return Ok(kept);
        }
        let scanned = scanned.unwrap_or_else(|| self.root.as_ref().clone());

        let settings = Arc::clone(&self.settings);
        let target = scanned.clone();
        let outcome = tokio::task::spawn_blocking(move || scan::scan(&target, &settings))
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?
            .map_err(|error| McpError::internal_error(format!("{error:#}"), None))?;

        let snapshot = Arc::new(Snapshot {
            id: self.scans.fetch_add(1, Ordering::Relaxed),
            scanned,
            outcome,
        });
        *slot = Some(Arc::clone(&snapshot));
        Ok(snapshot)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Rqx {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("rqx", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

pub fn serve(path: &Path, settings: Resolved, http: Option<SocketAddr>) -> Result<()> {
    let root = rqx_core::paths::canonicalize(path)
        .with_context(|| format!("`{}` cannot be read", path.display()))?;
    let token = std::env::var(TOKEN_ENV)
        .ok()
        .filter(|token| !token.is_empty());

    if let Some(address) = http
        && !address.ip().is_loopback()
        && token.is_none()
    {
        // Refused rather than warned: a warning on a server's stderr is a warning
        // nobody reads, and the server hands out the contents of every file it parses.
        bail!(
            "`--http {address}` is not a loopback address, and an rqx server with no token is \
             readable by anyone who can reach the port. Set {TOKEN_ENV}, or bind 127.0.0.1."
        );
    }

    // Nothing on the async side is CPU-bound — it drives a pipe or a socket while the
    // scan runs on its own threads — so a second worker pool would sit idle.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let server = Rqx::new(root, settings);
        match http {
            None => over_stdio(server).await,
            Some(address) => over_http(server, address, token).await,
        }
    })
}

async fn over_stdio(server: Rqx) -> Result<()> {
    server
        .serve(stdio())
        .await
        .context("the MCP handshake failed")?
        .waiting()
        .await?;
    Ok(())
}

async fn over_http(server: Rqx, address: SocketAddr, token: Option<String>) -> Result<()> {
    // Cloned rather than rebuilt per session, so two agents on the same project share
    // one scan instead of each paying for their own.
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let guard = axum::middleware::from_fn(move |request, next| {
        let token = token.clone();
        async move { admit(&token, request, next).await }
    });
    let app = axum::Router::new()
        .nest_service(ENDPOINT, service)
        .layer(guard);

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("cannot listen on {address}"))?;
    eprintln!("rqx mcp listening on http://{address}{ENDPOINT}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// An MCP client sends no `Origin`; a page doing DNS rebinding does, which is the
/// whole of the browser attack this transport is exposed to.
async fn admit(
    token: &Option<String>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{StatusCode, header};

    let refuse = |status: StatusCode, why: &str| {
        axum::response::IntoResponse::into_response((status, format!("{why}\n")))
    };

    if request.headers().contains_key(header::ORIGIN) {
        return refuse(StatusCode::FORBIDDEN, "rqx does not serve browser origins");
    }
    if let Some(expected) = token {
        let offered = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if !offered.is_some_and(|offered| constant_time_eq(offered, expected)) {
            return refuse(StatusCode::UNAUTHORIZED, "a bearer token is required");
        }
    }
    next.run(request).await
}

/// Compared without an early exit, because the time a `==` takes to fail says how much
/// of the token was right.
fn constant_time_eq(offered: &str, expected: &str) -> bool {
    if offered.len() != expected.len() {
        return false;
    }
    offered
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

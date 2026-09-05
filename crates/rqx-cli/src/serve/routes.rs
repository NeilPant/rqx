use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{RawQuery, State};
use axum::http::{Method, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Router, middleware};
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;

use super::Live;
use crate::report::{Report, projection};

/// The header a write carries to prove it came from the page this process served.
const NONCE_HEADER: &str = "x-rqx-nonce";

const PAGE: &str = include_str!("app/index.html");
const STYLE: &str = include_str!("app/app.css");
const SCRIPT: &str = include_str!("app/app.js");
const PREACT: &str = include_str!("app/vendor/preact.js");

/// The two families the stylesheet asks for. Shipped rather than linked for the same
/// reason Preact is: a binary that promises no runtime dependencies cannot render
/// correctly only on machines that happen to have the fonts installed.
const FONTS: [(&str, &[u8]); 3] = [
    (
        "/vendor/plex-sans.woff2",
        include_bytes!("app/vendor/plex-sans.woff2"),
    ),
    (
        "/vendor/plex-mono.woff2",
        include_bytes!("app/vendor/plex-mono.woff2"),
    ),
    (
        "/vendor/plex-mono-semibold.woff2",
        include_bytes!("app/vendor/plex-mono-semibold.woff2"),
    ),
];

pub fn router(live: Arc<Live>) -> Router {
    let guard = {
        let live = Arc::clone(&live);
        middleware::from_fn(move |request, next| {
            let live = Arc::clone(&live);
            async move { admit(&live, request, next).await }
        })
    };

    let router = Router::new()
        .route("/", get(page))
        .route(
            "/app.css",
            // The palette is prepended rather than written into the stylesheet, so
            // the page cannot hold a colour the terminal has stopped using.
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    format!("{}\n{STYLE}", super::theme::variables()),
                )
                    .into_response()
            }),
        )
        .route(
            "/app.js",
            get(|| async { asset("text/javascript", SCRIPT) }),
        )
        .route(
            "/vendor/preact.js",
            get(|| async { asset("text/javascript", PREACT) }),
        )
        .route("/api/report", get(report))
        .route("/api/rules", get(rules))
        .route("/api/plan", get(plan))
        .route("/api/perf", get(perf))
        .route("/api/levels", get(levels))
        .route("/api/trend", get(trend))
        .route("/api/graph", get(graph))
        .route("/api/config", get(config))
        .route("/api/suppressions", get(suppressions))
        .route("/api/file", get(file))
        .route("/api/fix", post(fix))
        .route("/api/suppress", post(suppress))
        .route("/api/disable", post(disable))
        .route("/api/baseline", post(baseline))
        .route("/api/check", post(check_diff))
        .route("/events", get(events));

    let router = FONTS.iter().fold(router, |router, (path, body)| {
        router.route(path, get(move || async move { font(body) }))
    });

    router.layer(guard).with_state(live)
}

/// A year, immutable: the file is embedded in the binary, so a new one only arrives
/// with a new binary, and its path changes when it does.
fn font(body: &'static [u8]) -> Response {
    (
        [
            (header::CONTENT_TYPE, "font/woff2"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        body,
    )
        .into_response()
}

/// The page is served with the process's nonce in it, so a write request can prove it
/// came from the page this process handed out rather than from another tab.
async fn page(State(live): State<Arc<Live>>) -> Response {
    let name = live.root.file_name().map_or_else(
        || "rqx".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let body = PAGE
        .replace("{{nonce}}", &live.nonce)
        .replace("{{owl}}", &super::theme::owl("idle"))
        .replace("{{project}}", &html_escape(&name));
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response()
}

fn asset(kind: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, kind)], body).into_response()
}

fn ok(value: serde_json::Value) -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        value.to_string(),
    )
        .into_response()
}

async fn report(State(live): State<Arc<Live>>) -> Response {
    let outcome = live.outcome();
    let settings = live.settings();
    let report = Report::everything(&outcome);
    let mut body = projection::scan(&report, &outcome);
    body["mascot"] = json!(crate::brand::mascot_ink(Some(report.health())).slug());
    body["findings"] = json!(
        outcome
            .findings
            .iter()
            .map(|finding| {
                let spec = finding.rule.spec();
                json!({
                    "rule": spec.slug,
                    "family": spec.category.slug(),
                    "file": finding.location.file,
                    "line": finding.location.line,
                    "column": finding.location.column,
                    "endLine": finding.location.end_line,
                    "start": finding.location.start,
                    "end": finding.location.end,
                    "severity": finding.severity.label(),
                    "confidence": finding.confidence.label(),
                    "evidence": finding.evidence.to_string(),
                    "why": spec.why,
                    "metric": spec.metric,
                    "hint": finding.fix_hint,
                    "technique": spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>(),
                    "autofixable": settings
                        .for_file(&outcome.root.join(&*finding.location.file))
                        .fix
                        .allows(finding.rule),
                    "known": finding.known,
                })
            })
            .collect::<Vec<_>>()
    );
    // The map sizes a file by how much of the project it is, which the scan knows and
    // the shared projection has no reason to carry.
    body["lines"] = json!(
        outcome
            .lines_by_file
            .iter()
            .map(|(file, lines)| (file.to_string(), *lines))
            .collect::<std::collections::BTreeMap<_, _>>()
    );
    body["project"] = json!(live.root.file_name().map(|name| name.to_string_lossy()));
    body["level"] = json!(settings.level().slug());
    body["suppressed"] = json!(outcome.suppressed.len());
    body["editorUrl"] = json!(live.editor_url);
    body["readOnly"] = json!(live.read_only);
    body["root"] = json!(live.root);
    ok(body)
}

async fn rules() -> Response {
    ok(projection::catalogue())
}

async fn plan(State(live): State<Arc<Live>>, RawQuery(query): RawQuery) -> Response {
    let outcome = live.outcome();
    let budget = param(query.as_deref(), "budget").and_then(|value| value.parse().ok());
    let limit = param(query.as_deref(), "limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);
    ok(projection::plan(&outcome, budget, limit))
}

async fn perf(State(live): State<Arc<Live>>, RawQuery(query): RawQuery) -> Response {
    let outcome = live.outcome();
    let limit = param(query.as_deref(), "limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(40);
    let threshold = live.settings().config.thresholds.loop_nesting;
    ok(projection::perf(&outcome, threshold, limit))
}

async fn config(State(live): State<Arc<Live>>) -> Response {
    ok(projection::config(&live.settings()))
}

async fn suppressions(State(live): State<Arc<Live>>) -> Response {
    ok(projection::waivers(&live.outcome()))
}

/// What the same project would score under each level. Answered from the findings
/// already in hand rather than by re-analysing four times: a level moves thresholds,
/// and a threshold decides whether a finding fires, so this is an estimate and says
/// so in the page.
async fn levels(State(live): State<Arc<Live>>) -> Response {
    use rqx_core::config::Level;
    let outcome = live.outcome();
    ok(json!(
        Level::ALL
            .iter()
            .map(|level| {
                let config: rqx_core::RqxConfig = toml::from_str(level.layer()).unwrap_or_default();
                let kept: Vec<&rqx_core::Finding> = outcome
                    .findings
                    .iter()
                    .filter(|finding| config.rules.enabled(finding.rule))
                    .collect();
                let counts = outcome.counts.without(
                    outcome
                        .findings
                        .iter()
                        .filter(|finding| !config.rules.enabled(finding.rule)),
                );
                json!({
                    "level": level.slug(),
                    "summary": level.summary(),
                    "findings": kept.len(),
                    "health": counts.weighted_health_score(outcome.code_lines, &outcome.impact),
                    "current": *level == live.settings().level(),
                })
            })
            .collect::<Vec<_>>()
    ))
}

async fn trend(State(live): State<Arc<Live>>) -> Response {
    ok(json!(*live.trend.read().expect("the trend lock")))
}

/// The import graph, with each file's grade, so the page can draw what depends on
/// what and colour it by how healthy it is.
async fn graph(State(live): State<Arc<Live>>) -> Response {
    let outcome = live.outcome();
    let report = Report::everything(&outcome);
    ok(json!({
        "nodes": report
            .files
            .iter()
            .map(|file| json!({
                "file": file,
                "grade": report.grade_of(file).label(),
                "findings": outcome.findings_by_file.get(file.as_str()).copied().unwrap_or(0),
                "lines": outcome.lines_by_file.get(file.as_str()).copied().unwrap_or(0),
                "importers": outcome.impact.get(file).importers,
            }))
            .collect::<Vec<_>>(),
        "edges": outcome.import_edges,
        "cycles": outcome.cycles,
    }))
}

/// One file's source, for the code view. Contained to the project root, so a page
/// cannot ask for anything the scan would not have read.
async fn file(State(live): State<Arc<Live>>, RawQuery(query): RawQuery) -> Response {
    let Some(name) = param(query.as_deref(), "path") else {
        return (StatusCode::BAD_REQUEST, "no path\n").into_response();
    };
    let Ok(path) = crate::mcp::root::within(&live.root, Some(&name)) else {
        return (StatusCode::FORBIDDEN, "outside the project\n").into_response();
    };
    match std::fs::read_to_string(&path) {
        Ok(source) => (
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            source,
        )
            .into_response(),
        Err(error) => (StatusCode::NOT_FOUND, format!("{error}\n")).into_response(),
    }
}

#[derive(Deserialize)]
struct FixRequest {
    file: String,
    #[serde(default)]
    write: bool,
}

/// A preview answers with the text before and after so the page can show the diff;
/// a write puts the file on disk and the poll thread notices it like any other save.
async fn fix(State(live): State<Arc<Live>>, body: Bytes) -> Response {
    let request = match parse::<FixRequest>(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if let Err(response) = writable(&live) {
        return response;
    }
    let settings = live.settings();
    let root = live.root.clone();
    let options = crate::fix::Options {
        write: request.write,
        // A write runs the rounds `rqx fix` runs, because deleting the last use of a
        // binding is what makes the binding unused and that is the next round's edit.
        max_rounds: if request.write { 3 } else { 1 },
        only: std::iter::once(request.file).collect(),
    };
    blocking(move || {
        crate::fix::run(&root, &settings, options)
            .map(|outcome| projection::fixes(&outcome, !request.write))
    })
    .await
}

#[derive(Deserialize)]
struct SuppressRequest {
    file: String,
    line: u32,
    rule: String,
    reason: String,
    until: Option<String>,
    #[serde(default)]
    whole_file: bool,
}

async fn suppress(State(live): State<Arc<Live>>, body: Bytes) -> Response {
    let request = match parse::<SuppressRequest>(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if let Err(response) = writable(&live) {
        return response;
    }
    let Ok(path) = crate::mcp::root::within(&live.root, Some(&request.file)) else {
        return (StatusCode::FORBIDDEN, "outside the project\n").into_response();
    };
    let Some(language) = rqx_core::Language::of_path(&path) else {
        return (StatusCode::BAD_REQUEST, "not a language rqx reads\n").into_response();
    };
    let Ok(source) = std::fs::read_to_string(&path) else {
        return (StatusCode::NOT_FOUND, "cannot read the file\n").into_response();
    };
    let Some(waiver) = rqx_core::suppress::waiver(
        &source,
        language,
        request.line,
        &request.rule,
        &request.reason,
        request.until.as_deref(),
        request.whole_file,
    ) else {
        return (
            StatusCode::BAD_REQUEST,
            "a waiver needs a reason and, if dated, a YYYY-MM-DD date\n",
        )
            .into_response();
    };
    let mut lines: Vec<&str> = source.split_inclusive('\n').collect();
    let at = (waiver.line as usize - 1).min(lines.len());
    lines.insert(at, &waiver.text);
    let written = lines.concat();
    match crate::fix::write_atomically(&path, &written) {
        Ok(()) => {
            ok(json!({ "suppressed": request.rule, "file": request.file, "at": waiver.line }))
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error:#}\n")).into_response(),
    }
}

#[derive(Deserialize)]
struct DisableRequest {
    name: String,
}

/// Writes `RQX.toml`; the poll thread watches it and re-analyses the project once the
/// file lands, the way the editor does through its own config watcher.
async fn disable(State(live): State<Arc<Live>>, body: Bytes) -> Response {
    let request = match parse::<DisableRequest>(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if let Err(response) = writable(&live) {
        return response;
    }
    let name = request.name;
    let known = rqx_core::rule::RuleId::ALL
        .iter()
        .any(|rule| rule.spec().slug == name)
        || rqx_core::rule::Category::ALL
            .iter()
            .any(|family| family.slug() == name);
    if !known {
        return (StatusCode::BAD_REQUEST, "not a rule or a family\n").into_response();
    }
    let path = live.root.join(rqx_core::config::CONFIG_FILE_NAME);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let Some(insertion) = crate::config::disable_entry(&existing, &name) else {
        return (
            StatusCode::CONFLICT,
            "already disabled, or RQX.toml has a shape rqx will not edit blind\n",
        )
            .into_response();
    };
    match crate::fix::write_atomically(&path, &crate::config::apply(&existing, &insertion)) {
        Ok(()) => ok(json!({ "disabled": name })),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error:#}\n")).into_response(),
    }
}

#[derive(Deserialize)]
struct BaselineRequest {
    #[serde(default)]
    update: bool,
}

async fn baseline(State(live): State<Arc<Live>>, body: Bytes) -> Response {
    let request = match parse::<BaselineRequest>(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if let Err(response) = writable(&live) {
        return response;
    }
    let settings = live.settings();
    let root = live.root.clone();
    blocking(move || {
        crate::baseline::take(&root, &settings, request.update, false).map(|taken| {
            json!({
                "path": taken.path,
                "recorded": taken.recorded,
                "findings": taken.findings,
                "replaced": taken.replaced,
                "configured": taken.configured,
            })
        })
    })
    .await
}

#[derive(Deserialize)]
struct CheckRequest {
    #[serde(default = "origin_head")]
    base: String,
}

fn origin_head() -> String {
    "origin/HEAD".to_owned()
}

/// A review reads the repository and scans; it changes nothing, but it is a POST
/// because it is an action a person asks for, not a view of state the page has.
async fn check_diff(State(live): State<Arc<Live>>, body: Bytes) -> Response {
    let request = match parse::<CheckRequest>(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let settings = live.settings();
    let root = live.root.clone();
    blocking(move || {
        let review = crate::check::review(&root, &settings, &request.base)?;
        let verdict = crate::check::judge(&review, &settings.config);
        let mut value = projection::review(&review, &verdict);
        value["findings"] = json!(
            review
                .findings()
                .map(|finding| projection::finding(finding, &settings, &root))
                .collect::<Vec<_>>()
        );
        Ok(value)
    })
    .await
}

/// Bodies are parsed by hand rather than through axum's `Json`, so the binary does
/// not grow a feature and a dependency for five small request shapes.
fn parse<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, Response> {
    serde_json::from_slice(body)
        .map_err(|error| (StatusCode::BAD_REQUEST, format!("{error}\n")).into_response())
}

fn writable(live: &Live) -> Result<(), Response> {
    if live.read_only {
        return Err((StatusCode::FORBIDDEN, "this dashboard is read-only\n").into_response());
    }
    Ok(())
}

/// Every action scans, and a scan on the runtime thread would stall every other
/// request — the SSE stream included — until it finished.
async fn blocking<F>(work: F) -> Response
where
    F: FnOnce() -> anyhow::Result<serde_json::Value> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(value)) => ok(value),
        Ok(Err(error)) => {
            (StatusCode::UNPROCESSABLE_ENTITY, format!("{error:#}\n")).into_response()
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}\n")).into_response(),
    }
}

async fn events(
    State(live): State<Arc<Live>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    use tokio_stream::StreamExt;
    let stream = BroadcastStream::new(live.events.subscribe())
        .filter_map(|message| message.ok().map(|body| Ok(Event::default().data(body))));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn param(query: Option<&str>, name: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let pair = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(pair, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The page is the only client, and it is served from this process. A request whose
/// `Host` is not the address we bound came through a name that resolves here from
/// somewhere else, which is the whole of the DNS-rebinding attack a local server is
/// exposed to.
async fn admit(live: &Live, request: axum::extract::Request, next: middleware::Next) -> Response {
    // Decided and finished with before the await: a borrow of the request's headers
    // held across it would make this middleware's future `!Send`.
    let refusal = {
        let text = |name: header::HeaderName| {
            request
                .headers()
                .get(&name)
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned)
        };
        refuse(
            text(header::HOST).as_deref().unwrap_or_default(),
            text(header::ORIGIN).as_deref(),
            request.method(),
            &live.nonce,
            text(header::HeaderName::from_static(NONCE_HEADER))
                .as_deref()
                .unwrap_or_default(),
        )
    };
    if let Some(refusal) = refusal {
        return (StatusCode::FORBIDDEN, format!("{refusal}\n")).into_response();
    }
    next.run(request).await
}

/// Why a request is not this page's, or `None` if it is. Split out from the
/// middleware because the policy is the part worth testing and `Next` cannot be
/// built without a router to run it through.
fn refuse(
    host: &str,
    origin: Option<&str>,
    method: &Method,
    nonce: &str,
    presented: &str,
) -> Option<&'static str> {
    let local = host.rsplit_once(':').map_or(host, |(name, _)| name);
    if !matches!(local, "127.0.0.1" | "localhost" | "[::1]" | "::1") {
        return Some("rqx serve answers the local host only");
    }
    if let Some(origin) = origin
        && origin != format!("http://{host}")
    {
        return Some("unexpected origin");
    }
    // A write has to carry the nonce the page was served with: a form on another
    // site can POST to a loopback port, but it cannot read our page to learn the
    // nonce, and a browser sends `Origin` on every cross-site POST anyway.
    if method != Method::GET && presented != nonce {
        return Some("this page is stale; reload it and try again");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONCE: &str = "0123456789abcdef";

    fn get(host: &str, origin: Option<&str>) -> Option<&'static str> {
        refuse(host, origin, &Method::GET, NONCE, "")
    }

    fn post(nonce: &str) -> Option<&'static str> {
        refuse(
            "127.0.0.1:7777",
            Some("http://127.0.0.1:7777"),
            &Method::POST,
            NONCE,
            nonce,
        )
    }

    #[test]
    fn a_read_from_the_local_page_is_admitted() {
        assert_eq!(get("127.0.0.1:7777", Some("http://127.0.0.1:7777")), None);
        assert_eq!(get("localhost:7777", None), None);
    }

    /// The whole of the DNS-rebinding attack a loopback server is exposed to is a
    /// name that resolves here from somewhere else.
    #[test]
    fn a_request_through_another_name_is_refused() {
        assert!(get("rqx.example.com", None).is_some());
        assert!(get("127.0.0.1:7777", Some("http://evil.example.com")).is_some());
    }

    /// The nonce is what separates this page from any other page in the same
    /// browser, and a write is the only thing that needs separating.
    #[test]
    fn a_write_without_the_nonce_is_refused() {
        assert!(post("").is_some());
        assert!(post("not-the-nonce").is_some());
        assert_eq!(post(NONCE), None);
    }

    /// A face the stylesheet asks for and the router does not serve falls back to the
    /// system stack silently, which is the drift a rename would cause.
    #[test]
    fn every_face_the_stylesheet_asks_for_is_served() {
        let asked: Vec<&str> = STYLE
            .split("url(\"")
            .skip(1)
            .filter_map(|rest| rest.split_once('"'))
            .map(|(url, _)| url)
            .collect();
        assert!(!asked.is_empty(), "the stylesheet loads no font at all");
        for url in &asked {
            assert!(
                FONTS.iter().any(|(path, _)| path == url),
                "{url} is not a route"
            );
        }
        for (path, body) in FONTS {
            assert!(asked.contains(&path), "{path} is served and never used");
            assert_eq!(&body[..4], b"wOF2", "{path} is not a woff2 file");
        }
    }

    /// The page is embedded in the binary and paints before it can fetch anything, so
    /// its weight is the binary's weight and a font is the easiest way to lose that.
    #[test]
    fn the_embedded_page_stays_small() {
        let total = PAGE.len()
            + STYLE.len()
            + SCRIPT.len()
            + PREACT.len()
            + FONTS.iter().map(|(_, body)| body.len()).sum::<usize>();
        assert!(total < 200 * 1024, "{total} bytes of embedded assets");
    }
}

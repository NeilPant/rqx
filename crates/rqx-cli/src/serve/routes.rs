use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Router, middleware};
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;

use super::Live;
use crate::report::{Report, projection};

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
        .route("/api/levels", get(levels))
        .route("/api/trend", get(trend))
        .route("/api/graph", get(graph))
        .route("/api/file", get(file))
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
                    "autofixable": live
                        .settings
                        .for_file(&outcome.root.join(&*finding.location.file))
                        .fix
                        .allows(finding.rule),
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
    body["level"] = json!(live.settings.level().slug());
    ok(body)
}

async fn rules() -> Response {
    ok(projection::catalogue())
}

async fn plan(State(live): State<Arc<Live>>, RawQuery(query): RawQuery) -> Response {
    let outcome = live.outcome();
    let budget = param(query.as_deref(), "budget").and_then(|value| value.parse().ok());
    let ranked = crate::plan::rank(&outcome, budget, 20);
    let (before, after) = crate::plan::simulated(&outcome, &ranked);
    ok(json!({
        "healthBefore": before,
        "healthAfter": after,
        "items": ranked
            .iter()
            .map(|item| {
                let spec = item.finding.rule.spec();
                json!({
                    "rule": spec.slug,
                    "family": spec.category.slug(),
                    "file": item.finding.location.file,
                    "line": item.finding.location.line,
                    "effort": item.effort,
                    "healthAfter": item.health_after,
                    "technique": spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>(),
                    "autofixable": spec.autofix.is_automatic(),
                })
            })
            .collect::<Vec<_>>(),
    }))
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
                    "current": *level == live.settings.level(),
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
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let local = host
        .rsplit_once(':')
        .map_or(host.as_str(), |(name, _)| name);
    if !matches!(local, "127.0.0.1" | "localhost" | "[::1]" | "::1") {
        return (
            StatusCode::FORBIDDEN,
            "rqx serve answers the local host only\n",
        )
            .into_response();
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let expected = format!("http://{host}");
        if origin.to_str().unwrap_or_default() != expected {
            return (StatusCode::FORBIDDEN, "unexpected origin\n").into_response();
        }
    }
    let _ = live;
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

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

//! The dashboard as one file. A manager gets a link to a page, not an invitation to
//! run a server, and a CI job can attach the page to a build.
//!
//! The page is the same Preact application `rqx serve` hands out. Everything it would
//! have fetched is written into the document instead, and `fetch` is replaced with a
//! shim that answers from those. Anything the page asks for that a scan cannot answer
//! — applying a fix, reloading — is refused rather than faked.

use std::io::Write;

use anyhow::Result;

use super::{Report, projection};
use crate::config::Resolved;

const PAGE: &str = include_str!("../serve/app/index.html");
const STYLE: &str = include_str!("../serve/app/app.css");
const SCRIPT: &str = include_str!("../serve/app/app.js");
const PREACT: &str = include_str!("../serve/app/vendor/preact.js");

const FONTS: [(&str, &[u8]); 3] = [
    (
        "/vendor/plex-sans.woff2",
        include_bytes!("../serve/app/vendor/plex-sans.woff2"),
    ),
    (
        "/vendor/plex-mono.woff2",
        include_bytes!("../serve/app/vendor/plex-mono.woff2"),
    ),
    (
        "/vendor/plex-mono-semibold.woff2",
        include_bytes!("../serve/app/vendor/plex-mono-semibold.woff2"),
    ),
];

pub fn render(report: &Report<'_>, settings: &Resolved, out: &mut dyn Write) -> Result<()> {
    let outcome = report.outcome;
    let project = outcome.root.file_name().map_or_else(
        || "rqx".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );

    let mut payloads = serde_json::Map::new();
    let mut dashboard = projection::dashboard(report, outcome, settings, &project);
    // No server means no editor to open and nothing to write back: the page hides
    // both rather than offering a button that cannot work.
    dashboard["editorUrl"] = serde_json::Value::Null;
    dashboard["readOnly"] = serde_json::Value::Bool(true);
    payloads.insert("/api/report".to_owned(), dashboard);
    payloads.insert("/api/rules".to_owned(), projection::catalogue());
    payloads.insert("/api/plan".to_owned(), projection::plan(outcome, None, 20));
    payloads.insert(
        "/api/perf".to_owned(),
        projection::perf(outcome, settings.config.thresholds.loop_nesting, 40),
    );
    payloads.insert("/api/config".to_owned(), projection::config(settings));
    payloads.insert("/api/suppressions".to_owned(), projection::waivers(outcome));
    payloads.insert(
        "/api/levels".to_owned(),
        projection::levels(outcome, settings.level()),
    );
    payloads.insert("/api/graph".to_owned(), projection::graph(report, outcome));
    // A trend is a record of runs, and a file is one run: the page draws nothing
    // rather than pretending a single point is a line.
    payloads.insert("/api/trend".to_owned(), serde_json::json!([]));

    // The fonts are the only assets the stylesheet reaches for by URL, so they are
    // written in as data rather than left to a server that will not be there.
    let mut style = format!("{}\n{STYLE}", crate::serve::theme::variables());
    for (path, body) in FONTS {
        style = style.replace(path, &format!("data:font/woff2;base64,{}", base64(body)));
    }

    let body = PAGE
        .replace("{{nonce}}", "static")
        .replace("{{owl}}", &crate::serve::theme::owl("idle"))
        .replace("{{project}}", &escape(&project))
        .replace(
            "<link rel=\"stylesheet\" href=\"/app.css\" />",
            &format!("<style>{style}</style>"),
        )
        .replace(
            "<script src=\"/vendor/preact.js\"></script>",
            &format!(
                "<script type=\"application/json\" id=\"rqx-payloads\">{}</script>\n\
                 <script>{}</script>\n<script>{PREACT}</script>",
                serde_json::to_string(&payloads)?,
                SHIM
            ),
        )
        .replace(
            "<script src=\"/app.js\"></script>",
            &format!("<script>{SCRIPT}</script>"),
        );
    out.write_all(body.as_bytes())?;
    Ok(())
}

/// Answers the page's own requests from the document. A path with no payload is a
/// 404 rather than an empty object, so the page shows what it shows when a server
/// cannot answer rather than an empty table it presents as real.
const SHIM: &str = r#"
(function () {
  window.__rqxStatic = true;
  var held = JSON.parse(document.getElementById('rqx-payloads').textContent);
  window.fetch = function (input) {
    var url = String(input && input.url ? input.url : input);
    var path = url.split('?')[0].replace(/^https?:\/\/[^/]+/, '');
    var body = held[path];
    if (body === undefined) {
      return Promise.resolve(new Response('{}', { status: 404 }));
    }
    return Promise.resolve(
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
  };
  window.EventSource = function () {
    return { addEventListener: function () {}, close: function () {} };
  };
})();
"#;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Written out rather than pulled from a crate: three fonts a run is the whole use
/// this binary has for base64.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let block = chunk.iter().enumerate().fold(0_u32, |held, (at, byte)| {
            held | (u32::from(*byte) << (16 - 8 * at))
        });
        for at in 0..=chunk.len() {
            out.push(char::from(
                ALPHABET[((block >> (18 - 6 * at)) & 0x3f) as usize],
            ));
        }
        for _ in chunk.len()..3 {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alphabet and the padding are the whole of what a base64 encoder can get
    /// wrong, and both are decided by the length of the last chunk.
    #[test]
    fn every_tail_length_is_padded_the_way_the_standard_says() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}

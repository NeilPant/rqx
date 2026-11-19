//! Stdio between the client and the MCP service, with the lines that are not JSON-RPC
//! taken out. The service ends the session on a line it cannot decode, and on the
//! first line it ended the process; such a line is answered with a parse error here
//! instead, and the session ends only when stdin does.

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader, DuplexStream};
use tokio::sync::mpsc;

/// JSON-RPC's code for a message that is not JSON.
const PARSE_ERROR: i64 = -32700;
/// JSON-RPC's code for JSON that is not a request, response or notification.
const INVALID_REQUEST: i64 = -32600;

/// Pipes deep enough for any one message rqx sends or receives.
const PIPE_BYTES: usize = 1 << 20;

/// The service's end of the filtered stdio, and the task that must be awaited after
/// the service stops so everything it wrote reaches stdout. Dropping the service's end
/// is what tells the rest that the service is done.
pub struct Filtered {
    pub service: DuplexStream,
    pub flushed: tokio::task::JoinHandle<()>,
}

pub fn stdio() -> Filtered {
    let (service, ours) = tokio::io::duplex(PIPE_BYTES);
    let (from_service, to_service) = tokio::io::split(ours);
    let (lines, mut out) = mpsc::unbounded_channel::<String>();

    let refusals = lines.clone();
    tokio::spawn(async move {
        let mut to_service = to_service;
        let mut input = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = input.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            match refusal_for(&line) {
                Some(refusal) => {
                    let _ = refusals.send(refusal);
                }
                None => {
                    if forward(&mut to_service, &line).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = to_service.shutdown().await;
    });
    tokio::spawn(async move {
        let mut replies = BufReader::new(from_service).lines();
        while let Ok(Some(line)) = replies.next_line().await {
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let flushed = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(line) = out.recv().await {
            if forward(&mut stdout, &line).await.is_err() {
                break;
            }
        }
    });
    Filtered { service, flushed }
}

async fn forward(out: &mut (impl AsyncWrite + Unpin), line: &str) -> std::io::Result<()> {
    out.write_all(line.as_bytes()).await?;
    out.write_all(b"\n").await?;
    out.flush().await
}

/// The error that answers `line`, or `None` when it is a JSON-RPC message the service
/// can read.
pub fn refusal_for(line: &str) -> Option<String> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Some(refusal(PARSE_ERROR, "the line is not JSON", Value::Null));
    };
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    let readable = value.is_object()
        && value.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
        && (value.get("method").is_some_and(Value::is_string)
            || value.get("result").is_some()
            || value.get("error").is_some());
    (!readable).then(|| {
        refusal(
            INVALID_REQUEST,
            "not a JSON-RPC 2.0 request, response or notification",
            id,
        )
    })
}

fn refusal(code: i64, message: &str, id: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_that_is_not_json_rpc_is_answered_and_a_message_is_let_through() {
        let parse: Value =
            serde_json::from_str(&refusal_for("not json").expect("refused")).expect("json");
        assert_eq!(parse["error"]["code"], PARSE_ERROR);
        assert_eq!(parse["id"], Value::Null);

        let invalid: Value =
            serde_json::from_str(&refusal_for(r#"{"id":3,"hello":1}"#).expect("refused"))
                .expect("json");
        assert_eq!(invalid["error"]["code"], INVALID_REQUEST);
        assert_eq!(invalid["id"], 3);

        assert!(refusal_for(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#).is_none());
        assert!(refusal_for(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    }
}

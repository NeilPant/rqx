//! The language server's stdio framing. `lsp_server::Connection::stdio` ends the
//! connection on the first frame it cannot read, so one malformed message from a buggy
//! client took the server down. This reader answers such a frame with a JSON-RPC
//! error and reads on, and the connection ends only when stdin does.

use std::io::{self, BufRead, Write};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, bounded, select, unbounded};
use lsp_server::{Connection, Message};
use serde_json::{Value, json};

/// JSON-RPC's codes for a body that is not JSON, and for JSON that is not a request,
/// a response or a notification.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;

/// The reader and writer threads behind a [`Connection`] over stdin and stdout.
pub struct Threads {
    reader: JoinHandle<()>,
    writer: JoinHandle<io::Result<()>>,
}

impl Threads {
    /// Waits for both threads. The writer stops once every clone of the connection's
    /// sender has been dropped.
    pub fn join(self) -> io::Result<()> {
        let _ = self.reader.join();
        self.writer
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("the writer thread panicked")))
    }
}

pub fn stdio() -> (Connection, Threads) {
    let (outgoing, to_write) = bounded::<Message>(0);
    let (refusals, refusals_to_write) = unbounded::<String>();
    let (incoming, received) = bounded::<Message>(0);
    let writer = thread::Builder::new()
        .name("LspServerWriter".to_owned())
        .spawn(move || write_all(&to_write, &refusals_to_write, &mut io::stdout().lock()))
        .expect("the writer thread starts");
    let reader = thread::Builder::new()
        .name("LspServerReader".to_owned())
        .spawn(move || read_all(&mut io::stdin().lock(), &incoming, &refusals))
        .expect("the reader thread starts");
    (
        Connection {
            sender: outgoing,
            receiver: received,
        },
        Threads { reader, writer },
    )
}

/// Writes messages and refusals as they come, until no one can send either.
fn write_all(
    messages: &Receiver<Message>,
    refusals: &Receiver<String>,
    out: &mut impl Write,
) -> io::Result<()> {
    let mut messages_open = true;
    let mut refusals_open = true;
    while messages_open || refusals_open {
        select! {
            recv(messages) -> message => match message {
                Ok(message) => message.write(out)?,
                Err(_) => messages_open = false,
            },
            recv(refusals) -> refusal => match refusal {
                Ok(body) => frame(out, &body)?,
                Err(_) => refusals_open = false,
            },
        }
        if !messages_open && refusals.is_empty() {
            break;
        }
    }
    Ok(())
}

fn frame(out: &mut impl Write, body: &str) -> io::Result<()> {
    write!(out, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    out.flush()
}

/// Reads frames until stdin ends or the client says `exit`, sending each message on
/// and answering each frame that is not one.
fn read_all(input: &mut impl BufRead, messages: &Sender<Message>, refusals: &Sender<String>) {
    loop {
        let message = match read_frame(input) {
            Ok(None) | Err(_) => return,
            Ok(Some(Ok(message))) => message,
            Ok(Some(Err(refusal))) => {
                let _ = refusals.send(refusal);
                continue;
            }
        };
        let exit = matches!(&message, Message::Notification(notification) if notification.method == "exit");
        if messages.send(message).is_err() || exit {
            return;
        }
    }
}

/// One frame: a message, or the JSON-RPC error that answers a frame that is not one.
/// `None` at the end of the input.
fn read_frame(input: &mut impl BufRead) -> io::Result<Option<Result<Message, String>>> {
    let mut length: Option<Result<usize, String>> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = Some(
                value
                    .trim()
                    .parse()
                    .map_err(|_| format!("Content-Length `{}` is not a length", value.trim())),
            );
        }
    }
    let length = match length {
        Some(Ok(length)) => length,
        Some(Err(why)) => return Ok(Some(Err(refusal(PARSE_ERROR, &why, Value::Null)))),
        None => return Ok(None),
    };
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    Ok(Some(message_of(&body)))
}

fn message_of(body: &[u8]) -> Result<Message, String> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Err(refusal(PARSE_ERROR, "the body is not JSON", Value::Null));
    };
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    serde_json::from_value::<Message>(value).map_err(|error| {
        refusal(
            INVALID_REQUEST,
            &format!("not a request, response or notification: {error}"),
            id,
        )
    })
}

fn refusal(code: i64, message: &str, id: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(input: &str) -> Vec<Result<Message, String>> {
        let mut input = io::Cursor::new(input.as_bytes().to_vec());
        let mut read = Vec::new();
        while let Some(frame) = read_frame(&mut input).expect("reads") {
            read.push(frame);
        }
        read
    }

    fn framed(body: &str) -> String {
        format!("Content-Length: {}\r\n\r\n{body}", body.len())
    }

    #[test]
    fn a_malformed_frame_is_answered_and_the_next_one_is_read() {
        let good = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
        let input = format!(
            "Content-Length: abc\r\n\r\n{}{}{}",
            framed("{{{{{"),
            framed(r#"{"jsonrpc":"2.0","id":7,"nonsense":true}"#),
            framed(good)
        );
        let read = frames(&input);
        let refusals: Vec<Value> = read
            .iter()
            .filter_map(|frame| frame.as_ref().err())
            .map(|body| serde_json::from_str(body).expect("json"))
            .collect();
        assert_eq!(refusals.len(), 3, "{refusals:?}");
        assert_eq!(refusals[0]["error"]["code"], PARSE_ERROR);
        assert_eq!(refusals[1]["error"]["code"], PARSE_ERROR);
        assert_eq!(refusals[2]["error"]["code"], INVALID_REQUEST);
        assert_eq!(refusals[2]["id"], 7);
        assert!(matches!(read.last(), Some(Ok(Message::Notification(_)))));
    }

    #[test]
    fn the_end_of_the_input_ends_the_reading() {
        assert!(frames("").is_empty());
    }
}

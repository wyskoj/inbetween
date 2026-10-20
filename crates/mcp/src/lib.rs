//! An MCP server over the engine's command registry.
//!
//! Every command is one tool; one [`Session`] lives for the whole connection. The transport
//! is newline-delimited JSON-RPC 2.0 on a pair of streams (stdio for `ib mcp`). The server
//! never panics on input: bad messages get a JSON-RPC error, failing commands get a tool
//! result with `isError`, and over-long lines are skipped.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod protocol;
pub mod schema;
pub mod tools;

use inbetween_engine::{Registry, Session};
use protocol::{INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

/// Longest accepted input line, in bytes.
pub const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;

pub struct Server {
    registry: Registry,
    session: Session,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    pub fn new() -> Self {
        Self::with_session(Session::new())
    }

    pub fn with_session(session: Session) -> Self {
        Self {
            registry: Registry::standard(),
            session,
        }
    }

    /// Run a command directly (the CLI uses this to open `--project` before serving).
    pub fn run_command(&mut self, id: &str, params: Value) -> Result<Value, String> {
        self.registry
            .run_caught(&mut self.session, id, &params)
            .map_err(|e| e.to_string())
    }

    /// Handle one message. Returns the response line, or `None` for a notification.
    pub fn handle(&mut self, line: &str) -> Option<String> {
        let message: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                return Some(protocol::error(
                    &Value::Null,
                    PARSE_ERROR,
                    &format!("not valid JSON: {e}"),
                ))
            }
        };
        let Some(object) = message.as_object() else {
            return Some(protocol::error(
                &Value::Null,
                INVALID_REQUEST,
                "expected one JSON object per line (batches are not supported)",
            ));
        };
        let id = object.get("id");
        if let Some(id) = id {
            if !(id.is_string() || id.is_number()) {
                return Some(protocol::error(
                    &Value::Null,
                    INVALID_REQUEST,
                    "`id` must be a string or a number",
                ));
            }
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            // A response to something we never asked, or garbage.
            if object.contains_key("result") || object.contains_key("error") {
                return None;
            }
            return id.map(|id| protocol::error(id, INVALID_REQUEST, "missing `method`"));
        };
        // Notifications get no answer, whatever they say.
        let id = id?;
        let params = object.get("params").unwrap_or(&Value::Null);
        Some(match self.dispatch(method, params) {
            Ok(result) => protocol::result(id, result),
            Err((code, message)) => protocol::error(id, code, &message),
        })
    }

    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => {
                let requested = params.get("protocolVersion").and_then(Value::as_str);
                Ok(protocol::initialize_result(protocol::negotiate(requested)))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(tools::list(&self.registry)),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).ok_or((
                    INVALID_PARAMS,
                    "`name` (the tool to call) is required".to_string(),
                ))?;
                let spec = tools::find(&self.registry, name)
                    .ok_or_else(|| (INVALID_PARAMS, format!("unknown tool `{name}`")))?;
                let arguments = match params.get("arguments") {
                    None | Some(Value::Null) => Default::default(),
                    Some(Value::Object(o)) => o.clone(),
                    Some(_) => {
                        return Err((INVALID_PARAMS, "`arguments` must be an object".into()))
                    }
                };
                Ok(tools::call(
                    &self.registry,
                    &mut self.session,
                    spec,
                    arguments,
                ))
            }
            other => Err((METHOD_NOT_FOUND, format!("unknown method `{other}`"))),
        }
    }

    /// Serve until `input` ends. Each response is flushed as soon as it is written.
    pub fn serve(&mut self, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
        loop {
            let reply = match read_line(&mut input)? {
                Line::End => return Ok(()),
                Line::TooLong => Some(protocol::error(
                    &Value::Null,
                    INVALID_REQUEST,
                    "message is too long and was skipped",
                )),
                Line::Bytes(bytes) => match std::str::from_utf8(&bytes) {
                    Ok(text) if text.trim().is_empty() => None,
                    Ok(text) => self.handle(text),
                    Err(_) => Some(protocol::error(
                        &Value::Null,
                        PARSE_ERROR,
                        "input is not valid UTF-8",
                    )),
                },
            };
            if let Some(reply) = reply {
                output.write_all(reply.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
    }
}

enum Line {
    End,
    TooLong,
    Bytes(Vec<u8>),
}

/// Read up to the next newline, keeping at most `limit` bytes; the rest of a longer line is
/// read and dropped.
fn read_line_limited(input: &mut impl BufRead, limit: usize) -> io::Result<Line> {
    let mut line = Vec::new();
    let mut too_long = false;
    let mut any = false;
    loop {
        let buf = match input.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if buf.is_empty() {
            break;
        }
        any = true;
        let (chunk, found) = match buf.iter().position(|b| *b == b'\n') {
            Some(i) => (buf.get(..i).unwrap_or(buf), true),
            None => (buf, false),
        };
        if !too_long {
            if line.len().saturating_add(chunk.len()) > limit {
                too_long = true;
                line = Vec::new();
            } else {
                line.extend_from_slice(chunk);
            }
        }
        let used = chunk.len() + usize::from(found);
        input.consume(used);
        if found {
            break;
        }
    }
    if !any {
        return Ok(Line::End);
    }
    if too_long {
        return Ok(Line::TooLong);
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Line::Bytes(line))
}

fn read_line(input: &mut impl BufRead) -> io::Result<Line> {
    read_line_limited(input, MAX_LINE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(line: io::Result<Line>) -> Option<Vec<u8>> {
        match line {
            Ok(Line::Bytes(b)) => Some(b),
            _ => None,
        }
    }

    #[test]
    fn long_lines_are_skipped_and_the_next_one_is_read() {
        let mut input = io::Cursor::new(b"0123456789\nok\r\n".to_vec());
        assert!(matches!(
            read_line_limited(&mut input, 4),
            Ok(Line::TooLong)
        ));
        assert_eq!(
            bytes(read_line_limited(&mut input, 4)),
            Some(b"ok".to_vec())
        );
        assert!(matches!(read_line_limited(&mut input, 4), Ok(Line::End)));
    }

    #[test]
    fn a_last_line_without_a_newline_is_still_read() {
        let mut input = io::Cursor::new(b"abc".to_vec());
        assert_eq!(
            bytes(read_line_limited(&mut input, 10)),
            Some(b"abc".to_vec())
        );
        assert!(matches!(read_line_limited(&mut input, 10), Ok(Line::End)));
    }
}

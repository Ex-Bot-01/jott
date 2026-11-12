//! One JSON object per line, in and out.
//!
//! np's stdout is the protocol: every sentence it has goes to stderr, so a line here is
//! either a response, an error object, or a progress notification — and a line that is
//! none of those means the contract broke, not that we should parse harder.

use super::ProgressNote;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Response { id: i64, result: Value },
    /// `id` is absent for a parse error: there may be no id to echo back, and inventing
    /// one would leak into the client's request table as a request that never happened.
    Fault { id: Option<i64>, fault: super::NpFault },
    Note(ProgressNote),
    /// A blank line, which np ignores and so do we.
    Ignored,
}

impl Frame {
    pub fn decode(line: &str) -> Frame {
        if line.trim().is_empty() {
            return Frame::Ignored;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Frame::Fault {
                id: None,
                fault: super::NpFault { code: -32700, message: format!("not JSON: {}", trim(line)) },
            };
        };
        let id = value.get("id").and_then(Value::as_i64);
        if value.get("method").and_then(Value::as_str) == Some("np/progress") {
            if let Some(params) = value.get("params") {
                return Frame::Note(ProgressNote::from_value(params));
            }
        }
        if let Some(err) = value.get("error") {
            return Frame::Fault {
                id,
                fault: super::NpFault {
                    code: err.get("code").and_then(Value::as_i64).unwrap_or(-32603) as i32,
                    message: err.get("message").and_then(Value::as_str).unwrap_or_default().to_string(),
                },
            };
        }
        match id {
            Some(id) => Frame::Response { id, result: value.get("result").cloned().unwrap_or(Value::Null) },
            // A notification we do not recognise: np answers nothing else without an id.
            None => Frame::Ignored,
        }
    }
}

/// One request line. `serde_json` escapes newlines inside strings, so a snippet or a
/// message can never split a frame — which is why no length or quoting is needed here.
pub fn encode_request(id: i64, method: &str, params: Option<&Value>) -> String {
    let mut msg = serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method});
    if let Some(p) = params {
        msg["params"] = p.clone();
    }
    let mut line = serde_json::to_string(&msg).expect("a json! literal serializes");
    line.push('\n');
    line
}

fn trim(s: &str) -> String {
    let s = s.trim();
    s.chars().take(120).collect()
}

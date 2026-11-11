//! Everything that crosses the pipe to `np serve`.

pub mod client;
pub mod frame;
pub mod handshake;
pub mod resolve;

pub use handshake::{check_version, NpFeatures, NpInfo};
pub use resolve::{install_hint, resolve_np, NpMissing, NpSource, Os};

use thiserror::Error;

/// Anything that stopped a piece of np work from being asked for at all. A refusal
/// *from* np is a [`NpFault`](self::client::NpFault) once the client exists.
#[derive(Debug, Error)]
pub enum NpError {
    #[error(transparent)]
    Missing(#[from] NpMissing),
    #[error("could not talk to np: {0}")]
    Transport(String),
    #[error(transparent)]
    Fault(NpFault),
}

/// A JSON-RPC error object from np. The message is np's own sentence, kept verbatim:
/// it already names what to do, and rewriting it into "something went wrong" loses the
/// only helpful part (spec §4).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code}: {message}")]
pub struct NpFault {
    pub code: i32,
    pub message: String,
}

/// One `np/progress` notification. `request` is present only when the request had an id:
/// "belongs to no request" and "this request had a null id" are different facts (spec §18.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressNote {
    pub command: String,
    pub phase: String,
    pub request: Option<i64>,
    pub since_ms: u64,
    /// Whatever else this phase carries — `bytes_done`/`bytes_total` while fetching,
    /// `chunks_done`/`chunks_total` while embedding, `code` when refused.
    pub extra: serde_json::Value,
}

impl ProgressNote {
    pub(crate) fn from_value(params: &serde_json::Value) -> ProgressNote {
        let mut extra = params.clone();
        if let Some(map) = extra.as_object_mut() {
            for key in ["command", "phase", "request", "since_ms"] {
                map.remove(key);
            }
        }
        ProgressNote {
            command: params.get("command").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            phase: params.get("phase").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            request: params.get("request").and_then(|v| v.as_i64()),
            since_ms: params.get("since_ms").and_then(|v| v.as_u64()).unwrap_or(0),
            extra,
        }
    }
}

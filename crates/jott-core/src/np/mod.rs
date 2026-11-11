//! Everything that crosses the pipe to `np serve`.

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

//! SHA-256 over raw bytes — one definition of "the hash of a file", shared by the stamps, the
//! reload check and the workspace key.
//!
//! np hashes notes the same way (`np-core/src/hash.rs`: SHA-256 of the bytes as they sit on disk,
//! §18.8), so a stamp written here means the same thing as a row in np's index. That is the whole
//! reason CRLF normalisation is forbidden in Task 8: rewriting line endings would change every
//! hash in the vault and make the index believe the corpus had been rewritten overnight.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// The lowercase-hex digest of some bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// The digest of a file's bytes, streamed so that a large note costs a read and not a copy.
///
/// A file that has changed between the tree's stat and this read hashes whatever it holds now;
/// Task 8's reload policy is what turns "the bytes moved under us" into a decision rather than a
/// stale stamp.
pub fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(to_hex(&hasher.finalize()))
}

/// The first `chars` hex characters of a digest — the shape np uses to name a workspace's files
/// (§18.6 names 16), which is why this lives beside the hash rather than in the code that keys state.
pub fn sha256_prefix(bytes: &[u8], chars: usize) -> String {
    let hex = sha256_hex(bytes);
    hex.chars().take(chars).collect()
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

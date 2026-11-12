//! Reading `initialize`'s answer, and deciding whether this np can do what the UI promises.
//!
//! Feature detection is the whole point: Jott ships ahead of, and behind, np releases. A
//! build without `models.list`, or without `progress`, gets a smaller status line — never a
//! method call that faults on every request (spec §18.12).

use super::resolve::{NpMissing, MIN_NP_VERSION};
use serde::Deserialize;

/// The oldest np whose answers this code is written against.
pub const MIN_NP: (u64, u64, u64) = MIN_NP_VERSION;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NpFeatures {
    pub local_model: bool,
    pub fetch: bool,
    pub vision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpInfo {
    pub version: String,
    pub protocol: u8,
    pub methods: Vec<String>,
    pub features: NpFeatures,
    /// Where np keeps its databases and model artifacts — the path the consent prompt names.
    pub data_dir: String,
    /// Whether this connection was accepted with progress notifications on.
    pub progress: bool,
}

#[derive(Deserialize)]
struct RawInitialize {
    #[serde(default)]
    np: String,
    #[serde(default)]
    protocol: Option<u8>,
    #[serde(default)]
    methods: Option<Vec<String>>,
    #[serde(default)]
    data_dir: Option<String>,
    #[serde(default)]
    progress: Option<bool>,
    #[serde(default)]
    features: RawFeatures,
}

#[derive(Default, Deserialize)]
struct RawFeatures {
    #[serde(default, rename = "local_model")]
    local_model: Option<bool>,
    #[serde(default)]
    fetch: Option<bool>,
    #[serde(default)]
    vision: Option<bool>,
}

/// Compare `major.minor.patch` numerically. A string compare would rank `0.10.0` below
/// `0.3.1`, and np's next minor version is exactly the case that would break.
pub fn check_version(version: &str) -> Result<(u64, u64, u64), NpMissing> {
    let mut parts = version.split('.').map(str::trim);
    let mut nums = [0u64; 3];
    for slot in nums.iter_mut() {
        let Some(raw) = parts.next() else { break };
        // Tolerate a suffix (`0.4.0-rc.1`) by reading the leading digits and nothing else.
        let digits: String = raw.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return Err(NpMissing::NoProtocol);
        }
        *slot = digits.parse().map_err(|_| NpMissing::NoProtocol)?;
    }
    let min = [MIN_NP.0, MIN_NP.1, MIN_NP.2];
    if nums >= min {
        Ok((nums[0], nums[1], nums[2]))
    } else {
        Err(NpMissing::TooOld { found: version.to_string() })
    }
}

/// Turn np's `initialize` result into [`NpInfo`]. Anything that leaves us unable to tell
/// what this build answers is `NoProtocol`, because every later request would be a guess.
pub fn parse_initialize(value: &serde_json::Value) -> Result<NpInfo, NpMissing> {
    let raw: RawInitialize =
        serde_json::from_value(value.clone()).map_err(|_| NpMissing::NoProtocol)?;
    check_version(&raw.np)?;
    if raw.protocol != Some(1) {
        return Err(NpMissing::NoProtocol);
    }
    let methods = raw.methods.filter(|m| !m.is_empty()).ok_or(NpMissing::NoProtocol)?;
    Ok(NpInfo {
        version: raw.np,
        protocol: 1,
        methods,
        features: NpFeatures {
            local_model: raw.features.local_model.unwrap_or(false),
            fetch: raw.features.fetch.unwrap_or(false),
            vision: raw.features.vision.unwrap_or(false),
        },
        data_dir: raw.data_dir.unwrap_or_default(),
        progress: raw.progress.unwrap_or(false),
    })
}

impl NpInfo {
    pub fn has_method(&self, name: &str) -> bool {
        self.methods.iter().any(|m| m == name)
    }
}

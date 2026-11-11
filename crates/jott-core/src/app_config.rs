//! The shell's `tauri.conf.json`, read as data.
//!
//! The window itself cannot be built on a headless box, and an identity typo
//! (`productName`, `identifier`) produces an installed app with the wrong name or a
//! data folder nobody expected. So the file that decides those is parsed and checked
//! here — where every platform's CI runs it — instead of being trusted to a GUI build.

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub product_name: String,
    pub identifier: String,
    pub frontend_dist: String,
    /// Absent when the shell is pointed at bundled files only.
    pub dev_url: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    /// The file is not JSON at all.
    #[error("tauri.conf.json is not valid JSON: {0}")]
    InvalidJson(String),
    /// One or more keys Jott depends on are missing. All of them are named at once,
    /// so a hand-edited config is fixed in one pass rather than one error per run.
    #[error("tauri.conf.json is missing required key(s): {}", .0.join(", "))]
    MissingKeys(Vec<String>),
}

#[derive(Deserialize)]
struct Raw {
    #[serde(rename = "productName")]
    product_name: Option<String>,
    identifier: Option<String>,
    build: Option<RawBuild>,
}

#[derive(Deserialize)]
struct RawBuild {
    #[serde(rename = "frontendDist")]
    frontend_dist: Option<String>,
    #[serde(rename = "devUrl")]
    dev_url: Option<String>,
}

pub fn parse_app_config(json: &str) -> Result<AppConfig, ConfigError> {
    let raw: Raw = serde_json::from_str(json).map_err(|e| ConfigError::InvalidJson(e.to_string()))?;

    let mut missing = Vec::new();
    let want = |key: &str, have: &Option<String>, missing: &mut Vec<String>| {
        if have.as_deref().unwrap_or("").is_empty() {
            missing.push(key.to_string());
        }
    };
    want("productName", &raw.product_name, &mut missing);
    want("identifier", &raw.identifier, &mut missing);
    let build = raw.build.unwrap_or(RawBuild { frontend_dist: None, dev_url: None });
    want("build.frontendDist", &build.frontend_dist, &mut missing);
    if !missing.is_empty() {
        return Err(ConfigError::MissingKeys(missing));
    }

    Ok(AppConfig {
        product_name: raw.product_name.expect("checked above"),
        identifier: raw.identifier.expect("checked above"),
        frontend_dist: build.frontend_dist.expect("checked above"),
        dev_url: build.dev_url.filter(|u| !u.is_empty()),
    })
}

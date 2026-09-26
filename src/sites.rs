//! Site catalog: where to look and how to tell a hit from a miss.

use std::collections::BTreeMap;

use serde::Deserialize;

/// How to decide whether a username exists on a site.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    /// Claimed if the profile URL returns HTTP 2xx.
    StatusCode,
    /// Claimed if `error_msg` is NOT present in the response body.
    Message,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Site {
    /// URL template; `{}` is replaced by the username.
    pub url: String,
    #[serde(rename = "errorType")]
    pub error_type: ErrorType,
    /// Body marker that means "not found" (only for `Message`).
    #[serde(rename = "errorMsg", default)]
    pub error_msg: Option<String>,
}

/// Load the site catalog from a JSON file (name -> Site).
pub fn load(path: &str) -> Result<BTreeMap<String, Site>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("{path}: {e}"))
}

//! Site catalog: where to look and how to tell a hit from a miss.

use std::collections::BTreeMap;

use serde::Deserialize;

/// How to decide whether a username exists on a site.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    /// Claimed if the profile URL returns HTTP 2xx.
    StatusCode,
    /// Claimed if `error_msg` (substring or regex) is NOT present in the body.
    Message,
    /// Claimed if the final URL (after redirects) still matches the profile URL,
    /// i.e. the site did not bounce us to a login / not-found page.
    ResponseUrl,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Site {
    /// URL template; `{}` is replaced by the username.
    pub url: String,
    #[serde(rename = "errorType")]
    pub error_type: ErrorType,
    /// Body marker that means "not found" (Message type). Treated as regex.
    #[serde(rename = "errorMsg", default)]
    pub error_msg: Option<String>,
    /// Optional category for filtering/grouping (social, dev, gaming, ...).
    #[serde(default)]
    pub category: Option<String>,
    /// Optional regex the username must satisfy for this site (e.g. length).
    #[serde(rename = "regexCheck", default)]
    pub regex_check: Option<String>,
}

/// Load the catalog from a JSON file (name -> Site).
pub fn load_from_file(path: &str) -> Result<BTreeMap<String, Site>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    parse(&raw)
}

/// Parse a catalog from a JSON string.
pub fn parse(raw: &str) -> Result<BTreeMap<String, Site>, String> {
    serde_json::from_str(raw).map_err(|e| e.to_string())
}

/// Built-in catalog, embedded at compile time so the tool runs with no files.
pub fn builtin() -> BTreeMap<String, Site> {
    parse(include_str!("../sites.json")).expect("embedded sites.json is invalid")
}

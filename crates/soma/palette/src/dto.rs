//! Palette DTOs shared by the Soma HTTP server and the desktop app.
//!
//! These are plain, transport-neutral data shapes — `crates/soma/palette`
//! owns the mapping *into* these from provider catalogs, and the routes that
//! serialize them, but the shapes themselves carry no server or Tauri
//! dependency so a desktop client can mirror them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One launcher-visible action, derived from a provider `ToolSpec` whose
/// `palette` overlay exposes it (or exposes it by default).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherCatalogEntry {
    /// The action id to pass to `/v1/palette/execute`. Stable across catalog
    /// refreshes as long as the underlying tool name doesn't change.
    pub id: String,
    pub provider: String,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_view: Option<String>,
    pub destructive: bool,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherCatalogResponse {
    pub schema_version: u32,
    pub fingerprint: String,
    pub entries: Vec<LauncherCatalogEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSearchResponse {
    pub entries: Vec<LauncherCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LauncherSchemaQuery {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSchemaResponse {
    pub id: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherExecuteRequest {
    pub id: String,
    /// Defaults to an empty object, not `Value::Null` (`Value::default()`) —
    /// provider input schemas validate against object-shaped schemas, so a
    /// zero-argument action (e.g. `status`) would otherwise fail dispatch
    /// with `input_schema_failed` whenever a client omits `params` entirely.
    #[serde(default = "default_params")]
    pub params: Value,
    #[serde(default)]
    pub confirm_destructive: bool,
}

fn default_params() -> Value {
    Value::Object(serde_json::Map::new())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherExecuteResponse {
    pub output: Value,
    pub request_id: String,
}

#[cfg(test)]
#[path = "dto_tests.rs"]
mod tests;

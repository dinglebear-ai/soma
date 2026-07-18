use serde::{Deserialize, Serialize};
use serde_json::Value;
use soma_provider_core::ProviderCatalog;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecuteActionRequest {
    pub action: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExecuteActionResponse {
    pub output: Value,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElicitedName {
    Accepted(String),
    NoInput,
    Declined,
    Cancelled,
    Unsupported,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScaffoldIntentRequest {
    pub display_name: String,
    pub crate_name: String,
    pub binary_name: String,
    pub server_category: String,
    pub env_prefix: String,
    pub auth_kind: String,
    pub host: String,
    pub port: u16,
    pub mcp_transport: String,
    pub mcp_primitives: String,
    pub deployment: String,
    pub plugins: String,
    pub publish_mcp: bool,
    pub crawl_urls: String,
    pub crawl_repos: String,
    pub crawl_search_topics: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GatewayReloadRequest {
    #[serde(default)]
    pub config: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GatewayExecuteRequest {
    pub action: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayRouteScope {
    pub upstreams: Vec<String>,
    pub services: Vec<String>,
    pub expose_code_mode: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GatewayToolRoute {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayResourceRoute {
    pub uri: String,
    pub native_uri: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayPromptRoute {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodeModeExecuteRequest {
    pub source: String,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiExecuteRequest {
    pub operation: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationResponse {
    pub output: Value,
    pub request_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReadResourceRequest {
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ResourceContent {
    Text {
        text: String,
        mime_type: Option<String>,
    },
    Blob {
        blob_base64: String,
        mime_type: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceTemplateSpec {
    pub uri_template: String,
    pub name: String,
    pub description: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogSnapshot {
    pub id: String,
    pub fingerprint: String,
    pub catalogs: Vec<ProviderCatalog>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DoctorReport {
    pub ready: bool,
    pub status: Option<Value>,
    pub problems: Vec<String>,
}

use serde::{Deserialize, Serialize};
use serde_json::Value;
use soma_provider_core::ProviderCatalog;

/// Request to execute a named action with free-form JSON parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecuteActionRequest {
    /// Name of the action to execute.
    pub action: String,
    /// JSON parameters passed to the action (defaults to `null` when absent).
    #[serde(default)]
    pub params: Value,
}

/// Result of executing an action, pairing its output with a correlation id.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExecuteActionResponse {
    /// JSON output produced by the action.
    pub output: Value,
    /// Unique id correlating this response with its originating request.
    pub request_id: String,
}

/// Outcome of an MCP elicitation prompt asking the client for a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElicitedName {
    /// The user supplied a name.
    Accepted(String),
    /// The user accepted the prompt but left the name empty.
    NoInput,
    /// The user declined to provide a name.
    Declined,
    /// The user cancelled the elicitation.
    Cancelled,
    /// The client does not support elicitation.
    Unsupported,
}

/// Parameters collected by the scaffold-intent wizard to generate a new server.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScaffoldIntentRequest {
    /// Human-readable display name for the scaffolded server.
    pub display_name: String,
    /// Cargo crate name for the generated project.
    pub crate_name: String,
    /// Name of the produced binary.
    pub binary_name: String,
    /// Category classifying the kind of server.
    pub server_category: String,
    /// Environment variable prefix for the server's configuration.
    pub env_prefix: String,
    /// Authentication scheme to wire in (e.g. bearer, oauth).
    pub auth_kind: String,
    /// Default bind host.
    pub host: String,
    /// Default bind port.
    pub port: u16,
    /// MCP transport(s) to enable (e.g. http, stdio).
    pub mcp_transport: String,
    /// MCP primitives to include (tools, resources, prompts).
    pub mcp_primitives: String,
    /// Deployment target/model for the scaffolded server.
    pub deployment: String,
    /// Plugin surfaces to generate (Claude/Codex/Gemini).
    pub plugins: String,
    /// Whether to publish MCP registry metadata.
    pub publish_mcp: bool,
    /// Seed URLs to crawl for the knowledge base.
    pub crawl_urls: String,
    /// Seed repositories to ingest for the knowledge base.
    pub crawl_repos: String,
    /// Seed search topics for automated web crawling.
    pub crawl_search_topics: String,
}

/// Request to reload the gateway with an optional replacement config.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GatewayReloadRequest {
    /// New gateway configuration to apply (defaults to `null` when absent).
    #[serde(default)]
    pub config: Value,
}

/// Request to execute a gateway action with free-form JSON parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GatewayExecuteRequest {
    /// Name of the gateway action to execute.
    pub action: String,
    /// JSON parameters passed to the action (defaults to `null` when absent).
    #[serde(default)]
    pub params: Value,
}

/// Scope constraining which gateway upstreams and services a route may reach.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayRouteScope {
    /// Allowed upstream identifiers.
    pub upstreams: Vec<String>,
    /// Allowed service identifiers.
    pub services: Vec<String>,
    /// Whether the code-mode surface is exposed to this route.
    pub expose_code_mode: bool,
}

/// A tool advertised through the gateway routing layer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GatewayToolRoute {
    /// Tool name as exposed by the gateway.
    pub name: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// Optional JSON schema for the tool's input.
    pub input_schema: Option<Value>,
    /// Optional JSON schema for the tool's output.
    pub output_schema: Option<Value>,
    /// Whether invoking the tool may have destructive effects.
    pub destructive: bool,
}

/// A resource advertised through the gateway routing layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayResourceRoute {
    /// Gateway-facing URI for the resource.
    pub uri: String,
    /// Native upstream URI the gateway URI maps to.
    pub native_uri: String,
    /// Optional human-readable resource name.
    pub name: Option<String>,
}

/// A prompt advertised through the gateway routing layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayPromptRoute {
    /// Prompt name as exposed by the gateway.
    pub name: String,
    /// Optional human-readable description.
    pub description: Option<String>,
}

/// Request to execute a code-mode script with optional JSON input.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodeModeExecuteRequest {
    /// Source code to execute in the code-mode sandbox.
    pub source: String,
    /// JSON input made available to the script (defaults to `null` when absent).
    #[serde(default)]
    pub input: Value,
}

/// Request to invoke an OpenAPI operation with free-form JSON parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiExecuteRequest {
    /// Identifier of the OpenAPI operation to invoke.
    pub operation: String,
    /// JSON parameters passed to the operation (defaults to `null` when absent).
    #[serde(default)]
    pub params: Value,
}

/// Result of an operation, pairing its output with a correlation id.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationResponse {
    /// JSON output produced by the operation.
    pub output: Value,
    /// Unique id correlating this response with its originating request.
    pub request_id: String,
}

/// Request to read a resource identified by URI.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReadResourceRequest {
    /// URI of the resource to read.
    pub uri: String,
}

/// Content returned when reading a resource, either text or binary blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ResourceContent {
    /// Textual resource content.
    Text {
        /// The resource text.
        text: String,
        /// Optional MIME type of the text.
        mime_type: Option<String>,
    },
    /// Binary resource content encoded as base64.
    Blob {
        /// Base64-encoded resource bytes.
        blob_base64: String,
        /// Optional MIME type of the blob.
        mime_type: Option<String>,
    },
}

/// Specification of a templated resource with a parameterized URI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceTemplateSpec {
    /// RFC 6570 URI template describing the resource address.
    pub uri_template: String,
    /// Human-readable name for the resource template.
    pub name: String,
    /// Human-readable description of the resource template.
    pub description: String,
    /// Optional MIME type of resources produced by the template.
    pub mime_type: Option<String>,
}

/// Snapshot of the provider catalogs at a point in time.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogSnapshot {
    /// Identifier of this snapshot.
    pub id: String,
    /// Fingerprint of the catalog contents used for change detection.
    pub fingerprint: String,
    /// Provider catalogs captured in the snapshot.
    pub catalogs: Vec<ProviderCatalog>,
}

/// Result of the `doctor` pre-flight readiness check.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DoctorReport {
    /// Whether the service is ready to serve requests.
    pub ready: bool,
    /// Optional status detail captured during the check.
    pub status: Option<Value>,
    /// List of problems detected during the check.
    pub problems: Vec<String>,
}

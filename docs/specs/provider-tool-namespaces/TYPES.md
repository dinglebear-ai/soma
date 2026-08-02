---
title: "Provider Tool Namespace Rust Types"
created: 2026-08-02
updated: 2026-08-02
doc_type: "type-design"
status: "proposed"
owner: "soma"
---

# Provider Tool Namespace Rust Types

These signatures are normative design targets, not compiled source.

## Identity Types

```rust
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash,
    serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct ToolId(String);

impl ToolId {
    pub fn new(value: impl Into<String>) -> Result<Self, ToolIdError>;
    pub fn as_str(&self) -> &str;
    pub fn into_string(self) -> String;
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash,
    serde::Serialize, serde::Deserialize,
)]
pub struct ProviderToolId {
    pub provider: ProviderId,
    pub tool: ToolId,
}

impl ProviderToolId {
    pub fn new(provider: ProviderId, tool: ToolId) -> Self;
    pub fn display_name(&self) -> String; // Presentation only: provider.tool
}
```

`ProviderId` and `ToolId` SHOULD share one private identifier validator while
retaining distinct public types and error codes.

## Registered Tool

```rust
#[derive(Clone)]
pub struct RegisteredTool {
    id: ProviderToolId,
    tool: ToolSpec,
    input_validator: Arc<jsonschema::Validator>,
    output_validator: Option<Arc<jsonschema::Validator>>,
}

impl RegisteredTool {
    pub fn id(&self) -> &ProviderToolId;
    pub fn provider_id(&self) -> &ProviderId;
    pub fn tool_id(&self) -> &ToolId;
    pub fn spec(&self) -> &ToolSpec;
}
```

## Surface Keys

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CliToolKey {
    pub provider_command: String,
    pub tool_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RestRouteKey {
    pub method: http::Method,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyToolResolution {
    Unique(ProviderToolId),
    Ambiguous(Vec<ProviderToolId>),
}
```

The ambiguous vector is sorted for diagnostics and MUST NOT be used to select a
winner.

## Registry Indexes

```rust
#[derive(Clone, Default)]
pub struct ProviderIndexes {
    tools: BTreeMap<ProviderToolId, RegisteredTool>,
    cli: BTreeMap<CliToolKey, ProviderToolId>,
    rest: BTreeMap<RestRouteKey, ProviderToolId>,
    legacy_flat: BTreeMap<String, LegacyToolResolution>,
    primitives: BTreeMap<String, PrimitiveKind>,
}

impl ProviderIndexes {
    pub fn tool(&self, id: &ProviderToolId) -> Option<&RegisteredTool>;
    pub fn provider_tools(
        &self,
        provider: &ProviderId,
    ) -> impl Iterator<Item = &RegisteredTool>;
    pub fn cli_tool(&self, key: &CliToolKey) -> Option<&ProviderToolId>;
    pub fn rest_tool(&self, key: &RestRouteKey) -> Option<&ProviderToolId>;
    pub fn legacy_tool(&self, action: &str) -> Option<&LegacyToolResolution>;
}
```

## Invocation Types

```rust
#[derive(Debug, Clone)]
pub struct ProviderCall {
    pub id: ProviderToolId,
    pub params: serde_json::Value,
    pub surface: ProviderSurface,
    pub snapshot_id: String,
    pub context: ProviderInvocationContext,
}

impl ProviderCall {
    pub fn new(id: ProviderToolId, params: serde_json::Value) -> Self;
    pub fn provider(&self) -> &ProviderId;
    pub fn tool(&self) -> &ToolId;
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteProviderToolRequest {
    pub provider: ProviderId,
    pub tool: ToolId,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolResultEnvelope {
    pub provider: ProviderId,
    pub tool: ToolId,
    pub output: serde_json::Value,
    pub request_id: String,
    pub progress: Vec<ProviderProgressEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ProviderSurfaceWarning>,
}
```

## Compatibility Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderManifestSemantics {
    V1Flat,
    V2Namespaced,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderSurfaceWarning {
    pub code: String,
    pub message: String,
    pub canonical_provider: ProviderId,
    pub canonical_tool: ToolId,
}
```

Compatibility resolution belongs in provider-core/application policy, never in
individual CLI, MCP, or REST adapters.

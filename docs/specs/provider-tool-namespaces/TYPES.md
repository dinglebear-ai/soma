---
title: "Provider Tool Namespace Rust Types"
created: 2026-08-02
updated: 2026-08-02
doc_type: "type-design"
status: "proposed"
owner: "soma"
---

# Provider Tool Namespace Rust Types

These signatures are normative design targets, not compiled source. Private
helper layout may follow crate conventions, but validation and ownership
boundaries are part of the contract.

## Identity Types

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ToolId(String);

impl ToolId {
    pub fn new(value: impl Into<String>) -> Result<Self, ToolIdError>;
    pub fn as_str(&self) -> &str;
    pub fn into_string(self) -> String;
}

impl TryFrom<String> for ToolId {
    type Error = ToolIdError;
    fn try_from(value: String) -> Result<Self, Self::Error>;
}

// Manual Deserialize, or #[serde(try_from = "String")], is required.
// Transparent derived Deserialize would bypass ToolId::new.

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash,
    Serialize, Deserialize,
)]
pub struct ProviderToolId {
    pub provider: ProviderId,
    pub tool: ToolId,
}

impl ProviderToolId {
    pub fn new(provider: ProviderId, tool: ToolId) -> Self;
    pub fn display_name(&self) -> String; // Presentation only.
}
```

`ProviderId` and `ToolId` share one private grammar validator while
retaining distinct public types and error codes. `ProviderId` must receive the
same serde hardening; current transparent deserialization is not sufficient.

## Manifest Version

```rust
impl ProviderManifest {
    pub fn require_namespaced_v2(&self) -> Result<(), ProviderValidationError>;
}
```

`ProviderManifest` rejects every schema version except v2 at validation.

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

Provider-core stays transport-neutral and does not add an `http` dependency.

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CliToolKey {
    pub provider: ProviderId,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RestMethod(String); // Validated, uppercase ASCII method.

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomRestRouteKey {
    pub method: RestMethod,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RestRouteShapeKey {
    pub method: RestMethod,
    pub normalized_segments: Vec<RestRouteSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RestRouteSegment {
    Static(String),
    Capture,
    CatchAll,
}

```

## Registry Indexes

```rust
#[derive(Clone, Default)]
pub struct ProviderIndexes {
    tools: BTreeMap<ProviderToolId, RegisteredTool>,
    cli: BTreeMap<CliToolKey, ProviderToolId>,
    custom_rest: BTreeMap<CustomRestRouteKey, ProviderToolId>,
    custom_rest_shapes: BTreeMap<RestRouteShapeKey, ProviderToolId>,
    primitives: BTreeMap<String, PrimitiveKind>,
}

impl ProviderIndexes {
    pub fn tool(&self, id: &ProviderToolId) -> Option<&RegisteredTool>;
    pub fn provider_tools(
        &self,
        provider: &ProviderId,
    ) -> impl Iterator<Item = &RegisteredTool>;
    pub fn cli_tool(&self, key: &CliToolKey) -> Option<&ProviderToolId>;
    pub fn custom_rest_tool(
        &self,
        key: &CustomRestRouteKey,
    ) -> Option<&ProviderToolId>;
}
```

The canonical route does not appear here: its validated path segments form a
`ProviderToolId` and use `tool()` directly. App-owned route validation layers
in infrastructure/reserved-path policy and dynamic resource-template checks.

## Canonical Request and Core Invocation

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteProviderToolRequest {
    pub id: ProviderToolId,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ProviderInvocation {
    pub id: ProviderToolId,
    pub params: serde_json::Value,
    pub surface: ProviderSurface,
    pub snapshot_id: String,
    pub context: ProviderInvocationContext,
}

impl ProviderInvocation {
    pub fn new(id: ProviderToolId, params: serde_json::Value) -> Self;
    pub fn provider(&self) -> &ProviderId;
    pub fn tool(&self) -> &ToolId;
}
```

`ProviderInvocation` is product-neutral and belongs in provider-core. It does
not replace the application type that carries principal, auth mode,
confirmation, limits, traces, and progress.

## Application Preflight and Execution

```rust
#[derive(Debug, Clone)]
pub struct ProviderToolPreflight {
    pub id: ProviderToolId,
    pub snapshot_id: String,
    pub destructive: bool,
    pub requires_admin: bool,
    pub required_scope: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfirmationProof {
    pub id: ProviderToolId,
    pub snapshot_id: String,
    pub destructive: bool,
    pub confirmed: bool,
}

pub struct PreparedProviderExecution {
    pub id: ProviderToolId,
    pub snapshot: Arc<RegistrySnapshot>,
    pub entry: RegisteredTool,
    pub provider: Arc<dyn Provider>,
    pub dispatch_lease: DispatchLease,
    pub principal: ProviderPrincipal,
    pub auth_mode: ProviderAuthMode,
    pub limits: ProviderRequestLimits,
    pub context: ProviderInvocationContext,
}
```

Preflight does not contain a `DispatchLease`. Final preparation re-resolves the
identity, validates any proof, then acquires the lease. The exact visibility of
private fields may vary, but the lifetime boundary is normative.

## Results and Refresh Events

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolResultEnvelope {
    pub provider: ProviderId,
    pub tool: ToolId,
    pub output: serde_json::Value,
    pub request_id: String,
    pub progress: Vec<ProviderProgressEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderRefreshEvent {
    pub generation_id: u64,
    pub fingerprint: String,
    pub added: Vec<ProviderToolId>,
    pub removed: Vec<ProviderToolId>,
    pub surface_changes: Vec<ProviderToolId>,
    pub schema_changed: bool,
}
```

MCP paging cache entries additionally retain `ProviderToolId`; Palette and web
DTOs use the same structured identity rather than parsing display strings.

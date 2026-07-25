//! Provider dispatch registry: the [`ProviderRegistry`] resolves actions to
//! providers, builds catalog snapshots, and indexes resources and prompts.
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use soma_domain::provider_validation::{validate_provider_manifest, ProviderValidationError};
use soma_provider_core::{
    ProviderCall as CoreProviderCall, ProviderCatalog, ProviderRegistry as CoreRegistry,
    ProviderResource, RegistrySnapshot as CoreRegistrySnapshot,
};

use crate::{
    capabilities::CapabilityBroker, provider_errors::ProviderError,
    providers::filesystem::FileProviderSource,
};

mod enforcement;
mod refresh;
mod reports;
mod resources;
pub(super) use enforcement::provider_tool_surface_enabled;
use enforcement::{enforce_capabilities, enforce_pre_input, enforce_response_limit};
use refresh::ProviderRefreshEvent;
use resources::ResourceIndex;
pub use resources::{DynamicResourceTemplate, ResourceReadOutput};
pub use soma_provider_core::{Provider as CoreProvider, ProviderOutput};
/// Product-neutral provider call shape (`soma_provider_core::ProviderCall`),
/// as handed to the shared core registry once auth/scope fields are stripped.
pub type ProviderInvocation = CoreProviderCall;

/// A loaded provider: exposes a catalog and dispatches action calls, with
/// optional MCP resource-read support layered on top.
#[async_trait]
pub trait Provider: Send + Sync {
    /// The provider's catalog — its tools, prompts, resources, and metadata.
    fn catalog(&self) -> ProviderCatalog;

    /// Executes one provider action call and returns its output.
    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError>;

    /// Dynamic resource templates this provider serves. Every provider
    /// inherits the empty default — only file-based dynamic resource
    /// readers (`providers/resources/*.ts`) override it.
    fn dynamic_resource_templates(&self) -> Vec<DynamicResourceTemplate> {
        Vec::new()
    }

    /// Whether this provider can actually serve `read_resource` calls.
    /// `catalog().resources` is a schema-legal field on every provider
    /// kind's manifest (OpenAPI, MCP, ai-sdk, WASM, Python, generic JSON),
    /// but only file-based `ResourceFileProvider`s have any mechanism to
    /// read content back — every other kind inherits `read_resource`'s
    /// default error. `ResourceIndex::register` uses this to reject a
    /// manifest that declares resources it can never serve at snapshot-build
    /// time, rather than letting them appear in `resources/list` and always
    /// fail `resources/read` with an opaque error.
    fn supports_resource_reads(&self) -> bool {
        false
    }

    /// Reads resource content for a URI the registry has already matched
    /// against either this provider's `catalog().resources` (exact,
    /// `params` empty) or one of its `dynamic_resource_templates()`
    /// (`params` holds captured path parameters).
    async fn read_resource(
        &self,
        uri: &str,
        params: &BTreeMap<String, String>,
    ) -> Result<ResourceReadOutput, ProviderError> {
        let _ = params;
        Err(ProviderError::validation(
            &self.catalog().provider.name,
            uri,
            "resource_read_not_supported",
            "this provider does not support resource reads",
        ))
    }
}

/// The Soma-facing surface a provider call arrives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSurface {
    /// MCP tool/resource/prompt surface.
    Mcp,
    /// REST HTTP surface.
    Rest,
    /// Command-line surface.
    Cli,
    /// Command palette surface.
    Palette,
}

impl ProviderSurface {
    /// The lowercase string identifier for this surface.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::Rest => "rest",
            Self::Cli => "cli",
            Self::Palette => "palette",
        }
    }

    fn core(self) -> soma_provider_core::ProviderSurface {
        match self {
            Self::Mcp => soma_provider_core::ProviderSurface::Mcp,
            Self::Rest => soma_provider_core::ProviderSurface::Rest,
            Self::Cli => soma_provider_core::ProviderSurface::Cli,
            Self::Palette => soma_provider_core::ProviderSurface::Palette,
        }
    }
}

/// How the caller authenticated, governing whether scope checks apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthMode {
    /// Loopback development mode; scope checks bypassed.
    LoopbackDev,
    /// Non-loopback behind an authz-enforcing gateway; scope checks bypassed.
    TrustedGateway,
    /// Mounted auth middleware; scope checks enforced.
    Mounted,
}

/// The authenticated caller: its subject identity and granted scopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPrincipal {
    /// Subject identifier for the caller.
    pub subject: String,
    /// Scopes granted to the caller.
    pub scopes: Vec<String>,
}

impl ProviderPrincipal {
    /// The synthetic principal used in loopback development mode, granted the
    /// read scope.
    pub fn loopback_dev() -> Self {
        Self {
            subject: "loopback-dev".to_owned(),
            scopes: vec![soma_domain::actions::READ_SCOPE.to_owned()],
        }
    }

    /// An anonymous principal with no scopes.
    pub fn anonymous() -> Self {
        Self {
            subject: "anonymous".to_owned(),
            scopes: Vec::new(),
        }
    }
}

/// Byte-size ceilings enforced on a provider call's input and response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderRequestLimits {
    /// Maximum accepted request input size in bytes.
    pub max_input_bytes: usize,
    /// Maximum permitted response size in bytes.
    pub max_response_bytes: usize,
}

impl Default for ProviderRequestLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024,
            max_response_bytes: soma_domain::token_limit::MAX_RESPONSE_BYTES,
        }
    }
}

/// A fully-resolved provider action invocation, carrying auth, scope, surface,
/// and limit context alongside the action and its parameters.
#[derive(Debug, Clone)]
pub struct ProviderCall {
    /// Target provider name.
    pub provider: String,
    /// Action to invoke on the provider.
    pub action: String,
    /// Action parameters as JSON.
    pub params: Value,
    /// Authenticated caller.
    pub principal: ProviderPrincipal,
    /// Authentication mode governing scope enforcement.
    pub auth_mode: ProviderAuthMode,
    /// Surface the call arrived on.
    pub surface: ProviderSurface,
    /// Whether the caller confirmed a destructive action.
    pub destructive_confirmed: bool,
    /// Byte-size limits applied to this call.
    pub limits: ProviderRequestLimits,
    /// Registry snapshot id this call is bound to.
    pub snapshot_id: String,
}

impl ProviderCall {
    /// Strips this call down to the product-neutral core invocation shape.
    pub fn provider_invocation(&self) -> ProviderInvocation {
        ProviderInvocation {
            provider: self.provider.clone(),
            action: self.action.clone(),
            params: self.params.clone(),
            surface: self.surface.core(),
            snapshot_id: self.snapshot_id.clone(),
        }
    }

    /// Builds the serializable execution envelope describing this call.
    pub fn execution_envelope(&self) -> ProviderExecutionEnvelope {
        ProviderExecutionEnvelope {
            schema_version: 1,
            provider: self.provider.clone(),
            action: self.action.clone(),
            params: self.params.clone(),
            surface: self.surface,
            snapshot_id: self.snapshot_id.clone(),
        }
    }

    /// Serializes the execution envelope to JSON bytes.
    pub fn execution_payload(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.execution_envelope())
    }
}

/// The serializable record of a provider action invocation, emitted for
/// downstream execution/audit consumers.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderExecutionEnvelope {
    /// Envelope schema version.
    pub schema_version: u32,
    /// Target provider name.
    pub provider: String,
    /// Action invoked.
    pub action: String,
    /// Action parameters as JSON.
    pub params: Value,
    /// Surface the call arrived on.
    pub surface: ProviderSurface,
    /// Registry snapshot id the call is bound to.
    pub snapshot_id: String,
}

/// An immutable, fingerprinted view of the loaded providers: their catalogs,
/// routing/resource indexes, and cached derived artifacts.
#[derive(Clone)]
pub struct RegistrySnapshot {
    /// Snapshot identifier (equal to the fingerprint).
    pub id: String,
    /// Content fingerprint of the loaded provider set.
    pub fingerprint: String,
    /// Catalogs for every loaded provider.
    pub catalogs: Vec<ProviderCatalog>,
    core: Arc<CoreRegistrySnapshot>,
    exact_resources: HashMap<String, (String, ProviderResource)>,
    dynamic_resources: Vec<(String, DynamicResourceTemplate)>,
    /// Number of JSON schema validators compiled for this snapshot.
    pub compiled_validator_count: usize,
    /// Cached, pre-serialized OpenAPI document bytes.
    pub cached_openapi_bytes: Arc<Vec<u8>>,
    /// Cached catalog summary JSON.
    pub cached_catalog_summary: Arc<Value>,
}

impl RegistrySnapshot {
    /// The names of every action in this snapshot.
    pub fn action_names(&self) -> Vec<&str> {
        self.core.action_names().collect()
    }

    /// Resolves a REST method+path to its action name, if routed.
    pub fn route_action(&self, method: &str, path: &str) -> Option<&str> {
        self.core.route_action(method, path)
    }

    /// Resolves a CLI command to its action name, if routed.
    pub fn cli_action(&self, command: &str) -> Option<&str> {
        self.core.cli_action(command)
    }

    /// The primitive kind (tool/prompt/resource) for a named primitive.
    pub fn primitive_kind(&self, name: &str) -> Option<&str> {
        self.core.primitive_kind(name)
    }

    /// Whether the action is marked destructive and requires confirmation.
    pub fn action_requires_confirmation(&self, action: &str) -> bool {
        self.core
            .tool(action)
            .map(|entry| entry.spec().destructive)
            .unwrap_or(false)
    }

    /// The provider that owns the given action.
    pub fn provider_for_action(&self, action: &str) -> Option<&str> {
        self.core
            .tool(action)
            .map(|entry| entry.provider_id().as_str())
    }

    /// Borrows the underlying product-neutral core snapshot.
    pub fn core_snapshot(&self) -> &CoreRegistrySnapshot {
        &self.core
    }

    pub(crate) fn rest_routes(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.core.rest_routes()
    }
}

/// Thread-safe registry of loaded providers backing every Soma surface,
/// holding the active snapshot and supporting hot-reload from a file source.
#[derive(Clone)]
pub struct ProviderRegistry {
    state: Arc<RwLock<RegistryState>>,
    capabilities: CapabilityBroker,
    base_providers: Arc<Vec<Arc<dyn Provider>>>,
    file_source: Option<FileProviderSource>,
}

struct RegistryState {
    providers: BTreeMap<String, Arc<dyn Provider>>,
    core_registry: CoreRegistry,
    snapshot: Arc<RegistrySnapshot>,
    file_fingerprint: Option<String>,
}

impl ProviderRegistry {
    /// Builds a registry from the given providers with a default-deny
    /// capability broker.
    pub fn new(providers: Vec<Arc<dyn Provider>>) -> Result<Self, ProviderValidationError> {
        Self::with_capabilities(providers, CapabilityBroker::default_deny())
    }

    /// Builds a registry from the given providers and capability broker.
    pub fn with_capabilities(
        providers: Vec<Arc<dyn Provider>>,
        capabilities: CapabilityBroker,
    ) -> Result<Self, ProviderValidationError> {
        let (providers, core_registry, snapshot) = build_registry(providers)?;
        Ok(Self {
            state: Arc::new(RwLock::new(RegistryState {
                providers,
                core_registry,
                snapshot,
                file_fingerprint: None,
            })),
            capabilities,
            base_providers: Arc::new(Vec::new()),
            file_source: None,
        })
    }

    /// Builds a registry from the given base providers plus dynamic providers
    /// loaded from a file source, enabling later hot-reloads.
    pub fn with_file_source(
        providers: Vec<Arc<dyn Provider>>,
        capabilities: CapabilityBroker,
        file_source: FileProviderSource,
    ) -> Result<Self, ProviderValidationError> {
        let file_fingerprint = file_source.fingerprint().map_err(|error| {
            ProviderValidationError::new("provider_file_load_failed", error.to_string())
        })?;
        let dynamic_providers = file_source.load().map_err(|error| {
            ProviderValidationError::new("provider_file_load_failed", error.to_string())
        })?;
        let base_providers = Arc::new(providers);
        let mut all_providers = base_providers.iter().cloned().collect::<Vec<_>>();
        all_providers.extend(dynamic_providers);
        let (providers, core_registry, snapshot) = build_registry(all_providers)?;
        Ok(Self {
            state: Arc::new(RwLock::new(RegistryState {
                providers,
                core_registry,
                snapshot,
                file_fingerprint: Some(file_fingerprint),
            })),
            capabilities,
            base_providers,
            file_source: Some(file_source),
        })
    }

    /// Returns the currently active registry snapshot.
    pub fn snapshot(&self) -> Arc<RegistrySnapshot> {
        self.state
            .read()
            .expect("provider registry lock should not be poisoned")
            .snapshot
            .clone()
    }

    /// Refreshes providers from the file source, if any. Per the drop-in
    /// provider layout contract ("If a resource disappears or becomes
    /// invalid, a reload must leave the last valid snapshot active until a
    /// valid replacement snapshot is available"), a failure anywhere in this
    /// pipeline (an unreadable directory, a newly-invalid or colliding
    /// provider file) is logged and this returns the previous snapshot
    /// rather than propagating the error — one bad drop-in file must not
    /// take down `list_tools`/`list_prompts`/`read_resource`/etc. for every
    /// other, unrelated, already-loaded provider.
    pub fn refresh_file_providers(&self) -> Result<Arc<RegistrySnapshot>, ProviderValidationError> {
        let Some(file_source) = &self.file_source else {
            return Ok(self.snapshot());
        };
        let file_fingerprint = match file_source.fingerprint() {
            Ok(fingerprint) => fingerprint,
            Err(error) => {
                return Ok(self.snapshot_after_refresh_failure(
                    file_source,
                    "provider_file_fingerprint_failed",
                    &error.to_string(),
                ));
            }
        };
        {
            let state = self
                .state
                .read()
                .expect("provider registry lock should not be poisoned");
            if state.file_fingerprint.as_deref() == Some(file_fingerprint.as_str()) {
                return Ok(state.snapshot.clone());
            }
        }

        let rebuilt: Result<_, ProviderValidationError> = (|| {
            let dynamic_providers = file_source.load().map_err(|error| {
                ProviderValidationError::new("provider_file_load_failed", error.to_string())
            })?;
            let mut providers = self.base_providers.iter().cloned().collect::<Vec<_>>();
            providers.extend(dynamic_providers);
            let (providers, core_registry, snapshot) = build_registry(providers)?;
            Ok((providers, core_registry, snapshot))
        })();
        let (providers, core_registry, snapshot) = match rebuilt {
            Ok(parts) => parts,
            Err(error) => {
                return Ok(self.snapshot_after_refresh_failure(
                    file_source,
                    error.code(),
                    error.message(),
                ));
            }
        };

        let mut state = self
            .state
            .write()
            .expect("provider registry lock should not be poisoned");
        if state.snapshot.fingerprint == snapshot.fingerprint {
            return Ok(state.snapshot.clone());
        }
        let previous = state.snapshot.clone();
        let event = ProviderRefreshEvent::new(&previous, &snapshot);
        state.providers = providers;
        state.core_registry = core_registry;
        state.snapshot = snapshot.clone();
        state.file_fingerprint = Some(file_fingerprint);
        event.log(file_source.root());
        Ok(snapshot)
    }

    fn snapshot_after_refresh_failure(
        &self,
        file_source: &FileProviderSource,
        code: &str,
        message: &str,
    ) -> Arc<RegistrySnapshot> {
        tracing::warn!(
            root = %file_source.root().display(),
            code,
            message,
            "provider directory refresh failed; keeping the last valid snapshot active"
        );
        self.snapshot()
    }

    /// Builds and validates a snapshot from the given providers without
    /// installing it, for pre-flight checking a candidate reload.
    pub fn validate_reload(
        &self,
        providers: Vec<Arc<dyn Provider>>,
    ) -> Result<Arc<RegistrySnapshot>, ProviderValidationError> {
        let (_, _, snapshot) = build_registry(providers)?;
        Ok(snapshot)
    }

    /// Atomically replaces the loaded providers and active snapshot,
    /// clearing any file fingerprint.
    pub fn reload(
        &self,
        providers: Vec<Arc<dyn Provider>>,
    ) -> Result<Arc<RegistrySnapshot>, ProviderValidationError> {
        let (providers, core_registry, snapshot) = build_registry(providers)?;
        let mut state = self
            .state
            .write()
            .expect("provider registry lock should not be poisoned");
        state.providers = providers;
        state.core_registry = core_registry;
        state.snapshot = snapshot.clone();
        state.file_fingerprint = None;
        Ok(snapshot)
    }

    /// Routes a provider call to its owning provider, enforcing pre-input
    /// checks, declared capabilities, and the response size limit around the
    /// provider's own `call`.
    pub async fn dispatch(&self, mut call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let (snapshot, core_registry, provider, tool, capabilities, provider_kind) = {
            let state = self
                .state
                .read()
                .expect("provider registry lock should not be poisoned");
            let snapshot = state.snapshot.clone();
            let entry = snapshot.core_snapshot().tool(&call.action).ok_or_else(|| {
                ProviderError::validation(
                    "registry",
                    call.action.clone(),
                    "unknown_action",
                    format!("unknown provider action `{}`", call.action),
                )
            })?;
            let provider_name = entry.provider_id().as_str();
            let tool = entry.spec().clone();
            let provider = state.providers.get(provider_name).cloned().ok_or_else(|| {
                ProviderError::new(
                    "provider_not_loaded",
                    provider_name,
                    Some(entry.spec().name.clone()),
                    "provider is not loaded in the active registry",
                    "Reload providers and retry.",
                )
            })?;
            let (capabilities, provider_kind) = snapshot
                .catalogs
                .iter()
                .find(|catalog| catalog.provider.name == provider_name)
                .map(|catalog| (catalog.capabilities.clone(), catalog.provider.kind))
                .expect("core provider index must reference an active catalog");
            (
                snapshot,
                state.core_registry.clone(),
                provider,
                tool,
                capabilities,
                provider_kind,
            )
        };

        call.provider = provider.catalog().provider.name;
        call.snapshot_id = snapshot.id.clone();
        let pre_input_call = call.clone();
        let invocation_call = call.clone();
        let pre_input_tool = tool.clone();
        let capability_broker = self.capabilities.clone();
        core_registry
            .dispatch_with_pre_input(
                call.provider_invocation(),
                move |invocation| {
                    let mut call = pre_input_call;
                    call.provider.clone_from(&invocation.provider);
                    call.snapshot_id.clone_from(&invocation.snapshot_id);
                    enforce_pre_input(&pre_input_tool, &call, provider_kind)
                },
                move |_, invocation| {
                    let mut call = invocation_call;
                    call.provider = invocation.provider;
                    call.snapshot_id = invocation.snapshot_id;
                    async move {
                        enforce_capabilities(&capabilities, &call, &capability_broker)?;
                        let output = provider.call(call.clone()).await?;
                        enforce_response_limit(&tool, &call, &output)?;
                        Ok(output)
                    }
                },
            )
            .await
            .inspect_err(|error| {
                let (provider, action, code) = error.log_code();
                tracing::warn!(provider, action, code, "provider call failed");
            })
    }
}

fn provider_map(
    providers: Vec<Arc<dyn Provider>>,
) -> Result<BTreeMap<String, Arc<dyn Provider>>, ProviderValidationError> {
    let mut map = BTreeMap::new();
    for provider in providers {
        let name = provider.catalog().provider.name;
        if map.insert(name.clone(), provider).is_some() {
            return Err(ProviderValidationError::new(
                "duplicate_provider_name",
                format!("duplicate provider `{name}`"),
            ));
        }
    }
    Ok(map)
}

/// Wraps a product-neutral `soma_provider_core::Provider` (as implemented by
/// every adapter in `soma-provider-adapters`) so it satisfies this crate's
/// own `Provider` trait, which carries additional auth/scope fields
/// (`principal`, `auth_mode`, `destructive_confirmed`, `limits`) that no
/// shared adapter reads — see `ProviderCall::provider_invocation()`, which
/// this reuses to build the generic call. This is the mirror image of
/// `CoreProviderAdapter` below, which wraps the other direction.
#[derive(Clone)]
pub struct SharedAdapter(Arc<dyn soma_provider_core::Provider>);

impl SharedAdapter {
    /// Wraps a shared core provider so it satisfies this crate's `Provider`
    /// trait.
    pub fn wrap(inner: Arc<dyn soma_provider_core::Provider>) -> Arc<dyn Provider> {
        Arc::new(Self(inner))
    }
}

#[async_trait]
impl Provider for SharedAdapter {
    fn catalog(&self) -> ProviderCatalog {
        self.0.catalog()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        self.0.call(call.provider_invocation()).await
    }
}

#[derive(Clone)]
struct CoreProviderAdapter(Arc<dyn Provider>);

type BuiltRegistry = (
    BTreeMap<String, Arc<dyn Provider>>,
    CoreRegistry,
    Arc<RegistrySnapshot>,
);

#[async_trait]
impl CoreProvider for CoreProviderAdapter {
    fn catalog(&self) -> ProviderCatalog {
        self.0.catalog()
    }

    async fn call(&self, call: CoreProviderCall) -> Result<ProviderOutput, ProviderError> {
        let surface = match call.surface {
            soma_provider_core::ProviderSurface::Mcp => ProviderSurface::Mcp,
            soma_provider_core::ProviderSurface::Rest => ProviderSurface::Rest,
            soma_provider_core::ProviderSurface::Cli => ProviderSurface::Cli,
            soma_provider_core::ProviderSurface::Palette => ProviderSurface::Palette,
            soma_provider_core::ProviderSurface::Internal
            | soma_provider_core::ProviderSurface::Ui => {
                return Err(ProviderError::validation(
                    call.provider,
                    call.action,
                    "unsupported_product_surface",
                    "Soma providers expose only MCP, REST, CLI, and Palette surfaces",
                ));
            }
        };
        self.0
            .call(ProviderCall {
                provider: call.provider,
                action: call.action,
                params: call.params,
                principal: ProviderPrincipal::anonymous(),
                auth_mode: ProviderAuthMode::TrustedGateway,
                surface,
                destructive_confirmed: false,
                limits: ProviderRequestLimits::default(),
                snapshot_id: call.snapshot_id,
            })
            .await
    }
}

fn build_registry(
    providers: Vec<Arc<dyn Provider>>,
) -> Result<BuiltRegistry, ProviderValidationError> {
    let providers = provider_map(providers)?;
    let mut builder = CoreRegistry::builder();
    for provider in providers.values() {
        validate_provider_manifest(&provider.catalog())?;
        builder = builder.register_arc(Arc::new(CoreProviderAdapter(provider.clone())))?;
    }
    let core_registry = builder.build()?;
    let snapshot = Arc::new(build_snapshot(&providers, &core_registry)?);
    Ok((providers, core_registry, snapshot))
}

fn build_snapshot(
    providers: &BTreeMap<String, Arc<dyn Provider>>,
    core_registry: &CoreRegistry,
) -> Result<RegistrySnapshot, ProviderValidationError> {
    let core = core_registry.snapshot();
    let mut resources = ResourceIndex::new();
    for provider in providers.values() {
        let catalog = provider.catalog();
        resources.register(&**provider, &catalog.provider.name, &catalog.resources)?;
    }
    let catalogs = core.catalogs().to_vec();
    let fingerprint = core.fingerprint().to_string();
    let id = fingerprint.clone();
    let action_names = core.action_names().map(str::to_owned).collect::<Vec<_>>();
    let openapi_paths = openapi_paths_from_core(&core);
    let cached_catalog_summary = Arc::new(json!({
        "schema_version": 1,
        "provider_fingerprint": fingerprint,
        "actions": action_names,
    }));
    let cached_openapi_bytes = Arc::new(
        serde_json::to_vec_pretty(&json!({
            "openapi": "3.1.0",
            "info": {"title": "soma provider API", "version": env!("CARGO_PKG_VERSION")},
            "x-soma": {
                "preferred_rest_style": "direct_routes",
                "provider_fingerprint": fingerprint
            },
            "paths": openapi_paths
        }))
        .expect("static OpenAPI summary serializes"),
    );

    Ok(RegistrySnapshot {
        id,
        fingerprint,
        catalogs,
        core: core.clone(),
        exact_resources: resources.exact,
        dynamic_resources: resources.dynamic,
        compiled_validator_count: core.compiled_validator_count(),
        cached_openapi_bytes,
        cached_catalog_summary,
    })
}

fn openapi_paths_from_core(core: &CoreRegistrySnapshot) -> Value {
    let mut paths = Map::new();
    paths.insert(
        "/v1/capabilities".to_owned(),
        json!({
            "get": {
                "summary": "List REST capabilities",
                "operationId": "v1Capabilities",
                "responses": {
                    "200": {"description": "Route inventory and server metadata"}
                }
            }
        }),
    );
    paths.insert(
        "/v1/providers".to_owned(),
        json!({
            "get": {
                "summary": "Inspect live providers",
                "operationId": "v1Providers",
                "responses": {
                    "200": {"description": "Live provider catalog and runtime inventory"}
                }
            }
        }),
    );
    paths.insert(
        "/v1/tools/{action}".to_owned(),
        json!({
            "post": {
                "summary": "Run a provider tool",
                "operationId": "runProviderTool",
                "parameters": [{
                    "name": "action",
                    "in": "path",
                    "required": true,
                    "schema": {"type": "string"},
                    "description": "Provider tool action name"
                }],
                "requestBody": {
                    "required": false,
                    "content": {
                        "application/json": {
                            "schema": {"type": "object", "additionalProperties": true}
                        }
                    }
                },
                "responses": {
                    "200": {"description": "Provider action response"},
                    "400": {"description": "Provider validation error"},
                    "403": {"description": "Provider authorization error"},
                    "404": {"description": "Unknown action or surface not exposed"}
                }
            }
        }),
    );

    let mut routes = core
        .rest_routes()
        .map(|(method, path, action)| (method.to_owned(), path.to_owned(), action.to_owned()))
        .collect::<Vec<_>>();
    routes.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));

    for (method, path, action) in routes {
        let entry = paths
            .entry(path)
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(methods) = entry {
            methods.insert(
                method.to_ascii_lowercase(),
                json!({
                    "summary": format!("Provider action `{action}`"),
                    "operationId": action,
                    "responses": {
                        "200": {"description": "Provider action response"},
                        "400": {"description": "Provider validation error"}
                    }
                }),
            );
        }
    }
    Value::Object(paths)
}

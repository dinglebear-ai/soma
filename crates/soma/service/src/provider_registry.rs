use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use jsonschema::Validator;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use soma_contracts::{
    provider_validation::{validate_provider_manifest, ProviderValidationError},
    providers::{HostCapabilities, ProviderCatalog, ProviderResource, ProviderTool},
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
use enforcement::{enforce_call, enforce_output_schema, enforce_response_limit};
use refresh::ProviderRefreshEvent;
use resources::ResourceIndex;
pub use resources::{DynamicResourceTemplate, ResourceReadOutput};

#[async_trait]
pub trait Provider: Send + Sync {
    fn catalog(&self) -> ProviderCatalog;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSurface {
    Mcp,
    Rest,
    Cli,
    Palette,
}

impl ProviderSurface {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::Rest => "rest",
            Self::Cli => "cli",
            Self::Palette => "palette",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthMode {
    LoopbackDev,
    TrustedGateway,
    Mounted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPrincipal {
    pub subject: String,
    pub scopes: Vec<String>,
}

impl ProviderPrincipal {
    pub fn loopback_dev() -> Self {
        Self {
            subject: "loopback-dev".to_owned(),
            scopes: vec![soma_contracts::actions::READ_SCOPE.to_owned()],
        }
    }

    pub fn anonymous() -> Self {
        Self {
            subject: "anonymous".to_owned(),
            scopes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderRequestLimits {
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
}

impl Default for ProviderRequestLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024,
            max_response_bytes: soma_contracts::token_limit::MAX_RESPONSE_BYTES,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderCall {
    pub provider: String,
    pub action: String,
    pub params: Value,
    pub principal: ProviderPrincipal,
    pub auth_mode: ProviderAuthMode,
    pub surface: ProviderSurface,
    pub destructive_confirmed: bool,
    pub limits: ProviderRequestLimits,
    pub snapshot_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderExecutionEnvelope {
    pub schema_version: u32,
    pub provider: String,
    pub action: String,
    pub params: Value,
    pub surface: ProviderSurface,
    pub snapshot_id: String,
}

impl ProviderCall {
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

    pub fn execution_payload(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.execution_envelope())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderOutput {
    pub value: Value,
}

impl ProviderOutput {
    pub fn json(value: Value) -> Self {
        Self { value }
    }
}

#[derive(Clone)]
pub struct RegistrySnapshot {
    pub id: String,
    pub fingerprint: String,
    pub catalogs: Vec<ProviderCatalog>,
    action_index: HashMap<String, ToolEntry>,
    rest_index: HashMap<(String, String), String>,
    cli_index: HashMap<String, String>,
    primitive_index: HashMap<String, String>,
    exact_resources: HashMap<String, (String, ProviderResource)>,
    dynamic_resources: Vec<(String, DynamicResourceTemplate)>,
    pub compiled_validator_count: usize,
    pub cached_openapi_bytes: Arc<Vec<u8>>,
    pub cached_catalog_summary: Arc<Value>,
    pub cached_palette_manifest: Arc<Value>,
}

impl RegistrySnapshot {
    pub fn action_names(&self) -> Vec<&str> {
        let mut names = self
            .action_index
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    }

    pub fn route_action(&self, method: &str, path: &str) -> Option<&str> {
        self.rest_index
            .get(&(method.to_owned(), path.to_owned()))
            .map(String::as_str)
    }

    pub fn cli_action(&self, command: &str) -> Option<&str> {
        self.cli_index.get(command).map(String::as_str)
    }

    pub fn primitive_kind(&self, name: &str) -> Option<&str> {
        self.primitive_index.get(name).map(String::as_str)
    }

    pub fn action_requires_confirmation(&self, action: &str) -> bool {
        self.tool_entry(action)
            .map(|entry| entry.tool.destructive)
            .unwrap_or(false)
    }

    pub fn provider_for_action(&self, action: &str) -> Option<&str> {
        self.tool_entry(action).map(|entry| entry.provider.as_str())
    }

    fn tool_entry(&self, action: &str) -> Option<&ToolEntry> {
        self.action_index.get(action)
    }
}

#[derive(Clone)]
struct ToolEntry {
    provider: String,
    action: String,
    tool: ProviderTool,
    capabilities: HostCapabilities,
    input_validator: Arc<Validator>,
    output_validator: Option<Arc<Validator>>,
}

#[derive(Clone)]
pub struct ProviderRegistry {
    state: Arc<RwLock<RegistryState>>,
    capabilities: CapabilityBroker,
    base_providers: Arc<Vec<Arc<dyn Provider>>>,
    file_source: Option<FileProviderSource>,
}

struct RegistryState {
    providers: BTreeMap<String, Arc<dyn Provider>>,
    snapshot: Arc<RegistrySnapshot>,
    file_fingerprint: Option<String>,
}

impl ProviderRegistry {
    pub fn new(providers: Vec<Arc<dyn Provider>>) -> Result<Self, ProviderValidationError> {
        Self::with_capabilities(providers, CapabilityBroker::default_deny())
    }

    pub fn with_capabilities(
        providers: Vec<Arc<dyn Provider>>,
        capabilities: CapabilityBroker,
    ) -> Result<Self, ProviderValidationError> {
        let providers = provider_map(providers)?;
        let snapshot = Arc::new(build_snapshot(providers.values().cloned().collect())?);
        Ok(Self {
            state: Arc::new(RwLock::new(RegistryState {
                providers,
                snapshot,
                file_fingerprint: None,
            })),
            capabilities,
            base_providers: Arc::new(Vec::new()),
            file_source: None,
        })
    }

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
        let providers = provider_map(all_providers)?;
        let snapshot = Arc::new(build_snapshot(providers.values().cloned().collect())?);
        Ok(Self {
            state: Arc::new(RwLock::new(RegistryState {
                providers,
                snapshot,
                file_fingerprint: Some(file_fingerprint),
            })),
            capabilities,
            base_providers,
            file_source: Some(file_source),
        })
    }

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
            let providers = provider_map(providers)?;
            let snapshot = Arc::new(build_snapshot(providers.values().cloned().collect())?);
            Ok((providers, snapshot))
        })();
        let (providers, snapshot) = match rebuilt {
            Ok(pair) => pair,
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

    pub fn validate_reload(
        &self,
        providers: Vec<Arc<dyn Provider>>,
    ) -> Result<Arc<RegistrySnapshot>, ProviderValidationError> {
        let providers = provider_map(providers)?;
        Ok(Arc::new(build_snapshot(
            providers.values().cloned().collect(),
        )?))
    }

    pub fn reload(
        &self,
        providers: Vec<Arc<dyn Provider>>,
    ) -> Result<Arc<RegistrySnapshot>, ProviderValidationError> {
        let providers = provider_map(providers)?;
        let snapshot = Arc::new(build_snapshot(providers.values().cloned().collect())?);
        let mut state = self
            .state
            .write()
            .expect("provider registry lock should not be poisoned");
        state.providers = providers;
        state.snapshot = snapshot.clone();
        state.file_fingerprint = None;
        Ok(snapshot)
    }

    pub async fn dispatch(&self, mut call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let (snapshot, provider, entry) = {
            let state = self
                .state
                .read()
                .expect("provider registry lock should not be poisoned");
            let snapshot = state.snapshot.clone();
            let entry = snapshot.tool_entry(&call.action).ok_or_else(|| {
                ProviderError::validation(
                    "registry",
                    call.action.clone(),
                    "unknown_action",
                    format!("unknown provider action `{}`", call.action),
                )
            })?;
            let entry = entry.clone();
            let provider = state
                .providers
                .get(&entry.provider)
                .cloned()
                .ok_or_else(|| {
                    ProviderError::new(
                        "provider_not_loaded",
                        &entry.provider,
                        Some(entry.action.clone()),
                        "provider is not loaded in the active registry",
                        "Reload providers and retry.",
                    )
                })?;
            (snapshot, provider, entry)
        };

        call.provider = entry.provider.clone();
        call.snapshot_id = snapshot.id.clone();
        enforce_call(&entry, &call, &self.capabilities)?;

        let output = provider.call(call.clone()).await.inspect_err(|error| {
            let (provider, action, code) = error.log_code();
            tracing::warn!(provider, action, code, "provider call failed");
        })?;
        enforce_response_limit(&entry, &call, &output)?;
        enforce_output_schema(&entry, &output)?;
        Ok(output)
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

fn build_snapshot(
    providers: Vec<Arc<dyn Provider>>,
) -> Result<RegistrySnapshot, ProviderValidationError> {
    let mut catalogs = Vec::new();
    let mut action_index = HashMap::new();
    let mut rest_index = HashMap::new();
    let mut cli_index = HashMap::new();
    let mut primitive_index = HashMap::new();
    let mut resources = ResourceIndex::new();
    let mut compiled_validator_count = 0usize;

    for provider in providers {
        let catalog = provider.catalog();
        validate_provider_manifest(&catalog)?;
        resources.register(&*provider, &catalog.provider.name, &catalog.resources)?;
        for tool in &catalog.tools {
            let input_validator = Arc::new(jsonschema::validator_for(&tool.input_schema).map_err(
                |error| {
                    ProviderValidationError::new(
                        "input_schema_invalid",
                        format!("tool `{}` has invalid input_schema: {error}", tool.name),
                    )
                },
            )?);
            compiled_validator_count += 1;
            let output_validator = match &tool.output_schema {
                Some(output_schema) => {
                    let validator =
                        Arc::new(jsonschema::validator_for(output_schema).map_err(|error| {
                            ProviderValidationError::new(
                                "output_schema_invalid",
                                format!("tool `{}` has invalid output_schema: {error}", tool.name),
                            )
                        })?);
                    compiled_validator_count += 1;
                    Some(validator)
                }
                None => None,
            };
            let action = tool.name.clone();
            let entry = ToolEntry {
                provider: catalog.provider.name.clone(),
                action: action.clone(),
                tool: tool.clone(),
                capabilities: catalog.capabilities.clone(),
                input_validator,
                output_validator,
            };
            if action_index.insert(action.clone(), entry).is_some() {
                return Err(ProviderValidationError::new(
                    "duplicate_tool_name",
                    format!("duplicate action `{action}`"),
                ));
            }
            if let Some(rest) = &tool.rest {
                if rest.enabled {
                    let method = rest.method.clone().unwrap_or_else(|| "POST".to_owned());
                    let path = rest
                        .path
                        .clone()
                        .unwrap_or_else(|| format!("/v1/{}", tool.name));
                    if rest_index
                        .insert((method.clone(), path.clone()), tool.name.clone())
                        .is_some()
                    {
                        return Err(ProviderValidationError::new(
                            "duplicate_rest_route",
                            format!("duplicate REST route {method} {path}"),
                        ));
                    }
                }
            }
            if let Some(cli) = &tool.cli {
                if cli.enabled {
                    let command = cli.command.clone().unwrap_or_else(|| tool.name.clone());
                    if cli_index
                        .insert(command.clone(), tool.name.clone())
                        .is_some()
                    {
                        return Err(ProviderValidationError::new(
                            "duplicate_cli_command",
                            format!("duplicate CLI command `{command}`"),
                        ));
                    }
                    for alias in &cli.aliases {
                        if cli_index.insert(alias.clone(), tool.name.clone()).is_some() {
                            return Err(ProviderValidationError::new(
                                "duplicate_cli_command",
                                format!("duplicate CLI alias `{alias}`"),
                            ));
                        }
                    }
                }
            }
        }
        for prompt in &catalog.prompts {
            insert_primitive(&mut primitive_index, "prompt", &prompt.name)?;
        }
        for resource in &catalog.resources {
            insert_primitive(&mut primitive_index, "resource", &resource.name)?;
        }
        for task in &catalog.tasks {
            insert_primitive(&mut primitive_index, "task", &task.name)?;
        }
        for elicitation in &catalog.elicitation {
            insert_primitive(&mut primitive_index, "elicitation", &elicitation.name)?;
        }
        catalogs.push(catalog);
    }
    catalogs.sort_by(|left, right| left.provider.name.cmp(&right.provider.name));
    let fingerprint = fingerprint_catalogs(&catalogs);
    let id = fingerprint.clone();
    let mut action_names = action_index.keys().cloned().collect::<Vec<_>>();
    action_names.sort();
    let openapi_paths = openapi_paths_from_rest_index(&rest_index);
    let cached_catalog_summary = Arc::new(json!({
        "schema_version": 1,
        "provider_fingerprint": fingerprint,
        "actions": action_names.clone(),
    }));
    let cached_palette_manifest = Arc::new(json!({
        "schema_version": 1,
        "provider_fingerprint": fingerprint,
        "commands": action_names,
        "builtins": {
            "file_explorer": false,
            "github": false,
            "browser": false,
            "terminal": false
        }
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
        action_index,
        rest_index,
        cli_index,
        primitive_index,
        exact_resources: resources.exact,
        dynamic_resources: resources.dynamic,
        compiled_validator_count,
        cached_openapi_bytes,
        cached_catalog_summary,
        cached_palette_manifest,
    })
}

fn openapi_paths_from_rest_index(rest_index: &HashMap<(String, String), String>) -> Value {
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

    let mut routes = rest_index
        .iter()
        .map(|((method, path), action)| (method.clone(), path.clone(), action.clone()))
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

fn insert_primitive(
    index: &mut HashMap<String, String>,
    kind: &str,
    name: &str,
) -> Result<(), ProviderValidationError> {
    if index.insert(name.to_owned(), kind.to_owned()).is_some() {
        return Err(ProviderValidationError::new(
            "duplicate_mcp_primitive",
            format!("duplicate MCP primitive `{name}`"),
        ));
    }
    Ok(())
}

fn fingerprint_catalogs(catalogs: &[ProviderCatalog]) -> String {
    let canonical = serde_json::to_vec(catalogs).expect("catalogs serialize");
    let digest = Sha256::digest(canonical);
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

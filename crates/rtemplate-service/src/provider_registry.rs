use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use jsonschema::JSONSchema;
use rtemplate_contracts::{
    actions::scopes_satisfy,
    provider_validation::{validate_provider_manifest, ProviderValidationError},
    providers::{HostCapabilities, ProviderCatalog, ProviderTool},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::{
    capabilities::CapabilityBroker, provider_errors::ProviderError,
    providers::filesystem::FileProviderSource,
};

#[async_trait]
pub trait Provider: Send + Sync {
    fn catalog(&self) -> ProviderCatalog;
    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSurface {
    Mcp,
    Rest,
    Cli,
    Palette,
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
            scopes: vec![rtemplate_contracts::actions::READ_SCOPE.to_owned()],
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
            max_response_bytes: rtemplate_contracts::token_limit::MAX_RESPONSE_BYTES,
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
    input_validator: Arc<JSONSchema>,
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

    pub fn refresh_file_providers(&self) -> Result<Arc<RegistrySnapshot>, ProviderValidationError> {
        let Some(file_source) = &self.file_source else {
            return Ok(self.snapshot());
        };
        let dynamic_providers = file_source.load().map_err(|error| {
            ProviderValidationError::new("provider_file_load_failed", error.to_string())
        })?;
        let mut providers = self.base_providers.iter().cloned().collect::<Vec<_>>();
        providers.extend(dynamic_providers);
        let providers = provider_map(providers)?;
        let snapshot = Arc::new(build_snapshot(providers.values().cloned().collect())?);

        let mut state = self
            .state
            .write()
            .expect("provider registry lock should not be poisoned");
        if state.snapshot.fingerprint == snapshot.fingerprint {
            return Ok(state.snapshot.clone());
        }
        state.providers = providers;
        state.snapshot = snapshot.clone();
        tracing::info!(
            provider_dir = %file_source.root().display(),
            fingerprint = %snapshot.fingerprint,
            "file providers refreshed"
        );
        Ok(snapshot)
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
    let mut compiled_validator_count = 0usize;

    for provider in providers {
        let catalog = provider.catalog();
        validate_provider_manifest(&catalog)?;
        for tool in &catalog.tools {
            let input_validator =
                Arc::new(JSONSchema::compile(&tool.input_schema).map_err(|error| {
                    ProviderValidationError::new(
                        "input_schema_invalid",
                        format!("tool `{}` has invalid input_schema: {error}", tool.name),
                    )
                })?);
            compiled_validator_count += 1;
            let action = tool.name.clone();
            let entry = ToolEntry {
                provider: catalog.provider.name.clone(),
                action: action.clone(),
                tool: tool.clone(),
                capabilities: catalog.capabilities.clone(),
                input_validator,
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
            "info": {"title": "rmcp-template provider API", "version": env!("CARGO_PKG_VERSION")},
            "x-template": {"preferred_rest_style": "direct_routes"},
            "x-rtemplate": {"provider_fingerprint": fingerprint},
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
    format!("sha256:{digest:x}")
}

fn enforce_call(
    entry: &ToolEntry,
    call: &ProviderCall,
    capabilities: &CapabilityBroker,
) -> Result<(), ProviderError> {
    enforce_surface(entry, call)?;
    enforce_scope(entry, call)?;
    enforce_admin(entry, call)?;
    enforce_destructive(entry, call)?;
    enforce_input_limit(entry, call)?;
    enforce_schema(entry, call)?;
    capabilities.authorize(&entry.provider, &entry.action, &entry.capabilities)?;
    Ok(())
}

fn enforce_surface(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    let allowed = match call.surface {
        ProviderSurface::Mcp => entry
            .tool
            .mcp
            .as_ref()
            .map(|mcp| mcp.enabled)
            .unwrap_or(true),
        ProviderSurface::Rest => entry
            .tool
            .rest
            .as_ref()
            .map(|rest| rest.enabled)
            .unwrap_or(false),
        ProviderSurface::Cli => entry
            .tool
            .cli
            .as_ref()
            .map(|cli| cli.enabled)
            .unwrap_or(false),
        ProviderSurface::Palette => entry
            .tool
            .palette
            .as_ref()
            .map(|palette| palette.enabled)
            .unwrap_or(true),
    };
    if allowed {
        return Ok(());
    }
    Err(ProviderError::validation(
        &entry.provider,
        &entry.action,
        "surface_not_exposed",
        format!(
            "action `{}` is not exposed on {:?}",
            entry.action, call.surface
        ),
    ))
}

fn enforce_scope(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    if !matches!(call.auth_mode, ProviderAuthMode::Mounted) {
        return Ok(());
    }
    let Some(scope) = entry.tool.scope.as_deref() else {
        return Ok(());
    };
    if scopes_satisfy(&call.principal.scopes, scope) {
        return Ok(());
    }
    Err(ProviderError::new(
        "insufficient_scope",
        &entry.provider,
        Some(entry.action.clone()),
        format!("action `{}` requires scope `{scope}`", entry.action),
        "Authenticate with a token that includes the required scope.",
    ))
}

fn enforce_admin(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    if !entry.tool.requires_admin || provider_principal_is_admin(&call.principal) {
        return Ok(());
    }
    Err(ProviderError::new(
        "admin_required",
        &entry.provider,
        Some(entry.action.clone()),
        format!("action `{}` requires an admin principal", entry.action),
        "Authenticate with an admin-scoped token and retry.",
    ))
}

fn provider_principal_is_admin(principal: &ProviderPrincipal) -> bool {
    principal
        .scopes
        .iter()
        .any(|scope| scope == "admin" || scope == "example:admin")
}

fn enforce_destructive(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    if !entry.tool.destructive || call.destructive_confirmed {
        return Ok(());
    }
    Err(ProviderError::validation(
        &entry.provider,
        &entry.action,
        "confirmation_required",
        format!(
            "action `{}` is destructive and requires confirmation",
            entry.action
        ),
    ))
}

fn enforce_input_limit(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    let max = entry
        .tool
        .limits
        .as_ref()
        .and_then(|limits| limits.max_input_bytes)
        .unwrap_or(call.limits.max_input_bytes);
    let len = serde_json::to_vec(&call.params)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if len <= max {
        return Ok(());
    }
    Err(ProviderError::validation(
        &entry.provider,
        &entry.action,
        "input_too_large",
        format!("provider input exceeded {max} bytes"),
    ))
}

fn enforce_schema(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    if let Err(errors) = entry.input_validator.validate(&call.params) {
        let details = errors
            .map(|error| format!("{}: {}", error.instance_path, error))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ProviderError::validation(
            &entry.provider,
            &entry.action,
            "input_schema_failed",
            details,
        ));
    }
    Ok(())
}

fn enforce_response_limit(
    entry: &ToolEntry,
    call: &ProviderCall,
    output: &ProviderOutput,
) -> Result<(), ProviderError> {
    let max = entry
        .tool
        .limits
        .as_ref()
        .and_then(|limits| limits.max_response_bytes)
        .unwrap_or(call.limits.max_response_bytes);
    let len = serde_json::to_vec(&output.value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if len <= max {
        return Ok(());
    }
    Err(ProviderError::new(
        "response_too_large",
        &entry.provider,
        Some(entry.action.clone()),
        format!("provider response exceeded {max} bytes"),
        "Reduce the response size or add paging before exposing this provider action.",
    ))
}

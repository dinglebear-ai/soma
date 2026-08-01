//! Wasmtime-backed provider runtime for both the legacy core-Wasm ABI and the
//! versioned Soma component-model ABI.

use std::{
    collections::BTreeMap,
    io::Read,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use serde_json::Value;
use soma_provider_core::{
    Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderOutput, ProviderTool,
};
use tokio::time::timeout;
use wasmtime::{
    Cache, CacheConfig, Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, Linker},
};

#[cfg(test)]
use crate::wasm_memory::public_ip as component_public_ip;
use crate::{
    broker_state::BrokerStateStore,
    sidecar::execution_payload,
    wasm_limits::WasmRuntimeLimits,
    wasm_memory::{read_memory, typed, write_memory},
};

mod artifact;
mod direct;
mod host;
mod runtime_support;
use artifact::{artifact_digest, compile_artifact, read_artifact};
pub use direct::{
    PreparedComponentArtifact, invoke_authorized_component_artifact,
    invoke_component_artifact_async, invoke_component_artifact_before_async,
    invoke_prepared_component_artifact_before_async, prepare_component_artifact_before,
    verify_component_artifact, verify_component_artifact_before,
};
use host::{component_diagnostic, component_linker};
#[cfg(test)]
use host::{component_state_get, component_state_put};
use runtime_support::{
    EpochTicker, WasmArtifactCache, acquire_execution, component_broker,
    component_forbidden_header, component_metric, component_progress, component_remaining,
    component_require_scope, resolve_component_hosts,
};

const MAX_WASM_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_ARTIFACT_COMPILE_TIMEOUT_SECS: u64 = 30;
const COMPONENTIZE_ARTIFACT_COMPILE_TIMEOUT_SECS: u64 = 600;
const COMPONENTIZE_MARKER_NAME: &[u8] = b"soma.componentize-py.v1";
const VERIFY_MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const VERIFY_MAX_TABLE_ELEMENTS: usize = 10_000;
const VERIFY_MAX_INSTANCES: usize = 16;
const WASMTIME_CACHE_FILE_COUNT_SOFT_LIMIT: u64 = 256;
const WASMTIME_CACHE_BYTES_SOFT_LIMIT: u64 = 2_147_483_648;

/// Append Soma's deterministic experimental componentize marker custom section.
pub fn mark_componentize_artifact(bytes: &mut Vec<u8>) -> Result<(), String> {
    if bytes.len() < 8 || &bytes[..4] != b"\0asm" {
        return Err("componentize artifact is not a WebAssembly binary".to_owned());
    }
    let marker = componentize_marker_section()?;
    if bytes.ends_with(&marker) {
        return Ok(());
    }
    if bytes.len().saturating_add(marker.len()) > MAX_WASM_ARTIFACT_BYTES {
        return Err(format!(
            "marked componentize artifact exceeds {MAX_WASM_ARTIFACT_BYTES} bytes"
        ));
    }
    bytes.extend_from_slice(&marker);
    Ok(())
}

/// Return whether a WebAssembly artifact carries Soma's componentize marker.
#[must_use]
pub fn is_componentize_artifact(bytes: &[u8]) -> bool {
    componentize_marker_section()
        .map(|marker| bytes.ends_with(&marker))
        .unwrap_or(false)
}

fn artifact_compile_timeout(bytes: &[u8]) -> Duration {
    let seconds = if is_componentize_artifact(bytes) {
        COMPONENTIZE_ARTIFACT_COMPILE_TIMEOUT_SECS
    } else {
        DEFAULT_ARTIFACT_COMPILE_TIMEOUT_SECS
    };
    Duration::from_secs(seconds)
}

fn componentize_marker_section() -> Result<Vec<u8>, String> {
    let name_len = u32::try_from(COMPONENTIZE_MARKER_NAME.len())
        .map_err(|_| "componentize marker name is too large".to_owned())?;
    let mut body = Vec::with_capacity(COMPONENTIZE_MARKER_NAME.len() + 5);
    push_u32_leb(&mut body, name_len);
    body.extend_from_slice(COMPONENTIZE_MARKER_NAME);
    let body_len = u32::try_from(body.len())
        .map_err(|_| "componentize marker section is too large".to_owned())?;
    let mut section = Vec::with_capacity(body.len() + 6);
    section.push(0);
    push_u32_leb(&mut section, body_len);
    section.extend_from_slice(&body);
    Ok(section)
}

fn push_u32_leb(output: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

#[derive(Clone)]
pub struct WasmProvider {
    path: PathBuf,
    catalog: ProviderCatalog,
    runtime: Result<PreparedWasmProvider, String>,
}

#[derive(Clone)]
struct PreparedWasmProvider {
    runtime: Arc<WasmRuntime>,
    artifact: Arc<WasmArtifact>,
    digest: String,
    componentize: bool,
}

impl WasmProvider {
    pub fn new(path: PathBuf, catalog: ProviderCatalog) -> Self {
        let runtime = shared_wasm_runtime().and_then(|runtime| {
            let bytes = read_artifact(&path)?;
            let digest = artifact_digest(&bytes);
            let componentize = is_componentize_artifact(&bytes);
            let artifact =
                runtime.artifact(&bytes, Instant::now() + artifact_compile_timeout(&bytes))?;
            Ok(PreparedWasmProvider {
                runtime,
                artifact,
                digest,
                componentize,
            })
        });
        Self {
            path,
            catalog,
            runtime,
        }
    }

    pub fn arc(path: PathBuf, catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::new(path, catalog))
    }
}

#[async_trait]
impl Provider for WasmProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let tool = self.tool(&call)?.clone();
        let provider = self.catalog.provider.name.clone();
        let action = call.action.clone();
        let source = self.path.display().to_string();
        let prepared = self.runtime.clone().map_err(|error| {
            ProviderError::execution(&provider, action.clone(), error)
                .with_provider_kind("wasm")
                .with_source(source.clone())
                .with_phase("runtime-initialization")
        })?;
        let current_digest = read_artifact(&self.path)
            .map(|bytes| artifact_digest(&bytes))
            .map_err(|error| {
                ProviderError::execution(&provider, action.clone(), error)
                    .with_provider_kind("wasm")
                    .with_source(source.clone())
                    .with_phase("artifact-verification")
            })?;
        if current_digest != prepared.digest {
            return Err(ProviderError::execution(
                &provider,
                action.clone(),
                "WASM artifact changed after provider activation",
            )
            .with_provider_kind("wasm")
            .with_source(source)
            .with_phase("artifact-verification"));
        }
        let capabilities = self.catalog.capabilities.clone();
        let context = call.context.clone();
        let input = execution_payload(&call).map_err(|error| {
            ProviderError::execution(&provider, call.action.clone(), error)
                .with_provider_kind("wasm")
                .with_source(source.clone())
                .with_phase("input-serialization")
        })?;
        let limits =
            WasmRuntimeLimits::from_tool(&tool).with_componentize_minimums(prepared.componentize);
        if input.len() > limits.max_input_bytes {
            return Err(ProviderError::validation(
                provider,
                call.action,
                "wasm_input_too_large",
                format!("WASM input exceeds {} bytes", limits.max_input_bytes),
            )
            .with_provider_kind("wasm")
            .with_source(source)
            .with_phase("input-validation"));
        }

        let timeout_ms = limits.timeout_ms;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let resolved_hosts = resolve_component_hosts(&capabilities, deadline)
            .await
            .map_err(|error| {
                ProviderError::execution(&provider, action.clone(), error)
                    .with_provider_kind("wasm")
                    .with_source(source.clone())
                    .with_phase("network-resolution")
            })?;
        let task = tokio::task::spawn_blocking(move || {
            prepared.runtime.run_artifact(
                prepared.artifact,
                WasmInvocation {
                    input: &input,
                    limits,
                    capabilities: &capabilities,
                    context: &context,
                    resolved_hosts,
                    deadline,
                },
            )
        });
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or(Duration::ZERO);
        let task_result = timeout(remaining, task)
            .await
            .map_err(|_| {
                ProviderError::new(
                    "wasm_provider_timeout",
                    &provider,
                    Some(action.clone()),
                    format!("WASM provider exceeded {timeout_ms}ms timeout"),
                    "Increase tool.limits.timeout_ms or fix the WASM provider.",
                )
                .with_provider_kind("wasm")
                .with_source(source.clone())
                .with_phase("execution")
            })?
            .map_err(|error| {
                ProviderError::execution(&provider, action.clone(), error)
                    .with_provider_kind("wasm")
                    .with_source(source.clone())
                    .with_phase("execution")
            })?;
        if Instant::now() >= deadline {
            return Err(ProviderError::new(
                "wasm_provider_timeout",
                &provider,
                Some(action.clone()),
                format!("WASM provider exceeded {timeout_ms}ms timeout"),
                "Increase tool.limits.timeout_ms or fix the WASM provider.",
            )
            .with_provider_kind("wasm")
            .with_source(source.clone())
            .with_phase("execution"));
        }
        let output = task_result.map_err(|error| {
            ProviderError::execution(&provider, action.clone(), error)
                .with_provider_kind("wasm")
                .with_source(source.clone())
                .with_phase("execution")
        })?;

        let value = serde_json::from_slice(&output).map_err(|error| {
            ProviderError::validation(
                &provider,
                &action,
                "wasm_invalid_json_output",
                error.to_string(),
            )
            .with_provider_kind("wasm")
            .with_source(source)
            .with_phase("output-validation")
        })?;
        Ok(ProviderOutput::json(value))
    }

    fn runtime_status(&self) -> Option<Value> {
        Some(match &self.runtime {
            Ok(prepared) => serde_json::json!({
                "kind": "wasm",
                "ready": true,
                "artifact": self.path,
                "artifact_sha256": prepared.digest,
            }),
            Err(error) => serde_json::json!({
                "kind": "wasm",
                "ready": false,
                "artifact": self.path,
                "error": error,
            }),
        })
    }
}

impl WasmProvider {
    fn tool(&self, call: &ProviderCall) -> Result<&ProviderTool, ProviderError> {
        self.catalog
            .tools
            .iter()
            .find(|tool| tool.name == call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unknown_wasm_action",
                    format!("WASM provider has no action `{}`", call.action),
                )
            })
    }
}

enum WasmArtifact {
    Core(Module),
    Component(Component),
}

pub(super) struct WasmStoreState {
    limits: StoreLimits,
    capabilities: soma_provider_core::HostCapabilities,
    state: Result<Arc<BrokerStateStore>, String>,
    context: soma_provider_core::ProviderInvocationContext,
    resolved_hosts: BTreeMap<String, Vec<IpAddr>>,
    deadline: Instant,
    wasi: wasmtime_wasi::WasiCtx,
    table: wasmtime::component::ResourceTable,
}

impl wasmtime_wasi::WasiView for WasmStoreState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

struct WasmRuntime {
    engine: Engine,
    cache: Mutex<WasmArtifactCache>,
    state: Result<Arc<BrokerStateStore>, String>,
    _ticker: EpochTicker,
}

struct WasmInvocation<'a> {
    input: &'a [u8],
    limits: WasmRuntimeLimits,
    capabilities: &'a soma_provider_core::HostCapabilities,
    context: &'a soma_provider_core::ProviderInvocationContext,
    resolved_hosts: BTreeMap<String, Vec<IpAddr>>,
    deadline: Instant,
}

fn shared_wasm_runtime() -> Result<Arc<WasmRuntime>, String> {
    static RUNTIME: OnceLock<Result<Arc<WasmRuntime>, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| WasmRuntime::new().map(Arc::new))
        .clone()
}

fn wasmtime_cache_config() -> CacheConfig {
    let mut config = CacheConfig::new();
    config
        .with_file_count_soft_limit(WASMTIME_CACHE_FILE_COUNT_SOFT_LIMIT)
        .with_files_total_size_soft_limit(WASMTIME_CACHE_BYTES_SOFT_LIMIT)
        .with_file_count_limit_percent_if_deleting(75)
        .with_files_total_size_limit_percent_if_deleting(75);
    config
}

fn wasmtime_cache() -> Result<Cache, String> {
    Cache::new(wasmtime_cache_config())
        .map_err(|error| format!("failed to initialize Wasmtime cache: {error}"))
}

impl WasmRuntime {
    fn new() -> Result<Self, String> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.wasm_component_model(true);
        config.cache(Some(wasmtime_cache()?));
        let engine = Engine::new(&config).map_err(|error| error.to_string())?;
        let ticker = EpochTicker::start(engine.clone())?;
        Ok(Self {
            engine,
            cache: Mutex::new(WasmArtifactCache::default()),
            state: BrokerStateStore::configured(),
            _ticker: ticker,
        })
    }

    fn artifact(&self, bytes: &[u8], deadline: Instant) -> Result<Arc<WasmArtifact>, String> {
        let digest = artifact_digest(bytes);
        let cell = self
            .cache
            .lock()
            .map_err(|_| "WASM cache lock is poisoned".to_owned())?
            .cell(digest, bytes.len().saturating_mul(8));
        let result = cell.get_or_compile(deadline, || compile_artifact(self, bytes, deadline));
        self.cache
            .lock()
            .map_err(|_| "WASM cache lock is poisoned".to_owned())?
            .prune();
        result
    }

    fn verify_prepared_component(
        &self,
        artifact: &Arc<WasmArtifact>,
        componentize: bool,
        deadline: Instant,
    ) -> Result<(), String> {
        let limits = WasmRuntimeLimits {
            timeout_ms: 1_000,
            max_input_bytes: 0,
            max_output_bytes: 0,
            fuel: 100_000,
            max_memory_bytes: VERIFY_MAX_MEMORY_BYTES,
            max_table_elements: VERIFY_MAX_TABLE_ELEMENTS,
            max_instances: VERIFY_MAX_INSTANCES,
        }
        .with_componentize_minimums(componentize);
        let _execution = acquire_execution(deadline, limits.max_memory_bytes)?;
        let WasmArtifact::Component(component) = artifact.as_ref() else {
            return Err("artifact is core Wasm, not a component".to_owned());
        };
        let mut store = self.store(
            limits,
            &soma_provider_core::HostCapabilities::default(),
            &soma_provider_core::ProviderInvocationContext {
                request_id: "component-verification".to_owned(),
                actor_id: Some("soma-verifier".to_owned()),
                actor_scopes: vec!["soma:read".to_owned()],
                ..Default::default()
            },
            BTreeMap::new(),
            deadline,
        )?;
        let linker = component_linker(&self.engine)?;
        let instance = linker
            .instantiate(&mut store, component)
            .map_err(|error| error.to_string())?;
        instance
            .get_typed_func::<(String,), (Result<String, String>,)>(&mut store, "invoke")
            .map(|_| ())
            .map_err(|error| format!("component does not implement soma:provider@1.0.0: {error}"))
    }

    #[cfg(test)]
    fn verify_component(&self, bytes: &[u8], deadline: Instant) -> Result<(), String> {
        let componentize = is_componentize_artifact(bytes);
        let artifact = self.artifact(bytes, deadline)?;
        self.verify_prepared_component(&artifact, componentize, deadline)
    }

    #[cfg(test)]
    fn run(
        &self,
        path: &std::path::Path,
        invocation: WasmInvocation<'_>,
    ) -> Result<Vec<u8>, String> {
        let bytes = read_artifact(path)?;
        let artifact = self.artifact(&bytes, invocation.deadline)?;
        self.run_artifact(artifact, invocation)
    }

    fn run_artifact(
        &self,
        artifact: Arc<WasmArtifact>,
        invocation: WasmInvocation<'_>,
    ) -> Result<Vec<u8>, String> {
        let WasmInvocation {
            input,
            limits,
            capabilities,
            context,
            resolved_hosts,
            deadline,
        } = invocation;
        let _execution = acquire_execution(deadline, limits.max_memory_bytes)?;
        let mut store = self.store(limits, capabilities, context, resolved_hosts, deadline)?;
        match artifact.as_ref() {
            WasmArtifact::Core(module) => run_core_wasm(&mut store, module, input, limits),
            WasmArtifact::Component(component) => {
                run_component_wasm(&self.engine, &mut store, component, input, limits)
            }
        }
    }

    fn store(
        &self,
        limits: WasmRuntimeLimits,
        capabilities: &soma_provider_core::HostCapabilities,
        context: &soma_provider_core::ProviderInvocationContext,
        resolved_hosts: BTreeMap<String, Vec<IpAddr>>,
        deadline: Instant,
    ) -> Result<Store<WasmStoreState>, String> {
        let store_limits = StoreLimitsBuilder::new()
            .memory_size(limits.max_memory_bytes)
            .table_elements(limits.max_table_elements)
            .instances(limits.max_instances)
            .memories(4)
            .tables(4)
            .build();
        let mut store = Store::new(
            &self.engine,
            WasmStoreState {
                limits: store_limits,
                capabilities: capabilities.clone(),
                state: self.state.clone(),
                context: context.clone(),
                resolved_hosts,
                deadline,
                wasi: {
                    let mut builder = wasmtime_wasi::WasiCtxBuilder::new();
                    builder.allow_blocking_current_thread(true);
                    builder.build()
                },
                table: wasmtime::component::ResourceTable::new(),
            },
        );
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(limits.fuel)
            .map_err(|error| error.to_string())?;
        let remaining_ms = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or(Duration::ZERO)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        store.set_epoch_deadline(remaining_ms.div_ceil(10).max(1));
        store.epoch_deadline_trap();
        Ok(store)
    }
}

fn run_core_wasm(
    store: &mut Store<WasmStoreState>,
    module: &Module,
    input: &[u8],
    limits: WasmRuntimeLimits,
) -> Result<Vec<u8>, String> {
    let instance = Instance::new(&mut *store, module, &[]).map_err(|error| error.to_string())?;
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| "WASM provider must export memory".to_owned())?;
    let input_alloc = typed::<i32, i32>(&instance, &mut *store, "soma_input_alloc")?;
    let input_ptr_fn = typed::<(), i32>(&instance, &mut *store, "soma_input_ptr")?;
    let call_fn = typed::<(), i32>(&instance, &mut *store, "soma_call")?;
    let output_ptr_fn = typed::<(), i32>(&instance, &mut *store, "soma_output_ptr")?;
    let output_len_fn = typed::<(), i32>(&instance, &mut *store, "soma_output_len")?;

    let ptr = input_alloc
        .call(&mut *store, input.len() as i32)
        .map_err(|error| error.to_string())? as usize;
    let input_ptr = input_ptr_fn
        .call(&mut *store, ())
        .map_err(|error| error.to_string())? as usize;
    if ptr != input_ptr {
        return Err("WASM provider input pointer mismatch".to_owned());
    }
    write_memory(&memory, &mut *store, ptr, input)?;
    let status = call_fn
        .call(&mut *store, ())
        .map_err(|error| error.to_string())?;
    if status != 0 {
        return Err(format!("WASM provider returned non-zero status {status}"));
    }
    let output_ptr = output_ptr_fn
        .call(&mut *store, ())
        .map_err(|error| error.to_string())? as usize;
    let output_len = output_len_fn
        .call(&mut *store, ())
        .map_err(|error| error.to_string())? as usize;
    if output_len > limits.max_output_bytes {
        return Err(format!(
            "WASM provider output exceeds {} bytes",
            limits.max_output_bytes
        ));
    }
    read_memory(&memory, &mut *store, output_ptr, output_len)
}

fn run_component_wasm(
    engine: &Engine,
    store: &mut Store<WasmStoreState>,
    component: &Component,
    input: &[u8],
    limits: WasmRuntimeLimits,
) -> Result<Vec<u8>, String> {
    let linker = component_linker(engine)?;
    let instance = linker
        .instantiate(&mut *store, component)
        .map_err(|error| error.to_string())?;
    let invoke = instance
        .get_typed_func::<(String,), (Result<String, String>,)>(&mut *store, "invoke")
        .map_err(|error| error.to_string())?;
    let input = String::from_utf8(input.to_vec())
        .map_err(|_| "component input must be UTF-8 JSON".to_owned())?;
    let (result,) = invoke
        .call(&mut *store, (input,))
        .map_err(|error| format!("{error:#}"))?;
    let output = result.map_err(|error| format!("component provider failed: {error}"))?;
    if output.len() > limits.max_output_bytes {
        return Err(format!(
            "WASM provider output exceeds {} bytes",
            limits.max_output_bytes
        ));
    }
    Ok(output.into_bytes())
}

#[cfg(test)]
#[path = "wasm_tests.rs"]
mod tests;

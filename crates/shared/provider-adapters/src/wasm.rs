//! Wasmtime-backed provider runtime for both the legacy core-Wasm ABI and the
//! versioned Soma component-model ABI.

use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    net::ToSocketAddrs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use serde_json::Value;
use soma_provider_core::{
    Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderOutput, ProviderTool,
};
use tokio::time::timeout;
use wasmtime::{
    Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, Linker},
};

use crate::{
    sidecar::execution_payload,
    wasm_limits::WasmRuntimeLimits,
    wasm_memory::{public_ip as component_public_ip, read_memory, typed, write_memory},
};

#[derive(Clone)]
pub struct WasmProvider {
    path: PathBuf,
    catalog: ProviderCatalog,
    runtime: Arc<WasmRuntime>,
}

impl WasmProvider {
    pub fn new(path: PathBuf, catalog: ProviderCatalog) -> Self {
        Self {
            path,
            catalog,
            runtime: Arc::new(WasmRuntime::new().expect("Wasmtime runtime configuration is valid")),
        }
    }

    pub fn arc(path: PathBuf, catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::new(path, catalog))
    }
}

/// Validate and invoke a component artifact directly. Graduation tooling uses
/// this seam to replay recorded Python fixtures before activation.
pub fn invoke_component_artifact(
    path: &std::path::Path,
    input: &Value,
    capabilities: &soma_provider_core::HostCapabilities,
) -> Result<Value, String> {
    let bytes = serde_json::to_vec(input).map_err(|error| error.to_string())?;
    let runtime = WasmRuntime::new()?;
    let output = runtime.run(
        path,
        &bytes,
        WasmRuntimeLimits {
            timeout_ms: 5_000,
            max_input_bytes: 64 * 1024,
            max_output_bytes: 256 * 1024,
            fuel: 1_000_000,
            max_memory_bytes: 64 * 1024 * 1024,
            max_table_elements: 10_000,
            max_instances: 16,
        },
        capabilities,
    )?;
    serde_json::from_slice(&output).map_err(|error| error.to_string())
}

/// Validate that `path` contains a component-model artifact, not merely a
/// legacy core-Wasm module.
pub fn verify_component_artifact(path: &std::path::Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let runtime = WasmRuntime::new()?;
    runtime.verify_component(&bytes)
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
        let path = self.path.clone();
        let runtime = self.runtime.clone();
        let capabilities = self.catalog.capabilities.clone();
        let input = execution_payload(&call).map_err(|error| {
            ProviderError::execution(&provider, call.action.clone(), error)
                .with_provider_kind("wasm")
                .with_source(source.clone())
                .with_phase("input-serialization")
        })?;
        let limits = WasmRuntimeLimits::from_tool(&tool);
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
        let task =
            tokio::task::spawn_blocking(move || runtime.run(&path, &input, limits, &capabilities));
        let output = timeout(Duration::from_millis(timeout_ms), task)
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
            })?
            .map_err(|error| {
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
    state: Arc<Mutex<BTreeMap<String, Value>>>,
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
    cache: Mutex<BTreeMap<String, Arc<WasmArtifact>>>,
    state: Arc<Mutex<BTreeMap<String, Value>>>,
}

impl WasmRuntime {
    fn new() -> Result<Self, String> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.wasm_component_model(true);
        let engine = Engine::new(&config).map_err(|error| error.to_string())?;
        let ticker = engine.clone();
        std::thread::Builder::new()
            .name("soma-wasm-epoch".to_owned())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(10));
                    ticker.increment_epoch();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            engine,
            cache: Mutex::new(BTreeMap::new()),
            state: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    fn artifact(&self, bytes: &[u8]) -> Result<Arc<WasmArtifact>, String> {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if let Some(artifact) = self
            .cache
            .lock()
            .map_err(|_| "WASM cache lock is poisoned".to_owned())?
            .get(&digest)
            .cloned()
        {
            return Ok(artifact);
        }
        let artifact = Component::from_binary(&self.engine, bytes)
            .map(WasmArtifact::Component)
            .or_else(|_| Module::from_binary(&self.engine, bytes).map(WasmArtifact::Core))
            .map_err(|error| error.to_string())?;
        let artifact = Arc::new(artifact);
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "WASM cache lock is poisoned".to_owned())?;
        if cache.len() >= 32
            && let Some(oldest) = cache.keys().next().cloned()
        {
            cache.remove(&oldest);
        }
        cache.insert(digest, artifact.clone());
        Ok(artifact)
    }

    fn verify_component(&self, bytes: &[u8]) -> Result<(), String> {
        let component =
            Component::from_binary(&self.engine, bytes).map_err(|error| error.to_string())?;
        let mut store = self.store(
            WasmRuntimeLimits {
                timeout_ms: 1_000,
                max_input_bytes: 0,
                max_output_bytes: 0,
                fuel: 100_000,
                max_memory_bytes: 16 * 1024 * 1024,
                max_table_elements: 1_000,
                max_instances: 8,
            },
            &soma_provider_core::HostCapabilities::default(),
        )?;
        let linker = component_linker(&self.engine)?;
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|error| error.to_string())?;
        instance
            .get_typed_func::<(String,), (Result<String, String>,)>(&mut store, "invoke")
            .map(|_| ())
            .map_err(|error| format!("component does not implement soma:provider@1.0.0: {error}"))
    }

    fn run(
        &self,
        path: &std::path::Path,
        input: &[u8],
        limits: WasmRuntimeLimits,
        capabilities: &soma_provider_core::HostCapabilities,
    ) -> Result<Vec<u8>, String> {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        let artifact = self.artifact(&bytes)?;
        let mut store = self.store(limits, capabilities)?;
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
        store.set_epoch_deadline(limits.timeout_ms.div_ceil(10).max(1));
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
        .map_err(|error| error.to_string())?;
    let output = result.map_err(|error| format!("component provider failed: {error}"))?;
    if output.len() > limits.max_output_bytes {
        return Err(format!(
            "WASM provider output exceeds {} bytes",
            limits.max_output_bytes
        ));
    }
    Ok(output.into_bytes())
}

fn component_linker(engine: &Engine) -> Result<Linker<WasmStoreState>, String> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|error| error.to_string())?;
    let mut host = linker
        .instance("soma:provider/host@1.0.0")
        .map_err(|error| error.to_string())?;
    host.func_wrap("http", |mut store, (request,): (String,)| {
        Ok((component_http(store.data_mut(), &request),))
    })
    .map_err(|error| error.to_string())?;
    host.func_wrap("secret", |mut store, (name,): (String,)| {
        Ok((component_secret(store.data_mut(), &name),))
    })
    .map_err(|error| error.to_string())?;
    host.func_wrap("state-get", |mut store, (key,): (String,)| {
        Ok((component_state_get(store.data_mut(), &key),))
    })
    .map_err(|error| error.to_string())?;
    host.func_wrap("state-put", |mut store, (key, value): (String, String)| {
        Ok((component_state_put(store.data_mut(), &key, &value),))
    })
    .map_err(|error| error.to_string())?;
    host.func_wrap(
        "log",
        |mut store, (level, message, fields): (String, String, String)| {
            Ok((component_log(store.data_mut(), &level, &message, &fields),))
        },
    )
    .map_err(|error| error.to_string())?;
    host.func_wrap(
        "metric",
        |mut store, (name, value, attributes): (String, f64, String)| {
            Ok((component_metric(
                store.data_mut(),
                &name,
                value,
                &attributes,
            ),))
        },
    )
    .map_err(|error| error.to_string())?;
    host.func_wrap(
        "progress",
        |mut store, (current, total, message): (u64, Option<u64>, Option<String>)| {
            Ok((component_progress(
                store.data_mut(),
                current,
                total,
                message.as_deref(),
            ),))
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(linker)
}

fn component_broker(
    state: &WasmStoreState,
) -> Result<&soma_provider_core::BrokerCapability, String> {
    state
        .capabilities
        .broker
        .as_ref()
        .filter(|capability| capability.enabled)
        .ok_or_else(|| "broker capability not declared".to_owned())
}

fn component_http(state: &WasmStoreState, request: &str) -> Result<String, String> {
    let network = state
        .capabilities
        .network
        .as_ref()
        .filter(|capability| capability.enabled)
        .ok_or_else(|| "network capability not declared".to_owned())?;
    let request: Value =
        serde_json::from_str(request).map_err(|_| "HTTP request JSON is invalid".to_owned())?;
    let raw_url = request
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "HTTP request URL is required".to_owned())?;
    let url = url::Url::parse(raw_url).map_err(|_| "HTTP request URL is invalid".to_owned())?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("component HTTP requires HTTPS without URL credentials".to_owned());
    }
    let hostname = url
        .host_str()
        .ok_or_else(|| "HTTP request host is required".to_owned())?;
    if !network
        .allowed_hosts
        .iter()
        .any(|allowed| allowed == hostname)
    {
        return Err("HTTP request host is not declared".to_owned());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = (hostname, port)
        .to_socket_addrs()
        .map_err(|_| "HTTP host resolution failed".to_owned())?
        .collect::<Vec<_>>();
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !component_public_ip(address.ip()))
    {
        return Err("HTTP host resolved to a non-public address".to_owned());
    }
    let mut client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .https_only(true);
    for address in addresses {
        client = client.resolve(hostname, address);
    }
    let client = client.build().map_err(|error| error.to_string())?;
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .parse::<reqwest::Method>()
        .map_err(|_| "HTTP method is invalid".to_owned())?;
    let mut outbound = client.request(method, url);
    if let Some(headers) = request.get("headers").and_then(Value::as_object) {
        for (name, value) in headers {
            if component_forbidden_header(name) {
                return Err("HTTP header is controlled by the component host".to_owned());
            }
            let value = value
                .as_str()
                .ok_or_else(|| "HTTP header values must be strings".to_owned())?;
            outbound = outbound.header(name, value);
        }
    }
    if let Some(body) = request.get("body").and_then(Value::as_str) {
        outbound = outbound.body(body.to_owned());
    }
    let response = outbound
        .send()
        .map_err(|_| "HTTP request failed".to_owned())?;
    if response.status().is_redirection() {
        return Err("HTTP redirects are not followed by the component host".to_owned());
    }
    let status = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|length| length > 256 * 1024)
    {
        return Err("HTTP response exceeds component host limit".to_owned());
    }
    let mut body = Vec::new();
    response
        .take(256 * 1024 + 1)
        .read_to_end(&mut body)
        .map_err(|_| "HTTP response body failed".to_owned())?;
    if body.len() > 256 * 1024 {
        return Err("HTTP response exceeds component host limit".to_owned());
    }
    serde_json::to_string(&serde_json::json!({
        "status": status,
        "body": String::from_utf8_lossy(&body),
    }))
    .map_err(|error| error.to_string())
}

fn component_forbidden_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "connection"
            | "content-length"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn component_secret(state: &WasmStoreState, name: &str) -> Result<String, String> {
    let capability = component_broker(state)?;
    if !capability
        .secret_names
        .iter()
        .any(|allowed| allowed == name)
    {
        return Err("secret name is not declared".to_owned());
    }
    let normalized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    std::env::var(format!("SOMA_COMPONENT_SECRET_{normalized}"))
        .map_err(|_| "declared secret is unavailable".to_owned())
}

fn component_state_get(state: &WasmStoreState, key: &str) -> Result<String, String> {
    let namespace = component_broker(state)?
        .state_namespace
        .as_ref()
        .ok_or_else(|| "state namespace is not declared".to_owned())?;
    let values = state
        .state
        .lock()
        .map_err(|_| "component state lock is poisoned".to_owned())?;
    serde_json::to_string(
        values
            .get(&format!("{namespace}\0{key}"))
            .unwrap_or(&Value::Null),
    )
    .map_err(|error| error.to_string())
}

fn component_state_put(state: &WasmStoreState, key: &str, value: &str) -> Result<(), String> {
    let capability = component_broker(state)?;
    if !capability.state_write {
        return Err("state write capability is not declared".to_owned());
    }
    let namespace = capability
        .state_namespace
        .as_ref()
        .ok_or_else(|| "state namespace is not declared".to_owned())?;
    let value: Value =
        serde_json::from_str(value).map_err(|_| "state value JSON is invalid".to_owned())?;
    if value.to_string().len() > 64 * 1024 {
        return Err("state value exceeds component host limit".to_owned());
    }
    state
        .state
        .lock()
        .map_err(|_| "component state lock is poisoned".to_owned())?
        .insert(format!("{namespace}\0{key}"), value);
    Ok(())
}

fn component_log(
    state: &WasmStoreState,
    level: &str,
    message: &str,
    fields: &str,
) -> Result<(), String> {
    if !component_broker(state)?.logging {
        return Err("structured logging capability is not declared".to_owned());
    }
    tracing::info!(
        provider_level = level,
        message = %message.chars().take(1024).collect::<String>(),
        fields = %fields.chars().take(1024).collect::<String>(),
        "component provider log"
    );
    Ok(())
}

fn component_metric(
    state: &WasmStoreState,
    name: &str,
    value: f64,
    attributes: &str,
) -> Result<(), String> {
    if !component_broker(state)?.metrics {
        return Err("metrics capability is not declared".to_owned());
    }
    if !value.is_finite() {
        return Err("metric value must be finite".to_owned());
    }
    tracing::info!(
        metric = %name.chars().take(128).collect::<String>(),
        value,
        attributes = %attributes.chars().take(1024).collect::<String>(),
        "component provider metric"
    );
    Ok(())
}

fn component_progress(
    state: &WasmStoreState,
    current: u64,
    total: Option<u64>,
    message: Option<&str>,
) -> Result<(), String> {
    if !component_broker(state)?.progress {
        return Err("progress capability is not declared".to_owned());
    }
    tracing::info!(
        current,
        ?total,
        message = %message.unwrap_or_default().chars().take(1024).collect::<String>(),
        "component provider progress"
    );
    Ok(())
}

#[cfg(test)]
#[path = "wasm_tests.rs"]
mod tests;

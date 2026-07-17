//! The generic WASM provider kind: runs a drop-in `.wasm` module's exported
//! `soma_call` ABI in a fuel-bounded wasmtime sandbox. Ported unchanged
//! (beyond product-type swaps) from `soma-service::providers::wasm`.

use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::Value;
use soma_provider_core::{
    Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderOutput, ProviderTool,
};
use tokio::time::timeout;
use wasmtime::{Config, Engine, Instance, Memory, Module, Store, TypedFunc};

use crate::sidecar::execution_payload;

#[derive(Clone)]
pub struct WasmProvider {
    path: PathBuf,
    catalog: ProviderCatalog,
}

impl WasmProvider {
    pub fn new(path: PathBuf, catalog: ProviderCatalog) -> Self {
        Self { path, catalog }
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
        let path = self.path.clone();
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
        let task = tokio::task::spawn_blocking(move || run_wasm(&path, &input, limits));
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

#[derive(Debug, Clone, Copy)]
struct WasmRuntimeLimits {
    timeout_ms: u64,
    max_input_bytes: usize,
    max_output_bytes: usize,
    fuel: u64,
}

impl WasmRuntimeLimits {
    fn from_tool(tool: &ProviderTool) -> Self {
        let meta = tool.meta.get("wasm");
        Self {
            timeout_ms: tool
                .limits
                .as_ref()
                .and_then(|limits| limits.timeout_ms)
                .or_else(|| {
                    meta.and_then(|value| value.get("timeout_ms"))
                        .and_then(Value::as_u64)
                })
                .unwrap_or(5_000),
            max_input_bytes: tool
                .limits
                .as_ref()
                .and_then(|limits| limits.max_input_bytes)
                .unwrap_or(64 * 1024),
            max_output_bytes: tool
                .limits
                .as_ref()
                .and_then(|limits| limits.max_response_bytes)
                .unwrap_or(256 * 1024),
            fuel: meta
                .and_then(|value| value.get("fuel"))
                .and_then(Value::as_u64)
                .unwrap_or(1_000_000),
        }
    }
}

fn run_wasm(
    path: &std::path::Path,
    input: &[u8],
    limits: WasmRuntimeLimits,
) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mut config = Config::new();
    config.consume_fuel(true);
    let engine = Engine::new(&config).map_err(|error| error.to_string())?;
    let module = Module::from_binary(&engine, &bytes).map_err(|error| error.to_string())?;
    let mut store = Store::new(&engine, ());
    store
        .set_fuel(limits.fuel)
        .map_err(|error| error.to_string())?;
    let instance = Instance::new(&mut store, &module, &[]).map_err(|error| error.to_string())?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| "WASM provider must export memory".to_owned())?;
    let input_alloc = typed::<i32, i32>(&instance, &mut store, "soma_input_alloc")?;
    let input_ptr_fn = typed::<(), i32>(&instance, &mut store, "soma_input_ptr")?;
    let call_fn = typed::<(), i32>(&instance, &mut store, "soma_call")?;
    let output_ptr_fn = typed::<(), i32>(&instance, &mut store, "soma_output_ptr")?;
    let output_len_fn = typed::<(), i32>(&instance, &mut store, "soma_output_len")?;

    let ptr = input_alloc
        .call(&mut store, input.len() as i32)
        .map_err(|error| error.to_string())? as usize;
    let input_ptr = input_ptr_fn
        .call(&mut store, ())
        .map_err(|error| error.to_string())? as usize;
    if ptr != input_ptr {
        return Err("WASM provider input pointer mismatch".to_owned());
    }
    write_memory(&memory, &mut store, ptr, input)?;
    let status = call_fn
        .call(&mut store, ())
        .map_err(|error| error.to_string())?;
    if status != 0 {
        return Err(format!("WASM provider returned non-zero status {status}"));
    }
    let output_ptr = output_ptr_fn
        .call(&mut store, ())
        .map_err(|error| error.to_string())? as usize;
    let output_len = output_len_fn
        .call(&mut store, ())
        .map_err(|error| error.to_string())? as usize;
    if output_len > limits.max_output_bytes {
        return Err(format!(
            "WASM provider output exceeds {} bytes",
            limits.max_output_bytes
        ));
    }
    read_memory(&memory, &mut store, output_ptr, output_len)
}

fn typed<Params, Results>(
    instance: &Instance,
    store: &mut Store<()>,
    name: &str,
) -> Result<TypedFunc<Params, Results>, String>
where
    Params: wasmtime::WasmParams,
    Results: wasmtime::WasmResults,
{
    instance
        .get_typed_func(store, name)
        .map_err(|error| error.to_string())
}

fn write_memory(
    memory: &Memory,
    store: &mut Store<()>,
    offset: usize,
    bytes: &[u8],
) -> Result<(), String> {
    memory
        .write(store, offset, bytes)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn read_memory(
    memory: &Memory,
    store: &mut Store<()>,
    offset: usize,
    len: usize,
) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0; len];
    memory
        .read(store, offset, &mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "wasm_tests.rs"]
mod tests;

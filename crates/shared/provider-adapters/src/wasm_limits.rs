use serde_json::Value;
use soma_provider_core::ProviderTool;

#[derive(Debug, Clone, Copy)]
pub(super) struct WasmRuntimeLimits {
    pub(super) timeout_ms: u64,
    pub(super) max_input_bytes: usize,
    pub(super) max_output_bytes: usize,
    pub(super) fuel: u64,
    pub(super) max_memory_bytes: usize,
    pub(super) max_table_elements: usize,
    pub(super) max_instances: usize,
}

impl WasmRuntimeLimits {
    pub(super) fn from_tool(tool: &ProviderTool) -> Self {
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
            max_memory_bytes: meta
                .and_then(|value| value.get("max_memory_bytes"))
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(64 * 1024 * 1024),
            max_table_elements: meta
                .and_then(|value| value.get("max_table_elements"))
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(10_000),
            max_instances: meta
                .and_then(|value| value.get("max_instances"))
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(16),
        }
    }
}

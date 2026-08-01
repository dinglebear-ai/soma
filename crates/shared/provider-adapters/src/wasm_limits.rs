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
    const MAX_TIMEOUT_MS: u64 = 30_000;
    const MAX_INPUT_BYTES: usize = 1024 * 1024;
    const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
    const MAX_FUEL: u64 = 10_000_000;
    const MAX_MEMORY_BYTES: usize = 256 * 1024 * 1024;
    const MAX_TABLE_ELEMENTS: usize = 100_000;
    const MAX_INSTANCES: usize = 64;

    const COMPONENTIZE_MIN_TIMEOUT_MS: u64 = 30_000;
    const COMPONENTIZE_MIN_FUEL: u64 = 10_000_000;
    const COMPONENTIZE_MIN_MEMORY_BYTES: usize = 64 * 1024 * 1024;
    const COMPONENTIZE_MIN_TABLE_ELEMENTS: usize = 10_000;
    const COMPONENTIZE_MIN_INSTANCES: usize = 64;

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
                .unwrap_or(5_000)
                .clamp(1, Self::MAX_TIMEOUT_MS),
            max_input_bytes: tool
                .limits
                .as_ref()
                .and_then(|limits| limits.max_input_bytes)
                .unwrap_or(64 * 1024)
                .min(Self::MAX_INPUT_BYTES),
            max_output_bytes: tool
                .limits
                .as_ref()
                .and_then(|limits| limits.max_response_bytes)
                .unwrap_or(256 * 1024)
                .min(Self::MAX_OUTPUT_BYTES),
            fuel: meta
                .and_then(|value| value.get("fuel"))
                .and_then(Value::as_u64)
                .unwrap_or(1_000_000)
                .min(Self::MAX_FUEL),
            max_memory_bytes: meta
                .and_then(|value| value.get("max_memory_bytes"))
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(64 * 1024 * 1024)
                .min(Self::MAX_MEMORY_BYTES),
            max_table_elements: meta
                .and_then(|value| value.get("max_table_elements"))
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(10_000)
                .min(Self::MAX_TABLE_ELEMENTS),
            max_instances: meta
                .and_then(|value| value.get("max_instances"))
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(16)
                .min(Self::MAX_INSTANCES),
        }
    }

    pub(super) fn with_componentize_minimums(mut self, componentize: bool) -> Self {
        if componentize {
            self.timeout_ms = self.timeout_ms.max(Self::COMPONENTIZE_MIN_TIMEOUT_MS);
            self.fuel = self.fuel.max(Self::COMPONENTIZE_MIN_FUEL);
            self.max_memory_bytes = self
                .max_memory_bytes
                .max(Self::COMPONENTIZE_MIN_MEMORY_BYTES);
            self.max_table_elements = self
                .max_table_elements
                .max(Self::COMPONENTIZE_MIN_TABLE_ELEMENTS);
            self.max_instances = self.max_instances.max(Self::COMPONENTIZE_MIN_INSTANCES);
        }
        self
    }
}

use serde_json::{json, Value};

use crate::{CodeModeConfig, ToolError};

#[derive(Debug, Clone)]
pub(crate) struct RunBudget {
    max_operations: u64,
    operations: u64,
    result_max_bytes: usize,
    max_log_entries: usize,
    max_log_bytes: usize,
}

impl RunBudget {
    pub(crate) fn new(config: &CodeModeConfig) -> Self {
        Self {
            max_operations: crate::config::effective_max_calltool_per_run(config),
            operations: 0,
            result_max_bytes: crate::config::effective_calltool_result_max_bytes(config),
            max_log_entries: config.max_log_entries,
            max_log_bytes: config.max_log_bytes,
        }
    }

    pub(crate) fn record_operation(&mut self, label: &str) -> Result<(), ToolError> {
        self.operations = self.operations.saturating_add(1);
        if self.operations > self.max_operations {
            return Err(ToolError::Sdk {
                sdk_kind: "budget_exceeded".to_string(),
                message: format!(
                    "Code Mode {label} exceeded the per-run operation budget of {}",
                    self.max_operations
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn cap_tool_result(&self, value: Value) -> Value {
        let Ok(bytes) = serde_json::to_vec(&value) else {
            return json!({"truncated": true, "reason": "tool result was not serializable"});
        };
        if bytes.len() <= self.result_max_bytes {
            return value;
        }
        let serialized = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        let preview =
            crate::util::utf8_prefix_by_bytes(&serialized, self.result_max_bytes.min(1024));
        json!({
            "truncated": true,
            "original_size_bytes": bytes.len(),
            "max_size_bytes": self.result_max_bytes,
            "preview": preview,
        })
    }

    pub(crate) fn cap_logs(&self, logs: Vec<String>) -> Vec<String> {
        let mut capped = Vec::new();
        let mut total = 0usize;
        let max_entries = self.max_log_entries.max(1);
        let max_bytes = self.max_log_bytes.max(1);
        for log in logs {
            let sanitized = crate::truncate::sanitize_log_text(&log, max_bytes.min(4096));
            let next = sanitized.len();
            if capped.len() >= max_entries || total.saturating_add(next) > max_bytes {
                capped.push("[soma] Code Mode logs truncated".to_string());
                break;
            }
            total = total.saturating_add(next);
            capped.push(sanitized);
        }
        capped
    }
}

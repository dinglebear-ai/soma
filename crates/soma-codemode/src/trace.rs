#![allow(dead_code)]

use serde_json::{json, Map, Value};

use crate::types::CodeModeExecutionResponse;

const REDACTED: &str = "[redacted]";
const MAX_DEPTH: usize = 16;
const MAX_COLLECTION_ITEMS: usize = 64;
const MAX_STRING_CHARS: usize = 512;

#[must_use]
pub fn redact_trace_value(value: &Value, max_bytes: usize) -> Value {
    let redacted = redact_value(value, 0);
    let size = serde_json::to_vec(&redacted).map_or(usize::MAX, |bytes| bytes.len());
    if size <= max_bytes {
        redacted
    } else {
        json!({
            "truncated": true,
            "reason": "redacted_params_exceeded_cap",
            "original_size_bytes": size,
            "max_size_bytes": max_bytes,
        })
    }
}

#[must_use]
pub(crate) fn redact_trace_params(params: &Value, enabled: bool) -> Option<Value> {
    enabled.then(|| redact_trace_value(params, 4096))
}

#[must_use]
pub fn code_mode_execute_trace(response: &CodeModeExecutionResponse) -> Value {
    let calls = response
        .calls
        .iter()
        .map(|call| {
            let (namespace, tool) = crate::types::id::split_namespaced_id(&call.id)
                .map_or(("", call.id.as_str()), |(namespace, tool)| {
                    (namespace, tool)
                });
            json!({
                "id": call.id,
                "namespace": namespace,
                "tool": tool,
                "params": call
                    .params
                    .as_ref()
                    .map(|params| redact_trace_value(params, 4096)),
                "ok": call.result.is_some(),
            })
        })
        .collect::<Vec<_>>();
    let mut trace = Map::new();
    trace.insert("kind".to_string(), json!("code_mode_execute_trace"));
    trace.insert("call_count".to_string(), json!(response.calls.len()));
    trace.insert("calls".to_string(), json!(calls));
    if let Some(result) = &response.result {
        trace.insert("result".to_string(), redact_trace_value(result, 4096));
    }
    trace.insert(
        "result_shape".to_string(),
        response
            .result
            .as_ref()
            .map(compact_result_shape)
            .unwrap_or_else(|| json!({"type": "undefined"})),
    );
    trace.insert("logs_count".to_string(), json!(response.logs.len()));
    Value::Object(trace)
}

fn redact_value(value: &Value, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return json!({"truncated": true, "reason": "max_depth"});
    }
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .take(MAX_COLLECTION_ITEMS)
                .map(|(key, value)| {
                    if crate::redact::is_sensitive_key(key) {
                        (key.clone(), Value::String(REDACTED.to_string()))
                    } else {
                        (key.clone(), redact_value(value, depth + 1))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .take(MAX_COLLECTION_ITEMS)
                .map(|value| redact_value(value, depth + 1))
                .collect(),
        ),
        Value::String(text) => {
            let redacted = crate::truncate::redact_secret_like_segments(text);
            if redacted != *text {
                Value::String(redacted)
            } else if text.chars().count() > MAX_STRING_CHARS {
                Value::String(format!(
                    "{}[truncated]",
                    text.chars().take(MAX_STRING_CHARS).collect::<String>()
                ))
            } else {
                value.clone()
            }
        }
        other => other.clone(),
    }
}

fn compact_result_shape(value: &Value) -> Value {
    let size_bytes = serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len());
    match value {
        Value::Null => json!({"type": "null", "size_bytes": size_bytes}),
        Value::Bool(_) => json!({"type": "boolean", "size_bytes": size_bytes}),
        Value::Number(_) => json!({"type": "number", "size_bytes": size_bytes}),
        Value::String(text) => {
            json!({"type": "string", "size_bytes": size_bytes, "length": text.chars().count()})
        }
        Value::Array(values) => {
            json!({"type": "array", "size_bytes": size_bytes, "length": values.len()})
        }
        Value::Object(object) => {
            let mut keys = object.keys().take(16).cloned().collect::<Vec<_>>();
            keys.sort();
            json!({"type": "object", "size_bytes": size_bytes, "keys": keys, "key_count": object.len()})
        }
    }
}

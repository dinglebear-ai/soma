use serde_json::{json, Value};

use crate::types::ToolDescriptor;

pub fn describe_types(entries: &[ToolDescriptor]) -> Value {
    json!({
        "tools": entries.iter().map(|entry| {
            json!({"id": entry.id, "signature": entry.signature, "dts": entry.dts})
        }).collect::<Vec<_>>()
    })
}

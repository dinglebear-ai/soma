#![allow(dead_code)]

use serde::Serialize;

use crate::ToolError;

pub(crate) fn invalid_code_mode_id(message: impl Into<String>) -> ToolError {
    ToolError::InvalidParam {
        message: message.into(),
        param: "id".to_string(),
    }
}

pub(crate) fn unknown_local_provider(provider: &str) -> ToolError {
    ToolError::UnknownAction {
        message: format!("unsupported Code Mode local provider `{provider}`"),
        valid: vec!["state".to_string(), "git".to_string()],
        hint: None,
    }
}

pub(crate) fn serialized_size<T: Serialize>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

pub(crate) fn utf8_prefix_by_bytes(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = 0;
    for (idx, _) in value.char_indices() {
        if idx > max_bytes {
            break;
        }
        end = idx;
    }
    &value[..end]
}

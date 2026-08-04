use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use soma_infra::{FileKind, PathRead};

use crate::ExecutionError;

const INLINE_TEXT_CHARS: usize = 1_000_000;

pub(crate) fn serialize<T: Serialize>(value: T) -> Result<Value, ExecutionError> {
    serde_json::to_value(value).map_err(|error| ExecutionError::Serialization(error.to_string()))
}

pub(crate) fn resource<T: Serialize>(value: T) -> Result<Value, ExecutionError> {
    Ok(json!({"resource": serialize(value)?}))
}

pub(crate) fn items<T: Serialize>(
    values: T,
    count: usize,
    truncated: bool,
) -> Result<Value, ExecutionError> {
    Ok(json!({
        "items": serialize(values)?,
        "count": count,
        "truncated": truncated
    }))
}

pub(crate) fn metrics<T: Serialize>(value: T) -> Result<Value, ExecutionError> {
    Ok(json!({"metrics": serialize(value)?}))
}

pub(crate) fn status(name: &str, details: Value) -> Value {
    json!({"status": name, "details": details})
}

pub(crate) fn text(bytes: &[u8], truncated: bool, line_count: Option<usize>) -> Value {
    let source = String::from_utf8_lossy(bytes);
    let mut content = source.chars().take(INLINE_TEXT_CHARS).collect::<String>();
    let inline_truncated = source.chars().count() > INLINE_TEXT_CHARS;
    if content.len() > 1_048_576 {
        while content.len() > 1_048_576 {
            content.pop();
        }
    }
    json!({
        "content": content,
        "bytes": bytes.len(),
        "truncated": truncated || inline_truncated,
        "encoding": "utf-8",
        "line_count": line_count.unwrap_or_else(|| source.lines().count())
    })
}

pub(crate) fn file_content(value: PathRead, tree: bool) -> Value {
    let kind = if tree {
        "tree"
    } else {
        match value.kind {
            FileKind::File => "file",
            FileKind::Directory => "directory",
        }
    };
    let entries = value
        .entries
        .iter()
        .map(|path| json!({"path": path}))
        .collect::<Vec<_>>();
    if value.kind == FileKind::File {
        let mut result = text(&value.content, value.truncated, None);
        let object = result.as_object_mut().expect("text result is an object");
        object.insert("kind".into(), Value::String(kind.into()));
        object.insert("entries".into(), Value::Array(entries));
        result
    } else {
        json!({
            "bytes": 0,
            "truncated": value.truncated,
            "encoding": "utf-8",
            "line_count": 0,
            "kind": kind,
            "entries": entries
        })
    }
}

pub(crate) fn compare(
    source: &[u8],
    target: &[u8],
    source_label: &str,
    target_label: &str,
) -> Value {
    let equal = source == target;
    let source_digest = digest(source);
    let target_digest = digest(target);
    if equal {
        return json!({
            "equal": true,
            "summary": "Inputs are identical",
            "source_digest": source_digest,
            "target_digest": target_digest
        });
    }
    let source_text = String::from_utf8_lossy(source);
    let target_text = String::from_utf8_lossy(target);
    let source_lines = source_text
        .lines()
        .collect::<std::collections::BTreeSet<_>>();
    let target_lines = target_text
        .lines()
        .collect::<std::collections::BTreeSet<_>>();
    let mut patch = format!("--- {source_label}\n+++ {target_label}\n");
    for line in source_lines.difference(&target_lines) {
        append_patch(&mut patch, "- ", line);
    }
    for line in target_lines.difference(&source_lines) {
        append_patch(&mut patch, "+ ", line);
    }
    json!({
        "equal": false,
        "summary": "Inputs differ",
        "patch": patch,
        "source_digest": source_digest,
        "target_digest": target_digest
    })
}

fn append_patch(patch: &mut String, prefix: &str, line: &str) {
    if patch.len() >= 1_000_000 {
        return;
    }
    patch.push_str(prefix);
    patch.push_str(line);
    patch.push('\n');
    if patch.len() > 1_000_000 {
        patch.truncate(1_000_000);
    }
}

pub(crate) fn digest(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "runtime_result_tests.rs"]
mod tests;

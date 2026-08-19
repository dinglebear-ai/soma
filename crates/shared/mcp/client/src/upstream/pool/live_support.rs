//! Descriptor conversion, bearer-token normalization, stdio environment
//! construction, and process-stderr draining for `live.rs`. Split out to stay
//! under the PATTERNS.md module size hard limit.
use std::collections::BTreeMap;
use std::sync::Once;

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde_json::Value;
use tokio::io::AsyncReadExt;

use crate::config::UpstreamConfig;
use crate::upstream::{PromptDescriptor, ResourceDescriptor, ToolDescriptor};

pub(super) fn tool_descriptor(tool: Tool) -> ToolDescriptor {
    ToolDescriptor {
        name: tool.name.to_string(),
        description: tool.description.map(|value| value.to_string()),
        input_schema: Some(Value::Object((*tool.input_schema).clone())),
        output_schema: tool
            .output_schema
            .map(|schema| Value::Object((*schema).clone())),
        annotations: tool.annotations.as_ref().and_then(tool_annotations_value),
        destructive: upstream_destructive_from_annotations(tool.annotations.as_ref()),
    }
}

/// Derive the internal gateway safety verdict without changing the annotation
/// block that will be relayed back to downstream MCP clients. Missing or
/// underspecified annotations fail closed.
pub(super) fn upstream_destructive_from_annotations(annotations: Option<&ToolAnnotations>) -> bool {
    annotations.is_none_or(|annotations| {
        annotations
            .destructive_hint
            .unwrap_or_else(|| !annotations.read_only_hint.unwrap_or(false))
    })
}

fn tool_annotations_value(annotations: &ToolAnnotations) -> Option<Value> {
    match serde_json::to_value(annotations) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(
                error = %error,
                "failed to serialize upstream MCP tool annotations; dropping annotation block"
            );
            None
        }
    }
}

/// Interpret a completed MCP tool result as a tool-execution failure without
/// trusting the upstream to classify its own failure as infrastructure.
///
/// Upstream `kind` values are untrusted. Only the stable caller-facing subset
/// survives; infrastructure-looking or unknown values collapse to
/// `tool_error`. The cause is bounded so a malicious tool cannot inflate an
/// error envelope after the normal response cap has already accepted it.
pub(super) fn completed_tool_error(result: &CallToolResult) -> Option<(String, String)> {
    if result.is_error != Some(true) {
        return None;
    }
    let object = result
        .structured_content
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|object| {
            object
                .get("error")
                .and_then(Value::as_object)
                .or(Some(object))
        });
    let raw_kind = object
        .and_then(|object| object.get("kind"))
        .and_then(Value::as_str);
    let kind = canonical_completed_tool_error_kind(raw_kind).to_string();
    let cause = object
        .and_then(|object| {
            object
                .get("message")
                .or_else(|| object.get("cause"))
                .and_then(Value::as_str)
        })
        .unwrap_or("upstream tool reported an error");
    Some((kind, cause.chars().take(1024).collect()))
}

fn canonical_completed_tool_error_kind(kind: Option<&str>) -> &str {
    match kind {
        Some(
            "unknown_action"
            | "unknown_subaction"
            | "missing_param"
            | "invalid_param"
            | "unknown_instance"
            | "confirmation_required"
            | "conflict"
            | "forbidden"
            | "unknown_tool"
            | "route_scope_denied"
            | "path_traversal"
            | "permission_denied"
            | "timeout"
            | "budget_exceeded"
            | "quota_exceeded"
            | "invalid_code_mode_id"
            | "snippet_not_found"
            | "artifact_too_large"
            | "auth_failed"
            | "oauth_needs_reauth"
            | "not_found"
            | "rate_limited"
            | "validation_failed"
            | "code_mode_timeout",
        ) => kind.unwrap_or("tool_error"),
        _ => "tool_error",
    }
}

pub(super) fn resource_descriptor(resource: rmcp::model::Resource) -> ResourceDescriptor {
    ResourceDescriptor {
        uri: resource.uri,
        name: Some(resource.name),
    }
}

pub(super) fn prompt_descriptor(prompt: rmcp::model::Prompt) -> PromptDescriptor {
    PromptDescriptor {
        name: prompt.name,
        description: prompt.description,
    }
}

pub(super) fn normalize_bearer_value(token: &str) -> String {
    token
        .trim()
        .strip_prefix("Bearer ")
        .unwrap_or_else(|| token.trim())
        .to_owned()
}

pub(super) fn websocket_authorization(config: &UpstreamConfig) -> Option<String> {
    bearer_token_from_env(config).map(|token| format!("Bearer {token}"))
}

pub(super) fn bearer_token_from_env(config: &UpstreamConfig) -> Option<String> {
    let env_name = config.bearer_token_env.as_deref()?;
    let token = std::env::var(env_name).ok()?;
    let token = normalize_bearer_value(&token);
    (!token.is_empty()).then_some(token)
}

pub(super) fn ensure_rustls_crypto_provider() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

pub(super) fn stdio_env() -> BTreeMap<String, String> {
    const ALLOWLIST: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "TERM",
        "TZ",
        "TMPDIR",
        "TMP",
        "TEMP",
        "LANG",
        "LC_ALL",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "NODE_EXTRA_CA_CERTS",
        "REQUESTS_CA_BUNDLE",
        "CURL_CA_BUNDLE",
    ];
    ALLOWLIST
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| ((*key).to_owned(), value))
        })
        .collect()
}

pub(super) fn drain_stderr(upstream: String, stderr: Option<tokio::process::ChildStderr>) {
    let Some(mut stderr) = stderr else {
        return;
    };
    tokio::spawn(async move {
        let mut bytes = Vec::new();
        if stderr.read_to_end(&mut bytes).await.is_ok() && !bytes.is_empty() {
            tracing::debug!(
                upstream,
                stderr = %String::from_utf8_lossy(&bytes),
                "upstream stdio stderr"
            );
        }
    });
}

#[cfg(test)]
#[path = "live_support_tests.rs"]
mod tests;

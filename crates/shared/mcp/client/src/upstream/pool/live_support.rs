//! Descriptor conversion, bearer-token normalization, stdio environment
//! construction, and process-stderr draining for `live.rs`. Split out to stay
//! under the PATTERNS.md module size hard limit.
use std::collections::BTreeMap;
use std::sync::Once;

use rmcp::model::Tool;
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
        destructive: tool
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.destructive_hint)
            .unwrap_or(true),
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

pub(super) fn capability_is_absent(error: &str) -> bool {
    error.contains("-32601")
        || error.contains("Method not found")
        || error.contains("method not found")
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

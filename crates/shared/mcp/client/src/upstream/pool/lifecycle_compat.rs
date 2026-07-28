//! Compatibility negotiation for gateway-to-upstream MCP connections.
//!
//! Soma's downstream server remains on the modern stateless lifecycle. This
//! module only handles independently versioned upstream servers and always
//! reconnects on a fresh transport before attempting legacy initialization.

use rmcp::model::{ProtocolVersion, ServerResult};
use rmcp::service::{ClientInitializeError, ClientLifecycleMode};

const DISCOVERY_SERVER_INFO_META_KEY: &str = "io.modelcontextprotocol/serverInfo";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LifecycleAttempt {
    Modern,
    LegacyInitialize,
}

impl LifecycleAttempt {
    pub(super) fn mode(self) -> ClientLifecycleMode {
        match self {
            Self::Modern => ClientLifecycleMode::Discover {
                preferred_versions: ProtocolVersion::KNOWN_VERSIONS
                    .iter()
                    .rev()
                    .cloned()
                    .collect(),
            },
            Self::LegacyInitialize => ClientLifecycleMode::Initialize,
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Modern => "discover",
            Self::LegacyInitialize => "initialize",
        }
    }
}

fn result_carries_discovery_server_info(result: &ServerResult) -> bool {
    let Ok(value) = serde_json::to_value(result) else {
        return false;
    };

    value
        .get("_meta")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|meta| meta.contains_key(DISCOVERY_SERVER_INFO_META_KEY))
}

fn discovery_response_was_misclassified(error: &ClientInitializeError) -> bool {
    matches!(
        error,
        ClientInitializeError::ExpectedInitResult(Some(result))
            if result_carries_discovery_server_info(result)
    )
}

/// Select a retry only when the initialization error proves lifecycle
/// incompatibility. Operational, TLS, timeout, and authentication failures are
/// never downgraded to legacy initialization.
pub(super) fn compatibility_retry(error: &ClientInitializeError) -> Option<LifecycleAttempt> {
    if discovery_response_was_misclassified(error) {
        return Some(LifecycleAttempt::LegacyInitialize);
    }

    let message = error.to_string().to_ascii_lowercase();
    if message.contains("unsupported mcp-protocol-version")
        || message.contains("unsupported protocol version")
        || message.contains("method not found")
        || message.contains("unknown method")
        || message.contains("no compatible protocol version")
    {
        return Some(LifecycleAttempt::LegacyInitialize);
    }

    if message.contains("missing session id")
        || message.contains("no valid session id")
        || message.contains("expect initialize request")
        || message.contains("expected initialize request")
        || message.contains("connection closed: discover response")
        || (message.contains("server/discover")
            && (message.contains("invalid params")
                || message.contains("invalid request parameters")))
    {
        return Some(LifecycleAttempt::LegacyInitialize);
    }

    None
}

pub(super) fn log_fallback(
    upstream: &str,
    transport: &str,
    attempt: LifecycleAttempt,
    error: &ClientInitializeError,
) {
    tracing::warn!(
        upstream,
        transport,
        from = LifecycleAttempt::Modern.label(),
        to = attempt.label(),
        error = %error,
        "upstream is incompatible with the modern MCP lifecycle; reconnecting with compatibility initialization"
    );
}

#[cfg(test)]
#[path = "lifecycle_compat_tests.rs"]
mod tests;

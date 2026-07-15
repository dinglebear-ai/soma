use serde_json::Value;
use thiserror::Error;

pub mod cache;
pub mod lifecycle;
pub mod session;

pub use cache::{RelayCache, RelayCacheKey, RelayConnectSlot, RelayConnection};
pub use session::{RelaySessionId, RelaySessionMint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayOperation {
    CallTool,
    ListTools,
    ListResources,
    GetPrompt,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelayCapabilities {
    pub elicitation: bool,
    pub sampling: bool,
    pub roots: bool,
}

impl RelayCapabilities {
    #[must_use]
    pub fn mirrored_by(self, downstream: Self) -> bool {
        (!self.elicitation || downstream.elicitation)
            && (!self.sampling || downstream.sampling)
            && (!self.roots || downstream.roots)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RelayError {
    #[error("relay sessions only support call_tool")]
    UnsupportedOperation,
    #[error("relay session ids are gateway-minted and cannot come from user input")]
    ForgedSessionId,
    #[error("downstream client cannot mirror upstream relay capabilities")]
    CapabilityMirrorMissing,
}

pub fn ensure_call_tool_only(operation: RelayOperation) -> Result<(), RelayError> {
    if matches!(operation, RelayOperation::CallTool) {
        return Ok(());
    }
    Err(RelayError::UnsupportedOperation)
}

pub fn reject_user_supplied_session_ids(params: &Value) -> Result<(), RelayError> {
    let Some(object) = params.as_object() else {
        return Ok(());
    };
    let forbidden = ["session_id", "mcp-session-id", "mcp_session_id"];
    if forbidden.iter().any(|key| object.contains_key(*key)) {
        return Err(RelayError::ForgedSessionId);
    }
    Ok(())
}

pub fn ensure_capabilities_mirrored(
    upstream: RelayCapabilities,
    downstream: RelayCapabilities,
) -> Result<(), RelayError> {
    if upstream.mirrored_by(downstream) {
        return Ok(());
    }
    Err(RelayError::CapabilityMirrorMissing)
}

#[cfg(test)]
#[path = "relay_tests.rs"]
mod tests;

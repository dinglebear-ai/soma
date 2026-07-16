use super::*;

#[test]
fn relay_applies_to_call_tool_only() {
    assert_eq!(ensure_call_tool_only(RelayOperation::CallTool), Ok(()));
    assert_eq!(
        ensure_call_tool_only(RelayOperation::ListTools),
        Err(RelayError::UnsupportedOperation)
    );
}

#[test]
fn forged_session_ids_are_rejected_from_user_params() {
    let params = serde_json::json!({"mcp-session-id": "reuse-me"});

    assert_eq!(
        reject_user_supplied_session_ids(&params),
        Err(RelayError::ForgedSessionId)
    );
}

#[test]
fn mirrored_capabilities_check_elicitation_sampling_and_roots() {
    let upstream = RelayCapabilities {
        elicitation: true,
        sampling: true,
        roots: true,
    };
    let downstream = RelayCapabilities {
        elicitation: true,
        sampling: false,
        roots: true,
    };

    assert_eq!(
        ensure_capabilities_mirrored(upstream, downstream),
        Err(RelayError::CapabilityMirrorMissing)
    );
}

use super::id::{split_namespaced_id, CodeModeToolId, CodeModeToolRef};

#[test]
fn parses_canonical_tool_ids() {
    let parsed = CodeModeToolId::parse("state::read").unwrap();
    assert_eq!(parsed.raw, "state::read");
    assert!(matches!(parsed.reference, CodeModeToolRef::Tool { .. }));
    assert_eq!(split_namespaced_id("state::read"), Some(("state", "read")));
}

#[test]
fn rejects_legacy_prefixed_ids() {
    assert!(CodeModeToolId::parse("upstream::state::read").is_err());
}

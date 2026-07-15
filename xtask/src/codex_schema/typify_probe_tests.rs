use super::*;
use std::path::PathBuf;

/// Same raw pre-fix shape used in `merge_tests.rs`.
const RAW_MCP_ELICITATION_PARAMS: &str = include_str!("testdata/raw_mcp_elicitation_params.json");

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask parent")
        .to_path_buf()
}

fn load_committed_schema() -> Value {
    let path = repo_root().join(crate::codex_schema::PROTOCOL_SCHEMA_PATH);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed schema at {}: {e}", path.display()));
    serde_json::from_str(&text).expect("committed schema is valid JSON")
}

#[test]
fn probe_succeeds_on_the_currently_committed_schema() {
    // The committed schema already has the McpServerElicitationRequestParams
    // flatten workaround applied - typify should convert it cleanly.
    let schema = load_committed_schema();
    let outcome = probe(&schema);
    assert!(
        matches!(outcome, ProbeOutcome::Success),
        "expected Success, got: {}",
        outcome.summary()
    );
}

#[test]
fn probe_reproduces_the_target_panic_on_the_unflattened_elicitation_shape() {
    // Same schema, but with McpServerElicitationRequestParams swapped back
    // to its raw, pre-fix shape (top-level object + sibling oneOf with a
    // wildcard branch) - this is the exact historical typify-0.7.0
    // merge.rs:427 panic this crate's README documents.
    let mut schema = load_committed_schema();
    let raw: Value = serde_json::from_str(RAW_MCP_ELICITATION_PARAMS).unwrap();
    schema["definitions"]["McpServerElicitationRequestParams"] = raw;

    let outcome = probe(&schema);
    assert!(
        outcome.reproduces_target(),
        "expected the target merge.rs panic, got: {}",
        outcome.summary()
    );
}

#[test]
fn probe_reports_invalid_root_schema_without_panicking() {
    let not_a_schema = serde_json::json!("just a string, not an object");
    let outcome = probe(&not_a_schema);
    assert!(matches!(outcome, ProbeOutcome::InvalidRootSchema(_)));
}

#[test]
fn probe_restores_the_panic_hook_after_a_target_panic() {
    // Regression guard: probing must not leave a custom panic hook installed
    // - a later, unrelated panic in this same test binary should still hit
    // the default hook's behavior (no hook-related state leakage). We can't
    // directly assert "the default hook is installed", but we can assert
    // that a second, independent probe() call still classifies correctly,
    // which would only happen if the previous call's hook was fully cleaned
    // up (a leaked hook would keep clobbering `location` from a stale Arc).
    let mut schema = load_committed_schema();
    let raw: Value = serde_json::from_str(RAW_MCP_ELICITATION_PARAMS).unwrap();
    schema["definitions"]["McpServerElicitationRequestParams"] = raw;

    let first = probe(&schema);
    assert!(first.reproduces_target());

    let second_schema = load_committed_schema();
    let second = probe(&second_schema);
    assert!(matches!(second, ProbeOutcome::Success));
}

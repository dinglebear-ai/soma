use std::{fs, path::Path};

#[test]
fn ai_sdk_provider_is_deferred_until_sidecar_isolation_contract_is_implemented() {
    let source = fs::read_to_string(
        workspace_root().join("crates/rtemplate-service/src/providers/ai_sdk.rs"),
    )
    .expect("AI SDK provider source should exist");

    assert!(source.contains("sidecar execution is deferred"));
    assert!(source.contains("empty inherited env"));
    assert!(source.contains("bounded IO"));
    assert!(source.contains("process-group cleanup"));
    assert!(source.contains("filesystem/network grants"));
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

use std::{fs, path::Path};

#[test]
fn wasm_provider_is_deferred_until_sandbox_contract_is_implemented() {
    let source =
        fs::read_to_string(workspace_root().join("crates/rtemplate-service/src/providers/wasm.rs"))
            .expect("WASM provider source should exist");

    assert!(source.contains("Untrusted WASM execution is deferred"));
    assert!(source.contains("traps"));
    assert!(source.contains("fuel"));
    assert!(source.contains("memory"));
    assert!(source.contains("concurrency"));
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

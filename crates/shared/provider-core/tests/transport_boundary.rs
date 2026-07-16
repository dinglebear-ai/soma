use std::{fs, path::PathBuf};

#[test]
fn provider_core_contains_no_sidecar_or_wasm_wire_dtos() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let call = fs::read_to_string(root.join("src/call.rs")).expect("call source reads");
    let facade = fs::read_to_string(root.join("src/lib.rs")).expect("lib source reads");

    for forbidden in ["ProviderExecutionEnvelope", "execution_payload"] {
        assert!(
            !call.contains(forbidden) && !facade.contains(forbidden),
            "provider-core must not own product wire DTO `{forbidden}`"
        );
    }
}

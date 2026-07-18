use super::provider_runtime_security;

#[test]
fn provider_runtime_security_classifies_provider_kinds() {
    assert_eq!(provider_runtime_security("wasm")["runtime"], "wasmtime");
    assert_eq!(
        provider_runtime_security("python")["runtime"],
        "sidecar-process"
    );
    assert_eq!(
        provider_runtime_security("mcp")["runtime"],
        "remote-or-upstream"
    );
    assert_eq!(
        provider_runtime_security("static-rust")["runtime"],
        "in-process"
    );
}

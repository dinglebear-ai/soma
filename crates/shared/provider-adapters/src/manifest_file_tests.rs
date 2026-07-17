use serde_json::json;
use soma_provider_core::ProviderManifest;

use super::*;

fn catalog(kind: &str) -> ProviderCatalog {
    serde_json::from_value::<ProviderManifest>(json!({
        "schema_version": 1,
        "provider": { "name": "fixture", "kind": kind },
        "tools": [],
    }))
    .expect("fixture manifest deserializes")
}

fn resolves(kind: &str) -> bool {
    build_provider(PathBuf::from("providers/fixture"), catalog(kind), "SOMA").is_some()
}

// Each assertion is gated on the same feature `build_provider` gates its own
// match arm on, so this test is correct under any feature combination the
// crate is compiled with (not just `--all-features`).

#[test]
fn openapi_kind_resolves_iff_openapi_feature_is_compiled_in() {
    assert_eq!(resolves("openapi"), cfg!(feature = "openapi"));
}

#[test]
fn mcp_kind_resolves_iff_gateway_feature_is_compiled_in() {
    assert_eq!(resolves("mcp"), cfg!(feature = "gateway"));
}

#[test]
fn ai_sdk_kind_resolves_iff_ai_sdk_feature_is_compiled_in() {
    assert_eq!(resolves("ai-sdk"), cfg!(feature = "ai-sdk"));
}

#[test]
fn wasm_kind_resolves_iff_wasm_feature_is_compiled_in() {
    assert_eq!(resolves("wasm"), cfg!(feature = "wasm"));
}

#[test]
fn python_family_kinds_resolve_iff_python_feature_is_compiled_in() {
    for kind in ["python", "langchain", "llamaindex"] {
        assert_eq!(resolves(kind), cfg!(feature = "python"), "kind `{kind}`");
    }
}

#[test]
fn static_rust_kind_resolves_iff_static_echo_feature_is_compiled_in() {
    assert_eq!(resolves("static-rust"), cfg!(feature = "static-echo"));
}

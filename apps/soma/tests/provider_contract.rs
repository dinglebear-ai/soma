use std::{fs, path::Path};

use soma_contracts::provider_validation::validate_provider_manifest_value;

fn fixture(name: &str) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("docs/contracts/examples/provider-manifests")
        .join(name);
    serde_json::from_slice(&fs::read(path).expect("fixture should exist")).expect("fixture JSON")
}

#[test]
fn provider_manifest_schema_accepts_all_valid_fixtures() {
    for name in [
        "static-rust.valid.json",
        "mcp.valid.json",
        "mcp-rest-opt-in.valid.json",
        "openapi.valid.json",
        "wasm.valid.json",
        "ai-sdk.valid.json",
    ] {
        validate_provider_manifest_value(&fixture(name))
            .unwrap_or_else(|error| panic!("{name} should validate: {error}"));
    }
}

#[test]
fn invalid_provider_manifest_fixtures_fail_with_named_codes() {
    for (name, code) in [
        ("duplicate-tool.invalid.json", "duplicate_tool_name"),
        ("duplicate-rest-path.invalid.json", "duplicate_rest_route"),
        ("duplicate-cli-alias.invalid.json", "duplicate_cli_command"),
        ("reserved-cli-command.invalid.json", "reserved_cli_command"),
        ("denied-capability.invalid.json", "empty_capability_scope"),
    ] {
        let error = validate_provider_manifest_value(&fixture(name))
            .expect_err("invalid fixture should fail");
        assert_eq!(error.code(), code, "{name}");
    }
}

use serde_json::json;

use super::*;

#[test]
fn embedded_catalog_cross_validates_complete_contract_set() {
    let catalog = SynapseCatalog::try_from_embedded().unwrap();
    assert_eq!(catalog.operation_count(), 59);
    assert_eq!(catalog.binding_count(), 59);
    assert_eq!(catalog.diagnostic_count(), 33);
    assert_eq!(catalog.operations().count(), 59);
    assert_eq!(catalog.bindings().count(), 59);
}

#[test]
fn shared_help_binding_resolves_for_both_tools() {
    let catalog = SynapseCatalog::embedded();
    for tool in [LegacyTool::Flux, LegacyTool::Scout] {
        let binding = catalog.binding(tool, "help", None).unwrap();
        assert_eq!(binding.canonical_name().as_str(), "product.help");
        assert_eq!(binding.tool(), LegacyTool::Both);
    }
}

#[test]
fn generated_legacy_tool_schemas_are_closed_and_executable() {
    let catalog = SynapseCatalog::embedded();
    let flux = catalog.legacy_tool_schema(LegacyTool::Flux);
    let scout = catalog.legacy_tool_schema(LegacyTool::Scout);
    let flux_validator = jsonschema::validator_for(&flux).unwrap();
    let scout_validator = jsonschema::validator_for(&scout).unwrap();

    flux_validator
        .validate(&json!({
            "action": "docker",
            "subaction": "build",
            "host": "devhost",
            "context": "/tmp/image",
            "tag": "example:latest",
            "response_format": "json"
        }))
        .unwrap();
    scout_validator
        .validate(&json!({
            "action": "delta",
            "source_host": "devhost",
            "source_path": "/tmp/a",
            "content": "hello"
        }))
        .unwrap();

    assert!(
        flux_validator
            .validate(&json!({
                "action": "docker",
                "subaction": "build",
                "host": "devhost",
                "context": "/tmp/image",
                "tag": "example:latest",
                "unknown": true
            }))
            .is_err()
    );
}

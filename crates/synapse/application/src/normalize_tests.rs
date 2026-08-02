use serde_json::json;

use super::*;

#[test]
fn flux_build_normalizes_to_canonical_parameters() {
    let request = SynapseCatalog::embedded()
        .normalize_legacy_request(
            LegacyTool::Flux,
            &json!({
                "action": "docker",
                "subaction": "build",
                "host": "dookie",
                "context": "/tmp/image",
                "tag": "example:latest",
                "response_format": "json"
            }),
        )
        .unwrap();
    assert_eq!(request.operation().as_str(), "docker.build");
    assert_eq!(request.legacy_name(), "flux.docker.build");
    assert_eq!(request.required_scope(), Some("synapse:write"));
    assert_eq!(request.presentation(), LegacyPresentation::Json);
    assert_eq!(
        request.parameters(),
        &json!({"host":"dookie","context":"/tmp/image","tag":"example:latest"})
    );
}

#[test]
fn shared_help_strips_legacy_presentation_fields() {
    let request = SynapseCatalog::embedded()
        .normalize_legacy_request(
            LegacyTool::Scout,
            &json!({"action":"help","topic":"exec","format":"markdown"}),
        )
        .unwrap();
    assert_eq!(request.operation().as_str(), "product.help");
    assert_eq!(request.parameters(), &json!({"topic":"exec"}));
    assert_eq!(request.required_scope(), None);
}

#[test]
fn unknown_fields_and_missing_required_fields_are_rejected() {
    let catalog = SynapseCatalog::embedded();
    assert!(matches!(
        catalog.normalize_legacy_request(
            LegacyTool::Flux,
            &json!({
                "action":"container",
                "subaction":"start",
                "host":"dookie",
                "container_id":"soma",
                "shell":true
            })
        ),
        Err(CompatibilityError::UnknownField { .. })
    ));
    assert!(matches!(
        catalog.normalize_legacy_request(
            LegacyTool::Flux,
            &json!({"action":"container","subaction":"start","host":"dookie"})
        ),
        Err(CompatibilityError::SchemaValidation { .. })
    ));
}

#[test]
fn delta_requires_exactly_one_complete_alternative() {
    let catalog = SynapseCatalog::embedded();
    for valid in [
        json!({
            "action":"delta",
            "source_host":"dookie",
            "source_path":"/tmp/a",
            "content":"hello"
        }),
        json!({
            "action":"delta",
            "source_host":"dookie",
            "source_path":"/tmp/a",
            "target_host":"squirts",
            "target_path":"/tmp/b"
        }),
    ] {
        assert!(
            catalog
                .normalize_legacy_request(LegacyTool::Scout, &valid)
                .is_ok()
        );
    }

    for invalid in [
        json!({
            "action":"delta",
            "source_host":"dookie",
            "source_path":"/tmp/a",
            "target_host":"squirts"
        }),
        json!({
            "action":"delta",
            "source_host":"dookie",
            "source_path":"/tmp/a",
            "target_host":"squirts",
            "target_path":"/tmp/b",
            "content":"hello"
        }),
    ] {
        assert!(matches!(
            catalog.normalize_legacy_request(LegacyTool::Scout, &invalid),
            Err(CompatibilityError::SchemaValidation { .. })
        ));
    }
}

#[test]
fn conflicting_presentations_are_rejected() {
    assert_eq!(
        SynapseCatalog::embedded().normalize_legacy_request(
            LegacyTool::Scout,
            &json!({"action":"help","format":"json","response_format":"markdown"})
        ),
        Err(CompatibilityError::ConflictingPresentation)
    );
}

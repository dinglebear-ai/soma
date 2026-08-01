use super::*;

fn restart() -> OperationName {
    OperationName::new("container.restart").unwrap()
}

#[test]
fn schema_ids_are_derived_from_operation_and_version() {
    assert_eq!(
        SchemaId::parameters(&restart(), 1).unwrap().as_str(),
        "schema.operations.container.restart.parameters.v1"
    );
    assert_eq!(
        SchemaId::result(&restart(), 3).unwrap().as_str(),
        "schema.operations.container.restart.result.v3"
    );
}

#[test]
fn schema_ids_reject_invalid_shapes_and_zero_versions() {
    assert_eq!(
        SchemaId::parameters(&restart(), 0),
        Err(SchemaIdError::ZeroVersion)
    );
    for invalid in [
        "schema.operation.container.restart.parameters.v1",
        "schema.operations.container.restart.input.v1",
        "schema.operations.container.restart.result.v0",
        "schema.operations.Container.restart.result.v1",
        "schema.operations.container.result.v1",
    ] {
        assert!(SchemaId::new(invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
fn deserialization_rejects_invalid_contract_identifiers() {
    assert!(serde_json::from_str::<SchemaId>("\"schema.operations.bad.result.v0\"").is_err());
    assert!(serde_json::from_str::<DiagnosticCode>("\"invalid\"").is_err());
    assert_eq!(
        serde_json::from_str::<DiagnosticCode>("\"target.not_found\"")
            .unwrap()
            .as_str(),
        "target.not_found"
    );
}

#[test]
fn diagnostic_codes_are_stable_dotted_identifiers() {
    let code = DiagnosticCode::new("target.not_found").unwrap();
    assert_eq!(code.as_str(), "target.not_found");
    assert_eq!(code.to_string(), "target.not_found");

    for invalid in [
        "invalid",
        "Target.not_found",
        "target.",
        "target..missing",
        "target.not found",
    ] {
        assert!(DiagnosticCode::new(invalid).is_err(), "accepted {invalid}");
    }
}

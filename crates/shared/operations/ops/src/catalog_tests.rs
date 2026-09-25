use super::*;

fn mutation_spec() -> OperationSpec {
    OperationSpec::new(
        OperationName::new("container.restart").unwrap(),
        TargetKind::Container,
        AccessClass::Mutation,
    )
}

#[test]
fn parameter_groups_are_sorted_and_unique() {
    let group = ParameterGroup::new(["host", "container_id"]).unwrap();
    assert_eq!(
        group.iter().collect::<Vec<_>>(),
        vec!["container_id", "host"]
    );
    assert!(ParameterGroup::new(["host", "host"]).is_err());
    assert!(ParameterGroup::new(["BadField"]).is_err());
}

#[test]
fn safe_retry_requires_idempotent_mutation() {
    let spec = mutation_spec().with_retry(RetryClass::Safe, false);
    assert_eq!(spec.validate(), Err(SpecError::UnsafeRetryClaim));

    let spec = mutation_spec().with_retry(RetryClass::Safe, true);
    assert!(spec.validate().is_ok());
}

#[test]
fn risky_mutations_require_planning() {
    let spec = mutation_spec().with_safety(RiskClass::Destructive, Reversibility::Conditional);
    assert_eq!(spec.validate(), Err(SpecError::RiskyMutationWithoutPlan));

    let spec = spec.with_lifecycle(
        CapabilitySupport::Required,
        CapabilitySupport::Optional,
        CapabilitySupport::Optional,
        CapabilitySupport::Required,
        CapabilitySupport::Unsupported,
    );
    assert!(spec.validate().is_ok());
}

#[test]
fn duplicate_alternatives_are_rejected() {
    let alternative = ParameterGroup::new(["content"]).unwrap();
    let spec = mutation_spec()
        .with_required_any(alternative.clone())
        .with_required_any(alternative);
    assert_eq!(spec.validate(), Err(SpecError::DuplicateAlternative));
}

#[test]
fn empty_alternative_is_rejected() {
    let spec = mutation_spec().with_required_any(ParameterGroup::empty());
    assert_eq!(spec.validate(), Err(SpecError::EmptyAlternative));
}

#[test]
fn capability_requirements_are_validated() {
    let spec = mutation_spec()
        .with_requirement("transport.ssh")
        .unwrap()
        .with_requirement("runtime.docker")
        .unwrap();
    assert_eq!(
        spec.requirements().collect::<Vec<_>>(),
        vec!["runtime.docker", "transport.ssh"]
    );
    assert!(mutation_spec().with_requirement("SSH").is_err());
}

#[derive(Clone, Serialize, Deserialize)]
struct Params {
    host: String,
}

struct HostInspect;

impl OperationDefinition for HostInspect {
    type Parameters = Params;
    type Output = serde_json::Value;

    fn spec() -> OperationSpec {
        OperationSpec::new(
            OperationName::new("host.inspect").unwrap(),
            TargetKind::Host,
            AccessClass::Read,
        )
    }

    fn target(parameters: &Self::Parameters) -> Result<TargetRef, TargetRefError> {
        TargetRef::new(TargetKind::Host, parameters.host.clone())
    }
}

#[test]
fn schema_ids_follow_operation_name_and_version() {
    let spec = mutation_spec().with_schema_version(3);
    assert_eq!(
        spec.parameter_schema().as_str(),
        "schema.operations.container.restart.parameters.v3"
    );
    assert_eq!(
        spec.result_schema().as_str(),
        "schema.operations.container.restart.result.v3"
    );
    assert!(spec.validate().is_ok());

    let mismatched = mutation_spec().with_schema_ids(
        SchemaId::new("schema.operations.container.stop.parameters.v1").unwrap(),
        SchemaId::new("schema.operations.container.stop.result.v1").unwrap(),
    );
    assert_eq!(
        mismatched.validate(),
        Err(SpecError::SchemaIdentityMismatch)
    );
}

#[test]
fn diagnostic_codes_are_sorted_unique_contract_metadata() {
    let spec = mutation_spec()
        .with_diagnostic_code(DiagnosticCode::new("target.not_found").unwrap())
        .with_diagnostic_code(DiagnosticCode::new("backend.unavailable").unwrap())
        .with_diagnostic_code(DiagnosticCode::new("target.not_found").unwrap());
    assert_eq!(
        spec.diagnostic_codes()
            .map(DiagnosticCode::as_str)
            .collect::<Vec<_>>(),
        vec!["backend.unavailable", "target.not_found"]
    );
    assert!(spec.allows_diagnostic(&DiagnosticCode::new("target.not_found").unwrap()));
    assert!(!spec.allows_diagnostic(&DiagnosticCode::new("plan.stale").unwrap()));
}

#[test]
fn typed_operation_definition_resolves_target() {
    let target = HostInspect::target(&Params {
        host: "devhost".into(),
    })
    .unwrap();
    assert_eq!(target.id(), "devhost");
    assert_eq!(HostInspect::spec().name().as_str(), "host.inspect");
}

use super::*;
use crate::TargetKind;

fn base_plan() -> OperationPlan {
    OperationPlan::new(
        OperationId::parse("01890f5e-7bbd-7cc3-98c8-bdc42b5f35bd").unwrap(),
        OperationName::new("container.restart").unwrap(),
        TargetRef::new(TargetKind::Container, "soma")
            .unwrap()
            .with_host("dookie")
            .unwrap(),
        RiskClass::Disruptive,
        Reversibility::Reversible,
    )
    .unwrap()
}

#[test]
fn identical_plans_have_identical_fingerprints() {
    let first = base_plan()
        .with_topology_revision("fleet:42")
        .unwrap()
        .with_prerequisite("container exists")
        .unwrap();
    let second = base_plan()
        .with_topology_revision("fleet:42")
        .unwrap()
        .with_prerequisite("container exists")
        .unwrap();

    assert_eq!(first.fingerprint(), second.fingerprint());
    first.validate_fingerprint().unwrap();
}

#[test]
fn changing_authorization_relevant_material_changes_fingerprint() {
    let first = base_plan();
    let second = base_plan()
        .with_conflict("maintenance window closed")
        .unwrap();
    assert_ne!(first.fingerprint(), second.fingerprint());
}

#[test]
fn steps_are_contiguous_and_one_based() {
    let step = PlanStep::new(
        2,
        OperationName::new("container.stop").unwrap(),
        TargetRef::new(TargetKind::Container, "soma").unwrap(),
        "stop the container",
    )
    .unwrap();
    assert_eq!(
        base_plan().with_step(step),
        Err(PlanError::InvalidStepSequence)
    );
}

#[test]
fn fingerprint_parser_rejects_non_sha256_values() {
    assert!(PlanFingerprint::parse("abc").is_err());
    assert!(PlanFingerprint::parse("G".repeat(64)).is_err());
}

#[test]
fn plan_can_bind_changes_and_verification() {
    let target = TargetRef::new(TargetKind::Container, "soma").unwrap();
    let change = PlannedChange::new(target.clone(), "restart", "restart container").unwrap();
    let verification = VerificationStrategy::new(
        OperationName::new("container.inspect").unwrap(),
        "confirm the container is running",
    )
    .unwrap();

    let plan = base_plan()
        .with_change(change)
        .unwrap()
        .with_step(
            PlanStep::new(
                1,
                OperationName::new("container.restart").unwrap(),
                target,
                "restart container",
            )
            .unwrap(),
        )
        .unwrap()
        .with_verification(verification)
        .unwrap();

    assert_eq!(plan.changes().len(), 1);
    assert_eq!(plan.steps().len(), 1);
    assert_eq!(
        plan.verification().unwrap().operation().as_str(),
        "container.inspect"
    );
    plan.validate_fingerprint().unwrap();
}

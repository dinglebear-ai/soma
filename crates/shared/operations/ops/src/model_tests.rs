use super::*;

#[test]
fn operation_names_require_lowercase_dotted_segments() {
    assert!(OperationName::new("container.restart").is_ok());
    assert!(OperationName::new("container").is_err());
    assert!(OperationName::new("Container.restart").is_err());
    assert!(OperationName::new("container..restart").is_err());
    assert!(OperationName::new("container.restart_").is_err());
}

#[test]
fn target_reference_preserves_hierarchy_and_revision() {
    let host = TargetRef::new(TargetKind::Host, "dookie").unwrap();
    let target = TargetRef::new(TargetKind::Container, "soma")
        .unwrap()
        .with_host("dookie")
        .unwrap()
        .with_parent(host)
        .unwrap()
        .with_revision("sha256:abc")
        .unwrap();

    assert_eq!(target.host(), Some("dookie"));
    assert_eq!(target.parent().unwrap().id(), "dookie");
    assert_eq!(target.revision(), Some("sha256:abc"));
}

#[test]
fn target_reference_rejects_control_characters() {
    assert!(
        TargetRef::new(
            TargetKind::File,
            "/tmp/bad
path"
        )
        .is_err()
    );
}

#[test]
fn parent_depth_is_bounded() {
    let mut target = TargetRef::new(TargetKind::Host, "0").unwrap();
    for index in 1..MAX_TARGET_DEPTH {
        target = TargetRef::new(TargetKind::Host, index.to_string())
            .unwrap()
            .with_parent(target)
            .unwrap();
    }
    assert!(
        TargetRef::new(TargetKind::Host, "overflow")
            .unwrap()
            .with_parent(target)
            .is_err()
    );
}

#[test]
fn idempotency_keys_are_bounded() {
    assert!(IdempotencyKey::new("restart:dookie:soma:1").is_ok());
    assert!(IdempotencyKey::new("").is_err());
    assert!(IdempotencyKey::new("x".repeat(257)).is_err());
}

#[test]
fn classification_order_tracks_increasing_risk() {
    assert!(RiskClass::Safe < RiskClass::Disruptive);
    assert!(RiskClass::Disruptive < RiskClass::Destructive);
    assert!(RiskClass::Destructive < RiskClass::Privileged);
}

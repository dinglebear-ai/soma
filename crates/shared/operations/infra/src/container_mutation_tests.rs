use std::time::Duration;

use super::*;

#[test]
fn lifecycle_actions_expose_stable_canonical_names() {
    assert_eq!(
        ContainerLifecycleAction::Start.operation_name(),
        "container.start"
    );
    assert_eq!(ContainerLifecycleAction::Stop.action_label(), "stop");
    assert_eq!(
        ContainerLifecycleAction::Restart.operation_name(),
        "container.restart"
    );
    assert_eq!(ContainerLifecycleAction::Pause.action_label(), "pause");
    assert_eq!(
        ContainerLifecycleAction::Resume.operation_name(),
        "container.resume"
    );
}

#[test]
fn requests_and_verification_policies_are_bounded() {
    let deadline = Timestamp::now();
    assert!(ContainerLifecycleRequest::new("", ContainerLifecycleAction::Start, deadline).is_err());
    assert!(
        ContainerLifecycleRequest::new("soma", ContainerLifecycleAction::Start, deadline).is_ok()
    );
    assert!(MutationVerificationPolicy::new(0, Duration::ZERO).is_err());
    assert!(MutationVerificationPolicy::new(21, Duration::ZERO).is_err());
    assert!(MutationVerificationPolicy::new(1, Duration::ZERO).is_ok());
}

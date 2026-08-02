use soma_fleet::HostId;

use super::*;

#[test]
fn retry_policy_preserves_the_declared_contract_after_a_send_attempt() {
    for send_state in [
        MutationSendState::NotSent,
        MutationSendState::Sent,
        MutationSendState::Unknown,
    ] {
        assert_eq!(
            retry_after_failure(send_state, RetryClass::Safe),
            RetryClass::Safe
        );
    }
    assert_eq!(
        retry_after_failure(MutationSendState::NotApplicable, RetryClass::Safe),
        RetryClass::Never
    );
}

#[test]
fn uncertain_send_state_overrides_backend_specific_diagnostics() {
    let error = InfraError::UnsupportedTarget {
        domain: "docker-mutation",
        host: HostId::new("dookie").unwrap(),
    };
    assert_eq!(
        diagnostic_code(&error, MutationSendState::Unknown),
        "mutation.uncertain"
    );
    assert!(next_action(MutationSendState::Unknown).contains("inspect"));
}

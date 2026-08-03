use soma_ops::MutationSendState;

use super::*;

#[test]
fn mutation_failures_preserve_send_state_and_source() {
    let failure = MutationFailure::new(
        MutationSendState::Unknown,
        InfraError::Docker("connection reset".into()),
    );
    assert_eq!(failure.send_state(), MutationSendState::Unknown);
    assert!(
        matches!(failure.error(), InfraError::Docker(message) if message == "connection reset")
    );
}

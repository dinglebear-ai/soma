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

#[test]
fn sha256_hex_is_lowercase_and_sha2_011_compatible() {
    assert_eq!(
        super::sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

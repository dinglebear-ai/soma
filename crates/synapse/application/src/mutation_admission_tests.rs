use super::*;

#[test]
fn admission_failures_keep_stable_operator_guidance() {
    assert_eq!(
        ExecutionError::MissingIdempotencyKey.to_string(),
        "idempotent mutation requires an idempotency key"
    );
    assert_eq!(
        ExecutionError::ConfirmationRequired.to_string(),
        "disruptive mutation requires a confirmation reference"
    );
    assert_eq!(
        ExecutionError::DeadlineExceeded.to_string(),
        "mutation deadline has passed"
    );
}

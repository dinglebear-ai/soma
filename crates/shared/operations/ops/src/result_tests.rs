use serde_json::json;

use super::*;
use crate::TargetKind;

fn target() -> TargetRef {
    TargetRef::new(TargetKind::Container, "soma").unwrap()
}

fn failed_result() -> OperationResult {
    OperationResult::new(
        OperationId::new(),
        OperationName::new("container.restart").unwrap(),
        target(),
        OperationStatus::Failed,
        ExecutionMetadata::new(
            Timestamp::from_unix_millis(10),
            Timestamp::from_unix_millis(20),
            MutationSendState::Unknown,
            RetryClass::Conditional,
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn transport_success_is_not_implicitly_verified() {
    let result = OperationResult::new(
        OperationId::new(),
        OperationName::new("container.restart").unwrap(),
        target(),
        OperationStatus::Succeeded,
        ExecutionMetadata::new(
            Timestamp::from_unix_millis(10),
            Timestamp::from_unix_millis(20),
            MutationSendState::Sent,
            RetryClass::Never,
        )
        .unwrap(),
    )
    .unwrap();

    assert!(result.verification().is_none());
    result.validate().unwrap();
}

#[test]
fn failures_require_error_diagnostics() {
    assert_eq!(
        failed_result().validate(),
        Err(ResultError::FailureWithoutErrorDiagnostic)
    );

    let diagnostic = Diagnostic::new(
        "transport.timeout",
        DiagnosticSeverity::Error,
        "target did not answer before the deadline",
    )
    .unwrap();
    assert_eq!(diagnostic.code(), "transport.timeout");
    assert_eq!(diagnostic.code_id().as_str(), "transport.timeout");
    failed_result()
        .with_diagnostic(diagnostic)
        .validate()
        .unwrap();
}

#[test]
fn failed_operation_cannot_be_verified_successful() {
    let diagnostic = Diagnostic::new(
        "operation.failed",
        DiagnosticSeverity::Error,
        "operation failed",
    )
    .unwrap();
    let verification = VerificationResult::new(
        VerificationStatus::Verified,
        Timestamp::from_unix_millis(30),
    );
    let result = failed_result()
        .with_diagnostic(diagnostic)
        .with_verification(verification)
        .unwrap();
    assert_eq!(result.validate(), Err(ResultError::FailedOperationVerified));
}

#[test]
fn inline_output_is_bounded() {
    let result = OperationResult::new(
        OperationId::new(),
        OperationName::new("host.inspect").unwrap(),
        TargetRef::new(TargetKind::Host, "devhost").unwrap(),
        OperationStatus::Succeeded,
        ExecutionMetadata::new(
            Timestamp::from_unix_millis(10),
            Timestamp::from_unix_millis(20),
            MutationSendState::NotApplicable,
            RetryClass::Never,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(result.clone().with_output(json!({"ok": true})).is_ok());
    assert!(
        result
            .with_output(json!({"data": "x".repeat(300_000)}))
            .is_err()
    );
}

#[test]
fn large_data_can_be_referenced_as_protected_artifact() {
    let artifact = ArtifactRef::new(
        "artifact://operations/123/logs",
        "application/x-ndjson",
        true,
    )
    .unwrap()
    .with_sha256("a".repeat(64))
    .unwrap();
    assert!(artifact.protected());
}

#[test]
fn redaction_labels_are_sorted_and_unique() {
    let metadata = RedactionMetadata::none()
        .with_label("secret")
        .unwrap()
        .with_label("credential")
        .unwrap()
        .with_label("secret")
        .unwrap();
    assert_eq!(metadata.labels(), &["credential", "secret"]);
}

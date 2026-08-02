use std::{convert::Infallible, sync::Mutex};

use super::*;
use crate::{
    ExecutionMetadata, MutationSendState, OperationStatus, RetryClass, TargetKind,
    VerificationStatus,
};

fn producer() -> ProducerRef {
    ProducerRef::new("synapse", "0.6.1").unwrap()
}

fn base_event(payload: OperationEventPayload) -> OperationEventEnvelope {
    OperationEventEnvelope::new(
        Timestamp::from_unix_millis(10),
        OperationId::new(),
        OperationName::new("container.restart").unwrap(),
        CorrelationId::new(),
        producer(),
        payload,
    )
}

#[test]
fn event_type_is_derived_from_payload() {
    let event = base_event(OperationEventPayload::Started {
        topology_revision: Some("fleet:42".into()),
    });
    assert_eq!(event.event_type(), OperationEventType::Started);
    assert_eq!(event.event_version(), 1);
}

#[test]
fn requested_event_does_not_imply_target_or_execution() {
    let event = base_event(OperationEventPayload::Requested {
        parameters_digest: "a".repeat(64),
        metadata: serde_json::json!({"surface": "mcp"}),
    });
    assert_eq!(event.event_type(), OperationEventType::Requested);
    assert!(event.target().is_none());
    event.validate().unwrap();
}

#[test]
fn succeeded_event_can_remain_unverified() {
    let operation_id = OperationId::new();
    let operation = OperationName::new("container.restart").unwrap();
    let target = TargetRef::new(TargetKind::Container, "soma").unwrap();
    let result = OperationResult::new(
        operation_id.clone(),
        operation.clone(),
        target.clone(),
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

    let event = OperationEventEnvelope::new(
        Timestamp::from_unix_millis(20),
        operation_id,
        operation,
        CorrelationId::new(),
        producer(),
        OperationEventPayload::Succeeded(result),
    )
    .with_target(target);
    assert_eq!(event.event_type(), OperationEventType::Succeeded);
    event.validate().unwrap();
}

#[test]
fn verification_is_a_separate_event() {
    let verification = VerificationResult::new(
        VerificationStatus::Verified,
        Timestamp::from_unix_millis(30),
    );
    let event = base_event(OperationEventPayload::Verified(verification));
    assert_eq!(event.event_type(), OperationEventType::Verified);
}

#[test]
fn target_is_required_after_request_resolution() {
    let event = base_event(OperationEventPayload::Started {
        topology_revision: None,
    });
    assert_eq!(event.validate(), Err(EventError::MissingTarget));
}

#[test]
fn requested_parameter_digest_is_validated() {
    let event = base_event(OperationEventPayload::Requested {
        parameters_digest: "not-a-digest".into(),
        metadata: serde_json::Value::Null,
    });
    assert_eq!(event.validate(), Err(EventError::InvalidParametersDigest));
}

#[test]
fn terminal_event_type_must_match_result_status() {
    let operation_id = OperationId::new();
    let operation = OperationName::new("container.restart").unwrap();
    let target = TargetRef::new(TargetKind::Container, "soma").unwrap();
    let result = OperationResult::new(
        operation_id.clone(),
        operation.clone(),
        target.clone(),
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
    .with_diagnostic(
        crate::Diagnostic::new(
            "operation.failed",
            crate::DiagnosticSeverity::Error,
            "operation failed",
        )
        .unwrap(),
    );
    let event = OperationEventEnvelope::new(
        Timestamp::from_unix_millis(20),
        operation_id,
        operation,
        CorrelationId::new(),
        producer(),
        OperationEventPayload::Succeeded(result),
    )
    .with_target(target);
    assert_eq!(event.validate(), Err(EventError::TerminalStatusMismatch));
}

#[test]
fn payload_identity_must_match_envelope() {
    let progress = ProgressEvent::new(
        OperationId::new(),
        OperationName::new("container.restart").unwrap(),
        1,
        Timestamp::from_unix_millis(20),
        "restarting",
    )
    .unwrap();
    let event = base_event(OperationEventPayload::Progressed(progress))
        .with_target(TargetRef::new(TargetKind::Container, "soma").unwrap());
    assert_eq!(event.validate(), Err(EventError::PayloadIdentityMismatch));
}

#[derive(Default)]
struct RecordingSink(Mutex<Vec<EventId>>);

impl EventSink for RecordingSink {
    type Error = Infallible;

    fn emit(&self, event: &OperationEventEnvelope) -> Result<(), Self::Error> {
        self.0.lock().unwrap().push(event.event_id().clone());
        Ok(())
    }
}

#[test]
fn event_sink_receives_stable_event_identity() {
    let sink = RecordingSink::default();
    let event = base_event(OperationEventPayload::Started {
        topology_revision: None,
    });
    sink.emit(&event).unwrap();
    sink.emit(&event).unwrap();
    let ids = sink.0.lock().unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], ids[1]);
}

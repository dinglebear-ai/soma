use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ActorRef, AuthorizationId, CorrelationId, EventId, OperationId, OperationName, OperationPlan,
    OperationResult, ProducerRef, ProgressEvent, RedactionMetadata, TargetRef, Timestamp,
    TraceContext, VerificationResult,
};

/// Canonical operation lifecycle event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OperationEventType {
    /// Intent was accepted before target resolution.
    Requested,
    /// A concrete plan was produced.
    Planned,
    /// Product policy authorized the exact operation and target.
    Authorized,
    /// External execution began and may now affect target state.
    Started,
    /// Bounded execution progress was observed.
    Progressed,
    /// Execution completed successfully, independent of verification.
    Succeeded,
    /// Execution failed.
    Failed,
    /// Execution was cancelled.
    Cancelled,
    /// Runtime state was independently verified or found inconclusive.
    Verified,
}

/// Lifecycle-specific operation event payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
#[non_exhaustive]
pub enum OperationEventPayload {
    /// Request metadata safe to persist before target resolution.
    Requested {
        /// SHA-256 or equivalent digest of redacted request parameters.
        parameters_digest: String,
        /// Additional bounded, redacted request metadata.
        #[serde(default)]
        metadata: Value,
    },
    /// Immutable authorization-relevant plan.
    Planned(OperationPlan),
    /// Opaque reference to product-issued authorization evidence.
    Authorized {
        /// Authorization identity.
        authorization_id: AuthorizationId,
        /// Optional human-confirmation reference safe for audit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confirmation_ref: Option<String>,
    },
    /// Concrete target at the point mutation may begin.
    Started {
        /// Bound topology revision when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        topology_revision: Option<String>,
    },
    /// One bounded progress update.
    Progressed(ProgressEvent),
    /// Successful terminal execution result.
    Succeeded(OperationResult),
    /// Failed terminal execution result.
    Failed(OperationResult),
    /// Cancelled terminal execution result.
    Cancelled(OperationResult),
    /// Independent runtime verification.
    Verified(VerificationResult),
}

impl OperationEventPayload {
    /// Returns the event type corresponding to this payload.
    #[must_use]
    pub const fn event_type(&self) -> OperationEventType {
        match self {
            Self::Requested { .. } => OperationEventType::Requested,
            Self::Planned(_) => OperationEventType::Planned,
            Self::Authorized { .. } => OperationEventType::Authorized,
            Self::Started { .. } => OperationEventType::Started,
            Self::Progressed(_) => OperationEventType::Progressed,
            Self::Succeeded(_) => OperationEventType::Succeeded,
            Self::Failed(_) => OperationEventType::Failed,
            Self::Cancelled(_) => OperationEventType::Cancelled,
            Self::Verified(_) => OperationEventType::Verified,
        }
    }
}

/// Common envelope for operation lifecycle events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OperationEventEnvelope {
    event_id: EventId,
    event_version: u32,
    event_type: OperationEventType,
    occurred_at: Timestamp,
    operation_id: OperationId,
    operation: OperationName,
    correlation_id: CorrelationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    causation_id: Option<OperationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    actor: Option<ActorRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<TargetRef>,
    producer: ProducerRef,
    #[serde(default)]
    trace: TraceContext,
    payload: OperationEventPayload,
    #[serde(default)]
    redaction: RedactionMetadata,
}

/// Backward-compatible short name for the canonical event envelope.
pub type OperationEvent = OperationEventEnvelope;

impl OperationEventEnvelope {
    /// Creates a version-one event envelope and derives its event type from the payload.
    #[must_use]
    pub fn new(
        occurred_at: Timestamp,
        operation_id: OperationId,
        operation: OperationName,
        correlation_id: CorrelationId,
        producer: ProducerRef,
        payload: OperationEventPayload,
    ) -> Self {
        Self {
            event_id: EventId::new(),
            event_version: 1,
            event_type: payload.event_type(),
            occurred_at,
            operation_id,
            operation,
            correlation_id,
            causation_id: None,
            actor: None,
            target: None,
            producer,
            trace: TraceContext::default(),
            payload,
            redaction: RedactionMetadata::none(),
        }
    }

    /// Records the operation that caused this event's operation.
    #[must_use]
    pub fn with_causation_id(mut self, causation_id: OperationId) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    /// Records the actor when known.
    #[must_use]
    pub fn with_actor(mut self, actor: ActorRef) -> Self {
        self.actor = Some(actor);
        self
    }

    /// Records the resolved target when known.
    #[must_use]
    pub fn with_target(mut self, target: TargetRef) -> Self {
        self.target = Some(target);
        self
    }

    /// Adds trace context.
    #[must_use]
    pub fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = trace;
        self
    }

    /// Adds redaction metadata.
    #[must_use]
    pub fn with_redaction(mut self, redaction: RedactionMetadata) -> Self {
        self.redaction = redaction;
        self
    }

    /// Validates envelope, payload, target, and terminal-status consistency.
    pub fn validate(&self) -> Result<(), EventError> {
        validate_envelope(self)
    }

    /// Returns the stable event identity.
    #[must_use]
    pub fn event_id(&self) -> &EventId {
        &self.event_id
    }

    /// Returns the schema version.
    #[must_use]
    pub const fn event_version(&self) -> u32 {
        self.event_version
    }

    /// Returns the lifecycle event type.
    #[must_use]
    pub const fn event_type(&self) -> OperationEventType {
        self.event_type
    }

    /// Returns event occurrence time.
    #[must_use]
    pub const fn occurred_at(&self) -> Timestamp {
        self.occurred_at
    }

    /// Returns operation execution identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns canonical operation name.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }

    /// Returns workflow correlation identity.
    #[must_use]
    pub fn correlation_id(&self) -> &CorrelationId {
        &self.correlation_id
    }

    /// Returns target when resolved.
    #[must_use]
    pub fn target(&self) -> Option<&TargetRef> {
        self.target.as_ref()
    }

    /// Returns producing component identity.
    #[must_use]
    pub fn producer(&self) -> &ProducerRef {
        &self.producer
    }

    /// Returns lifecycle payload.
    #[must_use]
    pub fn payload(&self) -> &OperationEventPayload {
        &self.payload
    }

    /// Returns redaction metadata.
    #[must_use]
    pub fn redaction(&self) -> &RedactionMetadata {
        &self.redaction
    }
}

fn require_target(target: Option<&TargetRef>) -> Result<(), EventError> {
    target.map(|_| ()).ok_or(EventError::MissingTarget)
}

fn validate_terminal_payload(
    envelope: &OperationEventEnvelope,
    result: &OperationResult,
    expected_status: crate::OperationStatus,
) -> Result<(), EventError> {
    require_target(envelope.target.as_ref())?;
    if result.operation_id() != &envelope.operation_id
        || result.operation() != &envelope.operation
        || Some(result.target()) != envelope.target.as_ref()
    {
        return Err(EventError::PayloadIdentityMismatch);
    }
    if result.status() != expected_status {
        return Err(EventError::TerminalStatusMismatch);
    }
    result
        .validate()
        .map_err(|_| EventError::InvalidTerminalResult)
}

fn validate_sha256(value: &str) -> Result<(), EventError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(EventError::InvalidParametersDigest)
    }
}

/// Invalid lifecycle event envelope.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EventError {
    /// Event schema versions are one-based.
    #[error("operation event version must be greater than zero")]
    ZeroVersion,
    /// The stored event type does not match its tagged payload.
    #[error("operation event type does not match payload")]
    EventTypeMismatch,
    /// A lifecycle stage that requires a resolved target omitted it.
    #[error("operation event requires a resolved target")]
    MissingTarget,
    /// Payload operation identity, name, or target differs from the envelope.
    #[error("operation event payload identity does not match envelope")]
    PayloadIdentityMismatch,
    /// Terminal payload status differs from the event type.
    #[error("terminal operation result status does not match event type")]
    TerminalStatusMismatch,
    /// The terminal result violates result invariants.
    #[error("terminal operation result is invalid")]
    InvalidTerminalResult,
    /// The embedded plan fingerprint is invalid.
    #[error("planned operation event contains an invalid plan fingerprint")]
    InvalidPlanFingerprint,
    /// Requested parameter digest is not lowercase SHA-256.
    #[error("requested operation parameters digest must be lowercase SHA-256")]
    InvalidParametersDigest,
}

fn validate_envelope(envelope: &OperationEventEnvelope) -> Result<(), EventError> {
    if envelope.event_version == 0 {
        return Err(EventError::ZeroVersion);
    }
    if envelope.event_type != envelope.payload.event_type() {
        return Err(EventError::EventTypeMismatch);
    }
    match &envelope.payload {
        OperationEventPayload::Requested {
            parameters_digest, ..
        } => validate_sha256(parameters_digest)?,
        OperationEventPayload::Planned(plan) => {
            require_target(envelope.target.as_ref())?;
            if plan.operation_id() != &envelope.operation_id
                || plan.operation() != &envelope.operation
                || Some(plan.target()) != envelope.target.as_ref()
            {
                return Err(EventError::PayloadIdentityMismatch);
            }
            plan.validate_fingerprint()
                .map_err(|_| EventError::InvalidPlanFingerprint)?;
        }
        OperationEventPayload::Authorized { .. }
        | OperationEventPayload::Started { .. }
        | OperationEventPayload::Verified(_) => {
            require_target(envelope.target.as_ref())?;
        }
        OperationEventPayload::Progressed(progress) => {
            require_target(envelope.target.as_ref())?;
            if progress.operation_id() != &envelope.operation_id
                || progress.operation() != &envelope.operation
            {
                return Err(EventError::PayloadIdentityMismatch);
            }
        }
        OperationEventPayload::Succeeded(result) => {
            validate_terminal_payload(envelope, result, crate::OperationStatus::Succeeded)?;
        }
        OperationEventPayload::Failed(result) => {
            validate_terminal_payload(envelope, result, crate::OperationStatus::Failed)?;
        }
        OperationEventPayload::Cancelled(result) => {
            validate_terminal_payload(envelope, result, crate::OperationStatus::Cancelled)?;
        }
    }
    Ok(())
}

/// Sink for operation lifecycle events, implemented by embedded or remote adapters.
pub trait EventSink: Send + Sync {
    /// Sink-specific delivery error.
    type Error;

    /// Delivers one idempotent event envelope.
    fn emit(&self, event: &OperationEventEnvelope) -> Result<(), Self::Error>;
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;

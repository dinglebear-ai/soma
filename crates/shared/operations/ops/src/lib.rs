//! Product-neutral contracts for safe infrastructure operations.
//!
//! `soma-ops` defines stable operation identities, typed request envelopes,
//! catalog metadata, authorization evidence, deterministic plans, bounded
//! progress, terminal results, verification, and lifecycle events. It does not
//! know about Soma principals, Synapse scopes, MCP elicitation, CLI prompts,
//! Docker, Incus, SSH, databases, or product configuration.
//!
//! Product applications translate their own identity and policy into these
//! contracts. Concrete engines such as `soma-infra` implement operation
//! definitions over these types.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod catalog;
mod contract_id;
mod event;
mod identity;
mod model;
mod plan;
mod progress;
mod request;
mod result;

pub use catalog::{
    CapabilitySupport, EvidenceKind, OperationDefinition, OperationSpec, ParameterGroup, SpecError,
};
pub use contract_id::{DiagnosticCode, DiagnosticCodeError, SchemaId, SchemaIdError};
pub use event::{
    EventError, EventSink, OperationEvent, OperationEventEnvelope, OperationEventPayload,
    OperationEventType,
};
pub use identity::{
    ActorRef, AuthorizationId, CorrelationId, EventId, IdentityError, OperationId, ProducerRef,
    Timestamp, TraceContext,
};
pub use model::{
    AccessClass, IdempotencyKey, IdempotencyKeyError, MutationSendState, OperationName,
    OperationNameError, RetryClass, Reversibility, RiskClass, TargetKind, TargetRef,
    TargetRefError, VerificationStatus,
};
pub use plan::{
    OperationPlan, PlanError, PlanFingerprint, PlanStep, PlannedChange, VerificationStrategy,
};
pub use progress::{NoopProgressSink, ProgressError, ProgressEvent, ProgressSink};
pub use request::{
    AuthorizationError, AuthorizationEvidence, AuthorizationScope, OperationContext,
    OperationRequest, RequestError,
};
pub use result::{
    ArtifactRef, Diagnostic, DiagnosticSeverity, EvidenceRef, ExecutionMetadata, OperationResult,
    OperationStatus, RedactionMetadata, ResultError, VerificationResult,
};

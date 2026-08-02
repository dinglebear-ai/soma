use serde::{Deserialize, Serialize};

use crate::{
    AccessClass, ActorRef, AuthorizationId, CorrelationId, IdempotencyKey, OperationDefinition,
    OperationId, OperationName, OperationSpec, PlanFingerprint, ProducerRef, SpecError, TargetKind,
    TargetRef, TargetRefError, Timestamp, TraceContext,
};

const MAX_CONFIRMATION_REF_CHARS: usize = 256;

/// Traceable execution context supplied by the calling product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OperationContext {
    operation_id: OperationId,
    correlation_id: CorrelationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    causation_id: Option<OperationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    actor: Option<ActorRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    idempotency_key: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deadline: Option<Timestamp>,
    #[serde(default)]
    trace: TraceContext,
}

impl OperationContext {
    /// Creates a context with fresh operation and correlation identities.
    #[must_use]
    pub fn new() -> Self {
        Self {
            operation_id: OperationId::new(),
            correlation_id: CorrelationId::new(),
            causation_id: None,
            actor: None,
            idempotency_key: None,
            deadline: None,
            trace: TraceContext::default(),
        }
    }

    /// Uses an existing workflow correlation identity.
    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: CorrelationId) -> Self {
        self.correlation_id = correlation_id;
        self
    }

    /// Records the operation that caused this operation.
    #[must_use]
    pub fn with_causation_id(mut self, causation_id: OperationId) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    /// Records the requesting actor.
    #[must_use]
    pub fn with_actor(mut self, actor: ActorRef) -> Self {
        self.actor = Some(actor);
        self
    }

    /// Adds a caller-provided idempotency key.
    #[must_use]
    pub fn with_idempotency_key(mut self, idempotency_key: IdempotencyKey) -> Self {
        self.idempotency_key = Some(idempotency_key);
        self
    }

    /// Sets an absolute deadline.
    #[must_use]
    pub fn with_deadline(mut self, deadline: Timestamp) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Adds trace context.
    #[must_use]
    pub fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = trace;
        self
    }

    /// Returns the execution identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the workflow correlation identity.
    #[must_use]
    pub fn correlation_id(&self) -> &CorrelationId {
        &self.correlation_id
    }

    /// Returns the causal operation when present.
    #[must_use]
    pub fn causation_id(&self) -> Option<&OperationId> {
        self.causation_id.as_ref()
    }

    /// Returns the requesting actor when known.
    #[must_use]
    pub fn actor(&self) -> Option<&ActorRef> {
        self.actor.as_ref()
    }

    /// Returns the idempotency key when present.
    #[must_use]
    pub fn idempotency_key(&self) -> Option<&IdempotencyKey> {
        self.idempotency_key.as_ref()
    }

    /// Returns the absolute deadline when present.
    #[must_use]
    pub const fn deadline(&self) -> Option<Timestamp> {
        self.deadline
    }

    /// Returns propagated trace context.
    #[must_use]
    pub fn trace(&self) -> &TraceContext {
        &self.trace
    }
}

impl Default for OperationContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Exact operation and target scope approved by product policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AuthorizationScope {
    operation: OperationName,
    target: TargetRef,
}

impl AuthorizationScope {
    /// Creates an exact authorization scope.
    #[must_use]
    pub const fn new(operation: OperationName, target: TargetRef) -> Self {
        Self { operation, target }
    }

    /// Returns the authorized operation.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }

    /// Returns the authorized target.
    #[must_use]
    pub fn target(&self) -> &TargetRef {
        &self.target
    }
}

/// Opaque, time-bounded authorization evidence issued by a product layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AuthorizationEvidence {
    id: AuthorizationId,
    issuer: ProducerRef,
    scope: AuthorizationScope,
    issued_at: Timestamp,
    expires_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plan_fingerprint: Option<PlanFingerprint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confirmation_ref: Option<String>,
}

impl AuthorizationEvidence {
    /// Creates time-bounded authorization evidence.
    pub fn new(
        issuer: ProducerRef,
        scope: AuthorizationScope,
        issued_at: Timestamp,
        expires_at: Timestamp,
    ) -> Result<Self, AuthorizationError> {
        if expires_at <= issued_at {
            return Err(AuthorizationError::InvalidLifetime);
        }
        Ok(Self {
            id: AuthorizationId::new(),
            issuer,
            scope,
            issued_at,
            expires_at,
            plan_fingerprint: None,
            confirmation_ref: None,
        })
    }

    /// Binds authorization to an immutable plan fingerprint.
    #[must_use]
    pub fn with_plan_fingerprint(mut self, fingerprint: PlanFingerprint) -> Self {
        self.plan_fingerprint = Some(fingerprint);
        self
    }

    /// Records an opaque human-confirmation reference.
    pub fn with_confirmation_ref(
        mut self,
        confirmation_ref: impl Into<String>,
    ) -> Result<Self, AuthorizationError> {
        let confirmation_ref = confirmation_ref.into();
        let chars = confirmation_ref.chars().count();
        if chars == 0
            || chars > MAX_CONFIRMATION_REF_CHARS
            || confirmation_ref.chars().any(char::is_control)
        {
            return Err(AuthorizationError::InvalidConfirmationRef);
        }
        self.confirmation_ref = Some(confirmation_ref);
        Ok(self)
    }

    /// Validates expiration and exact operation, target, and plan binding.
    pub fn validate_binding(
        &self,
        operation: &OperationName,
        target: &TargetRef,
        now: Timestamp,
        expected_plan: Option<&PlanFingerprint>,
    ) -> Result<(), AuthorizationError> {
        if now < self.issued_at {
            return Err(AuthorizationError::NotYetValid);
        }
        if now >= self.expires_at {
            return Err(AuthorizationError::Expired);
        }
        if self.scope.operation() != operation {
            return Err(AuthorizationError::OperationMismatch);
        }
        if self.scope.target() != target {
            return Err(AuthorizationError::TargetMismatch);
        }
        if self.plan_fingerprint.as_ref() != expected_plan {
            return Err(AuthorizationError::PlanMismatch);
        }
        Ok(())
    }

    /// Returns the evidence identity.
    #[must_use]
    pub fn id(&self) -> &AuthorizationId {
        &self.id
    }

    /// Returns the issuing product or policy component.
    #[must_use]
    pub fn issuer(&self) -> &ProducerRef {
        &self.issuer
    }

    /// Returns the exact approved scope.
    #[must_use]
    pub fn scope(&self) -> &AuthorizationScope {
        &self.scope
    }

    /// Returns the authorization expiry.
    #[must_use]
    pub const fn expires_at(&self) -> Timestamp {
        self.expires_at
    }

    /// Returns the bound plan fingerprint when present.
    #[must_use]
    pub fn plan_fingerprint(&self) -> Option<&PlanFingerprint> {
        self.plan_fingerprint.as_ref()
    }

    /// Returns the opaque confirmation reference when present.
    #[must_use]
    pub fn confirmation_ref(&self) -> Option<&str> {
        self.confirmation_ref.as_deref()
    }
}

/// Typed request envelope for one concrete operation definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OperationRequest<P> {
    context: OperationContext,
    operation: OperationName,
    target: TargetRef,
    parameters: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    authorization: Option<AuthorizationEvidence>,
}

impl<P> OperationRequest<P> {
    /// Builds a request from a typed operation definition.
    pub fn new<O>(context: OperationContext, parameters: P) -> Result<Self, TargetRefError>
    where
        O: OperationDefinition<Parameters = P>,
    {
        let spec = O::spec();
        let target = O::target(&parameters)?;
        Ok(Self {
            context,
            operation: spec.name().clone(),
            target,
            parameters,
            authorization: None,
        })
    }

    /// Adds product-issued authorization evidence.
    #[must_use]
    pub fn with_authorization(mut self, authorization: AuthorizationEvidence) -> Self {
        self.authorization = Some(authorization);
        self
    }

    /// Validates the request against catalog and authorization metadata.
    pub fn validate_against(
        &self,
        spec: &OperationSpec,
        now: Timestamp,
        expected_plan: Option<&PlanFingerprint>,
    ) -> Result<(), RequestError> {
        spec.validate()?;
        if &self.operation != spec.name() {
            return Err(RequestError::OperationMismatch);
        }
        if self.target.kind() != spec.target_kind() {
            return Err(RequestError::TargetKindMismatch {
                expected: spec.target_kind().clone(),
                actual: self.target.kind().clone(),
            });
        }
        if self
            .context
            .deadline()
            .is_some_and(|deadline| deadline <= now)
        {
            return Err(RequestError::DeadlineExceeded);
        }
        if spec.access() == AccessClass::Mutation
            && spec.idempotent()
            && self.context.idempotency_key().is_none()
        {
            return Err(RequestError::MissingIdempotencyKey);
        }
        match (&self.authorization, spec.access()) {
            (None, AccessClass::Mutation) => Err(RequestError::MissingAuthorization),
            (Some(authorization), _) => authorization
                .validate_binding(&self.operation, &self.target, now, expected_plan)
                .map_err(RequestError::Authorization),
            (None, AccessClass::Read) => Ok(()),
        }
    }

    /// Returns execution context.
    #[must_use]
    pub fn context(&self) -> &OperationContext {
        &self.context
    }

    /// Returns the canonical operation name.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }

    /// Returns the resolved target.
    #[must_use]
    pub fn target(&self) -> &TargetRef {
        &self.target
    }

    /// Returns typed operation parameters.
    #[must_use]
    pub fn parameters(&self) -> &P {
        &self.parameters
    }

    /// Returns authorization evidence when supplied.
    #[must_use]
    pub fn authorization(&self) -> Option<&AuthorizationEvidence> {
        self.authorization.as_ref()
    }
}

/// Authorization evidence validation failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AuthorizationError {
    /// Expiry must be later than issue time.
    #[error("authorization expiry must be later than issue time")]
    InvalidLifetime,
    /// Authorization issue time is in the future.
    #[error("authorization is not yet valid")]
    NotYetValid,
    /// Authorization has expired.
    #[error("authorization has expired")]
    Expired,
    /// The operation does not match the approved scope.
    #[error("authorization operation does not match request")]
    OperationMismatch,
    /// The target does not match the approved scope.
    #[error("authorization target does not match request")]
    TargetMismatch,
    /// Plan binding differs, including missing versus present binding.
    #[error("authorization plan fingerprint does not match request")]
    PlanMismatch,
    /// The confirmation reference was empty, oversized, or contained control characters.
    #[error("invalid authorization confirmation reference")]
    InvalidConfirmationRef,
}

/// Request validation failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RequestError {
    /// Operation specification is invalid.
    #[error(transparent)]
    Spec(#[from] SpecError),
    /// Request operation differs from the supplied specification.
    #[error("request operation does not match specification")]
    OperationMismatch,
    /// Target kind differs from the supplied specification.
    #[error("request target kind mismatch: expected {expected:?}, found {actual:?}")]
    TargetKindMismatch {
        /// Expected target kind.
        expected: TargetKind,
        /// Actual target kind.
        actual: TargetKind,
    },
    /// Absolute deadline has passed.
    #[error("operation deadline has passed")]
    DeadlineExceeded,
    /// An idempotent mutation omitted its idempotency key.
    #[error("idempotent mutation requires an idempotency key")]
    MissingIdempotencyKey,
    /// A mutation omitted authorization evidence.
    #[error("mutation requires authorization evidence")]
    MissingAuthorization,
    /// Authorization evidence is invalid for the request.
    #[error(transparent)]
    Authorization(#[from] AuthorizationError),
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;

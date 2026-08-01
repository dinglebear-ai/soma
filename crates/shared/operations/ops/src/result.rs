use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    MutationSendState, OperationId, OperationName, RetryClass, TargetRef, Timestamp,
    VerificationStatus,
};

const MAX_URI_CHARS: usize = 2_048;
const MAX_MEDIA_TYPE_CHARS: usize = 256;
const MAX_DIAGNOSTIC_CHARS: usize = 4_096;
const MAX_REDACTION_LABEL_CHARS: usize = 128;

/// Terminal operation status before independent verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OperationStatus {
    /// Execution completed successfully.
    Succeeded,
    /// Execution failed.
    Failed,
    /// Execution was cancelled.
    Cancelled,
}

/// Severity of one structured diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DiagnosticSeverity {
    /// Informational context.
    Info,
    /// Recoverable or degraded behavior.
    Warning,
    /// Operation failure or invalid state.
    Error,
}

/// Correctable diagnostic returned by an operation engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Diagnostic {
    code: String,
    severity: DiagnosticSeverity,
    message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next_action: Option<String>,
}

impl Diagnostic {
    /// Creates a validated diagnostic.
    pub fn new(
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Result<Self, ResultError> {
        let code = code.into();
        let message = message.into();
        validate_code(&code)?;
        validate_text("diagnostic message", &message, MAX_DIAGNOSTIC_CHARS)?;
        Ok(Self {
            code,
            severity,
            message,
            field: None,
            next_action: None,
        })
    }

    /// Adds the request field associated with the diagnostic.
    pub fn with_field(mut self, field: impl Into<String>) -> Result<Self, ResultError> {
        let field = field.into();
        validate_code(&field)?;
        self.field = Some(field);
        Ok(self)
    }

    /// Adds a bounded corrective next action.
    pub fn with_next_action(mut self, next_action: impl Into<String>) -> Result<Self, ResultError> {
        let next_action = next_action.into();
        validate_text("diagnostic next action", &next_action, MAX_DIAGNOSTIC_CHARS)?;
        self.next_action = Some(next_action);
        Ok(self)
    }

    /// Returns the stable diagnostic code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns diagnostic severity.
    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    /// Returns the human-readable diagnostic.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Reference to a protected or durable operation artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ArtifactRef {
    uri: String,
    media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    protected: bool,
}

impl ArtifactRef {
    /// Creates a validated artifact reference.
    pub fn new(
        uri: impl Into<String>,
        media_type: impl Into<String>,
        protected: bool,
    ) -> Result<Self, ResultError> {
        let uri = uri.into();
        let media_type = media_type.into();
        validate_text("artifact URI", &uri, MAX_URI_CHARS)?;
        validate_text("artifact media type", &media_type, MAX_MEDIA_TYPE_CHARS)?;
        Ok(Self {
            uri,
            media_type,
            sha256: None,
            protected,
        })
    }

    /// Adds a lowercase SHA-256 content digest.
    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Result<Self, ResultError> {
        let sha256 = sha256.into();
        validate_sha256(&sha256)?;
        self.sha256 = Some(sha256);
        Ok(self)
    }

    /// Returns the artifact URI.
    #[must_use]
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// Returns whether access should require protected artifact policy.
    #[must_use]
    pub const fn protected(&self) -> bool {
        self.protected
    }
}

/// Reference to evidence captured by an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EvidenceRef {
    kind: String,
    reference: String,
}

impl EvidenceRef {
    /// Creates a validated evidence reference.
    pub fn new(kind: impl Into<String>, reference: impl Into<String>) -> Result<Self, ResultError> {
        let kind = kind.into();
        let reference = reference.into();
        validate_code(&kind)?;
        validate_text("evidence reference", &reference, MAX_URI_CHARS)?;
        Ok(Self { kind, reference })
    }

    /// Returns the evidence kind.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the evidence locator.
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }
}

/// Description of fields or artifacts removed from a result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RedactionMetadata {
    applied: bool,
    labels: Vec<String>,
}

impl RedactionMetadata {
    /// Creates empty redaction metadata.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            applied: false,
            labels: Vec::new(),
        }
    }

    /// Records a redaction label.
    pub fn with_label(mut self, label: impl Into<String>) -> Result<Self, ResultError> {
        let label = label.into();
        validate_text("redaction label", &label, MAX_REDACTION_LABEL_CHARS)?;
        self.applied = true;
        if !self.labels.contains(&label) {
            self.labels.push(label);
            self.labels.sort();
        }
        Ok(self)
    }

    /// Returns whether any redaction was applied.
    #[must_use]
    pub const fn applied(&self) -> bool {
        self.applied
    }

    /// Returns sorted unique redaction labels.
    #[must_use]
    pub fn labels(&self) -> &[String] {
        &self.labels
    }
}

/// Independent runtime-state verification result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct VerificationResult {
    status: VerificationStatus,
    completed_at: Timestamp,
    diagnostics: Vec<Diagnostic>,
    evidence: Vec<EvidenceRef>,
}

impl VerificationResult {
    /// Creates a verification result.
    #[must_use]
    pub const fn new(status: VerificationStatus, completed_at: Timestamp) -> Self {
        Self {
            status,
            completed_at,
            diagnostics: Vec::new(),
            evidence: Vec::new(),
        }
    }

    /// Adds a structured diagnostic.
    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.diagnostics.push(diagnostic);
        self
    }

    /// Adds evidence supporting the verification outcome.
    #[must_use]
    pub fn with_evidence(mut self, evidence: EvidenceRef) -> Self {
        self.evidence.push(evidence);
        self
    }

    /// Returns the verification outcome.
    #[must_use]
    pub const fn status(&self) -> VerificationStatus {
        self.status
    }

    /// Returns verification completion time.
    #[must_use]
    pub const fn completed_at(&self) -> Timestamp {
        self.completed_at
    }

    /// Returns verification evidence.
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceRef] {
        &self.evidence
    }
}

/// Timing, mutation-send, and retry metadata for terminal execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExecutionMetadata {
    started_at: Timestamp,
    completed_at: Timestamp,
    mutation_send_state: MutationSendState,
    retry: RetryClass,
}

impl ExecutionMetadata {
    /// Creates validated execution metadata.
    pub fn new(
        started_at: Timestamp,
        completed_at: Timestamp,
        mutation_send_state: MutationSendState,
        retry: RetryClass,
    ) -> Result<Self, ResultError> {
        if completed_at < started_at {
            return Err(ResultError::CompletionBeforeStart);
        }
        Ok(Self {
            started_at,
            completed_at,
            mutation_send_state,
            retry,
        })
    }

    /// Returns execution start time.
    #[must_use]
    pub const fn started_at(self) -> Timestamp {
        self.started_at
    }

    /// Returns execution completion time.
    #[must_use]
    pub const fn completed_at(self) -> Timestamp {
        self.completed_at
    }

    /// Returns whether a mutation may have reached the target.
    #[must_use]
    pub const fn mutation_send_state(self) -> MutationSendState {
        self.mutation_send_state
    }

    /// Returns retry classification.
    #[must_use]
    pub const fn retry(self) -> RetryClass {
        self.retry
    }
}

/// Terminal result for one operation execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OperationResult {
    operation_id: OperationId,
    operation: OperationName,
    target: TargetRef,
    status: OperationStatus,
    started_at: Timestamp,
    completed_at: Timestamp,
    mutation_send_state: MutationSendState,
    retry: RetryClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output: Option<Value>,
    artifacts: Vec<ArtifactRef>,
    diagnostics: Vec<Diagnostic>,
    evidence: Vec<EvidenceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verification: Option<VerificationResult>,
    #[serde(default)]
    redaction: RedactionMetadata,
}

impl OperationResult {
    /// Creates a validated terminal result.
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        target: TargetRef,
        status: OperationStatus,
        execution: ExecutionMetadata,
    ) -> Result<Self, ResultError> {
        if status == OperationStatus::Succeeded && execution.retry() != RetryClass::Never {
            return Err(ResultError::RetryOnSuccess);
        }
        Ok(Self {
            operation_id,
            operation,
            target,
            status,
            started_at: execution.started_at(),
            completed_at: execution.completed_at(),
            mutation_send_state: execution.mutation_send_state(),
            retry: execution.retry(),
            output: None,
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            evidence: Vec::new(),
            verification: None,
            redaction: RedactionMetadata::none(),
        })
    }

    /// Adds bounded structured output.
    pub fn with_output(mut self, output: Value) -> Result<Self, ResultError> {
        const MAX_INLINE_BYTES: usize = 256 * 1_024;
        let encoded = serde_json::to_vec(&output)
            .map_err(|error| ResultError::OutputSerialization(error.to_string()))?;
        if encoded.len() > MAX_INLINE_BYTES {
            return Err(ResultError::InlineOutputTooLarge {
                bytes: encoded.len(),
                max_bytes: MAX_INLINE_BYTES,
            });
        }
        self.output = Some(output);
        Ok(self)
    }

    /// Adds an artifact reference.
    #[must_use]
    pub fn with_artifact(mut self, artifact: ArtifactRef) -> Self {
        self.artifacts.push(artifact);
        self
    }

    /// Adds a structured diagnostic.
    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.diagnostics.push(diagnostic);
        self
    }

    /// Adds captured evidence.
    #[must_use]
    pub fn with_evidence(mut self, evidence: EvidenceRef) -> Self {
        self.evidence.push(evidence);
        self
    }

    /// Adds an independent verification result.
    pub fn with_verification(
        mut self,
        verification: VerificationResult,
    ) -> Result<Self, ResultError> {
        if verification.completed_at() < self.completed_at {
            return Err(ResultError::VerificationBeforeCompletion);
        }
        self.verification = Some(verification);
        Ok(self)
    }

    /// Adds redaction metadata.
    #[must_use]
    pub fn with_redaction(mut self, redaction: RedactionMetadata) -> Self {
        self.redaction = redaction;
        self
    }

    /// Validates terminal-status diagnostics and verification invariants.
    pub fn validate(&self) -> Result<(), ResultError> {
        if self.status != OperationStatus::Succeeded
            && !self
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        {
            return Err(ResultError::FailureWithoutErrorDiagnostic);
        }
        if self.status != OperationStatus::Succeeded
            && self
                .verification
                .as_ref()
                .is_some_and(|verification| verification.status() == VerificationStatus::Verified)
        {
            return Err(ResultError::FailedOperationVerified);
        }
        Ok(())
    }

    /// Returns the operation identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
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

    /// Returns execution status.
    #[must_use]
    pub const fn status(&self) -> OperationStatus {
        self.status
    }

    /// Returns mutation-send state.
    #[must_use]
    pub const fn mutation_send_state(&self) -> MutationSendState {
        self.mutation_send_state
    }

    /// Returns retry classification.
    #[must_use]
    pub const fn retry(&self) -> RetryClass {
        self.retry
    }

    /// Returns inline output when present.
    #[must_use]
    pub fn output(&self) -> Option<&Value> {
        self.output.as_ref()
    }

    /// Returns artifact references.
    #[must_use]
    pub fn artifacts(&self) -> &[ArtifactRef] {
        &self.artifacts
    }

    /// Returns diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns verification when performed.
    #[must_use]
    pub fn verification(&self) -> Option<&VerificationResult> {
        self.verification.as_ref()
    }

    /// Returns redaction metadata.
    #[must_use]
    pub fn redaction(&self) -> &RedactionMetadata {
        &self.redaction
    }
}

/// Invalid operation result or result metadata.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ResultError {
    /// Completion preceded start time.
    #[error("operation completion cannot precede start")]
    CompletionBeforeStart,
    /// Successful results must not recommend retry.
    #[error("successful operations cannot carry retry advice")]
    RetryOnSuccess,
    /// Failed and cancelled results require an error diagnostic.
    #[error("failed or cancelled operation requires an error diagnostic")]
    FailureWithoutErrorDiagnostic,
    /// A failed or cancelled operation cannot be independently verified successful.
    #[error("failed or cancelled operation cannot be verified successful")]
    FailedOperationVerified,
    /// Verification completion preceded operation completion.
    #[error("verification cannot complete before operation execution")]
    VerificationBeforeCompletion,
    /// Inline output exceeded the bounded response budget.
    #[error("inline output is {bytes} bytes; maximum is {max_bytes}; use an artifact")]
    InlineOutputTooLarge {
        /// Encoded output size.
        bytes: usize,
        /// Maximum inline output size.
        max_bytes: usize,
    },
    /// Inline output could not be serialized.
    #[error("could not serialize inline output: {0}")]
    OutputSerialization(String),
    /// Text was empty, oversized, or contained control characters.
    #[error("invalid {field}: expected 1..={max_chars} non-control characters")]
    InvalidText {
        /// Field name.
        field: &'static str,
        /// Maximum accepted character count.
        max_chars: usize,
    },
    /// Stable code was invalid.
    #[error("invalid result code: {0}")]
    InvalidCode(String),
    /// SHA-256 digest was invalid.
    #[error("invalid SHA-256 digest")]
    InvalidSha256,
}

fn validate_text(field: &'static str, value: &str, max_chars: usize) -> Result<(), ResultError> {
    let chars = value.chars().count();
    if chars == 0 || chars > max_chars || value.chars().any(char::is_control) {
        return Err(ResultError::InvalidText { field, max_chars });
    }
    Ok(())
}

fn validate_code(value: &str) -> Result<(), ResultError> {
    let mut chars = value.chars();
    if !matches!(chars.next(), Some('a'..='z'))
        || !chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
        || value.ends_with(['.', '_', '-'])
    {
        return Err(ResultError::InvalidCode(value.to_owned()));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), ResultError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(ResultError::InvalidSha256)
    }
}

#[cfg(test)]
#[path = "result_tests.rs"]
mod tests;

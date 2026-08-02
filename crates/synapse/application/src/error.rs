use soma_ops::{DiagnosticCode, OperationName};

/// Failure while loading or applying Synapse compatibility contracts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CompatibilityError {
    /// A checked-in embedded contract could not be parsed or compiled.
    #[error("embedded contract {artifact} is invalid: {message}")]
    EmbeddedContract {
        /// Artifact name.
        artifact: &'static str,
        /// Parse, compile, or cross-reference failure.
        message: String,
    },
    /// A canonical operation was absent from the embedded registry.
    #[error("unknown canonical operation: {0}")]
    UnknownOperation(OperationName),
    /// No legacy routing key maps to a canonical operation.
    #[error("unknown legacy operation: tool={tool}, action={action}, subaction={subaction:?}")]
    UnknownLegacyOperation {
        /// Flux or Scout tool name.
        tool: &'static str,
        /// Legacy action.
        action: String,
        /// Optional legacy subaction.
        subaction: Option<String>,
    },
    /// The legacy request was not an object or had a field with the wrong type.
    #[error("invalid legacy request field {field}: {message}")]
    InvalidLegacyRequest {
        /// Invalid field.
        field: String,
        /// Corrective detail.
        message: String,
    },
    /// A legacy request carried a field outside the resolved operation schema.
    #[error("unknown field {field} for operation {operation}")]
    UnknownField {
        /// Canonical operation.
        operation: OperationName,
        /// Unknown field.
        field: String,
    },
    /// JSON Schema validation rejected canonical parameters or output.
    #[error("{kind} schema validation failed for {operation}: {details}")]
    SchemaValidation {
        /// Canonical operation.
        operation: OperationName,
        /// Parameters or result.
        kind: &'static str,
        /// Stable, joined validator messages.
        details: String,
    },
    /// Legacy presentation fields requested conflicting formats.
    #[error("legacy request has conflicting format and response_format values")]
    ConflictingPresentation,
    /// A legacy presentation format was unsupported.
    #[error("unsupported legacy presentation format: {0}")]
    InvalidPresentation(String),
    /// A diagnostic code has no global surface projection.
    #[error("unknown diagnostic projection: {0}")]
    UnknownDiagnostic(DiagnosticCode),
    /// A diagnostic is globally known but not declared by the operation.
    #[error("diagnostic {code} is not declared by operation {operation}")]
    DiagnosticNotDeclared {
        /// Canonical operation.
        operation: OperationName,
        /// Undeclared diagnostic.
        code: DiagnosticCode,
    },
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;

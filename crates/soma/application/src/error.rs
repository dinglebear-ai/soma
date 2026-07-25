use serde::Serialize;
use soma_domain::errors::ServiceErrorKind;

use crate::{PortError, ProviderError};

/// Error type surfaced by the application layer, carrying a stable code,
/// remediation guidance, and structured source-specific details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplicationError {
    /// Stable, machine-readable error code.
    pub code: String,
    /// Human-readable, redacted description of the failure.
    pub message: String,
    /// Whether retrying the operation might succeed.
    pub retryable: bool,
    /// Suggested remediation the caller can act on.
    pub remediation: String,
    /// Structured details describing where the error originated.
    pub details: Box<ApplicationErrorDetails>,
    /// Internal diagnostics never serialized to callers.
    #[serde(skip)]
    private_diagnostics: Option<String>,
}

/// Source-specific detail attached to an [`ApplicationError`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ApplicationErrorDetails {
    /// No additional structured detail.
    Generic,
    /// Error originating from a provider.
    Provider {
        /// Version of the provider error schema.
        schema_version: u32,
        /// Provider that produced the error.
        provider: String,
        /// Action being invoked when the error occurred, if known.
        action: Option<String>,
        /// Provider-specific error kind.
        provider_error_kind: String,
    },
    /// Error originating from the service layer.
    Service {
        /// Version of the service error schema.
        schema_version: u8,
        /// Service-specific error kind.
        service_error_kind: String,
        /// Field associated with the error, if any.
        field: Option<String>,
        /// Offending value, if any.
        bad_value: Option<String>,
        /// Expected value pattern, if any.
        expected_pattern: Option<String>,
        /// Machine-readable reason kind, if any.
        reason_kind: Option<String>,
        /// Actions available to the caller as recovery options.
        available_actions: Vec<String>,
    },
}

impl ApplicationError {
    /// Builds a generic `ApplicationError` from its core fields.
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            remediation: remediation.into(),
            details: Box::new(ApplicationErrorDetails::Generic),
            private_diagnostics: None,
        }
    }

    /// Attaches structured details and returns the updated error.
    pub fn with_details(mut self, details: ApplicationErrorDetails) -> Self {
        self.details = Box::new(details);
        self
    }

    /// Returns the internal diagnostics, which are never serialized to callers.
    pub fn private_diagnostics(&self) -> Option<&str> {
        self.private_diagnostics.as_deref()
    }

    pub(crate) fn service(error: &anyhow::Error) -> Self {
        let classified = crate::classify_service_error(error);
        Self {
            code: classified.code,
            message: classified.message,
            retryable: classified.retryable,
            remediation: classified.remediation,
            details: Box::new(ApplicationErrorDetails::Service {
                schema_version: classified.schema_version,
                service_error_kind: classified.kind.as_str().to_owned(),
                field: classified.field,
                bad_value: classified.bad_value,
                expected_pattern: classified.expected_pattern,
                reason_kind: classified.reason_kind,
                available_actions: classified
                    .available_actions
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect(),
            }),
            private_diagnostics: None,
        }
    }

    /// Returns the service error kind when the error came from the service
    /// layer, otherwise `None`.
    pub fn service_error_kind(&self) -> Option<&str> {
        match self.details.as_ref() {
            ApplicationErrorDetails::Service {
                service_error_kind, ..
            } => Some(service_error_kind),
            ApplicationErrorDetails::Generic | ApplicationErrorDetails::Provider { .. } => None,
        }
    }

    /// Returns `true` when this error represents a validation failure.
    pub fn is_validation(&self) -> bool {
        self.service_error_kind() == Some(ServiceErrorKind::Validation.as_str())
    }

    pub(crate) fn legacy(operation: &str, error: impl std::fmt::Display) -> Self {
        let diagnostic = crate::provider_errors::redact_public(&error.to_string());
        Self::new(
            "legacy_operation_failed",
            format!("{operation} failed: {diagnostic}"),
            false,
            "Check Soma service status and retry.",
        )
    }

    pub(crate) fn not_found(kind: &str, name: &str) -> Self {
        Self::new(
            format!("{kind}_not_found"),
            format!("unknown {kind} `{name}`"),
            false,
            format!("List available {kind}s and retry with a known name."),
        )
    }
}

impl From<ProviderError> for ApplicationError {
    fn from(error: ProviderError) -> Self {
        let private_diagnostics = error.private_diagnostics().map(ToOwned::to_owned);
        Self {
            code: error.code.to_string(),
            message: error.message.to_string(),
            retryable: error.retryable,
            remediation: error.remediation.to_string(),
            details: Box::new(ApplicationErrorDetails::Provider {
                schema_version: error.schema_version,
                provider: error.provider.to_string(),
                action: error.action.map(Into::into),
                provider_error_kind: error.kind.to_owned(),
            }),
            private_diagnostics,
        }
    }
}

impl From<PortError> for ApplicationError {
    fn from(error: PortError) -> Self {
        let message = crate::provider_errors::redact_public(&error.message);
        Self::new(error.code, message, error.retryable, error.remediation)
    }
}

impl std::fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApplicationError {}

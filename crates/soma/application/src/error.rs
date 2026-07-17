use serde::Serialize;
use soma_domain::errors::ServiceErrorKind;
use soma_service::ProviderError;

use crate::PortError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplicationError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub remediation: String,
    pub details: Box<ApplicationErrorDetails>,
    #[serde(skip)]
    private_diagnostics: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ApplicationErrorDetails {
    Generic,
    Provider {
        schema_version: u32,
        provider: String,
        action: Option<String>,
        provider_error_kind: String,
    },
    Service {
        schema_version: u8,
        service_error_kind: String,
        field: Option<String>,
        bad_value: Option<String>,
        expected_pattern: Option<String>,
        reason_kind: Option<String>,
        available_actions: Vec<String>,
    },
}

impl ApplicationError {
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

    pub fn with_details(mut self, details: ApplicationErrorDetails) -> Self {
        self.details = Box::new(details);
        self
    }

    pub fn private_diagnostics(&self) -> Option<&str> {
        self.private_diagnostics.as_deref()
    }

    pub(crate) fn service(error: &anyhow::Error) -> Self {
        let classified = soma_service::classify_service_error(error);
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

    pub fn service_error_kind(&self) -> Option<&str> {
        match self.details.as_ref() {
            ApplicationErrorDetails::Service {
                service_error_kind, ..
            } => Some(service_error_kind),
            ApplicationErrorDetails::Generic | ApplicationErrorDetails::Provider { .. } => None,
        }
    }

    pub fn is_validation(&self) -> bool {
        self.service_error_kind() == Some(ServiceErrorKind::Validation.as_str())
    }

    pub(crate) fn legacy(operation: &str, error: impl std::fmt::Display) -> Self {
        let diagnostic = soma_service::provider_errors::redact_public(&error.to_string());
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
        let message = soma_service::provider_errors::redact_public(&error.message);
        Self::new(error.code, message, error.retryable, error.remediation)
    }
}

impl std::fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApplicationError {}

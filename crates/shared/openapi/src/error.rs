#[derive(Debug, Clone, thiserror::Error)]
pub enum SsrfError {
    #[error("{0}")]
    InvalidUrl(String),
    #[error("{0}")]
    Blocked(String),
}

impl SsrfError {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidUrl(_) => "invalid_param",
            Self::Blocked(_) => "ssrf_blocked",
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum OpenApiError {
    #[error("spec `{label}` base URL rejected by SSRF guard: {reason}")]
    SsrfRejected { label: String, reason: String },
    #[error("failed to parse OpenAPI spec `{label}`")]
    SpecParse { label: String },
    #[error("spec document `{label}` exceeds the size cap")]
    SpecTooLarge { label: String },
    #[error("unknown spec label `{label}`")]
    UnknownInstance { label: String, valid: Vec<String> },
    #[error("unknown operation `{operation_id}` in spec `{label}`")]
    UnknownOperation { label: String, operation_id: String },
    #[error("request for spec `{label}` blocked: resolved to a private address")]
    RequestBlockedPrivateAddr { label: String },
    #[error("could not resolve host for spec `{label}`")]
    ResolveFailed { label: String },
    #[error("operation `{label}` path parameter `{param}` is missing or invalid")]
    InvalidPathParam { label: String, param: String },
    #[error("failed to build hardened HTTP client")]
    ClientBuildFailed,
    #[error("upstream request for spec `{label}` failed")]
    UpstreamRequest { label: String },
    #[error("upstream request for spec `{label}` timed out")]
    UpstreamTimeout { label: String },
}

impl OpenApiError {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SsrfRejected { .. } | Self::SpecParse { .. } | Self::SpecTooLarge { .. } => {
                "config_error"
            }
            Self::RequestBlockedPrivateAddr { .. } => "forbidden",
            Self::InvalidPathParam { .. } => "invalid_param",
            Self::UnknownInstance { .. } => "unknown_instance",
            Self::UnknownOperation { .. } => "unknown_action",
            Self::ResolveFailed { .. } | Self::ClientBuildFailed | Self::UpstreamRequest { .. } => {
                "internal_error"
            }
            Self::UpstreamTimeout { .. } => "timeout",
        }
    }
}

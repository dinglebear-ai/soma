use serde::{Deserialize, Serialize};

const MAX_REQUEST_ID_BYTES: usize = 128;

/// The entry point through which a request reached the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Surface {
    /// The MCP protocol surface.
    Mcp,
    /// The REST HTTP surface.
    Rest,
    /// The command-line interface surface.
    Cli,
    /// The command palette surface.
    Palette,
}

/// How the request's authorization was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthorizationMode {
    /// Loopback development mode with auth bypassed.
    LoopbackDev,
    /// Behind a trusted gateway that enforces authorization upstream.
    TrustedGateway,
    /// Full auth middleware mounted (bearer token or OAuth).
    Mounted,
}

/// Whether a destructive operation has been explicitly confirmed by the caller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confirmation {
    /// No confirmation was provided (the default).
    #[default]
    Missing,
    /// The caller explicitly confirmed the operation.
    Confirmed,
}

impl Confirmation {
    /// Returns `true` when the operation has been confirmed.
    pub fn is_confirmed(self) -> bool {
        matches!(self, Self::Confirmed)
    }
}

/// A validated request identifier: non-empty, bounded in length, and free of
/// control characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(String);

impl RequestId {
    /// Validates and constructs a `RequestId`, rejecting empty, over-long, or
    /// control-character-containing values.
    pub fn new(value: impl Into<String>) -> Result<Self, RequestIdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(RequestIdError::Empty);
        }
        if value.len() > MAX_REQUEST_ID_BYTES {
            return Err(RequestIdError::TooLong {
                actual: value.len(),
                maximum: MAX_REQUEST_ID_BYTES,
            });
        }
        if value.chars().any(char::is_control) {
            return Err(RequestIdError::ControlCharacter);
        }
        Ok(Self(value))
    }

    /// Returns the request id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Reasons a [`RequestId`] can fail validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestIdError {
    /// The value was empty or whitespace-only.
    Empty,
    /// The value exceeded the maximum allowed byte length.
    TooLong {
        /// Actual byte length of the supplied value.
        actual: usize,
        /// Maximum permitted byte length.
        maximum: usize,
    },
    /// The value contained a control character.
    ControlCharacter,
}

impl std::fmt::Display for RequestIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("request id must not be empty"),
            Self::TooLong { actual, maximum } => {
                write!(f, "request id is {actual} bytes; maximum is {maximum}")
            }
            Self::ControlCharacter => f.write_str("request id must not contain control characters"),
        }
    }
}

impl std::error::Error for RequestIdError {}

/// W3C Trace Context propagation fields carried with a request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    /// The `traceparent` header value, if present.
    pub traceparent: Option<String>,
    /// The `tracestate` header value, if present.
    pub tracestate: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Confirmation, RequestId};

    #[test]
    fn request_ids_reject_empty_and_control_characters() {
        assert!(RequestId::new("  ").is_err());
        assert!(RequestId::new("request\n1").is_err());
        assert_eq!(RequestId::new("request-1").unwrap().as_str(), "request-1");
    }

    #[test]
    fn confirmation_defaults_to_missing() {
        assert!(!Confirmation::default().is_confirmed());
        assert!(Confirmation::Confirmed.is_confirmed());
    }
}

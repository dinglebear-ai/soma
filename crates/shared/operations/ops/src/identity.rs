use std::{fmt, str::FromStr, time::SystemTime};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_REFERENCE_CHARS: usize = 256;
const MAX_TRACEPARENT_CHARS: usize = 512;
const MAX_TRACESTATE_CHARS: usize = 1_024;

macro_rules! uuid_identity {
    ($name:ident, $kind:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Generates a time-ordered UUIDv7 identity.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7().to_string())
            }

            /// Parses and normalizes an existing UUID identity.
            pub fn parse(value: impl AsRef<str>) -> Result<Self, IdentityError> {
                let value = value.as_ref();
                let parsed = Uuid::parse_str(value).map_err(|_| IdentityError::InvalidUuid {
                    kind: $kind,
                    value: value.to_owned(),
                })?;
                Ok(Self(parsed.hyphenated().to_string()))
            }

            /// Returns the canonical hyphenated UUID string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consumes the identity and returns its canonical string.
            #[must_use]
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdentityError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }
    };
}

uuid_identity!(
    OperationId,
    "operation",
    "Unique identity for one operation execution."
);
uuid_identity!(
    EventId,
    "event",
    "Unique identity for one operation lifecycle event."
);
uuid_identity!(
    CorrelationId,
    "correlation",
    "Identity shared by operations participating in one workflow or incident."
);
uuid_identity!(
    AuthorizationId,
    "authorization",
    "Opaque identity for authorization evidence issued by a product policy layer."
);

/// Milliseconds from the Unix epoch used in cross-process operation contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Creates a timestamp from Unix milliseconds.
    #[must_use]
    pub const fn from_unix_millis(value: i64) -> Self {
        Self(value)
    }

    /// Returns the current system time as Unix milliseconds.
    #[must_use]
    pub fn now() -> Self {
        match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            Ok(duration) => Self(clamp_millis(duration.as_millis() as i128)),
            Err(error) => Self(clamp_millis(-(error.duration().as_millis() as i128))),
        }
    }

    /// Returns Unix milliseconds.
    #[must_use]
    pub const fn unix_millis(self) -> i64 {
        self.0
    }
}

fn clamp_millis(value: i128) -> i64 {
    value.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// Stable reference to the product or agent that requested an operation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ActorRef {
    namespace: String,
    id: String,
}

impl ActorRef {
    /// Creates a validated actor reference.
    pub fn new(namespace: impl Into<String>, id: impl Into<String>) -> Result<Self, IdentityError> {
        let namespace = namespace.into();
        let id = id.into();
        validate_reference("actor namespace", &namespace)?;
        validate_reference("actor id", &id)?;
        Ok(Self { namespace, id })
    }

    /// Returns the namespace that interprets this actor identity.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns the identity within the actor namespace.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Identity and version of the component that emitted an operation event.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProducerRef {
    name: String,
    version: String,
}

impl ProducerRef {
    /// Creates a validated producer reference.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Result<Self, IdentityError> {
        let name = name.into();
        let version = version.into();
        validate_reference("producer name", &name)?;
        validate_reference("producer version", &version)?;
        Ok(Self { name, version })
    }

    /// Returns the producer name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the producer version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// W3C trace context safe to propagate across operation boundaries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TraceContext {
    traceparent: Option<String>,
    tracestate: Option<String>,
}

impl TraceContext {
    /// Creates a validated trace context.
    pub fn new(
        traceparent: Option<impl Into<String>>,
        tracestate: Option<impl Into<String>>,
    ) -> Result<Self, IdentityError> {
        let traceparent = traceparent.map(Into::into);
        let tracestate = tracestate.map(Into::into);
        validate_optional_trace("traceparent", traceparent.as_deref(), MAX_TRACEPARENT_CHARS)?;
        validate_optional_trace("tracestate", tracestate.as_deref(), MAX_TRACESTATE_CHARS)?;
        Ok(Self {
            traceparent,
            tracestate,
        })
    }

    /// Returns the W3C traceparent header value when present.
    #[must_use]
    pub fn traceparent(&self) -> Option<&str> {
        self.traceparent.as_deref()
    }

    /// Returns the W3C tracestate header value when present.
    #[must_use]
    pub fn tracestate(&self) -> Option<&str> {
        self.tracestate.as_deref()
    }
}

/// Validation failures for opaque identities and cross-product references.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityError {
    /// A supplied identity was not a UUID.
    #[error("invalid {kind} UUID: {value}")]
    InvalidUuid {
        /// Identity kind being parsed.
        kind: &'static str,
        /// Invalid value.
        value: String,
    },
    /// A reference was empty, too long, or contained control characters.
    #[error("invalid {field}: expected 1..={max_chars} non-control characters")]
    InvalidReference {
        /// Reference field.
        field: &'static str,
        /// Maximum accepted character count.
        max_chars: usize,
    },
}

fn validate_reference(field: &'static str, value: &str) -> Result<(), IdentityError> {
    let chars = value.chars().count();
    if chars == 0 || chars > MAX_REFERENCE_CHARS || value.chars().any(char::is_control) {
        return Err(IdentityError::InvalidReference {
            field,
            max_chars: MAX_REFERENCE_CHARS,
        });
    }
    Ok(())
}

fn validate_optional_trace(
    field: &'static str,
    value: Option<&str>,
    max_chars: usize,
) -> Result<(), IdentityError> {
    let Some(value) = value else {
        return Ok(());
    };
    let chars = value.chars().count();
    if chars == 0 || chars > max_chars || value.chars().any(char::is_control) {
        return Err(IdentityError::InvalidReference { field, max_chars });
    }
    Ok(())
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;

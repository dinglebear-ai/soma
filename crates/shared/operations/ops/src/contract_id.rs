use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::OperationName;

const MAX_SCHEMA_ID_CHARS: usize = 256;
const MAX_DIAGNOSTIC_CODE_CHARS: usize = 128;

/// Stable identity for a versioned operation parameter or result schema.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct SchemaId(String);

impl SchemaId {
    /// Creates and validates a schema identity.
    pub fn new(value: impl Into<String>) -> Result<Self, SchemaIdError> {
        let value = value.into();
        validate_schema_id(&value)?;
        Ok(Self(value))
    }

    /// Derives the parameter schema identity for one operation version.
    pub fn parameters(operation: &OperationName, version: u32) -> Result<Self, SchemaIdError> {
        Self::for_kind(operation, "parameters", version)
    }

    /// Derives the result schema identity for one operation version.
    pub fn result(operation: &OperationName, version: u32) -> Result<Self, SchemaIdError> {
        Self::for_kind(operation, "result", version)
    }

    fn for_kind(
        operation: &OperationName,
        kind: &'static str,
        version: u32,
    ) -> Result<Self, SchemaIdError> {
        if version == 0 {
            return Err(SchemaIdError::ZeroVersion);
        }
        Self::new(format!(
            "schema.operations.{}.{kind}.v{version}",
            operation.as_str()
        ))
    }

    /// Returns the stable schema identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SchemaId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SchemaId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Invalid operation schema identity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaIdError {
    /// Schema versions start at one.
    #[error("operation schema version must be greater than zero")]
    ZeroVersion,
    /// The identity did not follow the canonical schema naming contract.
    #[error("invalid operation schema id: {0}")]
    Invalid(String),
}

fn validate_schema_id(value: &str) -> Result<(), SchemaIdError> {
    if value.chars().count() > MAX_SCHEMA_ID_CHARS {
        return Err(SchemaIdError::Invalid(value.to_owned()));
    }
    let segments = value.split('.').collect::<Vec<_>>();
    if segments.len() < 6 || segments[0..2] != ["schema", "operations"] {
        return Err(SchemaIdError::Invalid(value.to_owned()));
    }
    let kind = segments[segments.len() - 2];
    if !matches!(kind, "parameters" | "result") {
        return Err(SchemaIdError::Invalid(value.to_owned()));
    }
    let version = segments[segments.len() - 1];
    let Some(version) = version.strip_prefix('v') else {
        return Err(SchemaIdError::Invalid(value.to_owned()));
    };
    if version
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .is_none()
    {
        return Err(SchemaIdError::Invalid(value.to_owned()));
    }
    let operation = segments[2..segments.len() - 2].join(".");
    OperationName::new(operation).map_err(|_| SchemaIdError::Invalid(value.to_owned()))?;
    Ok(())
}

/// Stable machine-readable diagnostic code such as `target.not_found`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct DiagnosticCode(String);

impl DiagnosticCode {
    /// Creates and validates a diagnostic code.
    pub fn new(value: impl Into<String>) -> Result<Self, DiagnosticCodeError> {
        let value = value.into();
        if valid_diagnostic_code(&value) {
            Ok(Self(value))
        } else {
            Err(DiagnosticCodeError(value))
        }
    }

    /// Returns the stable code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DiagnosticCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Invalid stable diagnostic code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid diagnostic code: {0}")]
pub struct DiagnosticCodeError(String);

fn valid_diagnostic_code(value: &str) -> bool {
    let count = value.chars().count();
    if !(3..=MAX_DIAGNOSTIC_CODE_CHARS).contains(&count) || !value.contains('.') {
        return false;
    }
    value.split('.').all(valid_code_segment)
}

fn valid_code_segment(segment: &str) -> bool {
    let mut characters = segment.chars();
    matches!(characters.next(), Some('a'..='z'))
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        })
        && !segment.ends_with(['-', '_'])
}

#[cfg(test)]
#[path = "contract_id_tests.rs"]
mod tests;

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

const MAX_HOST_ID_CHARS: usize = 128;
const MAX_CAPABILITY_CHARS: usize = 128;

/// Stable lowercase identity for one managed host.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct HostId(String);

impl HostId {
    /// Creates and validates a host identity.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        if valid_token(&value, MAX_HOST_ID_CHARS, false) {
            Ok(Self(value))
        } else {
            Err(IdentityError::InvalidHostId(value))
        }
    }

    /// Returns the stable host identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for HostId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Lowercase dotted capability advertised by a host or transport.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct CapabilityName(String);

impl CapabilityName {
    /// Creates and validates a capability name such as `transport.ssh`.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        if valid_token(&value, MAX_CAPABILITY_CHARS, true) && value.contains('.') {
            Ok(Self(value))
        } else {
            Err(IdentityError::InvalidCapability(value))
        }
    }

    /// Returns the capability name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CapabilityName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CapabilityName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// SHA-256 revision of all transport-affecting topology material.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct TopologyRevision(String);

impl TopologyRevision {
    /// Parses a lowercase 64-character SHA-256 revision.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            Ok(Self(value))
        } else {
            Err(IdentityError::InvalidTopologyRevision)
        }
    }

    /// Derives a deterministic revision from canonical topology material.
    #[must_use]
    pub fn from_material(material: impl AsRef<[u8]>) -> Self {
        Self(format!("{:x}", Sha256::digest(material.as_ref())))
    }

    /// Returns the lowercase SHA-256 revision.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TopologyRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TopologyRevision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Connection-cache key bound to host identity and exact topology revision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PoolKey {
    host: HostId,
    revision: TopologyRevision,
}

impl PoolKey {
    /// Creates a revision-bound connection key.
    #[must_use]
    pub fn new(host: HostId, revision: TopologyRevision) -> Self {
        Self { host, revision }
    }

    /// Returns the host identity.
    #[must_use]
    pub fn host(&self) -> &HostId {
        &self.host
    }

    /// Returns the topology revision.
    #[must_use]
    pub fn revision(&self) -> &TopologyRevision {
        &self.revision
    }
}

impl fmt::Display for PoolKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.host, self.revision)
    }
}

/// Invalid fleet identity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityError {
    /// Host identity was empty, oversized, or not canonical lowercase ASCII.
    #[error("invalid host id: {0}")]
    InvalidHostId(String),
    /// Capability was not a valid lowercase dotted identifier.
    #[error("invalid capability name: {0}")]
    InvalidCapability(String),
    /// Topology revision was not a lowercase SHA-256 digest.
    #[error("invalid topology revision")]
    InvalidTopologyRevision,
}

fn valid_token(value: &str, max_chars: usize, allow_dot: bool) -> bool {
    let count = value.chars().count();
    if count == 0 || count > max_chars {
        return false;
    }
    value.split('.').all(|segment| {
        if !allow_dot && value.contains('.') {
            return false;
        }
        let mut chars = segment.chars();
        matches!(chars.next(), Some('a'..='z' | '0'..='9'))
            && chars.all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '-' | '_')
            })
            && !segment.ends_with(['-', '_'])
    })
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;

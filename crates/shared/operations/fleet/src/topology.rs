use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{CapabilityName, HostEndpoint, HostId, PoolKey, TopologyRevision};

const MAX_LABEL_CHARS: usize = 128;

/// Product-neutral managed host record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostRecord {
    id: HostId,
    endpoint: HostEndpoint,
    revision: TopologyRevision,
    labels: BTreeSet<String>,
    capabilities: BTreeSet<CapabilityName>,
}

impl HostRecord {
    /// Creates a host and derives its revision from endpoint material.
    #[must_use]
    pub fn new(id: HostId, endpoint: HostEndpoint) -> Self {
        let revision = endpoint.revision();
        Self {
            id,
            endpoint,
            revision,
            labels: BTreeSet::new(),
            capabilities: BTreeSet::new(),
        }
    }

    /// Adds a normalized host label.
    pub fn with_label(mut self, label: impl Into<String>) -> Result<Self, TopologyError> {
        let label = label.into();
        validate_label(&label)?;
        self.labels.insert(label);
        Ok(self)
    }

    /// Adds an advertised capability.
    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityName) -> Self {
        self.capabilities.insert(capability);
        self
    }

    /// Returns the host identity.
    #[must_use]
    pub fn id(&self) -> &HostId {
        &self.id
    }

    /// Returns the transport endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &HostEndpoint {
        &self.endpoint
    }

    /// Returns the transport-affecting topology revision.
    #[must_use]
    pub fn revision(&self) -> &TopologyRevision {
        &self.revision
    }

    /// Returns the connection-cache key.
    #[must_use]
    pub fn pool_key(&self) -> PoolKey {
        PoolKey::new(self.id.clone(), self.revision.clone())
    }

    /// Iterates over sorted labels.
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.labels.iter().map(String::as_str)
    }

    /// Iterates over advertised capabilities.
    pub fn capabilities(&self) -> impl Iterator<Item = &CapabilityName> {
        self.capabilities.iter()
    }
}

impl<'de> Deserialize<'de> for HostRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = HostRecordWire::deserialize(deserializer)?;
        let mut record = Self::new(wire.id, wire.endpoint);
        if record.revision != wire.revision {
            return Err(serde::de::Error::custom(TopologyError::RevisionMismatch));
        }
        for label in wire.labels {
            record = record.with_label(label).map_err(serde::de::Error::custom)?;
        }
        record.capabilities = wire.capabilities;
        Ok(record)
    }
}

#[derive(Deserialize)]
struct HostRecordWire {
    id: HostId,
    endpoint: HostEndpoint,
    revision: TopologyRevision,
    #[serde(default)]
    labels: BTreeSet<String>,
    #[serde(default)]
    capabilities: BTreeSet<CapabilityName>,
}

/// Immutable topology snapshot with unique host identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologySnapshot {
    revision: TopologyRevision,
    hosts: BTreeMap<HostId, HostRecord>,
}

impl TopologySnapshot {
    /// Builds a snapshot and rejects duplicate host identities.
    pub fn new<I>(hosts: I) -> Result<Self, TopologyError>
    where
        I: IntoIterator<Item = HostRecord>,
    {
        let mut indexed = BTreeMap::new();
        for host in hosts {
            let id = host.id.clone();
            if indexed.insert(id.clone(), host).is_some() {
                return Err(TopologyError::DuplicateHost(id));
            }
        }
        let material = indexed
            .iter()
            .map(|(id, host)| format!("{}:{}\n", id, host.revision))
            .collect::<String>();
        Ok(Self {
            revision: TopologyRevision::from_material(material),
            hosts: indexed,
        })
    }

    /// Returns the snapshot revision.
    #[must_use]
    pub fn revision(&self) -> &TopologyRevision {
        &self.revision
    }

    /// Returns a host by stable identity.
    #[must_use]
    pub fn get(&self, id: &HostId) -> Option<&HostRecord> {
        self.hosts.get(id)
    }

    /// Iterates over hosts in identity order.
    pub fn hosts(&self) -> impl Iterator<Item = &HostRecord> {
        self.hosts.values()
    }

    /// Returns the number of hosts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    /// Returns whether the snapshot has no hosts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }
}

/// Invalid host endpoint or topology snapshot.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TopologyError {
    /// Endpoint text was empty, oversized, or contained control characters.
    #[error("invalid {field}")]
    InvalidEndpointText {
        /// Invalid endpoint field.
        field: &'static str,
    },
    /// Port zero is never valid.
    #[error("fleet endpoint port must be greater than zero")]
    InvalidPort,
    /// A security-sensitive path was not absolute and normalized.
    #[error("fleet path must be absolute and contain no parent traversal: {0}")]
    InvalidAbsolutePath(PathBuf),
    /// HTTP endpoint was not plain HTTP(S) or contained embedded credentials.
    #[error("invalid HTTP fleet endpoint")]
    InvalidHttpEndpoint,
    /// Label was empty, oversized, or contained control characters.
    #[error("invalid host label: {0}")]
    InvalidLabel(String),
    /// Host identity appeared more than once.
    #[error("duplicate host identity: {0}")]
    DuplicateHost(HostId),
    /// Serialized endpoint material did not match its claimed revision.
    #[error("host topology revision does not match endpoint material")]
    RevisionMismatch,
}

fn validate_label(label: &str) -> Result<(), TopologyError> {
    let count = label.chars().count();
    if count == 0 || count > MAX_LABEL_CHARS || label.chars().any(char::is_control) {
        Err(TopologyError::InvalidLabel(label.to_owned()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "topology_tests.rs"]
mod tests;

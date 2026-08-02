use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

const MAX_OPERATION_NAME_CHARS: usize = 128;
const MAX_TARGET_VALUE_CHARS: usize = 1_024;
const MAX_TARGET_DEPTH: usize = 8;
const MAX_IDEMPOTENCY_KEY_CHARS: usize = 256;

/// Stable dotted operation identity such as `container.restart`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct OperationName(String);

impl OperationName {
    /// Creates a validated dotted operation name.
    pub fn new(value: impl Into<String>) -> Result<Self, OperationNameError> {
        let value = value.into();
        if valid_operation_name(&value) {
            Ok(Self(value))
        } else {
            Err(OperationNameError(value))
        }
    }

    /// Returns the operation name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the name and returns the owned string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for OperationName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for OperationName {
    type Err = OperationNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Error returned for an invalid canonical operation name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid operation name {0}; expected lowercase dotted segments")]
pub struct OperationNameError(String);

fn valid_operation_name(value: &str) -> bool {
    let length = value.chars().count();
    if !(3..=MAX_OPERATION_NAME_CHARS).contains(&length) || !value.contains('.') {
        return false;
    }
    value.split('.').all(valid_name_segment)
}

fn valid_name_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        })
        && !segment.ends_with(['-', '_'])
}

/// Typed category of a resource targeted by an operation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TargetKind {
    /// A physical or virtual host.
    Host,
    /// A Docker-compatible daemon.
    DockerDaemon,
    /// A Docker or OCI container.
    Container,
    /// A Compose project.
    ComposeProject,
    /// An Incus daemon.
    IncusServer,
    /// An Incus container or virtual machine.
    IncusInstance,
    /// A container or Incus image.
    Image,
    /// A network resource.
    Network,
    /// A storage pool.
    StoragePool,
    /// A storage volume.
    StorageVolume,
    /// A filesystem path.
    File,
    /// A process.
    Process,
    /// A log source.
    LogSource,
    /// A ZFS pool.
    ZfsPool,
    /// A ZFS dataset.
    ZfsDataset,
    /// A ZFS snapshot.
    ZfsSnapshot,
    /// A namespaced target kind defined by an external engine.
    Custom(OperationName),
}

/// Stable, serializable reference to an operation target.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TargetRef {
    kind: TargetKind,
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<Box<TargetRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revision: Option<String>,
}

impl TargetRef {
    /// Creates a target reference with a validated resource identity.
    pub fn new(kind: TargetKind, id: impl Into<String>) -> Result<Self, TargetRefError> {
        let id = id.into();
        validate_target_value("id", &id)?;
        Ok(Self {
            kind,
            id,
            host: None,
            parent: None,
            revision: None,
        })
    }

    /// Associates the target with an explicit host identity.
    pub fn with_host(mut self, host: impl Into<String>) -> Result<Self, TargetRefError> {
        let host = host.into();
        validate_target_value("host", &host)?;
        self.host = Some(host);
        Ok(self)
    }

    /// Associates the target with a parent resource.
    pub fn with_parent(mut self, parent: TargetRef) -> Result<Self, TargetRefError> {
        if parent.depth() >= MAX_TARGET_DEPTH {
            return Err(TargetRefError::ExcessiveDepth {
                max_depth: MAX_TARGET_DEPTH,
            });
        }
        self.parent = Some(Box::new(parent));
        Ok(self)
    }

    /// Binds the target to a topology or resource revision.
    pub fn with_revision(mut self, revision: impl Into<String>) -> Result<Self, TargetRefError> {
        let revision = revision.into();
        validate_target_value("revision", &revision)?;
        self.revision = Some(revision);
        Ok(self)
    }

    /// Returns the target category.
    #[must_use]
    pub fn kind(&self) -> &TargetKind {
        &self.kind
    }

    /// Returns the target identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the explicit host identity when present.
    #[must_use]
    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }

    /// Returns the parent target when present.
    #[must_use]
    pub fn parent(&self) -> Option<&TargetRef> {
        self.parent.as_deref()
    }

    /// Returns the bound revision when present.
    #[must_use]
    pub fn revision(&self) -> Option<&str> {
        self.revision.as_deref()
    }

    fn depth(&self) -> usize {
        1 + self.parent.as_deref().map_or(0, Self::depth)
    }
}

/// Validation failure for a target reference.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TargetRefError {
    /// A textual target field was empty, oversized, or contained control characters.
    #[error("invalid target {field}: expected 1..={max_chars} non-control characters")]
    InvalidValue {
        /// Target field.
        field: &'static str,
        /// Maximum accepted character count.
        max_chars: usize,
    },
    /// The parent chain exceeded the defensive recursion bound.
    #[error("target parent chain exceeds maximum depth {max_depth}")]
    ExcessiveDepth {
        /// Maximum parent depth.
        max_depth: usize,
    },
}

fn validate_target_value(field: &'static str, value: &str) -> Result<(), TargetRefError> {
    let chars = value.chars().count();
    if chars == 0 || chars > MAX_TARGET_VALUE_CHARS || value.chars().any(char::is_control) {
        return Err(TargetRefError::InvalidValue {
            field,
            max_chars: MAX_TARGET_VALUE_CHARS,
        });
    }
    Ok(())
}

/// Caller-provided key that makes a supported mutation safely repeatable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Creates a bounded idempotency key.
    pub fn new(value: impl Into<String>) -> Result<Self, IdempotencyKeyError> {
        let value = value.into();
        let chars = value.chars().count();
        if chars == 0 || chars > MAX_IDEMPOTENCY_KEY_CHARS || value.chars().any(char::is_control) {
            return Err(IdempotencyKeyError);
        }
        Ok(Self(value))
    }

    /// Returns the idempotency key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned for an invalid idempotency key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid idempotency key")]
pub struct IdempotencyKeyError;

/// Whether an operation only observes state or may mutate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AccessClass {
    /// Observation with no intended external mutation.
    Read,
    /// Operation that may change external state.
    Mutation,
}

/// Operational risk independent of product-specific authorization policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RiskClass {
    /// Expected to be non-disruptive.
    Safe,
    /// May interrupt availability or active work.
    Disruptive,
    /// May destroy data or resources.
    Destructive,
    /// Exercises host-level or otherwise elevated authority.
    Privileged,
}

/// Expected ability to reverse an operation's effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Reversibility {
    /// The operation has a defined inverse under normal conditions.
    Reversible,
    /// Reversal depends on snapshots, backups, or runtime conditions.
    Conditional,
    /// The operation has no general rollback.
    Irreversible,
}

/// Whether a failed call may be retried automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RetryClass {
    /// Retrying is unsafe without operator analysis.
    Never,
    /// Retrying is safe because no mutation was sent or the operation is idempotent.
    Safe,
    /// Retrying depends on structured failure details.
    Conditional,
}

/// Whether a failed operation may already have reached the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MutationSendState {
    /// No mutation request was sent.
    NotSent,
    /// The mutation request was sent.
    Sent,
    /// The transport cannot determine whether the mutation was sent.
    Unknown,
    /// The operation was read-only.
    NotApplicable,
}

/// Independent verification outcome after execution completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VerificationStatus {
    /// Runtime evidence confirms the intended state.
    Verified,
    /// Runtime evidence contradicts the intended state.
    Failed,
    /// Evidence was insufficient to decide.
    Inconclusive,
    /// The implementation has no verification operation.
    NotSupported,
    /// Verification was not requested.
    NotRequested,
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;

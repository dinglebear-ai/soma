use std::collections::BTreeSet;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    AccessClass, OperationName, RetryClass, Reversibility, RiskClass, TargetKind, TargetRef,
    TargetRefError,
};

const MAX_PARAMETER_CHARS: usize = 128;
const MAX_REQUIREMENT_CHARS: usize = 256;

/// Whether an implementation supports a lifecycle capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapabilitySupport {
    /// The capability is not implemented.
    Unsupported,
    /// The capability is available but not mandatory for every call.
    Optional,
    /// The capability is required by the operation contract.
    Required,
}

/// Kind of evidence an operation can return for audit or verification.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EvidenceKind {
    /// Current runtime state.
    RuntimeState,
    /// Bounded logs.
    Logs,
    /// Configuration state.
    Configuration,
    /// A protected or durable artifact.
    Artifact,
    /// A before-and-after difference.
    Diff,
    /// Metrics or measurements.
    Metrics,
    /// Namespaced evidence understood by a specific engine.
    Custom(String),
}

/// One complete set of parameter names required together.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParameterGroup {
    fields: BTreeSet<String>,
}

impl ParameterGroup {
    /// Creates an empty parameter group.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            fields: BTreeSet::new(),
        }
    }

    /// Creates a validated group from parameter names.
    pub fn new<I, S>(fields: I) -> Result<Self, SpecError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut validated = BTreeSet::new();
        for field in fields {
            let field = field.into();
            if !valid_parameter_name(&field) {
                return Err(SpecError::InvalidParameter(field));
            }
            if !validated.insert(field.clone()) {
                return Err(SpecError::DuplicateParameter(field));
            }
        }
        Ok(Self { fields: validated })
    }

    /// Returns true when the group contains no fields.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Iterates over parameter names in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.fields.iter().map(String::as_str)
    }
}

/// Product-neutral catalog record for one operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OperationSpec {
    name: OperationName,
    schema_version: u32,
    target_kind: TargetKind,
    access: AccessClass,
    risk: RiskClass,
    reversibility: Reversibility,
    required: ParameterGroup,
    required_any: Vec<ParameterGroup>,
    planning: CapabilitySupport,
    progress: CapabilitySupport,
    cancellation: CapabilitySupport,
    verification: CapabilitySupport,
    fanout: CapabilitySupport,
    retry: RetryClass,
    idempotent: bool,
    evidence: BTreeSet<EvidenceKind>,
    requirements: BTreeSet<String>,
}

impl OperationSpec {
    /// Creates a minimal version-one specification.
    #[must_use]
    pub fn new(name: OperationName, target_kind: TargetKind, access: AccessClass) -> Self {
        Self {
            name,
            schema_version: 1,
            target_kind,
            access,
            risk: RiskClass::Safe,
            reversibility: Reversibility::Reversible,
            required: ParameterGroup::empty(),
            required_any: Vec::new(),
            planning: CapabilitySupport::Unsupported,
            progress: CapabilitySupport::Unsupported,
            cancellation: CapabilitySupport::Unsupported,
            verification: CapabilitySupport::Unsupported,
            fanout: CapabilitySupport::Unsupported,
            retry: RetryClass::Never,
            idempotent: false,
            evidence: BTreeSet::new(),
            requirements: BTreeSet::new(),
        }
    }

    /// Sets the serialized contract version.
    #[must_use]
    pub fn with_schema_version(mut self, schema_version: u32) -> Self {
        self.schema_version = schema_version;
        self
    }

    /// Sets risk and reversibility metadata.
    #[must_use]
    pub fn with_safety(mut self, risk: RiskClass, reversibility: Reversibility) -> Self {
        self.risk = risk;
        self.reversibility = reversibility;
        self
    }

    /// Sets fields required for every request.
    #[must_use]
    pub fn with_required(mut self, required: ParameterGroup) -> Self {
        self.required = required;
        self
    }

    /// Adds an alternative complete parameter group.
    #[must_use]
    pub fn with_required_any(mut self, group: ParameterGroup) -> Self {
        self.required_any.push(group);
        self
    }

    /// Sets lifecycle and fanout capability support.
    #[must_use]
    pub fn with_lifecycle(
        mut self,
        planning: CapabilitySupport,
        progress: CapabilitySupport,
        cancellation: CapabilitySupport,
        verification: CapabilitySupport,
        fanout: CapabilitySupport,
    ) -> Self {
        self.planning = planning;
        self.progress = progress;
        self.cancellation = cancellation;
        self.verification = verification;
        self.fanout = fanout;
        self
    }

    /// Sets retry behavior and whether mutations are idempotent.
    #[must_use]
    pub fn with_retry(mut self, retry: RetryClass, idempotent: bool) -> Self {
        self.retry = retry;
        self.idempotent = idempotent;
        self
    }

    /// Adds an evidence kind returned by the operation.
    #[must_use]
    pub fn with_evidence(mut self, evidence: EvidenceKind) -> Self {
        self.evidence.insert(evidence);
        self
    }

    /// Adds an implementation capability requirement such as `transport.ssh`.
    pub fn with_requirement(mut self, requirement: impl Into<String>) -> Result<Self, SpecError> {
        let requirement = requirement.into();
        if !valid_requirement(&requirement) {
            return Err(SpecError::InvalidRequirement(requirement));
        }
        self.requirements.insert(requirement);
        Ok(self)
    }

    /// Validates cross-field safety and compatibility invariants.
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.schema_version == 0 {
            return Err(SpecError::ZeroSchemaVersion);
        }
        if self.required_any.iter().any(ParameterGroup::is_empty) {
            return Err(SpecError::EmptyAlternative);
        }
        let mut alternatives = BTreeSet::new();
        for group in &self.required_any {
            if !alternatives.insert(group) {
                return Err(SpecError::DuplicateAlternative);
            }
        }
        if self.access == AccessClass::Read && self.risk == RiskClass::Destructive {
            return Err(SpecError::DestructiveRead);
        }
        if self.access == AccessClass::Read && self.idempotent {
            return Err(SpecError::ReadMarkedIdempotent);
        }
        if self.access == AccessClass::Mutation
            && self.retry == RetryClass::Safe
            && !self.idempotent
        {
            return Err(SpecError::UnsafeRetryClaim);
        }
        if self.access == AccessClass::Mutation
            && self.risk >= RiskClass::Destructive
            && self.planning == CapabilitySupport::Unsupported
        {
            return Err(SpecError::RiskyMutationWithoutPlan);
        }
        Ok(())
    }

    /// Returns the canonical operation name.
    #[must_use]
    pub fn name(&self) -> &OperationName {
        &self.name
    }

    /// Returns the contract schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the required target kind.
    #[must_use]
    pub fn target_kind(&self) -> &TargetKind {
        &self.target_kind
    }

    /// Returns the access class.
    #[must_use]
    pub const fn access(&self) -> AccessClass {
        self.access
    }

    /// Returns the risk class.
    #[must_use]
    pub const fn risk(&self) -> RiskClass {
        self.risk
    }

    /// Returns expected reversibility.
    #[must_use]
    pub const fn reversibility(&self) -> Reversibility {
        self.reversibility
    }

    /// Returns fields required for every request.
    #[must_use]
    pub fn required(&self) -> &ParameterGroup {
        &self.required
    }

    /// Returns alternative complete parameter groups.
    #[must_use]
    pub fn required_any(&self) -> &[ParameterGroup] {
        &self.required_any
    }

    /// Returns planning support.
    #[must_use]
    pub const fn planning(&self) -> CapabilitySupport {
        self.planning
    }

    /// Returns progress support.
    #[must_use]
    pub const fn progress(&self) -> CapabilitySupport {
        self.progress
    }

    /// Returns cancellation support.
    #[must_use]
    pub const fn cancellation(&self) -> CapabilitySupport {
        self.cancellation
    }

    /// Returns verification support.
    #[must_use]
    pub const fn verification(&self) -> CapabilitySupport {
        self.verification
    }

    /// Returns fanout support.
    #[must_use]
    pub const fn fanout(&self) -> CapabilitySupport {
        self.fanout
    }

    /// Returns retry classification.
    #[must_use]
    pub const fn retry(&self) -> RetryClass {
        self.retry
    }

    /// Returns whether mutation repetition is idempotent.
    #[must_use]
    pub const fn idempotent(&self) -> bool {
        self.idempotent
    }

    /// Iterates over expected evidence kinds.
    pub fn evidence(&self) -> impl Iterator<Item = &EvidenceKind> {
        self.evidence.iter()
    }

    /// Iterates over implementation capability requirements.
    pub fn requirements(&self) -> impl Iterator<Item = &str> {
        self.requirements.iter().map(String::as_str)
    }
}

/// Typed definition implemented by a concrete infrastructure operation.
pub trait OperationDefinition {
    /// Operation-specific request parameters.
    type Parameters: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    /// Operation-specific successful output.
    type Output: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;

    /// Returns the operation catalog specification.
    fn spec() -> OperationSpec;

    /// Resolves the request parameters to a typed target.
    fn target(parameters: &Self::Parameters) -> Result<TargetRef, TargetRefError>;
}

/// Invalid operation catalog metadata.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SpecError {
    /// Schema version zero is invalid.
    #[error("operation schema version must be greater than zero")]
    ZeroSchemaVersion,
    /// A parameter name is invalid.
    #[error("invalid operation parameter name: {0}")]
    InvalidParameter(String),
    /// A parameter appears more than once in a group.
    #[error("duplicate operation parameter: {0}")]
    DuplicateParameter(String),
    /// An alternative parameter group is empty.
    #[error("alternative parameter groups must not be empty")]
    EmptyAlternative,
    /// The same alternative parameter group appears more than once.
    #[error("duplicate alternative parameter group")]
    DuplicateAlternative,
    /// A read operation was incorrectly classified as destructive.
    #[error("read operations cannot be classified as destructive")]
    DestructiveRead,
    /// Read operations do not use mutation idempotency metadata.
    #[error("read operations cannot be marked as idempotent mutations")]
    ReadMarkedIdempotent,
    /// A non-idempotent mutation claimed safe automatic retry.
    #[error("safe retry for a mutation requires idempotent behavior")]
    UnsafeRetryClaim,
    /// A destructive or privileged mutation omitted planning support.
    #[error("destructive and privileged mutations require planning support")]
    RiskyMutationWithoutPlan,
    /// An implementation capability requirement is invalid.
    #[error("invalid operation capability requirement: {0}")]
    InvalidRequirement(String),
}

fn valid_parameter_name(value: &str) -> bool {
    let mut chars = value.chars();
    let count = value.chars().count();
    count <= MAX_PARAMETER_CHARS
        && matches!(chars.next(), Some('a'..='z'))
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !value.ends_with('_')
}

fn valid_requirement(value: &str) -> bool {
    let count = value.chars().count();
    count > 0
        && count <= MAX_REQUIREMENT_CHARS
        && !value.chars().any(char::is_control)
        && value.split('.').all(valid_name_segment)
}

fn valid_name_segment(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        })
        && !value.ends_with(['-', '_'])
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;

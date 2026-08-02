use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{OperationId, OperationName, Reversibility, RiskClass, TargetRef};

const MAX_PLAN_TEXT_CHARS: usize = 2_048;

/// SHA-256 fingerprint of the complete authorization-relevant plan material.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct PlanFingerprint(String);

impl PlanFingerprint {
    /// Parses a lowercase SHA-256 fingerprint.
    pub fn parse(value: impl Into<String>) -> Result<Self, PlanError> {
        let value = value.into();
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            Ok(Self(value))
        } else {
            Err(PlanError::InvalidFingerprint)
        }
    }

    /// Returns the lowercase hexadecimal digest.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One intended resource change described before execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlannedChange {
    resource: TargetRef,
    action: String,
    summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    before_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after_digest: Option<String>,
}

impl PlannedChange {
    /// Creates a validated planned change.
    pub fn new(
        resource: TargetRef,
        action: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<Self, PlanError> {
        let action = action.into();
        let summary = summary.into();
        validate_plan_text("change action", &action)?;
        validate_plan_text("change summary", &summary)?;
        Ok(Self {
            resource,
            action,
            summary,
            before_digest: None,
            after_digest: None,
        })
    }

    /// Records before-and-after content digests.
    #[must_use]
    pub fn with_digests(mut self, before: Option<String>, after: Option<String>) -> Self {
        self.before_digest = before;
        self.after_digest = after;
        self
    }

    /// Returns the affected resource.
    #[must_use]
    pub fn resource(&self) -> &TargetRef {
        &self.resource
    }

    /// Returns the backend-neutral action label.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Returns the human-readable change summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }
}

/// One ordered execution step within a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanStep {
    sequence: u32,
    operation: OperationName,
    target: TargetRef,
    summary: String,
}

impl PlanStep {
    /// Creates a validated, one-based execution step.
    pub fn new(
        sequence: u32,
        operation: OperationName,
        target: TargetRef,
        summary: impl Into<String>,
    ) -> Result<Self, PlanError> {
        if sequence == 0 {
            return Err(PlanError::InvalidStepSequence);
        }
        let summary = summary.into();
        validate_plan_text("step summary", &summary)?;
        Ok(Self {
            sequence,
            operation,
            target,
            summary,
        })
    }

    /// Returns the one-based sequence number.
    #[must_use]
    pub const fn sequence(&self) -> u32 {
        self.sequence
    }

    /// Returns the operation performed by the step.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }

    /// Returns the step target.
    #[must_use]
    pub fn target(&self) -> &TargetRef {
        &self.target
    }
}

/// Operation used to verify actual runtime state after execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct VerificationStrategy {
    operation: OperationName,
    description: String,
}

impl VerificationStrategy {
    /// Creates a validated verification strategy.
    pub fn new(
        operation: OperationName,
        description: impl Into<String>,
    ) -> Result<Self, PlanError> {
        let description = description.into();
        validate_plan_text("verification description", &description)?;
        Ok(Self {
            operation,
            description,
        })
    }

    /// Returns the verification operation.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }

    /// Returns the verification intent.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }
}

/// Immutable authorization-relevant operation plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OperationPlan {
    operation_id: OperationId,
    operation: OperationName,
    target: TargetRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    topology_revision: Option<String>,
    changes: Vec<PlannedChange>,
    risk: RiskClass,
    reversibility: Reversibility,
    prerequisites: Vec<String>,
    conflicts: Vec<String>,
    steps: Vec<PlanStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verification: Option<VerificationStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rollback_guidance: Option<String>,
    fingerprint: PlanFingerprint,
}

impl OperationPlan {
    /// Creates an empty plan and computes its first fingerprint.
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        target: TargetRef,
        risk: RiskClass,
        reversibility: Reversibility,
    ) -> Result<Self, PlanError> {
        let mut plan = Self {
            operation_id,
            operation,
            target,
            topology_revision: None,
            changes: Vec::new(),
            risk,
            reversibility,
            prerequisites: Vec::new(),
            conflicts: Vec::new(),
            steps: Vec::new(),
            verification: None,
            rollback_guidance: None,
            fingerprint: PlanFingerprint(String::new()),
        };
        plan.refresh_fingerprint()?;
        Ok(plan)
    }

    /// Binds the plan to a topology revision.
    pub fn with_topology_revision(
        mut self,
        revision: impl Into<String>,
    ) -> Result<Self, PlanError> {
        let revision = revision.into();
        validate_plan_text("topology revision", &revision)?;
        self.topology_revision = Some(revision);
        self.refresh_fingerprint()?;
        Ok(self)
    }

    /// Adds a planned change.
    pub fn with_change(mut self, change: PlannedChange) -> Result<Self, PlanError> {
        self.changes.push(change);
        self.refresh_fingerprint()?;
        Ok(self)
    }

    /// Adds a prerequisite description.
    pub fn with_prerequisite(mut self, prerequisite: impl Into<String>) -> Result<Self, PlanError> {
        let prerequisite = prerequisite.into();
        validate_plan_text("prerequisite", &prerequisite)?;
        self.prerequisites.push(prerequisite);
        self.refresh_fingerprint()?;
        Ok(self)
    }

    /// Adds a conflict description.
    pub fn with_conflict(mut self, conflict: impl Into<String>) -> Result<Self, PlanError> {
        let conflict = conflict.into();
        validate_plan_text("conflict", &conflict)?;
        self.conflicts.push(conflict);
        self.refresh_fingerprint()?;
        Ok(self)
    }

    /// Adds an ordered execution step.
    pub fn with_step(mut self, step: PlanStep) -> Result<Self, PlanError> {
        self.steps.push(step);
        validate_step_sequence(&self.steps)?;
        self.refresh_fingerprint()?;
        Ok(self)
    }

    /// Sets the verification strategy.
    pub fn with_verification(
        mut self,
        verification: VerificationStrategy,
    ) -> Result<Self, PlanError> {
        self.verification = Some(verification);
        self.refresh_fingerprint()?;
        Ok(self)
    }

    /// Sets rollback or recovery guidance.
    pub fn with_rollback_guidance(
        mut self,
        guidance: impl Into<String>,
    ) -> Result<Self, PlanError> {
        let guidance = guidance.into();
        validate_plan_text("rollback guidance", &guidance)?;
        self.rollback_guidance = Some(guidance);
        self.refresh_fingerprint()?;
        Ok(self)
    }

    /// Recomputes and validates the deterministic fingerprint.
    pub fn refresh_fingerprint(&mut self) -> Result<(), PlanError> {
        self.fingerprint = compute_fingerprint(self)?;
        Ok(())
    }

    /// Verifies that serialized plan material still matches its fingerprint.
    pub fn validate_fingerprint(&self) -> Result<(), PlanError> {
        if compute_fingerprint(self)? == self.fingerprint {
            Ok(())
        } else {
            Err(PlanError::FingerprintMismatch)
        }
    }

    /// Returns the operation identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the canonical operation name.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }

    /// Returns the resolved target.
    #[must_use]
    pub fn target(&self) -> &TargetRef {
        &self.target
    }

    /// Returns the topology revision when present.
    #[must_use]
    pub fn topology_revision(&self) -> Option<&str> {
        self.topology_revision.as_deref()
    }

    /// Returns planned resource changes.
    #[must_use]
    pub fn changes(&self) -> &[PlannedChange] {
        &self.changes
    }

    /// Returns ordered execution steps.
    #[must_use]
    pub fn steps(&self) -> &[PlanStep] {
        &self.steps
    }

    /// Returns the verification strategy when present.
    #[must_use]
    pub fn verification(&self) -> Option<&VerificationStrategy> {
        self.verification.as_ref()
    }

    /// Returns the authorization-relevant plan fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> &PlanFingerprint {
        &self.fingerprint
    }
}

#[derive(Serialize)]
struct FingerprintMaterial<'a> {
    operation_id: &'a OperationId,
    operation: &'a OperationName,
    target: &'a TargetRef,
    topology_revision: &'a Option<String>,
    changes: &'a [PlannedChange],
    risk: RiskClass,
    reversibility: Reversibility,
    prerequisites: &'a [String],
    conflicts: &'a [String],
    steps: &'a [PlanStep],
    verification: &'a Option<VerificationStrategy>,
    rollback_guidance: &'a Option<String>,
}

fn compute_fingerprint(plan: &OperationPlan) -> Result<PlanFingerprint, PlanError> {
    let material = FingerprintMaterial {
        operation_id: &plan.operation_id,
        operation: &plan.operation,
        target: &plan.target,
        topology_revision: &plan.topology_revision,
        changes: &plan.changes,
        risk: plan.risk,
        reversibility: plan.reversibility,
        prerequisites: &plan.prerequisites,
        conflicts: &plan.conflicts,
        steps: &plan.steps,
        verification: &plan.verification,
        rollback_guidance: &plan.rollback_guidance,
    };
    let encoded = serde_json::to_vec(&material)
        .map_err(|error| PlanError::Serialization(error.to_string()))?;
    let digest = Sha256::digest(encoded);
    let value: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    PlanFingerprint::parse(value)
}

fn validate_step_sequence(steps: &[PlanStep]) -> Result<(), PlanError> {
    let mut seen = BTreeSet::new();
    for (index, step) in steps.iter().enumerate() {
        let expected = u32::try_from(index + 1).map_err(|_| PlanError::InvalidStepSequence)?;
        if step.sequence != expected || !seen.insert(step.sequence) {
            return Err(PlanError::InvalidStepSequence);
        }
    }
    Ok(())
}

fn validate_plan_text(field: &'static str, value: &str) -> Result<(), PlanError> {
    let chars = value.chars().count();
    if chars == 0 || chars > MAX_PLAN_TEXT_CHARS || value.chars().any(char::is_control) {
        return Err(PlanError::InvalidText {
            field,
            max_chars: MAX_PLAN_TEXT_CHARS,
        });
    }
    Ok(())
}

/// Invalid operation plan or plan fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    /// Fingerprints must be lowercase SHA-256 hex.
    #[error("invalid plan fingerprint")]
    InvalidFingerprint,
    /// Plan material no longer matches the recorded fingerprint.
    #[error("plan fingerprint does not match plan material")]
    FingerprintMismatch,
    /// Execution steps must be unique, contiguous, and one-based.
    #[error("plan steps must use contiguous one-based sequence numbers")]
    InvalidStepSequence,
    /// Plan text was empty, oversized, or contained control characters.
    #[error("invalid {field}: expected 1..={max_chars} non-control characters")]
    InvalidText {
        /// Plan field.
        field: &'static str,
        /// Maximum accepted character count.
        max_chars: usize,
    },
    /// Fingerprint material could not be serialized.
    #[error("could not serialize plan fingerprint material: {0}")]
    Serialization(String),
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;

use soma_fleet::HostRecord;
use soma_ops::{
    AuthorizationEvidence, OperationContext, OperationName, OperationPlan, TargetRef, Timestamp,
};

use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate_admission(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        target: &TargetRef,
        host: &HostRecord,
        now: Timestamp,
        idempotent: bool,
        expected_verification: &str,
    ) -> Result<(), ExecutionError> {
        plan.validate_fingerprint()?;
        if plan.operation_id() != context.operation_id() {
            return Err(ExecutionError::PlanMismatch(
                "operation identity differs from the execution context".into(),
            ));
        }
        if plan.operation() != operation {
            return Err(ExecutionError::PlanMismatch(
                "canonical operation differs from the requested mutation".into(),
            ));
        }
        if plan.target() != target {
            return Err(ExecutionError::PlanMismatch(
                "target identity, host, or resource revision changed".into(),
            ));
        }
        if plan.topology_revision() != Some(host.revision().as_str()) {
            return Err(ExecutionError::PlanMismatch(
                "fleet topology revision changed after planning".into(),
            ));
        }
        let verification = plan.verification().ok_or_else(|| {
            ExecutionError::PlanMismatch("required verification strategy is absent".into())
        })?;
        if verification.operation().as_str() != expected_verification {
            return Err(ExecutionError::PlanMismatch(format!(
                "unexpected verification operation; expected {expected_verification}"
            )));
        }
        if plan.changes().is_empty() || plan.steps().len() != 1 {
            return Err(ExecutionError::PlanMismatch(
                "lifecycle plan must contain one change and one execution step".into(),
            ));
        }
        if context.deadline().is_some_and(|deadline| deadline <= now) {
            return Err(ExecutionError::DeadlineExceeded);
        }
        if idempotent && context.idempotency_key().is_none() {
            return Err(ExecutionError::MissingIdempotencyKey);
        }
        if authorization.confirmation_ref().is_none() {
            return Err(ExecutionError::ConfirmationRequired);
        }
        authorization.validate_binding(operation, target, now, Some(plan.fingerprint()))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "mutation_admission_tests.rs"]
mod tests;

use soma_ops::{AuthorizationEvidence, OperationContext, OperationName, OperationPlan, Timestamp};

use crate::ExecutionError;
use crate::mutation_runtime::DEFAULT_MUTATION_DEADLINE_MS;

pub(crate) fn validate_final_admission(
    operation: &OperationName,
    context: &OperationContext,
    plan: &OperationPlan,
    authorization: &AuthorizationEvidence,
    expected: &OperationPlan,
    now: Timestamp,
    idempotent: bool,
) -> Result<(), ExecutionError> {
    plan.validate_fingerprint()?;
    if plan != expected {
        return Err(ExecutionError::PlanMismatch(
            "cleanup inventory, Compose state, transfer content, target revision, or operation parameters changed after planning".into(),
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
    authorization.validate_binding(operation, plan.target(), now, Some(plan.fingerprint()))?;
    Ok(())
}

pub(crate) fn validate_final_changes(
    plan: &OperationPlan,
    expected: &[soma_ops::PlannedChange],
) -> Result<(), ExecutionError> {
    if plan.changes() != expected {
        return Err(ExecutionError::PlanMismatch(
            "authorization-relevant final mutation changes differ from the current state".into(),
        ));
    }
    Ok(())
}

pub(crate) fn final_execution_deadline(
    context: &OperationContext,
    started: Timestamp,
) -> Timestamp {
    context.deadline().unwrap_or_else(|| {
        Timestamp::from_unix_millis(
            started
                .unix_millis()
                .saturating_add(DEFAULT_MUTATION_DEADLINE_MS),
        )
    })
}

#[cfg(test)]
#[path = "mutation_final_admission_tests.rs"]
mod tests;

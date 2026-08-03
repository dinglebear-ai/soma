use serde_json::Value;
use soma_infra::MutationProgressReporter;
use soma_ops::{AuthorizationEvidence, OperationContext, OperationName, OperationPlan};
use tokio_util::sync::CancellationToken;

use crate::mutation_runtime::lifecycle_action;
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    /// Builds a deterministic, topology-bound plan for a supported mutation.
    pub async fn plan(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        if lifecycle_action(operation).is_ok() {
            self.plan_container(operation, parameters, context).await
        } else if crate::mutation_compose::compose_action(operation).is_ok() {
            self.plan_compose(operation, parameters, context).await
        } else if crate::mutation_pull::pull_operation(operation) {
            self.plan_pull(operation, parameters, context).await
        } else {
            Err(ExecutionError::UnsupportedOperation(operation.clone()))
        }
    }

    /// Executes one supported mutation while intentionally discarding progress events.
    pub async fn execute(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        self.execute_with_progress(
            operation,
            parameters,
            context,
            plan,
            authorization,
            &soma_ops::NoopProgressSink,
            cancellation,
        )
        .await
    }

    /// Executes one supported mutation with canonical progress delivery.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_with_progress(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        if lifecycle_action(operation).is_ok() {
            self.execute_container(
                operation,
                parameters,
                context,
                plan,
                authorization,
                cancellation,
            )
            .await
        } else if crate::mutation_compose::compose_action(operation).is_ok() {
            self.execute_compose(
                operation,
                parameters,
                context,
                plan,
                authorization,
                cancellation,
            )
            .await
        } else if crate::mutation_pull::pull_operation(operation) {
            self.execute_pull(
                operation,
                parameters,
                context,
                plan,
                authorization,
                progress,
                cancellation,
            )
            .await
        } else {
            Err(ExecutionError::UnsupportedOperation(operation.clone()))
        }
    }
}

#[cfg(test)]
#[path = "mutation_dispatch_tests.rs"]
mod tests;

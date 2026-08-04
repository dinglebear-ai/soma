use serde_json::Value;
use soma_infra::VerifiedFileTransferRequest;
use soma_ops::{AuthorizationEvidence, OperationContext, OperationName, OperationPlan, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::mutation_final_admission::{
    final_execution_deadline, validate_final_admission, validate_final_changes,
};
use crate::mutation_final_contract::{transfer_change, transfer_target};
use crate::runtime_params::{required_path, required_str};
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_transfer(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started = Timestamp::now();
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let source = self
            .resolve_host(required_str(parameters, "source_host")?)
            .await?;
        let destination = self
            .resolve_host(required_str(parameters, "dest_host")?)
            .await?;
        let source_path = required_path(parameters, "source_path")?;
        let destination_path = required_path(parameters, "dest_path")?;
        let target = transfer_target(&source, &source_path, &destination, &destination_path)?;
        let expected = self.plan_final(operation, parameters, context).await?;
        validate_final_admission(
            operation,
            context,
            plan,
            authorization,
            &expected,
            started,
            spec.idempotent(),
        )?;
        let ports = self.final_ports("file-transfer")?;
        let fingerprint = self
            .file_transfer
            .inspect(
                ports.transfer.as_ref(),
                &source,
                &source_path,
                &destination,
                &destination_path,
                cancellation,
            )
            .await?;
        validate_final_changes(plan, &[transfer_change(&target, &fingerprint)?])?;
        let request = VerifiedFileTransferRequest {
            operation_id: context.operation_id().clone(),
            operation: operation.clone(),
            fingerprint,
            deadline: final_execution_deadline(context, started),
        };
        match self
            .file_transfer
            .execute(
                ports.transfer.as_ref(),
                &source,
                &destination,
                &request,
                cancellation,
            )
            .await
        {
            Ok(outcome) => {
                self.transfer_outcome_result(operation, context, target, started, outcome)
            }
            Err(failure) => self.transfer_failure_result(
                operation,
                context,
                target,
                started,
                failure.send_state(),
                spec.retry(),
                failure.into_error(),
                &source_path,
                &destination_path,
            ),
        }
    }
}

#[cfg(test)]
#[path = "mutation_final_transfer_execute_tests.rs"]
mod tests;

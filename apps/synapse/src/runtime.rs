use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use soma_fleet::OpenSshDriver;
use soma_ops::{
    AccessClass, ActorRef, AuthorizationEvidence, AuthorizationScope, IdempotencyKey,
    OperationContext, OperationName, OperationPlan, ProducerRef, Timestamp,
};
use synapse_application::{
    ExecutionError, LegacyTool, SynapseCatalog, SynapseMutationRuntime, SynapseReadRuntime,
};
use tokio_util::sync::CancellationToken;

use crate::activity::ActivityLog;
use crate::config::SynapseConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExecuteOptions {
    pub confirmed: bool,
    pub idempotency_key: Option<String>,
    pub actor: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StandaloneError {
    #[error("unknown canonical operation: {0}")]
    UnknownOperation(String),
    #[error("mutation confirmation is required")]
    ConfirmationRequired(Box<OperationPlan>),
    #[error(transparent)]
    Execution(#[from] ExecutionError),
    #[error(transparent)]
    Compatibility(#[from] synapse_application::CompatibilityError),
    #[error(transparent)]
    FleetIdentity(#[from] soma_fleet::IdentityError),
    #[error(transparent)]
    Infra(#[from] soma_infra::InfraError),
    #[error(transparent)]
    OperationIdentity(#[from] soma_ops::IdentityError),
    #[error(transparent)]
    Idempotency(#[from] soma_ops::IdempotencyKeyError),
    #[error(transparent)]
    Authorization(#[from] soma_ops::AuthorizationError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl StandaloneError {
    pub fn plan(&self) -> Option<&OperationPlan> {
        match self {
            Self::ConfirmationRequired(plan) => Some(plan),
            _ => None,
        }
    }
}

pub struct StandaloneRuntime {
    pub(crate) config: SynapseConfig,
    pub(crate) catalog: &'static SynapseCatalog,
    pub(crate) read: SynapseReadRuntime,
    pub(crate) mutation: SynapseMutationRuntime,
    pub(crate) ssh: Arc<OpenSshDriver>,
    pub(crate) activity: ActivityLog,
}

impl StandaloneRuntime {
    pub fn config(&self) -> &SynapseConfig {
        &self.config
    }

    pub fn catalog(&self) -> &'static SynapseCatalog {
        self.catalog
    }

    pub fn activity(&self) -> &ActivityLog {
        &self.activity
    }

    pub async fn plan(
        &self,
        operation: &str,
        parameters: &Value,
        options: &ExecuteOptions,
    ) -> Result<OperationPlan, StandaloneError> {
        let operation = OperationName::new(operation)
            .map_err(|_| StandaloneError::UnknownOperation(operation.to_owned()))?;
        let spec = self
            .catalog
            .operation(&operation)
            .ok_or_else(|| StandaloneError::UnknownOperation(operation.to_string()))?;
        if spec.access() != AccessClass::Mutation {
            return Err(StandaloneError::UnknownOperation(format!(
                "{} is not a mutation",
                operation
            )));
        }
        let context = self.context(spec.idempotent(), options)?;
        self.mutation
            .plan(&operation, parameters, &context)
            .await
            .map_err(Into::into)
    }

    pub async fn execute(
        &self,
        operation: &str,
        parameters: &Value,
        options: &ExecuteOptions,
        cancellation: &CancellationToken,
    ) -> Result<Value, StandaloneError> {
        let started = Instant::now();
        let result = self
            .execute_inner(operation, parameters, options, cancellation)
            .await;
        self.activity.record(
            options.actor.as_deref().unwrap_or("standalone"),
            operation,
            result.is_ok(),
            started.elapsed(),
            result.as_ref().err().map(ToString::to_string),
        );
        result
    }

    async fn execute_inner(
        &self,
        operation: &str,
        parameters: &Value,
        options: &ExecuteOptions,
        cancellation: &CancellationToken,
    ) -> Result<Value, StandaloneError> {
        let operation = OperationName::new(operation)
            .map_err(|_| StandaloneError::UnknownOperation(operation.to_owned()))?;
        let spec = self
            .catalog
            .operation(&operation)
            .ok_or_else(|| StandaloneError::UnknownOperation(operation.to_string()))?;
        if spec.access() == AccessClass::Read {
            return self
                .read
                .execute(&operation, parameters, cancellation)
                .await
                .map_err(Into::into);
        }

        let context = self.context(spec.idempotent(), options)?;
        let plan = self.mutation.plan(&operation, parameters, &context).await?;
        if !options.confirmed && !self.config.server.allow_mutations {
            return Err(StandaloneError::ConfirmationRequired(Box::new(plan)));
        }
        let now = Timestamp::now();
        let ttl =
            i64::try_from(self.config.server.authorization_ttl().as_millis()).unwrap_or(i64::MAX);
        let authorization = AuthorizationEvidence::new(
            ProducerRef::new("synapse-standalone", env!("CARGO_PKG_VERSION"))?,
            AuthorizationScope::new(operation.clone(), plan.target().clone()),
            now,
            Timestamp::from_unix_millis(now.unix_millis().saturating_add(ttl)),
        )?
        .with_plan_fingerprint(plan.fingerprint().clone())
        .with_confirmation_ref(if options.confirmed {
            "standalone:explicit-confirmation"
        } else {
            "standalone:configured-auto-confirmation"
        })?;
        let result = self
            .mutation
            .execute(
                &operation,
                parameters,
                &context,
                &plan,
                &authorization,
                cancellation,
            )
            .await?;
        serde_json::to_value(result).map_err(|error| anyhow::anyhow!(error).into())
    }

    pub async fn execute_legacy(
        &self,
        tool: LegacyTool,
        input: &Value,
        options: &ExecuteOptions,
        cancellation: &CancellationToken,
    ) -> Result<Value, StandaloneError> {
        let normalized = self.catalog.normalize_legacy_request(tool, input)?;
        self.execute(
            normalized.operation().as_str(),
            normalized.parameters(),
            options,
            cancellation,
        )
        .await
    }

    pub fn operation_catalog_json(&self) -> Value {
        serde_json::to_value(self.catalog.operations().collect::<Vec<_>>())
            .expect("checked-in operations serialize")
    }

    pub async fn shutdown(&self) {
        let _ = self.ssh.shutdown().await;
    }

    fn context(
        &self,
        idempotent: bool,
        options: &ExecuteOptions,
    ) -> Result<OperationContext, StandaloneError> {
        let now = Timestamp::now();
        let timeout =
            i64::try_from(self.config.server.request_timeout().as_millis()).unwrap_or(i64::MAX);
        let actor = ActorRef::new(
            "synapse",
            options.actor.as_deref().unwrap_or("standalone-client"),
        )?;
        let mut context =
            OperationContext::new()
                .with_actor(actor)
                .with_deadline(Timestamp::from_unix_millis(
                    now.unix_millis().saturating_add(timeout),
                ));
        if idempotent {
            let key = options
                .idempotency_key
                .clone()
                .unwrap_or_else(|| format!("synapse-{}", context.operation_id()));
            context = context.with_idempotency_key(IdempotencyKey::new(key)?);
        }
        Ok(context)
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

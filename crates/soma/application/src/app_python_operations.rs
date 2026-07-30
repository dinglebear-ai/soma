use super::*;

impl SomaApplication {
    pub(super) async fn execute_python_environment_action(
        &self,
        request: ExecuteActionRequest,
        context: ExecutionContext,
    ) -> Result<ExecuteActionResponse, ApplicationError> {
        let limits = ProviderRequestLimits {
            max_response_bytes: context
                .response_limit
                .unwrap_or(ProviderRequestLimits::default().max_response_bytes),
            ..ProviderRequestLimits::default()
        };
        let call = ProviderCall {
            provider: String::new(),
            action: request.action.clone(),
            params: request.params.clone(),
            principal: provider_principal(context.principal.as_ref()),
            auth_mode: provider_auth_mode(context.authorization_mode),
            surface: provider_surface(context.surface),
            destructive_confirmed: context.destructive_confirmation.is_confirmed(),
            limits,
            snapshot_id: String::new(),
        };
        let call = self.legacy_registry.authorize_operator_action(call)?;
        let action = SomaAction::from_rest(&request.action, &request.params)
            .map_err(|error| ApplicationError::service(&error))?;
        let output = match action {
            SomaAction::PythonEnvironmentStatus => self.ports.python_environment.status().await?,
            SomaAction::PythonEnvironmentPrunePlan {
                stale_before_unix_seconds,
                max_entries,
            } => {
                self.ports
                    .python_environment
                    .prune(stale_before_unix_seconds, max_entries, false)
                    .await?
            }
            SomaAction::PythonEnvironmentPrune {
                stale_before_unix_seconds,
                max_entries,
            } => {
                self.ports
                    .python_environment
                    .prune(stale_before_unix_seconds, max_entries, true)
                    .await?
            }
            SomaAction::PythonEnvironmentRepair { provider_path } => {
                let provider_path = self.managed_python_provider_path(&provider_path)?;
                self.ports.python_environment.repair(&provider_path).await?
            }
            SomaAction::PythonEnvironmentUpdate { provider_path } => {
                let provider_path = self.managed_python_provider_path(&provider_path)?;
                let update = self.ports.python_environment.update(&provider_path).await?;
                let update = update.into_report();
                let candidate = update.candidate.clone();
                let snapshot = self
                    .legacy_registry
                    .activate_python_candidate(&provider_path, candidate)
                    .await
                    .map_err(|error| {
                        ApplicationError::new(
                            error.code(),
                            crate::provider_errors::redact_public(error.message()),
                            false,
                            "Keep the active generation, inspect the candidate, and retry.",
                        )
                    })?;
                serde_json::json!({
                    "update": update,
                    "active_snapshot": {
                        "id": snapshot.id,
                        "fingerprint": snapshot.fingerprint,
                    }
                })
            }
            SomaAction::PythonWorkerStatus => self.legacy_registry.python_worker_status(),
            SomaAction::PythonWorkerCancel { provider } => {
                let cancelled = self
                    .legacy_registry
                    .cancel_python_worker(&provider)
                    .map_err(ApplicationError::from)?;
                serde_json::json!({ "provider": provider, "cancelled": cancelled })
            }
            SomaAction::PythonWorkerReset { provider } => {
                self.legacy_registry
                    .reset_python_worker_quarantine(&provider)
                    .await
                    .map_err(ApplicationError::from)?;
                serde_json::json!({ "provider": provider, "reset": true })
            }
            SomaAction::PythonGenerationStatus => self.legacy_registry.python_generation_status(),
            SomaAction::PythonGenerationRollback { generation_id } => self
                .legacy_registry
                .rollback_python_generation(generation_id)
                .await
                .map_err(ApplicationError::from)?,
            _ => {
                return Err(ApplicationError::new(
                    "invalid_python_environment_action",
                    "action is not a Python environment operation",
                    false,
                    "Use one of the documented Python environment actions.",
                ));
            }
        };
        self.enforce_python_response_limit(output, &call, &context)
    }

    fn managed_python_provider_path(
        &self,
        provider_path: &str,
    ) -> Result<std::path::PathBuf, ApplicationError> {
        self.legacy_registry
            .resolve_python_provider_path(std::path::Path::new(provider_path))
            .map_err(|error| {
                ApplicationError::new(
                    error.code(),
                    error.message(),
                    false,
                    "Use a managed Python provider path and retry.",
                )
            })
    }

    fn enforce_python_response_limit(
        &self,
        output: Value,
        call: &ProviderCall,
        context: &ExecutionContext,
    ) -> Result<ExecuteActionResponse, ApplicationError> {
        let actual = serde_json::to_vec(&output)
            .map_err(|error| ApplicationError::legacy("response serialization", error))?
            .len();
        if actual > call.limits.max_response_bytes {
            return Err(ApplicationError::new(
                "response_too_large",
                format!(
                    "Python environment response exceeded {} bytes",
                    call.limits.max_response_bytes
                ),
                false,
                "Reduce the requested bound and retry.",
            ));
        }
        Ok(ExecuteActionResponse {
            output,
            request_id: context.request_id.as_str().to_owned(),
        })
    }
}

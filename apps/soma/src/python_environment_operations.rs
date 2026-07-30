use std::sync::Arc;

use anyhow::Result;
use soma_application::{PortError, PythonEnvironmentPort, PythonEnvironmentUpdateCandidate};
use soma_config::Config;

use super::verify_python_environment_inputs;

struct ConfiguredPythonEnvironmentOperations {
    lifecycle: Arc<soma_application::PythonEnvironmentLifecycle>,
    cache: soma_application::PythonEnvironmentCache,
    environment: soma_config::PythonEnvironmentConfig,
}

#[async_trait::async_trait]
impl PythonEnvironmentPort for ConfiguredPythonEnvironmentOperations {
    async fn status(&self) -> Result<serde_json::Value, PortError> {
        let cache = self.cache.clone();
        python_environment_blocking("status task", move || {
            let inventory = cache
                .inventory()
                .map_err(|error| python_environment_port_error("status", error))?;
            serde_json::to_value(inventory)
                .map_err(|error| python_environment_port_error("status serialization", error))
        })
        .await
    }

    async fn prune(
        &self,
        stale_before_unix_seconds: u64,
        max_entries: usize,
        apply: bool,
    ) -> Result<serde_json::Value, PortError> {
        let cache = self.cache.clone();
        python_environment_blocking("prune task", move || {
            let mut plan = cache
                .plan_prune(
                    soma_application::PythonEnvironmentPrunePolicy::conservative(
                        stale_before_unix_seconds,
                    ),
                )
                .map_err(|error| python_environment_port_error("prune planning", error))?;
            plan.candidates.truncate(max_entries);
            plan.reclaimable_size_bytes = plan.candidates.iter().fold(0_u64, |total, candidate| {
                total.saturating_add(candidate.entry.size_bytes)
            });
            plan.reclaimable_file_count = plan.candidates.iter().fold(0_u64, |total, candidate| {
                total.saturating_add(candidate.entry.file_count)
            });
            if !apply {
                return serde_json::to_value(serde_json::json!({
                    "applied": false,
                    "plan": plan,
                }))
                .map_err(|error| python_environment_port_error("prune plan serialization", error));
            }
            let report = cache
                .apply_prune(&plan)
                .map_err(|error| python_environment_port_error("prune", error))?;
            Ok(serde_json::json!({
                "applied": true,
                "plan": plan,
                "report": report,
            }))
        })
        .await
    }

    async fn repair(
        &self,
        provider_path: &std::path::Path,
    ) -> Result<serde_json::Value, PortError> {
        let lifecycle = self.lifecycle.clone();
        let environment = self.environment.clone();
        let provider_path = provider_path.to_path_buf();
        python_environment_blocking("repair task", move || {
            verify_python_environment_inputs(&environment)
                .map_err(|error| python_environment_port_error("repair preflight", error))?;
            let report = lifecycle
                .repair_provider(&provider_path)
                .map_err(|error| python_environment_port_error("repair", error))?;
            serde_json::to_value(report)
                .map_err(|error| python_environment_port_error("repair serialization", error))
        })
        .await
    }

    async fn update(
        &self,
        provider_path: &std::path::Path,
    ) -> Result<PythonEnvironmentUpdateCandidate, PortError> {
        let lifecycle = self.lifecycle.clone();
        let environment = self.environment.clone();
        let provider_path = provider_path.to_path_buf();
        python_environment_blocking("update task", move || {
            verify_python_environment_inputs(&environment)
                .map_err(|error| python_environment_port_error("update preflight", error))?;
            let report = lifecycle
                .update_provider(&provider_path)
                .map_err(|error| python_environment_port_error("update", error))?;
            Ok(PythonEnvironmentUpdateCandidate::from_report(report))
        })
        .await
    }
}

async fn python_environment_blocking<T, F>(operation: &str, work: F) -> Result<T, PortError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, PortError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| python_environment_port_error(operation, error))?
}

fn python_environment_port_error(operation: &str, error: impl std::fmt::Display) -> PortError {
    PortError {
        code: "python_environment_operation_failed".to_owned(),
        message: format!("{operation} failed: {error}"),
        retryable: false,
        remediation: "Inspect Python environment status, correct the configured inputs, and retry."
            .to_owned(),
    }
}

pub(super) fn python_environment_port(
    config: &Config,
) -> Result<Option<Arc<dyn PythonEnvironmentPort>>> {
    let environment = &config.python.environment;
    if !environment.enabled {
        return Ok(None);
    }
    let verified = verify_python_environment_inputs(environment)?;
    let fingerprint = soma_application::PythonRuntimeFingerprint::new(
        &environment.runtime_implementation,
        &environment.runtime_version,
        &environment.runtime_platform,
        &environment.wheel_platform_tag,
    )
    .map_err(|error| anyhow::anyhow!("invalid Python environment runtime identity: {error}"))?;
    let cache = soma_application::PythonEnvironmentCache::new(&verified.cache_root);
    let lifecycle = soma_application::PythonEnvironmentLifecycle::new(
        verified.uv_program,
        soma_application::PythonEnvironmentSpec {
            cache_root: verified.cache_root,
            runtime: fingerprint,
            python_executable: verified.python,
            sdk_wheel: verified.sdk_wheel,
            sdk_wheel_sha256: environment.sdk_wheel_sha256.clone(),
            uv_version: environment.uv_version.clone(),
            offline: environment.offline,
        },
    );
    Ok(Some(Arc::new(ConfiguredPythonEnvironmentOperations {
        lifecycle: Arc::new(lifecycle),
        cache,
        environment: environment.clone(),
    })))
}

#[cfg(test)]
#[path = "python_environment_operations_tests.rs"]
mod tests;

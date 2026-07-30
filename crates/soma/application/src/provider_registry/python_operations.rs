use super::*;

impl ProviderRegistry {
    /// Resolves one exact managed Python provider path without importing it.
    pub fn resolve_python_provider_path(
        &self,
        provider_path: &Path,
    ) -> Result<PathBuf, ProviderValidationError> {
        let source = self.file_source.as_ref().ok_or_else(|| {
            ProviderValidationError::new(
                "provider_file_source_unavailable",
                "Python environment operations require a file-backed provider registry",
            )
        })?;
        source
            .resolve_python_provider_path(provider_path)
            .map_err(|error| {
                ProviderValidationError::new("provider_candidate_path_invalid", error.to_string())
            })
    }

    /// Applies active-registry policy without invoking a provider.
    pub fn authorize_operator_action(
        &self,
        mut call: ProviderCall,
    ) -> Result<ProviderCall, ProviderError> {
        let state = self
            .state
            .read()
            .expect("provider registry lock should not be poisoned");
        let entry = state
            .snapshot
            .core_snapshot()
            .tool(&call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    "registry",
                    call.action.clone(),
                    "unknown_action",
                    format!("unknown provider action `{}`", call.action),
                )
            })?;
        let provider_name = entry.provider_id().as_str();
        let provider_kind = state
            .snapshot
            .catalogs
            .iter()
            .find(|catalog| catalog.provider.name == provider_name)
            .map(|catalog| catalog.provider.kind)
            .ok_or_else(|| {
                ProviderError::new(
                    "provider_not_loaded",
                    provider_name,
                    Some(call.action.clone()),
                    "provider is not loaded in the active registry",
                    "Reload providers and retry.",
                )
            })?;
        if !provider_tool_surface_enabled(entry.spec(), call.surface) {
            return Err(ProviderError::validation(
                provider_name,
                call.action.clone(),
                "surface_disabled",
                "action is not enabled on this surface",
            ));
        }
        call.provider = provider_name.to_owned();
        call.snapshot_id = state.snapshot.id.clone();
        enforce_pre_input(entry.spec(), &call, provider_kind)?;
        Ok(call)
    }

    /// Inventories persistent worker state and bounded redacted logs.
    pub fn python_worker_status(&self) -> Value {
        let state = self
            .state
            .read()
            .expect("provider registry lock should not be poisoned");
        let workers = state
            .providers
            .iter()
            .filter_map(|(name, provider)| {
                provider.runtime_status().map(|status| {
                    json!({
                        "provider": name,
                        "status": status,
                    })
                })
            })
            .collect::<Vec<_>>();
        json!({
            "snapshot_id": state.snapshot.id,
            "workers": workers,
        })
    }

    /// Cancels the active invocation for one persistent Python provider.
    pub fn cancel_python_worker(&self, provider_name: &str) -> Result<bool, ProviderError> {
        let state = self
            .state
            .read()
            .expect("provider registry lock should not be poisoned");
        let provider = state.providers.get(provider_name).ok_or_else(|| {
            ProviderError::validation(
                "registry",
                "python_worker_cancel",
                "provider_not_loaded",
                format!("provider `{provider_name}` is not loaded"),
            )
        })?;
        if provider.runtime_status().is_none() {
            return Err(ProviderError::validation(
                provider_name,
                "python_worker_cancel",
                "python_worker_unavailable",
                "provider does not use a persistent Python worker",
            ));
        }
        Ok(provider.cancel_active())
    }

    /// Clears one persistent worker's crash-loop quarantine.
    pub async fn reset_python_worker_quarantine(
        &self,
        provider_name: &str,
    ) -> Result<(), ProviderError> {
        let provider = {
            let state = self
                .state
                .read()
                .expect("provider registry lock should not be poisoned");
            let provider = state.providers.get(provider_name).cloned().ok_or_else(|| {
                ProviderError::validation(
                    "registry",
                    "python_worker_reset",
                    "provider_not_loaded",
                    format!("provider `{provider_name}` is not loaded"),
                )
            })?;
            if provider.runtime_status().is_none() {
                return Err(ProviderError::validation(
                    provider_name,
                    "python_worker_reset",
                    "python_worker_unavailable",
                    "provider does not use a persistent Python worker",
                ));
            }
            provider
        };
        provider.reset_quarantine().await;
        Ok(())
    }
}

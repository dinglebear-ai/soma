use serde_json::Value;

use crate::{ProviderCall, ProviderError, ProviderOutput};

use super::{ProviderRegistry, RegisteredTool};

impl ProviderRegistry {
    pub async fn dispatch(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        self.dispatch_with(
            call,
            |provider, call| async move { provider.call(call).await },
        )
        .await
    }

    pub async fn dispatch_with<F, Fut>(
        &self,
        call: ProviderCall,
        invoke: F,
    ) -> Result<ProviderOutput, ProviderError>
    where
        F: FnOnce(std::sync::Arc<dyn crate::Provider>, ProviderCall) -> Fut,
        Fut: std::future::Future<Output = Result<ProviderOutput, ProviderError>>,
    {
        self.dispatch_with_pre_input(call, |_| Ok(()), invoke).await
    }

    /// Dispatches with a host check after action/surface resolution and before
    /// provider-core validates the declared input limit and schema.
    pub async fn dispatch_with_pre_input<P, F, Fut>(
        &self,
        mut call: ProviderCall,
        pre_input: P,
        invoke: F,
    ) -> Result<ProviderOutput, ProviderError>
    where
        P: FnOnce(&ProviderCall) -> Result<(), ProviderError>,
        F: FnOnce(std::sync::Arc<dyn crate::Provider>, ProviderCall) -> Fut,
        Fut: std::future::Future<Output = Result<ProviderOutput, ProviderError>>,
    {
        let entry = self
            .snapshot
            .tool(&call.action)
            .cloned()
            .ok_or_else(|| ProviderError::tool_not_found(&call.action))?;
        let provider = self
            .providers
            .get(entry.provider_id().as_str())
            .cloned()
            .ok_or_else(|| {
                ProviderError::new(
                    "provider_not_loaded",
                    entry.provider_id().to_string(),
                    Some(call.action.clone()),
                    "provider is not loaded in the active registry",
                    "Rebuild the provider registry and retry.",
                )
            })?;

        call.provider = entry.provider_id().to_string();
        call.snapshot_id = self.snapshot.fingerprint().to_string();
        validate_surface(&entry, &call)?;
        pre_input(&call)?;
        validate_input(&entry, &call)?;
        let output = invoke(provider, call).await?;
        validate_output(&entry, &output)?;
        Ok(output)
    }
}

fn validate_surface(entry: &RegisteredTool, call: &ProviderCall) -> Result<(), ProviderError> {
    if entry.spec().exposed_on(call.surface) {
        return Ok(());
    }
    Err(ProviderError::validation(
        entry.provider_id().to_string(),
        &call.action,
        "surface_not_exposed",
        format!(
            "action `{}` is not exposed on {}",
            call.action,
            call.surface.as_str()
        ),
    ))
}

fn validate_input(entry: &RegisteredTool, call: &ProviderCall) -> Result<(), ProviderError> {
    if let Some(max) = entry
        .spec()
        .limits
        .as_ref()
        .and_then(|limits| limits.max_input_bytes)
    {
        let len = serialized_len(&call.params);
        if len > max {
            return Err(ProviderError::validation(
                entry.provider_id().to_string(),
                &call.action,
                "input_too_large",
                format!("provider input exceeded {max} bytes"),
            ));
        }
    }
    let details = entry
        .input_validator
        .iter_errors(&call.params)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect::<Vec<_>>();
    if details.is_empty() {
        Ok(())
    } else {
        Err(ProviderError::validation(
            entry.provider_id().to_string(),
            &call.action,
            "input_schema_failed",
            details.join("; "),
        ))
    }
}

fn validate_output(entry: &RegisteredTool, output: &ProviderOutput) -> Result<(), ProviderError> {
    if let Some(max) = entry
        .spec()
        .limits
        .as_ref()
        .and_then(|limits| limits.max_response_bytes)
        && serialized_len(&output.value) > max
    {
        return Err(ProviderError::new(
            "response_too_large",
            entry.provider_id().to_string(),
            Some(entry.spec().name.clone()),
            format!("provider response exceeded {max} bytes"),
            "Reduce the response size or add paging.",
        ));
    }
    let Some(validator) = &entry.output_validator else {
        return Ok(());
    };
    let details = validator
        .iter_errors(&output.value)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect::<Vec<_>>();
    if details.is_empty() {
        Ok(())
    } else {
        Err(ProviderError::new(
            "output_schema_failed",
            entry.provider_id().to_string(),
            Some(entry.spec().name.clone()),
            details.join("; "),
            "Fix the provider output or its declared output schema, then retry.",
        )
        .with_phase("output_validation"))
    }
}

fn serialized_len(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

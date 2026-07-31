use super::*;

/// A compiled component retained independently of the global deduplication cache.
#[derive(Clone)]
pub struct PreparedComponentArtifact {
    runtime: Arc<WasmRuntime>,
    artifact: Arc<WasmArtifact>,
}

/// Validate and invoke a component artifact directly without network access.
pub fn invoke_authorized_component_artifact(
    path: &std::path::Path,
    input: &Value,
    capabilities: &soma_provider_core::HostCapabilities,
    context: &soma_provider_core::ProviderInvocationContext,
) -> Result<Value, String> {
    if capabilities
        .network
        .as_ref()
        .is_some_and(|network| network.enabled)
    {
        return Err(
            "synchronous component conformance does not permit network host calls".to_owned(),
        );
    }
    let limits = conformance_limits();
    let bytes = conformance_input(input, limits)?;
    let runtime = shared_wasm_runtime()?;
    let output = runtime.run(
        path,
        WasmInvocation {
            input: &bytes,
            limits,
            capabilities,
            context,
            resolved_hosts: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(5),
        },
    )?;
    serde_json::from_slice(&output).map_err(|error| error.to_string())
}

/// Invoke a component artifact under the caller's real authority and a single
/// absolute deadline.
pub async fn invoke_component_artifact_async(
    path: &std::path::Path,
    input: &Value,
    capabilities: &soma_provider_core::HostCapabilities,
    context: &soma_provider_core::ProviderInvocationContext,
) -> Result<Value, String> {
    invoke_component_artifact_before_async(
        path,
        input,
        capabilities,
        context,
        Instant::now() + Duration::from_secs(5),
    )
    .await
}

/// Invoke a prepared component using the caller's absolute operation deadline.
pub async fn invoke_component_artifact_before_async(
    path: &std::path::Path,
    input: &Value,
    capabilities: &soma_provider_core::HostCapabilities,
    context: &soma_provider_core::ProviderInvocationContext,
    deadline: Instant,
) -> Result<Value, String> {
    let prepared = prepare_component_artifact_before(path, deadline)?;
    invoke_prepared_component_artifact_before_async(
        &prepared,
        input,
        capabilities,
        context,
        deadline,
    )
    .await
}

/// Compile and retain a component artifact through a later invocation.
pub fn prepare_component_artifact_before(
    path: &std::path::Path,
    deadline: Instant,
) -> Result<PreparedComponentArtifact, String> {
    let bytes = read_artifact(path)?;
    let runtime = shared_wasm_runtime()?;
    let artifact = runtime.artifact(&bytes, deadline)?;
    if !matches!(artifact.as_ref(), WasmArtifact::Component(_)) {
        return Err("artifact is core Wasm, not a component".to_owned());
    }
    Ok(PreparedComponentArtifact { runtime, artifact })
}

/// Invoke an already-compiled component under one absolute operation deadline.
pub async fn invoke_prepared_component_artifact_before_async(
    prepared: &PreparedComponentArtifact,
    input: &Value,
    capabilities: &soma_provider_core::HostCapabilities,
    context: &soma_provider_core::ProviderInvocationContext,
    deadline: Instant,
) -> Result<Value, String> {
    let limits = conformance_limits();
    let bytes = conformance_input(input, limits)?;
    let resolved_hosts = resolve_component_hosts(capabilities, deadline).await?;
    let runtime = prepared.runtime.clone();
    let artifact = prepared.artifact.clone();
    let capabilities = capabilities.clone();
    let context = context.clone();
    let task = tokio::task::spawn_blocking(move || {
        runtime.run_artifact(
            artifact,
            WasmInvocation {
                input: &bytes,
                limits,
                capabilities: &capabilities,
                context: &context,
                resolved_hosts,
                deadline,
            },
        )
    });
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .unwrap_or(Duration::ZERO);
    let task_result = timeout(remaining, task)
        .await
        .map_err(|_| "component conformance invocation timed out".to_owned())?
        .map_err(|error| format!("component conformance task failed: {error}"))?;
    if Instant::now() >= deadline {
        return Err("component conformance invocation timed out".to_owned());
    }
    let output = task_result?;
    serde_json::from_slice(&output).map_err(|error| error.to_string())
}

/// Validate that `path` contains a component-model artifact.
pub fn verify_component_artifact(path: &std::path::Path) -> Result<(), String> {
    verify_component_artifact_before(path, Instant::now() + Duration::from_secs(5))
}

/// Validate a component-model artifact within an existing absolute deadline.
pub fn verify_component_artifact_before(
    path: &std::path::Path,
    deadline: Instant,
) -> Result<(), String> {
    let prepared = prepare_component_artifact_before(path, deadline)?;
    prepared
        .runtime
        .verify_prepared_component(&prepared.artifact, deadline)
}

fn conformance_limits() -> WasmRuntimeLimits {
    WasmRuntimeLimits {
        timeout_ms: 5_000,
        max_input_bytes: 64 * 1024,
        max_output_bytes: 256 * 1024,
        fuel: 1_000_000,
        max_memory_bytes: 64 * 1024 * 1024,
        max_table_elements: 10_000,
        max_instances: 16,
    }
}

fn conformance_input(input: &Value, limits: WasmRuntimeLimits) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(input).map_err(|error| error.to_string())?;
    if bytes.len() > limits.max_input_bytes {
        return Err(format!(
            "component conformance input exceeds {} bytes",
            limits.max_input_bytes
        ));
    }
    Ok(bytes)
}

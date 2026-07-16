use crate::error::OpenApiError;
use crate::registry::OpenApiRegistry;

pub async fn dispatch_openapi_call(
    registry: &OpenApiRegistry,
    client: &reqwest::Client,
    label: &str,
    operation_id: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, OpenApiError> {
    let handle = registry.operation(label, operation_id)?;
    let host = handle.base_url.host_str().unwrap_or_default().to_string();
    let method = handle.method.clone();
    let started = std::time::Instant::now();
    let result = crate::http::execute_operation(client, handle, params).await;
    log_dispatch(label, operation_id, &host, &method, started, &result);
    result
}

#[cfg(test)]
pub(crate) async fn dispatch_openapi_call_no_ssrf(
    registry: &OpenApiRegistry,
    client: &reqwest::Client,
    label: &str,
    operation_id: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, OpenApiError> {
    let handle = registry.operation(label, operation_id)?;
    let host = handle.base_url.host_str().unwrap_or_default().to_string();
    let method = handle.method.clone();
    let started = std::time::Instant::now();
    let result = crate::http::execute_operation_no_ssrf(client, handle, params).await;
    log_dispatch(label, operation_id, &host, &method, started, &result);
    result
}

fn log_dispatch(
    label: &str,
    operation_id: &str,
    host: &str,
    method: &reqwest::Method,
    started: std::time::Instant,
    result: &Result<serde_json::Value, OpenApiError>,
) {
    let elapsed_ms = started.elapsed().as_millis();
    match result {
        Ok(_) => tracing::info!(
            service = "openapi",
            action = operation_id,
            label = %label,
            host = %host,
            method = %method,
            status = "ok",
            elapsed_ms = elapsed_ms as u64,
            "openapi dispatch complete"
        ),
        Err(error) => tracing::warn!(
            service = "openapi",
            action = operation_id,
            label = %label,
            host = %host,
            method = %method,
            status = "error",
            kind = error.kind(),
            elapsed_ms = elapsed_ms as u64,
            "openapi dispatch failed"
        ),
    }
}

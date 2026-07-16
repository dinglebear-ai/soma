use serde_json::Value;

use crate::ToolError;

pub async fn dispatch_openapi_call_outside_local_lock(
    registry: &soma_openapi::OpenApiRegistry,
    client: &reqwest::Client,
    method: &str,
    params: Value,
) -> Result<Value, ToolError> {
    crate::openapi_feature::dispatch_openapi_provider(registry, client, method, params).await
}

#[must_use]
pub fn is_openapi_provider_call(id: &str) -> bool {
    id.starts_with("openapi::")
}

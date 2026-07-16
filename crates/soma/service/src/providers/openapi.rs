use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use soma_contracts::providers::{ProviderCatalog, ProviderTool};
use url::Url;

use crate::{
    provider_errors::ProviderError,
    provider_registry::{Provider, ProviderCall, ProviderOutput},
};

#[derive(Clone)]
pub struct OpenApiProvider {
    catalog: ProviderCatalog,
}

impl OpenApiProvider {
    pub fn curated(catalog: ProviderCatalog) -> Self {
        Self { catalog }
    }

    pub fn arc(catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::curated(catalog))
    }
}

#[async_trait]
impl Provider for OpenApiProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let tool = self.tool(&call)?;
        let operation = OpenApiOperation::from_catalog(&self.catalog, tool, &call)?;
        let client = reqwest::Client::new();
        let query_params;
        let request = match operation.method.as_str() {
            "GET" => {
                query_params = object_pairs(&call.params)?;
                client.get(operation.url).query(&query_params)
            }
            "DELETE" => {
                query_params = object_pairs(&call.params)?;
                client.delete(operation.url).query(&query_params)
            }
            "POST" => client.post(operation.url).json(&call.params),
            "PUT" => client.put(operation.url).json(&call.params),
            "PATCH" => client.patch(operation.url).json(&call.params),
            method => {
                return Err(ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unsupported_openapi_method",
                    format!("unsupported OpenAPI provider method `{method}`"),
                ));
            }
        };
        let response = request.send().await.map_err(|error| {
            ProviderError::execution(&self.catalog.provider.name, call.action.clone(), error)
        })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| {
            ProviderError::execution(&self.catalog.provider.name, call.action.clone(), error)
        })?;
        let parsed =
            serde_json::from_str::<Value>(&body).unwrap_or_else(|_| json!({ "text": body }));
        if !status.is_success() {
            return Err(ProviderError::new(
                "openapi_upstream_error",
                &self.catalog.provider.name,
                Some(call.action),
                format!("OpenAPI upstream returned HTTP {}", status.as_u16()),
                "Check the provider endpoint, input, and credentials, then retry.",
            ));
        }
        Ok(ProviderOutput::json(parsed))
    }
}

impl OpenApiProvider {
    fn tool(&self, call: &ProviderCall) -> Result<&ProviderTool, ProviderError> {
        self.catalog
            .tools
            .iter()
            .find(|tool| tool.name == call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unknown_openapi_action",
                    format!("OpenAPI provider has no action `{}`", call.action),
                )
            })
    }
}

struct OpenApiOperation {
    method: String,
    url: Url,
}

impl OpenApiOperation {
    fn from_catalog(
        catalog: &ProviderCatalog,
        tool: &ProviderTool,
        call: &ProviderCall,
    ) -> Result<Self, ProviderError> {
        let base_url = catalog
            .meta
            .get("openapi")
            .and_then(|value| value.get("base_url"))
            .and_then(Value::as_str)
            .or_else(|| catalog.meta.get("base_url").and_then(Value::as_str))
            .ok_or_else(|| {
                ProviderError::validation(
                    &catalog.provider.name,
                    &call.action,
                    "missing_openapi_base_url",
                    "OpenAPI provider requires provider.meta.openapi.base_url",
                )
            })?;
        let base = Url::parse(base_url).map_err(|error| {
            ProviderError::validation(
                &catalog.provider.name,
                &call.action,
                "invalid_openapi_base_url",
                error.to_string(),
            )
        })?;
        validate_base_url(catalog, &call.action, &base)?;

        let operation_meta = tool.meta.get("openapi");
        let path = operation_meta
            .and_then(|value| value.get("path"))
            .and_then(Value::as_str)
            .or_else(|| tool.rest.as_ref().and_then(|rest| rest.path.as_deref()))
            .unwrap_or_else(|| {
                tool.rest
                    .as_ref()
                    .and_then(|rest| rest.path.as_deref())
                    .unwrap_or("")
            });
        let method = operation_meta
            .and_then(|value| value.get("method"))
            .and_then(Value::as_str)
            .or_else(|| tool.rest.as_ref().and_then(|rest| rest.method.as_deref()))
            .unwrap_or("POST")
            .to_ascii_uppercase();
        let url = join_pinned_path(catalog, &call.action, &base, path)?;
        Ok(Self { method, url })
    }
}

fn validate_base_url(
    catalog: &ProviderCatalog,
    action: &str,
    url: &Url,
) -> Result<(), ProviderError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ProviderError::validation(
            &catalog.provider.name,
            action,
            "openapi_scheme_denied",
            "OpenAPI provider base_url must use http or https",
        ));
    }
    if url.host_str().is_none() {
        return Err(ProviderError::validation(
            &catalog.provider.name,
            action,
            "openapi_host_required",
            "OpenAPI provider base_url must include a host",
        ));
    }
    if let Some(network) = &catalog.capabilities.network {
        if network.enabled {
            let host = url.host_str().unwrap_or_default();
            if !network.allowed_hosts.iter().any(|allowed| allowed == host) {
                return Err(ProviderError::validation(
                    &catalog.provider.name,
                    action,
                    "openapi_host_not_allowed",
                    format!("OpenAPI provider host `{host}` is not declared in allowed_hosts"),
                ));
            }
        }
    }
    Ok(())
}

fn join_pinned_path(
    catalog: &ProviderCatalog,
    action: &str,
    base: &Url,
    path: &str,
) -> Result<Url, ProviderError> {
    if path.starts_with("http://") || path.starts_with("https://") || path.starts_with("//") {
        return Err(ProviderError::validation(
            &catalog.provider.name,
            action,
            "openapi_absolute_operation_url_denied",
            "OpenAPI provider operation paths must be relative to the pinned base_url",
        ));
    }
    let mut url = base.clone();
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn object_pairs(value: &Value) -> Result<Vec<(&str, &Value)>, ProviderError> {
    let Value::Object(map) = value else {
        return Err(ProviderError::validation(
            "openapi",
            "",
            "openapi_params_must_be_object",
            "OpenAPI provider params must be a JSON object",
        ));
    };
    Ok(map
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect())
}

use serde_json::Value;

use crate::git::provider::GitProvider;
use crate::state::provider::StateProvider;
use crate::types::split_namespaced_id;
use crate::ToolError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalProviderName {
    State,
    Git,
    #[cfg(feature = "openapi")]
    Openapi,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalProviderCall {
    pub provider: LocalProviderName,
    pub method: String,
    pub params: Value,
}

pub fn is_reserved_provider_namespace(namespace: &str) -> bool {
    matches!(namespace, "state" | "git") || {
        #[cfg(feature = "openapi")]
        {
            namespace == "openapi"
        }
        #[cfg(not(feature = "openapi"))]
        {
            let _ = namespace;
            false
        }
    }
}

pub fn parse_local_provider_call(
    id: &str,
    params: Value,
) -> Result<Option<LocalProviderCall>, ToolError> {
    let Some((namespace, method)) = split_namespaced_id(id.trim()) else {
        return Ok(None);
    };
    let provider = match namespace {
        "state" => LocalProviderName::State,
        "git" => LocalProviderName::Git,
        #[cfg(feature = "openapi")]
        "openapi" => LocalProviderName::Openapi,
        _ => return Ok(None),
    };
    if method.trim().is_empty() {
        return Err(ToolError::InvalidParam {
            message: "local provider method must not be empty".to_string(),
            param: "id".to_string(),
        });
    }
    Ok(Some(LocalProviderCall {
        provider,
        method: method.to_string(),
        params,
    }))
}

pub async fn dispatch_local_provider(call: LocalProviderCall) -> Result<Value, ToolError> {
    match call.provider {
        LocalProviderName::State => {
            StateProvider::default()
                .dispatch(&call.method, call.params)
                .await
        }
        LocalProviderName::Git => {
            GitProvider::new(std::env::current_dir().map_err(|err| {
                ToolError::internal_message(format!(
                    "failed to resolve cwd for git provider: {err}"
                ))
            })?)
            .dispatch(&call.method, call.params)
            .await
        }
        #[cfg(feature = "openapi")]
        LocalProviderName::Openapi => Err(ToolError::internal_message(
            "openapi provider requires an OpenAPI registry and must be dispatched through openapi_feature",
        )),
    }
}

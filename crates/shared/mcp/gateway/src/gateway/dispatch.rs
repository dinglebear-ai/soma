use serde_json::{json, Value};
use thiserror::Error;

use crate::dispatch_helpers::{structured_error, GatewayStructuredError};
use crate::gateway::catalog::{GatewayAction, GatewayActionCatalog};
use crate::gateway::manager::GatewayManager;
#[cfg(feature = "oauth")]
use crate::gateway::params::string_param;
use crate::gateway::params::{
    object_params, required_string_param, test_upstream_config_from_params,
    upstream_config_from_params, ParamsError,
};
use crate::process::guard::SpawnGuard;
use crate::process::stdio::StdioProcessSpec;
use crate::upstream::pool::UpstreamPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayAccess {
    pub read: bool,
    pub admin: bool,
}

#[derive(Debug, Error)]
pub enum GatewayDispatchError {
    #[error("gateway admin access required")]
    AdminRequired,
    #[error(transparent)]
    Params(#[from] ParamsError),
    #[error("unknown gateway action")]
    UnknownAction,
    #[error("spawn validation failed")]
    SpawnValidation,
    #[error(transparent)]
    Manager(#[from] crate::gateway::manager::GatewayManagerError),
}

impl GatewayDispatchError {
    #[must_use]
    pub fn structured(&self, action: &str) -> GatewayStructuredError {
        match self {
            Self::AdminRequired => structured_error(
                action,
                "admin_required",
                "authorization",
                "use a principal with gateway admin access",
            ),
            Self::Params(_) => structured_error(
                action,
                "invalid_param",
                "validation",
                "pass an object with valid gateway action parameters",
            ),
            Self::UnknownAction => structured_error(
                action,
                "unknown_action",
                "validation",
                "use one of the advertised gateway actions",
            ),
            Self::SpawnValidation => structured_error(
                action,
                "spawn_validation_failed",
                "validation",
                "use an allowed command and safe environment",
            ),
            Self::Manager(error) => manager_error_shape(action, error),
        }
    }
}

pub async fn dispatch_gateway_action(
    manager: &GatewayManager,
    access: GatewayAccess,
    action_name: &str,
    params: Value,
) -> Result<Value, GatewayDispatchError> {
    let catalog = GatewayActionCatalog::standard();
    let action = catalog
        .get(action_name)
        .ok_or(GatewayDispatchError::UnknownAction)?;
    enforce_access(action, access)?;
    if action.spawn_validation_required {
        validate_spawn_params(&params)?;
    }
    match action_name {
        "gateway.list" => Ok(crate::gateway::view_models::gateway_list_view(manager).await?),
        "gateway.config.view" => Ok(crate::gateway::view_models::gateway_config_view(manager)),
        "gateway.add" => {
            let view = manager.add_upstream(upstream_config_from_params(&params)?)?;
            Ok(json!({"added": true, "config": view}))
        }
        "gateway.update" => {
            let view = manager.update_upstream(upstream_config_from_params(&params)?)?;
            Ok(json!({"updated": true, "config": view}))
        }
        "gateway.remove" => {
            let params = object_params(&params)?;
            let name = required_string_param(params, "name")?;
            let view = manager.remove_upstream(&name)?;
            Ok(json!({"removed": name, "config": view}))
        }
        "gateway.reload" => {
            let view = manager.reload_from_store()?;
            Ok(json!({"reloaded": true, "config": view}))
        }
        "gateway.test" => test_upstream_connectivity(&params).await,
        "gateway.import.approve" => {
            let view = manager.add_upstream(upstream_config_from_params(&params)?)?;
            Ok(json!({"approved": true, "config": view}))
        }
        "gateway.oauth.start" => oauth_start(manager, &params).await,
        "gateway.oauth.status" => oauth_status(manager, &params).await,
        "gateway.oauth.clear" => oauth_clear(manager, &params).await,
        _ => Err(GatewayDispatchError::UnknownAction),
    }
}

async fn oauth_start(
    manager: &GatewayManager,
    params: &Value,
) -> Result<Value, GatewayDispatchError> {
    #[cfg(feature = "oauth")]
    {
        let (upstream, subject) = oauth_params(params)?;
        return Ok(serde_json::to_value(
            manager
                .begin_upstream_authorization(&upstream, &subject)
                .await?,
        )
        .expect("begin authorization serializes"));
    }
    #[cfg(not(feature = "oauth"))]
    {
        let _ = (manager, params);
        Err(GatewayDispatchError::Manager(
            crate::gateway::manager::GatewayManagerError::OAuth(
                "gateway.oauth.start requires the oauth feature".to_owned(),
            ),
        ))
    }
}

async fn oauth_status(
    manager: &GatewayManager,
    params: &Value,
) -> Result<Value, GatewayDispatchError> {
    #[cfg(feature = "oauth")]
    {
        let (upstream, subject) = oauth_params(params)?;
        return Ok(
            serde_json::to_value(manager.upstream_oauth_status(&upstream, &subject).await?)
                .expect("oauth status serializes"),
        );
    }
    #[cfg(not(feature = "oauth"))]
    {
        let _ = (manager, params);
        Err(GatewayDispatchError::Manager(
            crate::gateway::manager::GatewayManagerError::OAuth(
                "gateway.oauth.status requires the oauth feature".to_owned(),
            ),
        ))
    }
}

async fn oauth_clear(
    manager: &GatewayManager,
    params: &Value,
) -> Result<Value, GatewayDispatchError> {
    #[cfg(feature = "oauth")]
    {
        let (upstream, subject) = oauth_params(params)?;
        manager
            .clear_upstream_credentials(&upstream, &subject)
            .await?;
        Ok(json!({"ok": true}))
    }
    #[cfg(not(feature = "oauth"))]
    {
        let _ = (manager, params);
        Err(GatewayDispatchError::Manager(
            crate::gateway::manager::GatewayManagerError::OAuth(
                "gateway.oauth.clear requires the oauth feature".to_owned(),
            ),
        ))
    }
}

#[cfg(feature = "oauth")]
fn oauth_params(params: &Value) -> Result<(String, String), GatewayDispatchError> {
    let params = object_params(params)?;
    let upstream = required_string_param(params, "upstream")?;
    let subject = string_param(params, "subject")?.unwrap_or_else(|| "gateway".to_owned());
    Ok((upstream, subject))
}

async fn test_upstream_connectivity(params: &Value) -> Result<Value, GatewayDispatchError> {
    let config = test_upstream_config_from_params(params)?;
    let pool = UpstreamPool::default();
    pool.register_config(config.clone()).map_err(|error| {
        GatewayDispatchError::Manager(crate::gateway::manager::GatewayManagerError::Upstream(
            error,
        ))
    })?;
    let snapshot = pool
        .discover_upstream(&config.name)
        .await
        .map_err(|error| {
            GatewayDispatchError::Manager(crate::gateway::manager::GatewayManagerError::Upstream(
                error,
            ))
        })?;
    if !snapshot.health.is_routable() {
        return Err(GatewayDispatchError::Manager(
            crate::gateway::manager::GatewayManagerError::Upstream(
                crate::upstream::UpstreamError::NotRoutable {
                    upstream: snapshot.name,
                    reason: format!("{:?}", snapshot.health),
                },
            ),
        ));
    }
    Ok(json!({
        "ok": true,
        "upstream": snapshot.name,
        "transport": snapshot.transport,
        "tool_count": snapshot.tools.len(),
        "resource_count": snapshot.resources.len(),
        "prompt_count": snapshot.prompts.len(),
    }))
}

fn enforce_access(
    action: GatewayAction,
    access: GatewayAccess,
) -> Result<(), GatewayDispatchError> {
    if action.admin_required && !access.admin {
        return Err(GatewayDispatchError::AdminRequired);
    }
    if !(action.admin_required || access.read || access.admin) {
        return Err(GatewayDispatchError::AdminRequired);
    }
    Ok(())
}

fn validate_spawn_params(params: &Value) -> Result<(), GatewayDispatchError> {
    let config = test_upstream_config_from_params(params)?;
    if let Some(command) = config.command {
        let spec = StdioProcessSpec {
            command,
            args: config.args,
            env: config.env,
        };
        spec.validate(&SpawnGuard::default())
            .map_err(|_| GatewayDispatchError::SpawnValidation)?;
    }
    Ok(())
}

fn manager_error_shape(
    action: &str,
    error: &crate::gateway::manager::GatewayManagerError,
) -> GatewayStructuredError {
    use crate::gateway::manager::GatewayManagerError;
    use crate::upstream::UpstreamError;

    match error {
        GatewayManagerError::GatewayReloading => structured_error(
            action,
            "gateway_reloading",
            "runtime",
            "retry after the gateway reload completes",
        ),
        GatewayManagerError::StoreNotMounted => structured_error(
            action,
            "store_not_mounted",
            "configuration",
            "start the host application with a mounted gateway config store",
        ),
        GatewayManagerError::UpstreamExists(_) => structured_error(
            action,
            "upstream_exists",
            "validation",
            "use gateway.update or remove the existing upstream first",
        ),
        GatewayManagerError::UpstreamMissing(_) => structured_error(
            action,
            "upstream_missing",
            "validation",
            "configure the upstream before operating on it",
        ),
        GatewayManagerError::Config(_) => structured_error(
            action,
            "invalid_config",
            "validation",
            "fix the gateway configuration and retry",
        ),
        GatewayManagerError::OAuth(_) => structured_error(
            action,
            "oauth_runtime_error",
            "runtime",
            "configure upstream OAuth resources and retry",
        ),
        GatewayManagerError::Upstream(UpstreamError::UnknownUpstream { .. }) => structured_error(
            action,
            "unknown_upstream",
            "validation",
            "configure the upstream before calling it",
        ),
        GatewayManagerError::Upstream(UpstreamError::NotExposed { .. }) => structured_error(
            action,
            "not_exposed",
            "authorization",
            "choose a tool/resource/prompt exposed by the upstream policy",
        ),
        GatewayManagerError::Upstream(UpstreamError::NotRoutable { .. }) => structured_error(
            action,
            "not_routable",
            "runtime",
            "wait for the upstream to connect or fix its configuration",
        ),
        GatewayManagerError::Upstream(UpstreamError::Unsupported { .. }) => structured_error(
            action,
            "unsupported_transport",
            "unsupported",
            "this gateway build cannot route that upstream transport yet",
        ),
        GatewayManagerError::Upstream(UpstreamError::LiveConnect { .. }) => structured_error(
            action,
            "upstream_connect_failed",
            "runtime",
            "fix the upstream configuration or wait for the server to become reachable",
        ),
        GatewayManagerError::Upstream(UpstreamError::LiveCall { .. }) => structured_error(
            action,
            "upstream_call_failed",
            "runtime",
            "retry after verifying the upstream server and requested capability",
        ),
        GatewayManagerError::Upstream(UpstreamError::ResponseTooLarge { .. }) => structured_error(
            action,
            "response_too_large",
            "limits",
            "narrow the request or lower the upstream response size",
        ),
        GatewayManagerError::Upstream(UpstreamError::ParamsMustBeObject) => structured_error(
            action,
            "invalid_param",
            "validation",
            "pass tool params as a JSON object",
        ),
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;

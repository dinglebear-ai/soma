use serde_json::{json, Value};
use thiserror::Error;

use crate::dispatch_helpers::{structured_error, GatewayStructuredError};
use crate::gateway::catalog::{GatewayAction, GatewayActionCatalog};
use crate::gateway::manager::GatewayManager;
use crate::gateway::params::{object_params, string_param, ParamsError};
use crate::process::guard::SpawnGuard;
use crate::process::stdio::StdioProcessSpec;

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
            Self::SpawnValidation => structured_error(
                action,
                "spawn_validation_failed",
                "validation",
                "use an allowed command and safe environment",
            ),
            Self::Manager(_) => structured_error(
                action,
                "gateway_runtime_error",
                "runtime",
                "retry after checking gateway state",
            ),
        }
    }
}

pub fn dispatch_gateway_action(
    manager: &GatewayManager,
    access: GatewayAccess,
    action_name: &str,
    params: Value,
) -> Result<Value, GatewayDispatchError> {
    let catalog = GatewayActionCatalog::standard();
    let action = catalog.get(action_name);
    enforce_access(action, access)?;
    if action.spawn_validation_required {
        validate_spawn_params(&params)?;
    }
    match action_name {
        "gateway.list" => Ok(crate::gateway::view_models::gateway_list_view(manager)?),
        "gateway.config.view" => Ok(crate::gateway::view_models::gateway_config_view(manager)),
        "gateway.test" => Ok(json!({"ok": true, "validated_spawn": true})),
        _ => Ok(json!({"accepted": action_name})),
    }
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
    let params = object_params(params)?;
    let command = string_param(params, "command")?;
    if let Some(command) = command {
        let spec = StdioProcessSpec {
            command,
            args: Vec::new(),
            env: Default::default(),
        };
        spec.validate(&SpawnGuard::default())
            .map_err(|_| GatewayDispatchError::SpawnValidation)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;

use async_trait::async_trait;
use serde_json::Value;
use soma_application::{
    ExecutionContext, GatewayExecuteRequest, GatewayPort, GatewayReloadRequest, PortError,
};
use soma_contracts::{
    actions::{scopes_satisfy, READ_SCOPE},
    scopes::has_admin_scope,
};
use soma_domain::AuthorizationMode;
use soma_gateway::gateway::dispatch::{
    dispatch_gateway_action, GatewayAccess, GatewayDispatchError,
};
use soma_runtime::server::GatewayProductState;

pub(crate) struct GatewayApplicationPort {
    gateway: GatewayProductState,
}

impl GatewayApplicationPort {
    pub(crate) fn new(gateway: GatewayProductState) -> Self {
        Self { gateway }
    }

    async fn dispatch(
        &self,
        action: &str,
        params: Value,
        context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        dispatch_gateway_action(&self.gateway, gateway_access(context), action, params)
            .await
            .map_err(|error| gateway_port_error(action, error))
    }
}

#[async_trait]
impl GatewayPort for GatewayApplicationPort {
    async fn status(&self, context: &ExecutionContext) -> Result<Value, PortError> {
        self.dispatch("gateway.list", serde_json::json!({}), context)
            .await
    }

    async fn reload(
        &self,
        request: GatewayReloadRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        self.dispatch("gateway.reload", request.config, context)
            .await
    }

    async fn execute(
        &self,
        request: GatewayExecuteRequest,
        context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        self.dispatch(&request.action, request.params, context)
            .await
    }
}

fn gateway_access(context: &ExecutionContext) -> GatewayAccess {
    if !matches!(context.authorization_mode, AuthorizationMode::Mounted) {
        return GatewayAccess {
            read: true,
            admin: true,
        };
    }
    let scopes = context
        .principal
        .as_ref()
        .map(|principal| principal.scopes.to_vec())
        .unwrap_or_default();
    let admin = has_admin_scope(&scopes);
    GatewayAccess {
        read: admin || scopes_satisfy(&scopes, READ_SCOPE),
        admin,
    }
}

fn gateway_port_error(action: &str, error: GatewayDispatchError) -> PortError {
    let structured = error.structured(action);
    PortError {
        code: structured.code.to_owned(),
        message: error.to_string(),
        retryable: matches!(structured.kind, "runtime"),
        remediation: structured.remediation.to_owned(),
    }
}

#[cfg(test)]
#[path = "application_ports_tests.rs"]
mod tests;

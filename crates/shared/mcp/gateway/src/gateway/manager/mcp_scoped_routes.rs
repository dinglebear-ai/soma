use serde_json::Value;

use crate::gateway::protected_routes::ProtectedRouteScope;

use super::{
    mcp_routes::{GatewayPromptRoute, GatewayResourceRoute, GatewayToolRoute},
    GatewayManager, GatewayManagerError,
};

impl GatewayManager {
    pub async fn tool_routes_for_subject_and_scope(
        &self,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<Vec<GatewayToolRoute>, GatewayManagerError> {
        Ok(self
            .tool_routes_for_subject(subject)
            .await?
            .into_iter()
            .filter(|route| route_allowed(scope, &route.upstream))
            .collect())
    }

    pub async fn call_mcp_tool_for_subject_and_scope(
        &self,
        name: &str,
        params: Value,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<Option<Value>, GatewayManagerError> {
        let allowed = self
            .tool_routes_for_subject_and_scope(subject, scope)
            .await?
            .into_iter()
            .any(|route| route.name == name);
        if !allowed {
            return Ok(None);
        }
        self.call_mcp_tool_for_subject(name, params, subject).await
    }

    pub async fn resource_routes_for_subject_and_scope(
        &self,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<Vec<GatewayResourceRoute>, GatewayManagerError> {
        Ok(self
            .resource_routes_for_subject(subject)
            .await?
            .into_iter()
            .filter(|route| route_allowed(scope, &route.upstream))
            .collect())
    }

    pub async fn read_mcp_resource_for_subject_and_scope(
        &self,
        uri: &str,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<Option<Value>, GatewayManagerError> {
        let allowed = self
            .resource_routes_for_subject_and_scope(subject, scope)
            .await?
            .into_iter()
            .any(|route| route.uri == uri);
        if !allowed {
            return Ok(None);
        }
        self.read_mcp_resource_for_subject(uri, subject).await
    }

    pub async fn prompt_routes_for_subject_and_scope(
        &self,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<Vec<GatewayPromptRoute>, GatewayManagerError> {
        Ok(self
            .prompt_routes_for_subject(subject)
            .await?
            .into_iter()
            .filter(|route| route_allowed(scope, &route.upstream))
            .collect())
    }

    pub async fn get_mcp_prompt_for_subject_and_scope(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<Option<Value>, GatewayManagerError> {
        let allowed = self
            .prompt_routes_for_subject_and_scope(subject, scope)
            .await?
            .into_iter()
            .any(|route| route.name == name);
        if !allowed {
            return Ok(None);
        }
        self.get_mcp_prompt_for_subject(name, arguments, subject)
            .await
    }
}

fn route_allowed(scope: Option<&ProtectedRouteScope>, upstream: &str) -> bool {
    match scope {
        None => true,
        Some(scope) => scope.upstreams.iter().any(|allowed| allowed == upstream),
    }
}

#[cfg(test)]
#[path = "mcp_scoped_routes_tests.rs"]
mod tests;

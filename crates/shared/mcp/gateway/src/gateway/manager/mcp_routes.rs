use serde_json::Value;
pub use soma_mcp_proxy::{
    parse_upstream_resource_uri, upstream_resource_uri, McpPromptRoute as GatewayPromptRoute,
    McpResourceRoute as GatewayResourceRoute, McpToolRoute as GatewayToolRoute,
};

use crate::upstream::{PromptDescriptor, ResourceDescriptor, ToolDescriptor, UpstreamHealth};

use super::{GatewayManager, GatewayManagerError};

impl GatewayManager {
    pub async fn tool_routes(&self) -> Result<Vec<GatewayToolRoute>, GatewayManagerError> {
        self.tool_routes_for_subject(None).await
    }

    pub async fn tool_routes_for_subject(
        &self,
        subject: Option<&str>,
    ) -> Result<Vec<GatewayToolRoute>, GatewayManagerError> {
        let pool = self.ready_pool()?;
        let snapshots = discover_snapshots(&pool, subject).await?;
        let mut candidates = Vec::new();
        for snapshot in snapshots {
            if !matches!(snapshot.health, UpstreamHealth::Connected) {
                continue;
            }
            for descriptor in exposed_tools(&pool, &snapshot.name, subject).await? {
                candidates.push((snapshot.name.clone(), descriptor));
            }
        }
        Ok(soma_mcp_proxy::tool_routes_from_candidates(
            candidates,
            std::iter::empty::<&str>(),
        ))
    }

    pub async fn call_mcp_tool(
        &self,
        name: &str,
        params: Value,
    ) -> Result<Option<Value>, GatewayManagerError> {
        self.call_mcp_tool_for_subject(name, params, None).await
    }

    pub async fn call_mcp_tool_for_subject(
        &self,
        name: &str,
        params: Value,
        subject: Option<&str>,
    ) -> Result<Option<Value>, GatewayManagerError> {
        let Some(route) = self
            .tool_routes_for_subject(subject)
            .await?
            .into_iter()
            .find(|route| route.name == name)
        else {
            return Ok(None);
        };
        let pool = self.ready_pool()?;
        call_tool(&pool, route, params, subject)
            .await
            .map(Some)
            .map_err(Into::into)
    }

    pub async fn resource_routes(&self) -> Result<Vec<GatewayResourceRoute>, GatewayManagerError> {
        self.resource_routes_for_subject(None).await
    }

    pub async fn resource_routes_for_subject(
        &self,
        subject: Option<&str>,
    ) -> Result<Vec<GatewayResourceRoute>, GatewayManagerError> {
        let pool = self.ready_pool()?;
        let snapshots = discover_snapshots(&pool, subject).await?;
        let mut routes = Vec::new();
        for snapshot in snapshots {
            if !matches!(snapshot.health, UpstreamHealth::Connected) {
                continue;
            }
            for descriptor in list_resources(&pool, &snapshot.name, subject).await? {
                routes.push(soma_mcp_proxy::resource_route(&snapshot.name, descriptor));
            }
        }
        Ok(routes)
    }

    pub async fn read_mcp_resource(&self, uri: &str) -> Result<Option<Value>, GatewayManagerError> {
        self.read_mcp_resource_for_subject(uri, None).await
    }

    pub async fn read_mcp_resource_for_subject(
        &self,
        uri: &str,
        subject: Option<&str>,
    ) -> Result<Option<Value>, GatewayManagerError> {
        let Some((upstream, native_uri)) = parse_upstream_resource_uri(uri) else {
            return Ok(None);
        };
        let pool = self.ready_pool()?;
        read_resource(&pool, &upstream, &native_uri, subject)
            .await
            .map(Some)
            .map_err(Into::into)
    }

    pub async fn prompt_routes(&self) -> Result<Vec<GatewayPromptRoute>, GatewayManagerError> {
        self.prompt_routes_for_subject(None).await
    }

    pub async fn prompt_routes_for_subject(
        &self,
        subject: Option<&str>,
    ) -> Result<Vec<GatewayPromptRoute>, GatewayManagerError> {
        let pool = self.ready_pool()?;
        let snapshots = discover_snapshots(&pool, subject).await?;
        let mut candidates = Vec::new();
        for snapshot in snapshots {
            if !matches!(snapshot.health, UpstreamHealth::Connected) {
                continue;
            }
            for descriptor in list_prompts(&pool, &snapshot.name, subject).await? {
                candidates.push((snapshot.name.clone(), descriptor));
            }
        }
        Ok(soma_mcp_proxy::prompt_routes_from_candidates(candidates))
    }

    pub async fn get_mcp_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
    ) -> Result<Option<Value>, GatewayManagerError> {
        self.get_mcp_prompt_for_subject(name, arguments, None).await
    }

    pub async fn get_mcp_prompt_for_subject(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
        subject: Option<&str>,
    ) -> Result<Option<Value>, GatewayManagerError> {
        let Some(route) = self
            .prompt_routes_for_subject(subject)
            .await?
            .into_iter()
            .find(|route| route.name == name)
        else {
            return Ok(None);
        };
        let pool = self.ready_pool()?;
        get_prompt(
            &pool,
            &route.upstream,
            &route.native_name,
            arguments,
            subject,
        )
        .await
        .map(Some)
        .map_err(Into::into)
    }

    fn ready_pool(
        &self,
    ) -> Result<std::sync::Arc<crate::upstream::pool::UpstreamPool>, GatewayManagerError> {
        self.ensure_ready()?;
        Ok(self.pool.read().expect("gateway pool poisoned").clone())
    }
}

async fn discover_snapshots(
    pool: &crate::upstream::pool::UpstreamPool,
    subject: Option<&str>,
) -> Result<Vec<crate::upstream::UpstreamSnapshot>, crate::upstream::UpstreamError> {
    let _ = subject;
    #[cfg(feature = "oauth")]
    if subject.is_some() {
        return pool.discover_for_subject(subject).await;
    }
    pool.discover().await
}

async fn exposed_tools(
    pool: &crate::upstream::pool::UpstreamPool,
    upstream: &str,
    subject: Option<&str>,
) -> Result<Vec<ToolDescriptor>, crate::upstream::UpstreamError> {
    let _ = subject;
    #[cfg(feature = "oauth")]
    if subject.is_some() {
        return pool.exposed_tools_for_subject(upstream, subject).await;
    }
    pool.exposed_tools(upstream)
}

async fn call_tool(
    pool: &crate::upstream::pool::UpstreamPool,
    route: GatewayToolRoute,
    params: Value,
    subject: Option<&str>,
) -> Result<Value, crate::upstream::UpstreamError> {
    let _ = subject;
    let call = crate::upstream::pool::ToolCall {
        upstream: route.upstream,
        tool: route.native_name,
        params,
    };
    #[cfg(feature = "oauth")]
    if subject.is_some() {
        return pool.call_tool_for_subject(call, subject).await;
    }
    pool.call_tool(call).await
}

async fn list_resources(
    pool: &crate::upstream::pool::UpstreamPool,
    upstream: &str,
    subject: Option<&str>,
) -> Result<Vec<ResourceDescriptor>, crate::upstream::UpstreamError> {
    let _ = subject;
    #[cfg(feature = "oauth")]
    if subject.is_some() {
        return pool.list_resources_for_subject(upstream, subject).await;
    }
    pool.list_resources(upstream).await
}

async fn read_resource(
    pool: &crate::upstream::pool::UpstreamPool,
    upstream: &str,
    uri: &str,
    subject: Option<&str>,
) -> Result<Value, crate::upstream::UpstreamError> {
    let _ = subject;
    #[cfg(feature = "oauth")]
    if subject.is_some() {
        return pool.read_resource_for_subject(upstream, uri, subject).await;
    }
    pool.read_resource(upstream, uri).await
}

async fn list_prompts(
    pool: &crate::upstream::pool::UpstreamPool,
    upstream: &str,
    subject: Option<&str>,
) -> Result<Vec<PromptDescriptor>, crate::upstream::UpstreamError> {
    let _ = subject;
    #[cfg(feature = "oauth")]
    if subject.is_some() {
        return pool.list_prompts_for_subject(upstream, subject).await;
    }
    pool.list_prompts(upstream).await
}

async fn get_prompt(
    pool: &crate::upstream::pool::UpstreamPool,
    upstream: &str,
    name: &str,
    arguments: Option<serde_json::Map<String, Value>>,
    subject: Option<&str>,
) -> Result<Value, crate::upstream::UpstreamError> {
    let _ = subject;
    #[cfg(feature = "oauth")]
    if subject.is_some() {
        return pool
            .get_prompt_for_subject(upstream, name, arguments, subject)
            .await;
    }
    pool.get_prompt(upstream, name, arguments).await
}

#[cfg(test)]
#[path = "mcp_routes_tests.rs"]
mod tests;

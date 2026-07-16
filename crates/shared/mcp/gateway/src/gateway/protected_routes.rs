use serde_json::Value;
use thiserror::Error;

use crate::config::{ProtectedGatewaySubsetTarget, ProtectedMcpRouteConfig};
use crate::security::ssrf::{validate_url, OutboundPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedRouteProjection {
    pub name: String,
    pub enabled: bool,
    pub public_resource: String,
    pub upstream: Option<String>,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedRouteScope {
    pub upstreams: Vec<String>,
    pub services: Vec<String>,
    pub expose_code_mode: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtectedRouteError {
    #[error("host does not match protected route")]
    HostMismatch,
    #[error("path does not match protected route")]
    PathMismatch,
    #[error("backend URL is not allowed")]
    BackendDenied,
}

pub fn project_route(route: &ProtectedMcpRouteConfig, connected: bool) -> ProtectedRouteProjection {
    ProtectedRouteProjection {
        name: route.name.clone(),
        enabled: route.enabled,
        public_resource: route.public_resource(),
        upstream: route.upstream.clone(),
        connected: route.enabled && connected,
    }
}

pub fn route_matches(
    route: &ProtectedMcpRouteConfig,
    host_header: &str,
    request_path: &str,
) -> Result<(), ProtectedRouteError> {
    if !host_matches(&route.public_host, host_header) {
        return Err(ProtectedRouteError::HostMismatch);
    }
    if !path_matches(&route.public_path, request_path) {
        return Err(ProtectedRouteError::PathMismatch);
    }
    Ok(())
}

pub fn validate_backend_for_dispatch(
    route: &ProtectedMcpRouteConfig,
) -> Result<(), ProtectedRouteError> {
    if route.backend_url.trim().is_empty() {
        return Ok(());
    }
    validate_url(&route.backend_url, OutboundPolicy::AdminProtectedBackend)
        .map(|_| ())
        .map_err(|_| ProtectedRouteError::BackendDenied)
}

pub fn resolve_scope(
    route: &ProtectedMcpRouteConfig,
    public_request_params: &Value,
) -> ProtectedRouteScope {
    let _ignored_public_scope = public_request_params.get("scope");
    let target = route.target.as_ref().cloned().unwrap_or_default();
    scope_from_target(&target)
}

pub fn protected_route_error_body(error: &ProtectedRouteError) -> String {
    format!(r#"{{"error":"{error}"}}"#)
}

fn host_matches(configured: &str, header: &str) -> bool {
    !header.contains(',')
        && crate::config::protected_routes::normalize_public_host(configured)
            == crate::config::protected_routes::normalize_public_host(header)
}

fn path_matches(public_path: &str, request_path: &str) -> bool {
    if contains_encoded_slash_or_dot(request_path) {
        return false;
    }
    let public_path = public_path.trim_end_matches('/');
    request_path == public_path
        || request_path
            .strip_prefix(public_path)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn contains_encoded_slash_or_dot(path: &str) -> bool {
    let lowercase = path.to_ascii_lowercase();
    lowercase.contains("%2f") || lowercase.contains("%2e")
}

fn scope_from_target(target: &ProtectedGatewaySubsetTarget) -> ProtectedRouteScope {
    ProtectedRouteScope {
        upstreams: target.upstreams.clone(),
        services: target.services.clone(),
        expose_code_mode: target.expose_code_mode,
    }
}

#[cfg(test)]
#[path = "protected_routes_tests.rs"]
mod tests;

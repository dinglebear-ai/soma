use std::{convert::Infallible, str::FromStr};

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, Method, Request, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Response},
    Json, Router,
};
use serde_json::json;
use soma_gateway::{
    config::{protected_routes::normalize_public_host, ProtectedMcpRouteConfig},
    gateway::protected_routes::resolve_scope,
};
use soma_runtime::server::{AppState, AuthPolicy};
use tower::ServiceExt;

use crate::protected_routes_proxy::proxy_protected_mcp_route;

pub async fn protected_route_resource_metadata(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Response {
    let Some(host) = request_host(&request) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(route) = state
        .gateway
        .resolve_protected_route_metadata(&host, request.uri().path())
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    protected_route_metadata_response(&state, route)
}

pub async fn protected_mcp_intercept(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, Infallible> {
    if is_reserved_public_path(request.uri().path()) {
        return Ok(next.run(request).await);
    }
    let route = request_host(&request).and_then(|host| {
        state
            .gateway
            .resolve_protected_route(&host, request.uri().path())
    });
    let Some(route) = route else {
        return Ok(next.run(request).await);
    };
    if is_route_well_known_path(&route, request.uri().path())
        && !is_route_metadata_path(&route, request.uri().path())
    {
        return Ok(next.run(request).await);
    }
    Ok(protected_mcp_route_entry(state, request, route).await)
}

async fn protected_mcp_route_entry(
    state: AppState,
    mut request: Request<Body>,
    route: ProtectedMcpRouteConfig,
) -> Response {
    if *request.method() == Method::GET && is_route_metadata_path(&route, request.uri().path()) {
        return protected_route_metadata_response(&state, route);
    }
    if !matches!(
        *request.method(),
        Method::GET | Method::POST | Method::DELETE
    ) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if let Err(response) = authenticate_protected_route_request(&state, &mut request, &route) {
        return *response;
    }
    if route.target.is_some() {
        return dispatch_gateway_subset(state, request, route).await;
    }
    proxy_protected_mcp_route(&state, request, route).await
}

fn protected_route_metadata_response(state: &AppState, route: ProtectedMcpRouteConfig) -> Response {
    let Some(auth_state) = auth_state(state) else {
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "oauth_missing",
            "OAuth auth state is not configured",
        );
    };
    refresh_protected_route_resource_scopes(state, &auth_state);
    let Some(public_url) = auth_state.config.public_url.as_ref() else {
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "public_url_missing",
            "OAuth public URL is not configured",
        );
    };
    let mut response = Json(soma_auth::types::ProtectedResourceMetadata {
        resource: route.public_resource(),
        authorization_servers: vec![public_url.as_str().trim_end_matches('/').to_owned()],
        scopes_supported: route.scopes,
        bearer_methods_supported: vec!["header".to_owned()],
    })
    .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    response
}

fn authenticate_protected_route_request(
    state: &AppState,
    request: &mut Request<Body>,
    route: &ProtectedMcpRouteConfig,
) -> Result<(), Box<Response>> {
    let Some(auth_state) = auth_state(state) else {
        return Err(Box::new(auth_error(
            route,
            "OAuth auth state is not configured",
        )));
    };
    refresh_protected_route_resource_scopes(state, &auth_state);
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(soma_auth::parse_bearer_token)
        .ok_or_else(|| Box::new(auth_error(route, "missing bearer token")))?;
    let issuer = auth_state
        .config
        .public_url
        .as_ref()
        .map(|url| url.as_str().trim_end_matches('/').to_owned())
        .ok_or_else(|| Box::new(auth_error(route, "OAuth public URL is not configured")))?;
    let claims = auth_state
        .signing_keys
        .validate_access_token_with_issuer(&token, &route.public_resource(), &issuer)
        .map_err(|_| Box::new(auth_error(route, "invalid bearer token")))?;
    let granted = claims
        .scope
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let is_admin = granted
        .iter()
        .any(|scope| scope == soma_contracts::scopes::ADMIN_SCOPE);
    if !is_admin
        && !route
            .scopes
            .iter()
            .all(|required| granted.contains(required))
    {
        return Err(Box::new(json_error(
            StatusCode::FORBIDDEN,
            "insufficient_scope",
            "insufficient OAuth scope for protected MCP route",
        )));
    }
    request.extensions_mut().insert(soma_auth::AuthContext {
        sub: claims.sub,
        actor_key: None,
        scopes: granted,
        issuer: claims.iss,
        via_session: false,
        csrf_token: None,
        email: None,
    });
    Ok(())
}

async fn dispatch_gateway_subset(
    state: AppState,
    mut request: Request<Body>,
    route: ProtectedMcpRouteConfig,
) -> Response {
    request
        .extensions_mut()
        .insert(resolve_scope(&route, &serde_json::Value::Null));
    if let Err(response) = rewrite_to_internal_mcp_path(&mut request, &route.public_path) {
        return *response;
    }
    let config = soma_mcp::streamable_http_config(&state.config);
    let router = Router::new()
        .nest_service(
            "/mcp",
            soma_mcp::streamable_http_service(state.clone(), config),
        )
        .with_state(state);
    router.oneshot(request).await.unwrap_or_else(|error| {
        json_error(
            StatusCode::BAD_GATEWAY,
            "gateway_subset_failed",
            format!("protected MCP gateway subset failed: {error}"),
        )
    })
}

fn rewrite_to_internal_mcp_path(
    request: &mut Request<Body>,
    public_path: &str,
) -> Result<(), Box<Response>> {
    let suffix = request.uri().path().strip_prefix(public_path).unwrap_or("");
    let mut path = "/mcp".to_owned();
    if !suffix.is_empty() {
        path.push('/');
        path.push_str(suffix.trim_start_matches('/'));
    }
    if let Some(query) = request.uri().query() {
        path.push('?');
        path.push_str(query);
    }
    *request.uri_mut() = Uri::from_str(&path).map_err(|error| {
        Box::new(json_error(
            StatusCode::BAD_REQUEST,
            "invalid_proxy_path",
            format!("failed to rewrite protected MCP path: {error}"),
        ))
    })?;
    Ok(())
}

fn refresh_protected_route_resource_scopes(
    state: &AppState,
    auth_state: &soma_auth::state::AuthState,
) {
    auth_state.set_allowed_resource_scopes(
        state
            .gateway
            .protected_route_list()
            .into_iter()
            .filter(|route| route.enabled)
            .map(|route| (route.public_resource(), route.scopes)),
    );
}

fn auth_state(state: &AppState) -> Option<std::sync::Arc<soma_auth::state::AuthState>> {
    match &state.auth_policy {
        AuthPolicy::Mounted {
            auth_state: Some(auth_state),
        } => Some(auth_state.clone()),
        AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => None,
        AuthPolicy::Mounted { auth_state: None } => None,
    }
}

fn request_host(request: &Request<Body>) -> Option<String> {
    request
        .headers()
        .get("x-forwarded-host")
        .or_else(|| request.headers().get(header::HOST))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(ToOwned::to_owned)
}

fn is_reserved_public_path(path: &str) -> bool {
    path.starts_with("/.well-known/")
        || matches!(path, "/authorize" | "/token" | "/register" | "/jwks")
        || path.starts_with("/native/")
}

fn is_route_well_known_path(route: &ProtectedMcpRouteConfig, path: &str) -> bool {
    let prefix = format!("{}/.well-known/", route.public_path.trim_end_matches('/'));
    path.starts_with(&prefix)
}

fn is_route_metadata_path(route: &ProtectedMcpRouteConfig, path: &str) -> bool {
    path == format!(
        "{}/.well-known/oauth-protected-resource",
        route.public_path.trim_end_matches('/')
    )
}

fn route_metadata_url(route: &ProtectedMcpRouteConfig) -> String {
    format!(
        "https://{}/.well-known/oauth-protected-resource{}",
        normalize_public_host(&route.public_host),
        route.public_path.trim_end_matches('/')
    )
}

fn auth_error(route: &ProtectedMcpRouteConfig, message: &str) -> Response {
    let mut response = json_error(StatusCode::UNAUTHORIZED, "unauthorized", message);
    let header = format!(
        "Bearer resource_metadata=\"{}\", scope=\"{}\"",
        route_metadata_url(route),
        route.scopes.join(" ")
    );
    if let Ok(value) = HeaderValue::from_str(&header) {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

pub(super) fn json_error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({
            "error": code,
            "message": message.into(),
        })),
    )
        .into_response()
}

#[cfg(test)]
#[path = "protected_routes_tests.rs"]
mod tests;

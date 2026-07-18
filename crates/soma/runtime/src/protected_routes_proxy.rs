//! Inbound-to-upstream HTTP forwarding for protected MCP routes: resolves
//! the backend target (static URL, named upstream, or gateway subset),
//! attaches upstream auth, and streams the request/response through
//! (`protected-routes` feature).
//!
//! Moved here from `crates/soma/integrations` as a PR 19 review fix,
//! alongside its sibling `protected_routes.rs` — see that module's doc
//! comment for the full rationale (soma-integrations must not depend on
//! soma-runtime/soma-mcp per plan section 3.20's target dependency shape).

use std::time::Instant;

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderName, Request, StatusCode},
    response::Response,
};
use soma_gateway::{
    config::{ProtectedMcpRouteConfig, UpstreamConfig},
    gateway::protected_routes::validate_backend_for_dispatch,
};

use crate::server::AppState;

const PROTECTED_PROXY_BODY_LIMIT: usize = 50 * 1024 * 1024;
#[cfg(feature = "oauth")]
const SHARED_GATEWAY_OAUTH_SUBJECT: &str = "gateway";

pub(crate) async fn proxy_protected_mcp_route(
    state: &AppState,
    request: Request<Body>,
    route: ProtectedMcpRouteConfig,
) -> Response {
    let started = Instant::now();
    if let Err(error) = validate_backend_for_dispatch(&route) {
        return crate::protected_routes::json_error(
            StatusCode::BAD_GATEWAY,
            "backend_denied",
            error.to_string(),
        );
    }
    let suffix = request
        .uri()
        .path()
        .strip_prefix(&route.public_path)
        .unwrap_or("");
    let (mut upstream, upstream_auth, target) =
        match protected_route_upstream_target(state, &route).await {
            Ok(target) => target,
            Err(response) => return response,
        };
    append_proxy_suffix(&mut upstream, suffix, request.uri().query());
    let method = request.method().clone();
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), PROTECTED_PROXY_BODY_LIMIT).await {
        Ok(body) => body,
        Err(error) => {
            return crate::protected_routes::json_error(
                StatusCode::BAD_REQUEST,
                "body_read_failed",
                format!("failed to read MCP request body: {error}"),
            )
        }
    };
    let mut builder = reqwest::Client::new().request(method.clone(), upstream);
    if let Some(token) = upstream_auth {
        builder = builder.bearer_auth(token);
    }
    for header_name in forwarded_mcp_headers() {
        if let Some(value) = headers.get(&header_name) {
            builder = builder.header(&header_name, value);
        }
    }
    let upstream_response = match builder.body(body).send().await {
        Ok(response) => response,
        Err(error) => {
            return crate::protected_routes::json_error(
                StatusCode::BAD_GATEWAY,
                "backend_request_failed",
                format!("protected MCP backend request to {target} failed: {error}"),
            )
        }
    };
    let status = StatusCode::from_u16(upstream_response.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    tracing::info!(
        route = %route.name,
        upstream = %target,
        status = status.as_u16(),
        elapsed_ms = started.elapsed().as_millis(),
        "protected MCP route proxy completed"
    );
    let mut response = Response::builder().status(status);
    for header_name in returned_mcp_headers() {
        if let Some(value) = upstream_response.headers().get(&header_name) {
            response = response.header(&header_name, value);
        }
    }
    response
        .body(Body::from_stream(upstream_response.bytes_stream()))
        .unwrap_or_else(|error| {
            crate::protected_routes::json_error(
                StatusCode::BAD_GATEWAY,
                "response_build_failed",
                format!("failed to build protected MCP response: {error}"),
            )
        })
}

async fn protected_route_upstream_target(
    state: &AppState,
    route: &ProtectedMcpRouteConfig,
) -> Result<(reqwest::Url, Option<String>, String), Response> {
    if !route.backend_url.trim().is_empty() {
        let url = reqwest::Url::parse(&route.backend_url).map_err(|error| {
            crate::protected_routes::json_error(
                StatusCode::BAD_GATEWAY,
                "invalid_backend_url",
                format!("protected MCP route backend_url is invalid: {error}"),
            )
        })?;
        return Ok((url, None, "backend_url".to_owned()));
    }
    let Some(upstream_name) = route.upstream.as_deref() else {
        return Err(crate::protected_routes::json_error(
            StatusCode::BAD_GATEWAY,
            "missing_target",
            "protected MCP route has no backend_url, upstream, or gateway subset target",
        ));
    };
    let Some(upstream) = state.upstream_config(upstream_name) else {
        return Err(crate::protected_routes::json_error(
            StatusCode::NOT_FOUND,
            "upstream_not_found",
            format!("upstream `{upstream_name}` not found for protected MCP route"),
        ));
    };
    let Some(raw_url) = upstream.url.as_deref() else {
        return Err(crate::protected_routes::json_error(
            StatusCode::BAD_GATEWAY,
            "upstream_url_missing",
            format!("upstream `{upstream_name}` does not have an HTTP MCP URL"),
        ));
    };
    let url = reqwest::Url::parse(raw_url).map_err(|error| {
        crate::protected_routes::json_error(
            StatusCode::BAD_GATEWAY,
            "invalid_upstream_url",
            format!("upstream `{upstream_name}` URL is invalid: {error}"),
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(crate::protected_routes::json_error(
            StatusCode::BAD_GATEWAY,
            "unsupported_upstream_transport",
            format!("upstream `{upstream_name}` protected proxy requires http(s) transport"),
        ));
    }
    let token = upstream_auth_token(state, &upstream).await?;
    Ok((url, token, format!("upstream:{upstream_name}")))
}

async fn upstream_auth_token(
    state: &AppState,
    upstream: &UpstreamConfig,
) -> Result<Option<String>, Response> {
    #[cfg(not(feature = "oauth"))]
    let _ = state;

    if upstream.oauth.is_some() {
        #[cfg(feature = "oauth")]
        {
            return state
                .upstream_oauth_access_token(upstream, SHARED_GATEWAY_OAUTH_SUBJECT)
                .await
                .map_err(|error| {
                    crate::protected_routes::json_error(
                        StatusCode::BAD_GATEWAY,
                        "upstream_oauth_required",
                        error.to_string(),
                    )
                });
        }
        #[cfg(not(feature = "oauth"))]
        {
            return Err(crate::protected_routes::json_error(
                StatusCode::BAD_GATEWAY,
                "upstream_oauth_unavailable",
                "upstream OAuth requires compiling Soma with the oauth feature",
            ));
        }
    }
    Ok(upstream
        .bearer_token_env
        .as_deref()
        .and_then(configured_bearer_token))
}

fn append_proxy_suffix(url: &mut reqwest::Url, suffix: &str, query: Option<&str>) {
    let mut path = url.path().trim_end_matches('/').to_owned();
    if path.is_empty() {
        path.push('/');
    }
    if !suffix.is_empty() {
        if !path.ends_with('/') {
            path.push('/');
        }
        path.push_str(suffix.trim_start_matches('/'));
    }
    url.set_path(&path);
    url.set_query(query);
}

fn configured_bearer_token(env_name: &str) -> Option<String> {
    std::env::var(env_name).ok().and_then(|value| {
        let value = value.trim();
        let value = value.strip_prefix("Bearer ").unwrap_or(value).trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn forwarded_mcp_headers() -> [HeaderName; 5] {
    [
        header::ACCEPT,
        header::CONTENT_TYPE,
        HeaderName::from_static("mcp-protocol-version"),
        HeaderName::from_static("mcp-session-id"),
        HeaderName::from_static("last-event-id"),
    ]
}

fn returned_mcp_headers() -> [HeaderName; 4] {
    [
        header::CONTENT_TYPE,
        header::CACHE_CONTROL,
        HeaderName::from_static("mcp-session-id"),
        HeaderName::from_static("mcp-protocol-version"),
    ]
}

#[cfg(test)]
#[path = "protected_routes_proxy_tests.rs"]
mod tests;

//! Axum router — wires HTTP endpoints to the MCP service, REST API, and auth middleware.
//!
//! Endpoints:
//!   `POST /mcp`         — MCP Streamable HTTP transport (tools, resources, prompts)
//!   `GET  /health`      — Health check (unauthenticated)
//!   `GET  /status`      — Runtime status (unauthenticated, redacts secrets)
//!   `GET  /openapi.json` — Generated REST OpenAPI schema (unauthenticated)
//!   `GET  /v1/capabilities` — Direct REST route inventory
//!   `/v1/*`            — Direct REST API routes (see `crate::api`)
//!   `/*`                — SPA fallback for embedded web UI (when web feature enabled)

use std::sync::Arc;

use axum::{
    http::{HeaderName, HeaderValue, Method, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::json;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};

use crate::api::{
    health, openapi_json, readyz, status, v1_capabilities, v1_dynamic_provider_route, v1_echo,
    v1_greet, v1_help, v1_service_status,
};
use rtemplate_mcp::{allowed_origins, streamable_http_config, streamable_http_service};
use rtemplate_runtime::server::{build_auth_layer, AppState, AuthPolicy};

const MCP_BODY_LIMIT_BYTES: usize = 65_536;

pub fn router(state: AppState) -> Router {
    let rmcp_config = streamable_http_config(&state.config);

    let resource_url = match &state.auth_policy {
        AuthPolicy::Mounted { .. } => state
            .config
            .auth
            .public_url
            .as_deref()
            .map(|u| Arc::<str>::from(format!("{}/mcp", u.trim_end_matches('/')))),
        AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => None,
    };

    // Auth layer applied to MCP and direct /v1 REST routes.
    let auth_layer = build_auth_layer(
        &state.auth_policy,
        state.config.api_token.as_deref().map(Arc::<str>::from),
        resource_url,
    );

    let api_and_mcp: Router<AppState> = Router::new()
        .nest_service("/mcp", streamable_http_service(state.clone(), rmcp_config))
        .route("/v1/capabilities", get(v1_capabilities))
        .route("/v1/greet", post(v1_greet))
        .route("/v1/echo", post(v1_echo))
        .route("/v1/status", get(v1_service_status))
        .route("/v1/help", get(v1_help))
        .route(
            "/v1/{*path}",
            get(v1_dynamic_provider_route)
                .post(v1_dynamic_provider_route)
                .put(v1_dynamic_provider_route)
                .patch(v1_dynamic_provider_route)
                .delete(v1_dynamic_provider_route),
        );

    let api_and_mcp_resolved: Router<()> = api_and_mcp.with_state(state.clone());

    let authenticated = if let Some(layer) = auth_layer {
        api_and_mcp_resolved.layer(layer)
    } else {
        api_and_mcp_resolved
    };

    let oauth_router: Option<Router> = if let AuthPolicy::Mounted {
        auth_state: Some(ref state_arc),
    } = state.auth_policy
    {
        let auth_state = state_arc.as_ref().clone();
        let path_based_discovery = Router::new()
            .route(
                "/mcp/.well-known/oauth-authorization-server",
                get(rtemplate_auth::metadata::authorization_server_metadata),
            )
            .route(
                "/mcp/.well-known/openid-configuration",
                get(rtemplate_auth::metadata::authorization_server_metadata),
            )
            .route(
                "/mcp/.well-known/oauth-protected-resource",
                get(rtemplate_auth::metadata::protected_resource_metadata),
            )
            .with_state(auth_state.clone());
        Some(rtemplate_auth::routes::router(auth_state).merge(path_based_discovery))
    } else {
        None
    };

    let public_state: Router<AppState> = Router::new()
        .route("/health", get(health))
        .route("/readyz", get(readyz))
        .route("/status", get(status))
        .route("/openapi.json", get(openapi_json));
    // Prometheus metrics are only meaningful when the observability feature
    // installed a recorder at startup; gate the route on the same feature.
    #[cfg(feature = "observability")]
    let public_state = public_state.route("/metrics", get(metrics_handler));
    let public: Router<()> = public_state.with_state(state.clone());

    let mut base: Router<()> = Router::new().merge(authenticated).merge(public);

    if let Some(oauth) = oauth_router {
        base = base.merge(oauth);
    }

    #[cfg(feature = "web")]
    let base = if rtemplate_web::web_assets_available() {
        base.fallback(rtemplate_web::serve_web_assets)
    } else {
        base.fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))) })
    };
    #[cfg(not(feature = "web"))]
    let base =
        base.fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))) });

    base.layer(RequestBodyLimitLayer::new(MCP_BODY_LIMIT_BYTES))
        .layer(cors_layer(&state.config))
}

/// `GET /metrics` — Prometheus text exposition (unauthenticated).
///
/// Returns 503 until the recorder is installed (which `serve_http_mcp` does at
/// startup), so scraping never panics on a partially-initialized process.
#[cfg(feature = "observability")]
async fn metrics_handler() -> axum::response::Response {
    use axum::response::IntoResponse;
    match rtemplate_observability::metrics::render() {
        Some(body) => (
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            body,
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "metrics recorder not initialized",
        )
            .into_response(),
    }
}

fn cors_layer(config: &rtemplate_contracts::config::McpConfig) -> CorsLayer {
    let origins: Vec<HeaderValue> = allowed_origins(config)
        .into_iter()
        .filter_map(|o| match o.parse::<HeaderValue>() {
            Ok(hv) => Some(hv),
            Err(e) => {
                tracing::warn!(origin = %o, error = %e, "invalid CORS origin — skipping");
                None
            }
        })
        .collect();
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::POST, Method::GET])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
            // MCP protocol headers: Mcp-Protocol-Version (2025-06-18+) and the
            // draft (2026-07-28 / SEP-2243) Mcp-Method, Mcp-Name, and x-mcp-header.
            // Permitting them lets browser-based MCP clients clear CORS preflight.
            HeaderName::from_static("mcp-protocol-version"),
            HeaderName::from_static("mcp-method"),
            HeaderName::from_static("mcp-name"),
            HeaderName::from_static("x-mcp-header"),
        ])
}

#[cfg(test)]
#[path = "routes_tests.rs"]
mod tests;

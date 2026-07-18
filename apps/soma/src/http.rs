//! Merges Soma's HTTP routers and runs the HTTP server.
//!
//! `router(state)` composes the MCP Streamable HTTP transport
//! (`soma_mcp::streamable_http_service`), the REST API (`soma_api`), the
//! Palette product API (`soma_palette`), OAuth discovery routes
//! (`soma_auth`), Prometheus metrics (`soma_observability`), and the
//! embedded web UI fallback (`soma_web`) into one Axum `Router`. `serve()`
//! builds the `AppState` (via `bootstrap::http_state`), binds a listener, and
//! calls `soma_http_server::serve_with_shutdown` with `shutdown::signal()`
//! (plan section 3.1).
//!
//! Endpoints:
//!   `POST /mcp`              — MCP Streamable HTTP transport (tools, resources, prompts)
//!   `GET  /health`           — Health check (unauthenticated)
//!   `GET  /status`           — Runtime status (unauthenticated, redacts secrets)
//!   `GET  /openapi.json`     — Generated REST OpenAPI schema (unauthenticated)
//!   `GET  /v1/capabilities`  — Direct REST route inventory
//!   `/v1/palette/*`          — Palette product API (see `soma_palette`)
//!   `/v1/*`                  — Direct REST API routes (see `crate::api`)
//!   `/*`                     — SPA fallback for embedded web UI (when web feature enabled)

use std::sync::Arc;

#[cfg(feature = "observability")]
use axum::http::StatusCode;
use axum::{
    http::{HeaderName, HeaderValue, Method},
    middleware,
    routing::{get, post},
    Router,
};
use tracing::info;

use crate::api::{
    health, openapi_json, readyz, status, v1_capabilities, v1_dynamic_provider_route, v1_echo,
    v1_greet, v1_help, v1_provider_tool_action, v1_providers, v1_service_status,
};
use crate::bootstrap::{authorization_mode, mcp_state_for_state};
use crate::gateway_api::v1_gateway_action;
use soma_api::ApiState;
use soma_http_server::middleware::body_limit::body_limit_layer;
use soma_http_server::middleware::cors::cors_layer as generic_cors_layer;
use soma_http_server::rejection::not_found_handler;
use soma_mcp::{allowed_origins, streamable_http_config, streamable_http_service};
use soma_runtime::server::{build_auth_layer, AppState, AuthPolicy};

const MCP_BODY_LIMIT_BYTES: usize = 65_536;

/// Build the HTTP `AppState`, compose the router, bind a listener, and serve
/// until a shutdown signal drains in-flight requests. Re-exported as
/// `soma::server::serve_http_mcp` (reachable under the `mcp-http` feature
/// alone; see `lib.rs`).
pub async fn serve() -> anyhow::Result<()> {
    let state = crate::bootstrap::http_state().await?;

    // Install the Prometheus recorder once, before the router exposes /metrics.
    #[cfg(feature = "observability")]
    soma_observability::metrics::init();

    info!(
        bind = %state.config.bind_addr(),
        server_name = %state.config.server_name,
        auth = ?state.auth_policy,
        "MCP HTTP server starting"
    );

    let bind = state.config.bind_addr();
    let app = router(state).layer(soma_http_server::middleware::tracing::trace_layer());
    let listener = soma_http_server::bind(&bind).await?;
    info!(bind = %bind, "MCP HTTP server listening");

    soma_http_server::serve_with_shutdown(listener, app, crate::shutdown::signal()).await?;
    Ok(())
}

pub fn router(state: AppState) -> Router {
    let rmcp_config = streamable_http_config(&state.config);
    let api_state = api_state(&state);
    let palette_state = palette_state(&state);

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

    let mcp_state = mcp_state_for_state(&state);
    let mcp: Router<soma_mcp::McpState> = Router::new().nest_service(
        "/mcp",
        streamable_http_service(mcp_state.clone(), rmcp_config),
    );
    let api: Router<ApiState> = Router::new()
        .route("/v1/capabilities", get(v1_capabilities))
        .route("/v1/providers", get(v1_providers))
        .route("/v1/greet", post(v1_greet))
        .route("/v1/echo", post(v1_echo))
        .route("/v1/status", get(v1_service_status))
        .route("/v1/help", get(v1_help))
        .route("/v1/gateway/{action}", post(v1_gateway_action))
        .route("/v1/tools/{action}", post(v1_provider_tool_action))
        .route(
            "/v1/{*path}",
            get(v1_dynamic_provider_route)
                .post(v1_dynamic_provider_route)
                .put(v1_dynamic_provider_route)
                .patch(v1_dynamic_provider_route)
                .delete(v1_dynamic_provider_route),
        );
    let palette: Router<soma_palette::PaletteState> = soma_palette::router();

    let api_and_mcp_resolved: Router<()> = mcp
        .with_state(mcp_state.clone())
        .merge(api.with_state(api_state.clone()))
        .merge(palette.with_state(palette_state));

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
                get(soma_auth::metadata::authorization_server_metadata),
            )
            .route(
                "/mcp/.well-known/openid-configuration",
                get(soma_auth::metadata::authorization_server_metadata),
            )
            .route(
                "/mcp/.well-known/oauth-protected-resource",
                get(soma_auth::metadata::protected_resource_metadata),
            )
            .with_state(auth_state.clone());
        Some(soma_auth::routes::router(auth_state).merge(path_based_discovery))
    } else {
        None
    };

    let public_api: Router<ApiState> = Router::new()
        .route("/health", get(health))
        .route("/readyz", get(readyz))
        .route("/status", get(status))
        .route("/openapi.json", get(openapi_json));
    let public_runtime: Router<AppState> = Router::new().route(
        "/.well-known/oauth-protected-resource/{*route}",
        get(soma_integrations::protected_routes::protected_route_resource_metadata),
    );
    // Prometheus metrics are only meaningful when the observability feature
    // installed a recorder at startup; gate the route on the same feature.
    #[cfg(feature = "observability")]
    let public_api = public_api.route("/metrics", get(metrics_handler));
    let public: Router<()> = public_api
        .with_state(api_state)
        .merge(public_runtime.with_state(state.clone()));

    let mut base: Router<()> = Router::new().merge(authenticated).merge(public);

    if let Some(oauth) = oauth_router {
        base = base.merge(oauth);
    }

    #[cfg(feature = "web")]
    let base = if soma_web::web_assets_available() {
        base.fallback(soma_web::serve_web_assets)
    } else {
        base.fallback(not_found_handler)
    };
    #[cfg(not(feature = "web"))]
    let base = base.fallback(not_found_handler);

    let base = base.layer(middleware::from_fn_with_state(
        soma_integrations::protected_routes::ProtectedMcpState::new(state.clone(), mcp_state),
        soma_integrations::protected_routes::protected_mcp_intercept,
    ));

    base.layer(body_limit_layer(MCP_BODY_LIMIT_BYTES))
        .layer(cors_layer(&state.config))
}

pub(crate) fn api_state(state: &AppState) -> ApiState {
    ApiState::new(
        state.application_handle(),
        authorization_mode(state),
        state.config.server_name.clone(),
    )
}

fn palette_state(state: &AppState) -> soma_palette::PaletteState {
    soma_palette::PaletteState::new(state.application_handle(), authorization_mode(state))
}

/// `GET /metrics` — Prometheus text exposition (unauthenticated).
///
/// Returns 503 until the recorder is installed (which `serve` does at
/// startup), so scraping never panics on a partially-initialized process.
#[cfg(feature = "observability")]
async fn metrics_handler() -> axum::response::Response {
    use axum::response::IntoResponse;
    match soma_observability::metrics::render() {
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

/// Soma's CORS policy: which origins are allowed (product config) and which
/// methods/headers Soma's routes actually need — including the MCP protocol
/// headers browser-based MCP clients send. The mechanical `CorsLayer`
/// construction itself is generic and lives in `soma_http_server`.
fn cors_layer(config: &soma_config::McpConfig) -> soma_http_server::middleware::cors::CorsLayer {
    let configured = allowed_origins(config);
    let configured_count = configured.len();
    let origins: Vec<HeaderValue> = configured
        .into_iter()
        .filter_map(|o| match o.parse::<HeaderValue>() {
            Ok(hv) => Some(hv),
            Err(e) => {
                tracing::warn!(origin = %o, error = %e, "invalid CORS origin — skipping");
                None
            }
        })
        .collect();
    // Every configured origin failed to parse: the resulting CORS policy
    // permits no browser origin at all, which is easy to miss among
    // per-origin `warn` lines above — call it out at `error` so it's
    // discoverable when triaging a "CORS is completely broken" report.
    if configured_count > 0 && origins.is_empty() {
        tracing::error!(
            configured_count,
            "all configured CORS origins failed to parse — effective CORS allow-list is empty"
        );
    }
    generic_cors_layer(
        origins,
        vec![Method::POST, Method::GET],
        vec![
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
        ],
    )
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;

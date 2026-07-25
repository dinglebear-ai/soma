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
    extract::State,
    http::{HeaderName, HeaderValue, Method},
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use tracing::info;

use crate::api::{
    health, readyz, status, v1_capabilities, v1_dynamic_provider_route, v1_echo, v1_greet, v1_help,
    v1_provider_tool_action, v1_providers, v1_service_status,
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
/// Matches the desktop Palette bridge's own cap
/// (`apps/palette/src-tauri/src/labby_bridge.rs`) so launcher params that
/// pass client-side validation are not rejected here as `413` before the
/// handler runs. Applied only to `/v1/palette/*` — every other route keeps
/// the tighter [`MCP_BODY_LIMIT_BYTES`].
const PALETTE_BODY_LIMIT_BYTES: usize = 256 * 1024;

/// `GET /openapi.json`, augmented with Palette's `/v1/palette/*` routes on
/// top of `soma-api`'s own gateway-route augmentation. `soma-api` cannot
/// depend on `soma-palette` directly (product-surface crates must not depend
/// on one another — see `xtask check-architecture`), so this composition
/// root, which already depends on both, layers the second augmentation on
/// here instead of leaving the live `/openapi.json` (and any client
/// generated from it) silently missing the mounted Palette endpoints.
async fn openapi_json_with_palette(State(state): State<ApiState>) -> axum::response::Response {
    match soma_api::api::build_openapi_document(&state).await {
        Ok(mut value) => {
            soma_palette::openapi::augment_with_palette_routes(&mut value);
            Json(value).into_response()
        }
        Err(response) => response,
    }
}

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
        state.config.static_token_write,
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
    let palette: Router<soma_palette::PaletteState> =
        soma_palette::router().layer(body_limit_layer(PALETTE_BODY_LIMIT_BYTES));

    let api_and_mcp_resolved: Router<()> = mcp
        .with_state(mcp_state.clone())
        .merge(api.with_state(api_state.clone()))
        .layer(body_limit_layer(MCP_BODY_LIMIT_BYTES))
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
        .route("/openapi.json", get(openapi_json_with_palette));
    let public_runtime: Router<AppState> = Router::new().route(
        "/.well-known/oauth-protected-resource/{*route}",
        get(soma_runtime::protected_routes::protected_route_resource_metadata),
    );
    // Prometheus metrics are only meaningful when the observability feature
    // installed a recorder at startup; gate the route on the same feature.
    #[cfg(feature = "observability")]
    let public_api = public_api.route("/metrics", get(metrics_handler));
    let public: Router<()> = public_api
        .with_state(api_state)
        .merge(public_runtime.with_state(state.clone()))
        .layer(body_limit_layer(MCP_BODY_LIMIT_BYTES));

    let mut base: Router<()> = Router::new().merge(authenticated).merge(public);

    if let Some(oauth) = oauth_router {
        base = base.merge(oauth.layer(body_limit_layer(MCP_BODY_LIMIT_BYTES)));
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
        soma_runtime::protected_routes::ProtectedMcpState::new(state.clone(), mcp_state),
        soma_runtime::protected_routes::protected_mcp_intercept,
    ));

    // No blanket body-limit layer here: `/v1/palette/*` needs a higher cap
    // (see `PALETTE_BODY_LIMIT_BYTES`) than the rest of the router, so each
    // branch above (mcp+api, palette, public, oauth) carries its own
    // explicit limit instead of one applied over the fully-merged router.
    base.layer(cors_layer(&state.config))
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
    let mut headers = vec![
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
    ];
    headers.extend(trace_header_cors_allow_list(config.trace_headers));

    generic_cors_layer(origins, vec![Method::POST, Method::GET], headers)
}

/// CORS is transport permission only, never the trust decision. This static
/// exact allow-list is computed once at router construction.
fn trace_header_cors_allow_list(mode: soma_config::TraceHeaderMode) -> Vec<HeaderName> {
    match mode {
        soma_config::TraceHeaderMode::Off => Vec::new(),
        soma_config::TraceHeaderMode::Trusted => vec![
            HeaderName::from_static("traceparent"),
            HeaderName::from_static("tracestate"),
        ],
        soma_config::TraceHeaderMode::TrustedWithBaggage => vec![
            HeaderName::from_static("traceparent"),
            HeaderName::from_static("tracestate"),
            HeaderName::from_static("baggage"),
        ],
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;

// `RestRoute` (the generic route-metadata shape) and `CapabilitiesResponse`
// live in `soma-http-api` (plan section 3.11) — reusable across any product
// built on this workspace. `REST_ROUTES` below is Soma's own concrete route
// table and stays here.
pub use soma_http_api::route_inventory::{CapabilitiesResponse, RestRoute};

pub(crate) const GATEWAY_ROUTE_PATH: &str = "/v1/gateway/{action}";

pub const REST_ROUTES: &[RestRoute] = &[
    RestRoute {
        method: "GET",
        path: "/health",
        action: None,
        auth: "public",
        description: "Fast liveness probe.",
    },
    RestRoute {
        method: "GET",
        path: "/readyz",
        action: None,
        auth: "public",
        description: "Readiness probe; 503 when the upstream dependency is unreachable.",
    },
    RestRoute {
        method: "GET",
        path: "/metrics",
        action: None,
        auth: "public",
        description:
            "Prometheus metrics (text exposition format; requires the observability feature).",
    },
    RestRoute {
        method: "GET",
        path: "/status",
        action: None,
        auth: "public",
        description: "Local redacted runtime status.",
    },
    RestRoute {
        method: "GET",
        path: "/openapi.json",
        action: None,
        auth: "public",
        description: "Generated OpenAPI schema.",
    },
    RestRoute {
        method: "GET",
        path: "/v1/capabilities",
        action: None,
        auth: "mounted auth policy",
        description: "Direct REST route inventory and server metadata.",
    },
    RestRoute {
        method: "GET",
        path: "/v1/providers",
        action: None,
        auth: "mounted auth policy",
        description: "Live provider catalog, including dropped provider tools and MCP primitives.",
    },
    RestRoute {
        method: "POST",
        path: "/v1/tools/{action}",
        action: None,
        auth: "mounted auth policy; requires the provider tool scope when scoped",
        description: "Generic REST execution route for provider-backed tools.",
    },
    RestRoute {
        method: "POST",
        path: GATEWAY_ROUTE_PATH,
        action: None,
        auth:
            "mounted auth policy; read actions require soma:read, admin actions require soma:admin",
        description: "Gateway management and discovery action dispatch.",
    },
    RestRoute {
        method: "POST",
        path: "/v1/greet",
        action: Some("greet"),
        auth: "mounted auth policy; requires soma:read when scoped",
        description: "Return a greeting.",
    },
    RestRoute {
        method: "POST",
        path: "/v1/echo",
        action: Some("echo"),
        auth: "mounted auth policy; requires soma:read when scoped",
        description: "Echo a message back unchanged.",
    },
    RestRoute {
        method: "GET",
        path: "/v1/status",
        action: Some("status"),
        auth: "mounted auth policy; requires soma:read when scoped",
        description: "Return authenticated service status.",
    },
    RestRoute {
        method: "GET",
        path: "/v1/help",
        action: Some("help"),
        auth: "mounted auth policy",
        description: "Return the action catalog and route help.",
    },
];

pub(crate) fn capabilities_response() -> CapabilitiesResponse {
    soma_http_api::route_inventory::capabilities_response(
        "soma-mcp",
        env!("CARGO_PKG_VERSION"),
        "direct_routes",
        REST_ROUTES,
    )
}

#[cfg(test)]
#[path = "route_inventory_tests.rs"]
mod tests;

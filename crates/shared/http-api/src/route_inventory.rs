//! Generic route inventory metadata and documentation helpers.
//!
//! `RestRoute` describes one route's shape (method, path, auth posture,
//! description) generically. Products own their own concrete route list
//! (Soma's lives in `soma-api::route_inventory::REST_ROUTES`) and pass it to
//! [`capabilities_response`] to build a `/v1/capabilities`-style discovery
//! payload.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct RestRoute {
    pub method: &'static str,
    pub path: &'static str,
    pub action: Option<&'static str>,
    pub auth: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Serialize)]
pub struct CapabilitiesResponse {
    pub server: &'static str,
    pub version: &'static str,
    pub preferred_rest_style: &'static str,
    pub supported_routes: Vec<String>,
    pub routes: &'static [RestRoute],
}

/// Build a `CapabilitiesResponse` from a product's own static route table.
pub fn capabilities_response(
    server: &'static str,
    version: &'static str,
    preferred_rest_style: &'static str,
    routes: &'static [RestRoute],
) -> CapabilitiesResponse {
    CapabilitiesResponse {
        server,
        version,
        preferred_rest_style,
        supported_routes: routes
            .iter()
            .map(|route| format!("{} {}", route.method, route.path))
            .collect(),
        routes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTES: &[RestRoute] = &[
        RestRoute {
            method: "GET",
            path: "/health",
            action: None,
            auth: "public",
            description: "Liveness probe.",
        },
        RestRoute {
            method: "POST",
            path: "/v1/echo",
            action: Some("echo"),
            auth: "mounted auth policy",
            description: "Echo a message back unchanged.",
        },
    ];

    #[test]
    fn builds_supported_routes_from_method_and_path() {
        let response = capabilities_response("demo", "1.2.3", "direct_routes", ROUTES);
        assert_eq!(response.server, "demo");
        assert_eq!(response.version, "1.2.3");
        assert_eq!(
            response.supported_routes,
            vec!["GET /health".to_owned(), "POST /v1/echo".to_owned()]
        );
        assert_eq!(response.routes.len(), 2);
    }
}

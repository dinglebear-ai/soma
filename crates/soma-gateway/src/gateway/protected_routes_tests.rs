use crate::config::{ProtectedGatewaySubsetTarget, ProtectedMcpRouteConfig};

use super::*;

fn route() -> ProtectedMcpRouteConfig {
    ProtectedMcpRouteConfig {
        name: "axon".to_owned(),
        public_host: "MCP.Example.COM:443.".to_owned(),
        public_path: "/axon".to_owned(),
        backend_url: "http://10.0.0.2:4000/mcp".to_owned(),
        target: Some(ProtectedGatewaySubsetTarget {
            upstreams: vec!["axon".to_owned()],
            services: vec![],
            expose_code_mode: true,
        }),
        ..ProtectedMcpRouteConfig::default()
    }
}

#[test]
fn host_normalization_handles_port_trailing_dot_and_rejects_spoofing() {
    assert!(route_matches(&route(), "mcp.example.com:443", "/axon").is_ok());
    assert_eq!(
        route_matches(&route(), "mcp.example.com:443, evil.test", "/axon"),
        Err(ProtectedRouteError::HostMismatch)
    );
}

#[test]
fn path_prefix_boundaries_reject_neighbors_and_encoded_segments() {
    assert!(route_matches(&route(), "mcp.example.com:443", "/axon").is_ok());
    assert!(route_matches(&route(), "mcp.example.com:443", "/axon/tools").is_ok());
    assert_eq!(
        route_matches(&route(), "mcp.example.com:443", "/axon2"),
        Err(ProtectedRouteError::PathMismatch)
    );
    assert_eq!(
        route_matches(&route(), "mcp.example.com:443", "/axon/%2e%2e/admin"),
        Err(ProtectedRouteError::PathMismatch)
    );
}

#[test]
fn backend_policy_runs_at_dispatch_without_leaking_backend_url() {
    let denied = ProtectedMcpRouteConfig {
        backend_url: "http://127.0.0.1:4000/mcp".to_owned(),
        ..route()
    };

    let error = validate_backend_for_dispatch(&denied).unwrap_err();
    let body = protected_route_error_body(&error);

    assert_eq!(error, ProtectedRouteError::BackendDenied);
    assert!(!body.contains("127.0.0.1"));
}

#[test]
fn public_request_params_cannot_define_trusted_scope() {
    let scope = resolve_scope(
        &route(),
        &serde_json::json!({"scope": {"upstreams": ["evil"]}}),
    );

    assert_eq!(scope.upstreams, vec!["axon"]);
    assert!(scope.expose_code_mode);
}

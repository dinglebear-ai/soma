use soma_gateway::config::ProtectedMcpRouteConfig;

use super::{is_reserved_public_path, is_route_metadata_path, route_metadata_url};

fn route() -> ProtectedMcpRouteConfig {
    ProtectedMcpRouteConfig {
        name: "media".to_owned(),
        public_host: "MCP.Example.COM.".to_owned(),
        public_path: "/media".to_owned(),
        scopes: vec!["soma:read".to_owned()],
        ..ProtectedMcpRouteConfig::default()
    }
}

#[test]
fn metadata_url_uses_normalized_host_and_route_path() {
    assert_eq!(
        route_metadata_url(&route()),
        "https://mcp.example.com/.well-known/oauth-protected-resource/media"
    );
}

#[test]
fn route_metadata_path_is_route_relative() {
    assert!(is_route_metadata_path(
        &route(),
        "/media/.well-known/oauth-protected-resource"
    ));
    assert!(!is_route_metadata_path(
        &route(),
        "/.well-known/oauth-protected-resource/media"
    ));
}

#[test]
fn oauth_public_paths_bypass_protected_route_intercept() {
    assert!(is_reserved_public_path(
        "/.well-known/oauth-protected-resource/media"
    ));
    assert!(is_reserved_public_path("/authorize"));
    assert!(!is_reserved_public_path("/media"));
}

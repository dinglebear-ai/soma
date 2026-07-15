use super::*;

#[test]
fn public_host_normalization_handles_case_and_trailing_dot() {
    assert_eq!(normalize_public_host("MCP.EXAMPLE.COM."), "mcp.example.com");
}

#[test]
fn public_resource_uses_normalized_host() {
    let route = ProtectedMcpRouteConfig {
        name: "demo".to_owned(),
        public_host: "MCP.EXAMPLE.COM.".to_owned(),
        public_path: "/demo".to_owned(),
        ..ProtectedMcpRouteConfig::default()
    };
    assert_eq!(route.public_resource(), "https://mcp.example.com/demo");
}

#[test]
fn route_requires_absolute_public_path() {
    let route = ProtectedMcpRouteConfig {
        name: "demo".to_owned(),
        public_host: "mcp.example.com".to_owned(),
        public_path: "demo".to_owned(),
        ..ProtectedMcpRouteConfig::default()
    };
    assert!(route.validate().is_err());
}

#[test]
fn route_rejects_spoofed_host_and_denied_backend_url_at_config_write() {
    let spoofed = ProtectedMcpRouteConfig {
        name: "demo".to_owned(),
        public_host: "mcp.example.com, attacker.example".to_owned(),
        public_path: "/demo".to_owned(),
        ..ProtectedMcpRouteConfig::default()
    };
    assert!(spoofed.validate().is_err());

    let denied_backend = ProtectedMcpRouteConfig {
        name: "demo".to_owned(),
        public_host: "mcp.example.com".to_owned(),
        public_path: "/demo".to_owned(),
        backend_url: "http://127.0.0.1:4000/mcp".to_owned(),
        ..ProtectedMcpRouteConfig::default()
    };
    assert!(denied_backend.validate().is_err());
}

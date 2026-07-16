use crate::config::{GatewayConfig, ProtectedMcpRouteConfig};
use crate::gateway::manager::GatewayManager;

#[tokio::test]
async fn manager_projects_protected_routes_without_backend_urls() {
    let manager = GatewayManager::new(GatewayConfig {
        protected_mcp_routes: vec![ProtectedMcpRouteConfig {
            name: "axon".to_owned(),
            public_host: "mcp.example.com".to_owned(),
            public_path: "/axon".to_owned(),
            backend_url: "http://10.0.0.2:4000/mcp".to_owned(),
            upstream: Some("axon".to_owned()),
            ..ProtectedMcpRouteConfig::default()
        }],
        ..GatewayConfig::default()
    })
    .unwrap();

    let projection = manager.protected_route_projections().await;
    let rendered = format!("{projection:?}");

    assert_eq!(
        projection[0].public_resource,
        "https://mcp.example.com/axon"
    );
    assert!(!rendered.contains("10.0.0.2"));
}

#[test]
fn manager_resolves_enabled_route_by_host_and_longest_path() {
    let manager = GatewayManager::new(GatewayConfig {
        protected_mcp_routes: vec![
            ProtectedMcpRouteConfig {
                name: "root".to_owned(),
                public_host: "mcp.example.com".to_owned(),
                public_path: "/mcp".to_owned(),
                ..ProtectedMcpRouteConfig::default()
            },
            ProtectedMcpRouteConfig {
                name: "nested".to_owned(),
                public_host: "mcp.example.com".to_owned(),
                public_path: "/mcp/nested".to_owned(),
                ..ProtectedMcpRouteConfig::default()
            },
        ],
        ..GatewayConfig::default()
    })
    .unwrap();

    let route = manager
        .resolve_protected_route("mcp.example.com", "/mcp/nested/tools")
        .expect("nested route should match");

    assert_eq!(route.name, "nested");
}

#[test]
fn manager_resolves_metadata_path_to_exact_route_resource() {
    let manager = GatewayManager::new(GatewayConfig {
        protected_mcp_routes: vec![ProtectedMcpRouteConfig {
            name: "media".to_owned(),
            public_host: "mcp.example.com".to_owned(),
            public_path: "/media".to_owned(),
            ..ProtectedMcpRouteConfig::default()
        }],
        ..GatewayConfig::default()
    })
    .unwrap();

    let route = manager
        .resolve_protected_route_metadata(
            "mcp.example.com",
            "/.well-known/oauth-protected-resource/media",
        )
        .expect("metadata route should match");

    assert_eq!(route.name, "media");
}

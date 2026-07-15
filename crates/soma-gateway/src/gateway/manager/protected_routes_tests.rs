use crate::config::{GatewayConfig, ProtectedMcpRouteConfig};
use crate::gateway::manager::GatewayManager;

#[test]
fn manager_projects_protected_routes_without_backend_urls() {
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

    let projection = manager.protected_route_projections();
    let rendered = format!("{projection:?}");

    assert_eq!(
        projection[0].public_resource,
        "https://mcp.example.com/axon"
    );
    assert!(!rendered.contains("10.0.0.2"));
}

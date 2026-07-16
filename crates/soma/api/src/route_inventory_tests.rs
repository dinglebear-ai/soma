use super::*;

#[test]
fn capabilities_routes_include_gateway_dispatch() {
    let response = capabilities_response();

    assert!(response
        .supported_routes
        .iter()
        .any(|route| route == "POST /v1/gateway/{action}"));
    assert!(response.routes.iter().any(|route| {
        route.method == "POST" && route.path == "/v1/gateway/{action}" && route.action.is_none()
    }));
}

#[test]
fn static_service_actions_keep_action_metadata() {
    let action_routes: Vec<_> = REST_ROUTES
        .iter()
        .filter_map(|route| route.action)
        .collect();

    assert_eq!(action_routes, ["greet", "echo", "status", "help"]);
}

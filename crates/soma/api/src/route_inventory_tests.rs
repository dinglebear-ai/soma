use super::*;
use soma_domain::actions::ACTION_SPECS;

#[test]
fn capabilities_routes_include_gateway_dispatch() {
    let response = capabilities_response();

    assert!(
        response
            .supported_routes
            .iter()
            .any(|route| route == "POST /v1/gateway/{action}")
    );
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

    assert_eq!(
        action_routes,
        [
            "greet",
            "echo",
            "status",
            "python_environment_status",
            "python_environment_prune_plan",
            "python_environment_prune",
            "python_environment_repair",
            "python_environment_update",
            "python_worker_status",
            "python_worker_cancel",
            "python_worker_reset",
            "python_generation_status",
            "python_generation_rollback",
            "help",
        ]
    );
}

#[test]
fn route_authorization_text_matches_canonical_action_specs() {
    for route in REST_ROUTES.iter().filter(|route| route.action.is_some()) {
        let action = route.action.expect("filtered action route");
        let spec = ACTION_SPECS
            .iter()
            .find(|spec| spec.name == action)
            .expect("route action has a canonical spec");
        if let Some(scope) = spec.required_scope {
            assert!(
                route.auth.contains(scope),
                "{action} route auth omits required scope {scope}"
            );
        }
        assert_eq!(
            route.auth.contains("soma:admin"),
            spec.requires_admin,
            "{action} admin metadata drifted"
        );
        assert_eq!(
            route.auth.contains("confirmation"),
            spec.destructive,
            "{action} destructive metadata drifted"
        );
    }
}

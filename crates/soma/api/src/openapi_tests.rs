use serde_json::json;

use super::{augment_with_gateway_route, augment_with_static_action_routes};

#[test]
fn gateway_route_is_added_to_openapi_paths() {
    let mut doc = json!({"openapi": "3.1.0", "paths": {}});

    augment_with_gateway_route(&mut doc);

    assert!(doc["paths"].get("/v1/gateway/{action}").is_some());
    assert_eq!(
        doc["paths"]["/v1/gateway/{action}"]["post"]["responses"]["404"]["description"],
        "Unknown gateway action"
    );
}

#[test]
fn existing_gateway_route_is_preserved() {
    let mut doc = json!({
        "paths": {
            "/v1/gateway/{action}": {
                "post": {"summary": "custom"}
            }
        }
    });

    augment_with_gateway_route(&mut doc);

    assert_eq!(
        doc["paths"]["/v1/gateway/{action}"]["post"]["summary"],
        "custom"
    );
}

#[test]
fn every_static_action_route_is_projected_into_openapi() {
    let mut doc = json!({"openapi": "3.1.0", "paths": {}});

    augment_with_static_action_routes(&mut doc);

    for route in super::REST_ROUTES
        .iter()
        .filter(|route| route.action.is_some())
    {
        assert!(
            doc["paths"][route.path]
                .get(route.method.to_ascii_lowercase())
                .is_some(),
            "{} {} is absent",
            route.method,
            route.path
        );
    }
}

#[test]
fn python_mutation_routes_publish_required_valid_request_contracts() {
    let mut doc = json!({"openapi": "3.1.0", "paths": {}});

    augment_with_static_action_routes(&mut doc);

    for (path, required) in [
        (
            "/v1/python/environments/prune-plan",
            &["stale_before_unix_seconds"][..],
        ),
        (
            "/v1/python/environments/prune",
            &["stale_before_unix_seconds", "confirm"][..],
        ),
        (
            "/v1/python/environments/repair",
            &["provider_path", "confirm"][..],
        ),
        (
            "/v1/python/environments/update",
            &["provider_path", "confirm"][..],
        ),
        ("/v1/python/workers/cancel", &["provider", "confirm"][..]),
        ("/v1/python/workers/reset", &["provider", "confirm"][..]),
        (
            "/v1/python/generations/rollback",
            &["generation_id", "confirm"][..],
        ),
    ] {
        let body = &doc["paths"][path]["post"]["requestBody"];
        assert_eq!(body["required"], true, "{path}");
        assert_eq!(
            body["content"]["application/json"]["schema"]["required"],
            json!(required)
        );
        assert!(
            body["content"]["application/json"]["examples"]["request"]["value"]
                .as_object()
                .is_some_and(|example| !example.is_empty()),
            "{path} needs a usable request example"
        );
    }
}

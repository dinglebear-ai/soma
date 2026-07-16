use serde_json::json;

use super::augment_with_gateway_route;

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

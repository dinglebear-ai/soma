use serde_json::{Map, Value, json};
use soma_provider_core::RegistrySnapshot as CoreRegistrySnapshot;

fn success_response() -> Value {
    json!({
        "description": "Stable provider action response envelope",
        "content": {
            "application/json": {
                "schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["output", "request_id", "progress"],
                    "properties": {
                        "output": {},
                        "request_id": {"type": "string"},
                        "progress": {"type": "array", "items": {"type": "object"}}
                    }
                }
            }
        }
    })
}

pub(super) fn openapi_paths_from_core(core: &CoreRegistrySnapshot) -> Value {
    let mut paths = Map::new();
    paths.insert(
        "/v1/capabilities".to_owned(),
        json!({
            "get": {
                "summary": "List REST capabilities",
                "operationId": "v1Capabilities",
                "responses": {
                    "200": {"description": "Route inventory and server metadata"}
                }
            }
        }),
    );
    paths.insert(
        "/v1/providers".to_owned(),
        json!({
            "get": {
                "summary": "Inspect live providers",
                "operationId": "v1Providers",
                "responses": {
                    "200": {"description": "Live provider catalog and runtime inventory"}
                }
            }
        }),
    );
    paths.insert(
        "/v1/tools/{action}".to_owned(),
        json!({
            "post": {
                "summary": "Run a provider tool",
                "operationId": "runProviderTool",
                "parameters": [{
                    "name": "action",
                    "in": "path",
                    "required": true,
                    "schema": {"type": "string"},
                    "description": "Provider tool action name"
                }],
                "requestBody": {
                    "required": false,
                    "content": {
                        "application/json": {
                            "schema": {"type": "object", "additionalProperties": true}
                        }
                    }
                },
                "responses": {
                    "200": success_response(),
                    "400": {"description": "Provider validation error"},
                    "403": {"description": "Provider authorization error"},
                    "404": {"description": "Unknown action or surface not exposed"}
                }
            }
        }),
    );

    let mut routes = core
        .rest_routes()
        .map(|(method, path, action)| (method.to_owned(), path.to_owned(), action.to_owned()))
        .collect::<Vec<_>>();
    routes.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));

    for (method, path, action) in routes {
        let entry = paths
            .entry(path)
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(methods) = entry {
            methods.insert(
                method.to_ascii_lowercase(),
                json!({
                    "summary": format!("Provider action `{action}`"),
                    "operationId": action,
                    "responses": {
                        "200": success_response(),
                        "400": {"description": "Provider validation error"}
                    }
                }),
            );
        }
    }
    Value::Object(paths)
}

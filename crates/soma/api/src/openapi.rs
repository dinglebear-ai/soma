use serde_json::{Value, json};
use soma_domain::actions::action_spec;

use super::route_inventory::{GATEWAY_ROUTE_PATH, REST_ROUTES};

pub(crate) fn augment_with_static_action_routes(value: &mut Value) {
    let Some(paths) = value.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    for route in REST_ROUTES.iter().filter(|route| route.action.is_some()) {
        let action = route.action.expect("filtered action route");
        let methods = paths
            .entry(route.path.to_owned())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let Some(methods) = methods.as_object_mut() else {
            continue;
        };
        let method = route.method.to_ascii_lowercase();
        methods.entry(method).or_insert_with(|| {
            let mut operation = json!({
                "tags": ["direct-rest"],
                "summary": format!("Run {action}"),
                "description": route.description,
                "operationId": action,
                "security": [{"BearerAuth": []}, {}],
                "responses": {
                    "200": {"description": format!("{action} result")},
                    "400": {"description": "Invalid action parameters"},
                    "401": {"description": "Authentication required"},
                    "403": {"description": "Required scope, admin role, or confirmation missing"},
                    "500": {"description": "Internal server error"}
                }
            });
            if let Some((schema, example)) = python_request_contract(action) {
                operation["requestBody"] = json!({
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": schema,
                            "examples": {"request": {"value": example}}
                        }
                    }
                });
            } else if route.method != "GET" {
                operation["requestBody"] = json!({
                    "required": false,
                    "content": {
                        "application/json": {
                            "schema": {"type": "object", "additionalProperties": true}
                        }
                    }
                });
            }
            operation
        });
    }
}

fn python_request_contract(action: &str) -> Option<(Value, Value)> {
    let spec = action_spec(action)?;
    if spec.params.is_empty() {
        return None;
    }
    let required = spec
        .params
        .iter()
        .filter(|param| param.required)
        .map(|param| param.name)
        .collect::<Vec<_>>();
    let properties = spec
        .params
        .iter()
        .map(|param| {
            let mut schema = json!({
                "type": param.ty,
                "description": param.description,
            });
            if let Some(max_len) = param.max_len {
                schema["maxLength"] = json!(max_len);
            }
            if !param.enum_values.is_empty() {
                schema["enum"] = json!(param.enum_values);
            }
            if param.name == "confirm" && spec.destructive {
                schema["const"] = json!(true);
            }
            (param.name.to_owned(), schema)
        })
        .collect::<serde_json::Map<_, _>>();
    let example = match action {
        "python_environment_prune_plan" => {
            json!({"stale_before_unix_seconds": 1_700_000_000, "max_entries": 100})
        }
        "python_environment_prune" => {
            json!({"stale_before_unix_seconds": 1_700_000_000, "max_entries": 100, "confirm": true})
        }
        "python_environment_repair" | "python_environment_update" => {
            json!({"provider_path": "example.py", "confirm": true})
        }
        "python_worker_cancel" | "python_worker_reset" => {
            json!({"provider": "example", "confirm": true})
        }
        "python_generation_rollback" => json!({"generation_id": 1, "confirm": true}),
        _ => return None,
    };
    Some((
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": required,
            "properties": properties,
        }),
        example,
    ))
}

pub(crate) fn augment_with_gateway_route(value: &mut Value) {
    let Some(paths) = value.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.entry(GATEWAY_ROUTE_PATH.to_owned()).or_insert_with(|| {
        json!({
            "post": {
                "summary": "Dispatch a gateway action",
                "description": "Read gateway actions require soma:read; mutating/admin gateway actions require soma:admin.",
                "parameters": [{
                    "name": "action",
                    "in": "path",
                    "required": true,
                    "schema": {"type": "string"}
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
                    "200": {"description": "Gateway action result"},
                    "400": {"description": "Invalid gateway params"},
                    "403": {"description": "Gateway admin access required"},
                    "404": {"description": "Unknown gateway action"}
                }
            }
        })
    });
}

#[cfg(test)]
#[path = "openapi_tests.rs"]
mod tests;

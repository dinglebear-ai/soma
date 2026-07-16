use serde_json::{json, Value};

use super::route_inventory::GATEWAY_ROUTE_PATH;

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

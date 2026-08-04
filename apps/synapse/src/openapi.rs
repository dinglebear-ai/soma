use serde_json::{Map, Value, json};
use soma_ops::{AccessClass, OperationName};

use crate::StandaloneRuntime;

pub fn document(runtime: &StandaloneRuntime) -> Value {
    let mut paths = Map::from_iter([
        ("/health".into(), simple_get("Health")),
        ("/ready".into(), simple_get("Readiness")),
        ("/status".into(), simple_get("Status")),
        ("/activity".into(), simple_get("Recent activity")),
        (
            "/operations".into(),
            simple_get("Canonical operation catalog"),
        ),
    ]);
    for operation in runtime.catalog().operations() {
        let name = operation.name();
        let parameter_schema = runtime
            .catalog()
            .parameter_schema(name)
            .expect("every operation has a parameter schema")
            .schema()
            .clone();
        let result_schema = runtime
            .catalog()
            .result_schema(name)
            .expect("every operation has a result schema")
            .schema()
            .clone();
        let request_schema = json!({
            "type":"object",
            "properties":{
                "parameters":parameter_schema,
                "confirmed":{"type":"boolean","default":false},
                "idempotency_key":{"type":"string","minLength":1,"maxLength":256},
                "actor":{"type":"string","minLength":1,"maxLength":256}
            },
            "required":["parameters"],
            "additionalProperties":false
        });
        paths.insert(
            format!("/v1/operations/{}/execute", name.as_str()),
            post_operation(name, request_schema.clone(), result_schema),
        );
        if operation.access() == AccessClass::Mutation {
            paths.insert(
                format!("/v1/operations/{}/plan", name.as_str()),
                post_operation(name, request_schema, json!({"type":"object"})),
            );
        }
    }
    json!({
        "openapi":"3.1.0",
        "info":{
            "title":"Synapse Canonical Operations API",
            "version":env!("CARGO_PKG_VERSION"),
            "description":"Standalone CLI, REST, and MCP product over the 59-operation canonical engine"
        },
        "paths":paths,
        "components":{
            "securitySchemes":{
                "bearerAuth":{"type":"http","scheme":"bearer"}
            }
        }
    })
}

fn simple_get(summary: &str) -> Value {
    json!({"get":{"summary":summary,"responses":{"200":{"description":"Success"}}}})
}

fn post_operation(name: &OperationName, request: Value, response: Value) -> Value {
    json!({
        "post":{
            "operationId":name.as_str().replace('.', "_"),
            "summary":name.as_str(),
            "requestBody":{
                "required":true,
                "content":{"application/json":{"schema":request}}
            },
            "responses":{
                "200":{"description":"Canonical result","content":{"application/json":{"schema":response}}},
                "400":{"description":"Invalid request"},
                "428":{"description":"Mutation confirmation required"}
            }
        }
    })
}

#[cfg(test)]
#[path = "openapi_tests.rs"]
mod tests;

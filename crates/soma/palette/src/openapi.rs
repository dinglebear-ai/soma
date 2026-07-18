//! Product OpenAPI route metadata for `/v1/palette/*`.
//!
//! Mirrors the pattern `soma-api` uses for `/v1/gateway/{action}`: augment an
//! already-generated OpenAPI document with hand-written path entries for
//! routes this crate owns but that the generic provider-route generator
//! doesn't know about.

use serde_json::{json, Value};

pub const CATALOG_PATH: &str = "/v1/palette/catalog";
pub const SEARCH_PATH: &str = "/v1/palette/search";
pub const SCHEMA_PATH: &str = "/v1/palette/schema";
pub const EXECUTE_PATH: &str = "/v1/palette/execute";

/// Insert Palette route entries into `value`'s `paths` object, if not
/// already present. `value` is expected to already have a top-level `paths`
/// object (as produced by `SomaApplication::openapi_document`).
pub fn augment_with_palette_routes(value: &mut Value) {
    let Some(paths) = value.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths
        .entry(CATALOG_PATH.to_owned())
        .or_insert_with(catalog_path_item);
    paths
        .entry(SEARCH_PATH.to_owned())
        .or_insert_with(search_path_item);
    paths
        .entry(SCHEMA_PATH.to_owned())
        .or_insert_with(schema_path_item);
    paths
        .entry(EXECUTE_PATH.to_owned())
        .or_insert_with(execute_path_item);
}

fn catalog_path_item() -> Value {
    json!({
        "get": {
            "summary": "List palette-exposed launcher entries",
            "description": "Every provider tool whose palette overlay exposes it (default: exposed).",
            "responses": {
                "200": {"description": "Launcher catalog"}
            }
        }
    })
}

fn search_path_item() -> Value {
    json!({
        "get": {
            "summary": "Search palette-exposed launcher entries",
            "parameters": [
                {"name": "q", "in": "query", "required": false, "schema": {"type": "string"}},
                {"name": "limit", "in": "query", "required": false, "schema": {"type": "integer"}}
            ],
            "responses": {
                "200": {"description": "Matching launcher entries"}
            }
        }
    })
}

fn schema_path_item() -> Value {
    json!({
        "get": {
            "summary": "Get a launcher entry's input/output schema",
            "parameters": [
                {"name": "id", "in": "query", "required": true, "schema": {"type": "string"}}
            ],
            "responses": {
                "200": {"description": "Launcher schema"},
                "404": {"description": "Unknown or no longer palette-exposed launcher id"}
            }
        }
    })
}

fn execute_path_item() -> Value {
    json!({
        "post": {
            "summary": "Execute a palette launcher action",
            "description": "Read/write scope, admin, and destructive-confirmation policy are enforced by the underlying provider dispatch.",
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "object",
                            "required": ["id"],
                            "properties": {
                                "id": {"type": "string"},
                                "params": {"type": "object"},
                                "confirmDestructive": {"type": "boolean"}
                            }
                        }
                    }
                }
            },
            "responses": {
                "200": {"description": "Launcher execution result"},
                "400": {"description": "Invalid params or missing destructive confirmation"},
                "403": {"description": "Insufficient scope or admin required"},
                "404": {"description": "Unknown or no longer palette-exposed launcher id"}
            }
        }
    })
}

#[cfg(test)]
#[path = "openapi_tests.rs"]
mod tests;

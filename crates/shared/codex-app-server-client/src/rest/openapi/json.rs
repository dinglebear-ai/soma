//! Low-level, generic JSON Schema / OpenAPI value builders shared by every
//! other submodule of [`super`]. Nothing here knows about a specific route
//! or schema name - see `super::schemas` and `super::paths` for the
//! domain-specific document pieces built on top of these.

use serde_json::{Value, json};

/// Builds a JSON object from `entries`, sorting by key first so the result
/// is identical regardless of whether the ambient build's `serde_json::Map`
/// happens to preserve insertion order or not. See the module docs'
/// "Determinism" section for why this matters here specifically. Every
/// object-shaped value in this module is built through this function.
pub(super) fn obj(mut entries: Vec<(&'static str, Value)>) -> Value {
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut map = serde_json::Map::with_capacity(entries.len());
    for (key, value) in entries {
        map.insert(key.to_owned(), value);
    }
    Value::Object(map)
}

/// A `{"$ref": "#/components/schemas/<name>"}` pointer.
pub(super) fn schema_ref(name: &str) -> Value {
    obj(vec![(
        "$ref",
        json!(format!("#/components/schemas/{name}")),
    )])
}

/// An unconstrained JSON Schema (no `type` keyword: matches any JSON value),
/// used for the crate's several `serde_json::Value`-typed fields where the
/// real shape is whatever the underlying `codex app-server` JSON-RPC method
/// happens to return - this crate deliberately does not attempt to model
/// that per-method surface (see README.md on the typed [`protocol`](crate::protocol)
/// layer being the place that happens, not the REST adapter).
pub(super) fn any_value_schema(description: &str) -> Value {
    obj(vec![("description", json!(description))])
}

pub(super) fn string_schema(description: &str) -> Value {
    obj(vec![
        ("type", json!("string")),
        ("description", json!(description)),
    ])
}

/// A `string` schema whose Rust field is `Option<String>` with no
/// `skip_serializing_if`, meaning it is always present on the wire but may
/// be JSON `null` - OpenAPI 3.1 models that as `"type": ["string", "null"]`
/// rather than the 3.0-era `nullable: true` (dropped in 3.1 in favor of
/// JSON Schema's own type-array convention).
pub(super) fn nullable_string_schema(description: &str) -> Value {
    obj(vec![
        ("type", json!(["string", "null"])),
        ("description", json!(description)),
    ])
}

pub(super) fn integer_schema(description: &str) -> Value {
    obj(vec![
        ("type", json!("integer")),
        ("description", json!(description)),
    ])
}

pub(super) fn nonneg_integer_schema(description: &str) -> Value {
    obj(vec![
        ("type", json!("integer")),
        ("minimum", json!(0)),
        ("description", json!(description)),
    ])
}

pub(super) fn array_schema(items: Value, description: &str) -> Value {
    obj(vec![
        ("type", json!("array")),
        ("items", items),
        ("description", json!(description)),
    ])
}

/// Builds an `object` JSON Schema. `additional_properties` is `Some(false)`
/// exactly when the Rust struct carries `#[serde(deny_unknown_fields)]`;
/// `None` leaves `additionalProperties` unset (permissive default) for
/// structs that don't - getting this wrong in either direction would
/// misrepresent what the route layer actually accepts, which is the whole
/// point of this file (see the module docs).
pub(super) fn object_schema(
    properties: Vec<(&'static str, Value)>,
    required: &[&'static str],
    additional_properties: Option<bool>,
) -> Value {
    let mut entries = vec![("type", json!("object")), ("properties", obj(properties))];
    if !required.is_empty() {
        entries.push(("required", json!(required)));
    }
    if let Some(allowed) = additional_properties {
        entries.push(("additionalProperties", json!(allowed)));
    }
    obj(entries)
}

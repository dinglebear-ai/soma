use serde_json::Value;

use crate::ToolError;

pub fn validate_code_mode_params_against_schema(
    params: &Value,
    schema: Option<&Value>,
) -> Result<(), ToolError> {
    let Some(schema) = schema else {
        return Ok(());
    };
    validate_value(params, schema, schema, "params")
}

fn validate_value(
    value: &Value,
    schema: &Value,
    root: &Value,
    path: &str,
) -> Result<(), ToolError> {
    let Some(object) = schema.as_object() else {
        return Ok(());
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        let Some(resolved) = reference
            .strip_prefix('#')
            .and_then(|pointer| root.pointer(pointer))
        else {
            return Err(invalid(path, "uses an unresolved local $ref"));
        };
        return validate_value(value, resolved, root, path);
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        if !values.iter().any(|candidate| candidate == value) {
            return Err(invalid(path, "must match enum"));
        }
    }
    if let Some(expected) = object.get("const") {
        if expected != value {
            return Err(invalid(path, "must match const"));
        }
    }
    if let Some(kind) = object.get("type") {
        let ok = match kind {
            Value::String(kind) => matches_type(value, kind),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| matches_type(value, kind)),
            _ => true,
        };
        if !ok {
            return Err(invalid(path, "has wrong type"));
        }
    }
    if let Some(object_value) = value.as_object() {
        if let Some(required) = object.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object_value.contains_key(key) {
                    return Err(ToolError::MissingParam {
                        message: format!("callTool params missing required field `{key}`"),
                        param: key.to_string(),
                    });
                }
            }
        }
        if let Some(properties) = object.get("properties").and_then(Value::as_object) {
            for (key, child_schema) in properties {
                if let Some(child) = object_value.get(key) {
                    validate_value(child, child_schema, root, &format!("{path}.{key}"))?;
                }
            }
        }
        if object.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
            if let Some(properties) = object.get("properties").and_then(Value::as_object) {
                for key in object_value.keys() {
                    if !properties.contains_key(key) {
                        return Err(invalid(&format!("{path}.{key}"), "is not allowed"));
                    }
                }
            }
        }
    }
    if let Some(items) = object.get("items") {
        if let Some(values) = value.as_array() {
            for (index, child) in values.iter().enumerate() {
                validate_value(child, items, root, &format!("{path}[{index}]"))?;
            }
        }
    }
    Ok(())
}

fn matches_type(value: &Value, kind: &str) -> bool {
    match kind {
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn invalid(path: &str, reason: &str) -> ToolError {
    ToolError::InvalidParam {
        message: format!("{path} {reason}"),
        param: path.to_string(),
    }
}

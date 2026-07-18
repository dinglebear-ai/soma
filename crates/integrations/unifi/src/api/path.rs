//! `{param}` substitution and `*path` connector-wildcard handling for
//! capability path templates.

use serde_json::{Number, Value};

use crate::error::{Result, UnifiError};

/// Substitutes every `{key}` placeholder in `template` with the matching
/// string/number/boolean field from `params`, and expands a trailing
/// `*path` wildcard segment (validated against `allowed_wildcard_prefixes`).
///
/// # Errors
/// Returns [`UnifiError::PathTemplate`] for a malformed template or a
/// missing/mistyped parameter, or [`UnifiError::ConnectorPath`] if the
/// `*path` wildcard value fails [`validate_connector_path`].
pub fn substitute_path(
    template: &str,
    params: &Value,
    allowed_wildcard_prefixes: &[&str],
) -> Result<String> {
    let mut path = template.to_string();
    while let Some(start) = path.find('{') {
        let Some(end_offset) = path[start..].find('}') else {
            return Err(UnifiError::PathTemplate(
                "path template contains an unmatched opening brace".to_string(),
            ));
        };
        let end = start + end_offset;
        let key = &path[start + 1..end];
        if key.is_empty() {
            return Err(UnifiError::PathTemplate(
                "path template contains an empty {} parameter".to_string(),
            ));
        }
        let value = path_scalar(params, key)?;
        path.replace_range(start..=end, &encode_path_segment(&value));
    }

    if path.contains("*path") {
        let Some(value) = params.get("path").and_then(Value::as_str) else {
            return Err(UnifiError::PathTemplate(
                "missing required path parameter: path".to_string(),
            ));
        };
        validate_connector_path(value, allowed_wildcard_prefixes)?;
        path = path.replace("*path", value.trim_start_matches('/'));
    }

    Ok(path)
}

/// Percent-encodes everything outside `[A-Za-z0-9._~-]`, matching RFC 3986's
/// unreserved set for a single path segment.
pub fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// Rejects a `*path` wildcard value unless it is an absolute, traversal-free
/// path under one of `allowed_prefixes`. This is the only user-influenced
/// input that reaches the controller as a raw path segment rather than a
/// query/body value, so it gets its own defense-in-depth check on top of
/// whatever the controller itself enforces.
///
/// # Errors
/// Returns [`UnifiError::ConnectorPath`] if `path` looks unsafe (empty,
/// relative, contains `..`, an encoded separator, or a null byte) or falls
/// outside `allowed_prefixes`.
pub fn validate_connector_path(path: &str, allowed_prefixes: &[&str]) -> Result<()> {
    if is_unsafe_connector_path(path) {
        return Err(UnifiError::ConnectorPath(path.to_string()));
    }
    if allowed_prefixes
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        Ok(())
    } else {
        Err(UnifiError::ConnectorPath(format!(
            "{path} is outside the supported integration API prefix"
        )))
    }
}

fn path_scalar(params: &Value, key: &str) -> Result<String> {
    match params.get(key) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(Value::Number(value)) => Ok(number_to_string(value)),
        Some(Value::Bool(value)) => Ok(value.to_string()),
        Some(_) => Err(UnifiError::PathTemplate(format!(
            "path parameter {key} must be a string, number, or boolean"
        ))),
        None => Err(UnifiError::PathTemplate(format!(
            "missing required path parameter: {key}"
        ))),
    }
}

fn number_to_string(value: &Number) -> String {
    value
        .as_i64()
        .map(|v| v.to_string())
        .or_else(|| value.as_u64().map(|v| v.to_string()))
        .or_else(|| value.as_f64().map(|v| v.to_string()))
        .unwrap_or_else(|| value.to_string())
}

fn is_unsafe_connector_path(path: &str) -> bool {
    if path.is_empty()
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.contains("://")
        || path.contains('\\')
        || path.contains('?')
        || path.contains('#')
        || path.contains("..")
    {
        return true;
    }

    let lowercase = path.to_ascii_lowercase();
    lowercase.contains("%2e")
        || lowercase.contains("%2f")
        || lowercase.contains("%5c")
        || lowercase.contains("%00")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn substitute_path_replaces_string_number_and_bool_params() {
        let params = json!({ "id": "abc", "count": 3, "active": true });

        let path = substitute_path("/sites/{id}/{count}/{active}", &params, &[]).unwrap();

        assert_eq!(path, "/sites/abc/3/true");
    }

    #[test]
    fn substitute_path_percent_encodes_replaced_segments() {
        let params = json!({ "id": "a b/c" });

        let path = substitute_path("/sites/{id}", &params, &[]).unwrap();

        assert_eq!(path, "/sites/a%20b%2Fc");
    }

    #[test]
    fn substitute_path_errors_on_missing_param() {
        let err = substitute_path("/sites/{id}", &json!({}), &[]).unwrap_err();

        assert!(
            matches!(err, UnifiError::PathTemplate(msg) if msg.contains("missing required path parameter: id"))
        );
    }

    #[test]
    fn substitute_path_errors_on_unmatched_brace() {
        let err = substitute_path("/sites/{id", &json!({}), &[]).unwrap_err();

        assert!(matches!(err, UnifiError::PathTemplate(_)));
    }

    #[test]
    fn substitute_path_expands_an_allowed_wildcard() {
        let params = json!({ "path": "/proxy/network/integration/v1/sites" });

        let path =
            substitute_path("/proxy/*path", &params, &["/proxy/network/integration/"]).unwrap();

        assert_eq!(path, "/proxy/proxy/network/integration/v1/sites");
    }

    #[test]
    fn substitute_path_rejects_a_wildcard_outside_the_allowed_prefix() {
        let params = json!({ "path": "/etc/passwd" });

        let err =
            substitute_path("/proxy/*path", &params, &["/proxy/network/integration/"]).unwrap_err();

        assert!(matches!(err, UnifiError::ConnectorPath(_)));
    }

    #[test]
    fn validate_connector_path_rejects_traversal() {
        assert!(validate_connector_path(
            "/proxy/network/integration/../secret",
            &["/proxy/network/integration/"]
        )
        .is_err());
    }

    #[test]
    fn validate_connector_path_rejects_encoded_separators() {
        assert!(validate_connector_path(
            "/proxy/network/integration/%2e%2e",
            &["/proxy/network/integration/"]
        )
        .is_err());
    }

    #[test]
    fn validate_connector_path_accepts_an_allowed_path() {
        assert!(validate_connector_path(
            "/proxy/network/integration/v1/sites",
            &["/proxy/network/integration/"]
        )
        .is_ok());
    }

    #[test]
    fn encode_path_segment_leaves_unreserved_characters_untouched() {
        assert_eq!(encode_path_segment("abc-123_ABC.~"), "abc-123_ABC.~");
    }

    #[test]
    fn encode_path_segment_percent_encodes_everything_else() {
        assert_eq!(encode_path_segment("a b"), "a%20b");
    }
}

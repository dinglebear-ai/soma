//! JSON rendering helpers.

use serde::Serialize;
use serde_json::Error as JsonError;

/// Pretty-print `value` the same way `serde_json::to_string_pretty` does —
/// a thin, named wrapper so CLI output call sites read as "render JSON"
/// rather than reaching for `serde_json` directly.
pub fn to_pretty_string<T: Serialize>(value: &T) -> Result<String, JsonError> {
    serde_json::to_string_pretty(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pretty_prints_objects() {
        let out = to_pretty_string(&json!({ "ok": true })).unwrap();
        assert_eq!(out, "{\n  \"ok\": true\n}");
    }
}

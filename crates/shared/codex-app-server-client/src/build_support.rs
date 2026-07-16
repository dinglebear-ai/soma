//! Pure, side-effect-free helper shared between `build.rs` (real usage,
//! generating code from `schema/methods.json`) and this crate's own test
//! suite (`#[cfg(test)]`-only inclusion in `lib.rs`) - kept here rather than
//! inline in `build.rs` because build scripts are compiled as a separate
//! target that `cargo test` never runs, so any `build.rs` logic that needs
//! unit tests has to live somewhere `cargo test` actually visits.

use serde_json::Value;

/// `server_notifications` entries never have a "response_type" key at all -
/// indexing a missing key on a `serde_json::Value::Object` yields `Value::Null`
/// (serde_json's `Index` impl, not a panic) - so a missing key and an explicit
/// `null` (the `xtask/codex-schema`-generated shape for a genuinely
/// void-response request, see `RequestEntry` in `xtask/src/codex_schema/merge.rs`)
/// are indistinguishable and both legitimately mean "no response". Anything
/// else (a number, bool, array, or object) can only mean the manifest is
/// malformed - fail the build loudly instead of silently treating it the same
/// as "no response", which would generate a wrapper method that quietly
/// discards the app-server's actual reply.
pub(crate) fn response_type_of(method: &str, e: &Value) -> Option<String> {
    match e.get("response_type") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => panic!(
            "schema/methods.json entry for method {method:?} has a non-string, non-null \
             \"response_type\": {other} - this indicates a corrupt or hand-edited \
             methods.json (the generator, `cargo xtask codex-schema regen`, only ever emits \
             a string or null/absent here). Regenerate the schema rather than editing \
             methods.json by hand."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn treats_a_missing_key_and_an_explicit_null_the_same() {
        assert_eq!(response_type_of("m", &json!({})), None);
        assert_eq!(response_type_of("m", &json!({"response_type": null})), None);
    }

    #[test]
    fn returns_the_string_when_present() {
        assert_eq!(
            response_type_of("m", &json!({"response_type": "FooResponse"})),
            Some("FooResponse".to_string())
        );
    }

    #[test]
    #[should_panic(expected = "non-string, non-null")]
    fn panics_on_a_present_non_string_non_null_value() {
        response_type_of("some/method", &json!({"response_type": 42}));
    }
}

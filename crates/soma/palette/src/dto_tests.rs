//! Wire-contract tests: the frontend depends on these exact `camelCase`
//! keys, so a `rename_all`/field-name typo should fail `cargo test`, not
//! only show up as a runtime break in `apps/palette`'s UI.

use serde_json::json;

use super::{
    LauncherCatalogEntry, LauncherCatalogResponse, LauncherExecuteRequest, LauncherExecuteResponse,
    LauncherSchemaResponse, LauncherSearchQuery,
};

#[test]
fn catalog_entry_serializes_with_camel_case_keys_and_omits_absent_optionals() {
    let entry = LauncherCatalogEntry {
        id: "ping".to_string(),
        provider: "net".to_string(),
        title: "Ping".to_string(),
        description: "Ping a host".to_string(),
        category: None,
        icon: None,
        tone: None,
        arg_mode: Some("form".to_string()),
        result_view: None,
        destructive: false,
        requires_admin: true,
    };

    let value = serde_json::to_value(&entry).unwrap();
    assert_eq!(
        value,
        json!({
            "id": "ping",
            "provider": "net",
            "title": "Ping",
            "description": "Ping a host",
            "argMode": "form",
            "destructive": false,
            "requiresAdmin": true,
        })
    );
}

#[test]
fn catalog_response_uses_schema_version_and_fingerprint_camel_case() {
    let response = LauncherCatalogResponse {
        schema_version: 1,
        fingerprint: "sha256:test".to_string(),
        entries: vec![],
    };
    let value = serde_json::to_value(&response).unwrap();
    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["fingerprint"], "sha256:test");
}

#[test]
fn search_query_deserializes_q_and_limit() {
    let query: LauncherSearchQuery =
        serde_json::from_value(json!({"q": "ping", "limit": 5})).unwrap();
    assert_eq!(query.q, "ping");
    assert_eq!(query.limit, Some(5));
}

#[test]
fn schema_response_uses_input_schema_camel_case() {
    let response = LauncherSchemaResponse {
        id: "ping".to_string(),
        input_schema: json!({"type": "object"}),
        output_schema: None,
    };
    let value = serde_json::to_value(&response).unwrap();
    assert_eq!(value["inputSchema"], json!({"type": "object"}));
    assert!(value.get("outputSchema").is_none());
}

#[test]
fn execute_request_deserializes_confirm_destructive_camel_case_and_defaults() {
    let request: LauncherExecuteRequest =
        serde_json::from_value(json!({"id": "ping", "confirmDestructive": true})).unwrap();
    assert_eq!(request.id, "ping");
    assert!(request.confirm_destructive);
    // Omitted `params` must default to an empty object, not `Value::Null` —
    // provider input schemas validate against object-shaped schemas, so a
    // zero-argument action would otherwise fail dispatch with
    // `input_schema_failed` whenever a client omits `params` entirely.
    assert_eq!(request.params, json!({}));

    let defaulted: LauncherExecuteRequest = serde_json::from_value(json!({"id": "ping"})).unwrap();
    assert!(!defaulted.confirm_destructive);
    assert_eq!(defaulted.params, json!({}));
}

#[test]
fn execute_response_uses_request_id_camel_case() {
    let response = LauncherExecuteResponse {
        output: json!({"ok": true}),
        request_id: "palette-1-1".to_string(),
    };
    let value = serde_json::to_value(&response).unwrap();
    assert_eq!(value["requestId"], "palette-1-1");
}

use crate::convert::convert_spec;

const FIXTURE_SPEC: &str = r#"{
    "openapi": "3.0.0",
    "info": { "title": "Fixture", "version": "1.0.0" },
    "paths": {
        "/users/{id}": {
            "get": {
                "operationId": "getUser",
                "parameters": [
                    { "name": "id", "in": "path", "required": true,
                      "schema": { "type": "string" } }
                ],
                "responses": { "200": { "description": "ok" } }
            },
            "delete": {
                "operationId": "deleteUser",
                "responses": { "204": { "description": "gone" } }
            }
        },
        "/health": {
            "get": { "responses": { "200": { "description": "ok" } } }
        }
    }
}"#;

#[test]
fn allowlist_filters_on_raw_operation_id() {
    let ops = convert_spec("vendor", FIXTURE_SPEC, &["getUser".to_string()]).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].operation_id, "getUser");
    assert_eq!(ops[0].method, reqwest::Method::GET);
    assert_eq!(ops[0].path_template, "/users/{id}");
    assert!(!ops.iter().any(|op| op.operation_id == "deleteUser"));
}

#[test]
fn convert_denies_raw_operation_id_when_allowlist_empty() {
    let ops = convert_spec("vendor", FIXTURE_SPEC, &[]).unwrap();
    assert!(ops.is_empty());
}

#[test]
fn invalid_json_is_spec_parse_error() {
    let err = convert_spec("vendor", "not json", &["getUser".into()]).unwrap_err();
    assert_eq!(err.kind(), "config_error");
}

#[test]
fn convert_skips_operations_without_operation_id() {
    let ops = convert_spec("vendor", FIXTURE_SPEC, &["GET_/health".to_string()]).unwrap();
    assert!(ops.is_empty());
}

#[test]
fn missing_paths_is_spec_parse_error() {
    let err = convert_spec(
        "vendor",
        r#"{ "openapi": "3.0.0", "info": { "title": "No paths", "version": "1.0.0" } }"#,
        &["getUser".into()],
    )
    .unwrap_err();
    assert_eq!(err.kind(), "config_error");
}

#[test]
fn allowed_operation_with_unsafe_path_template_is_rejected() {
    let spec = r#"{
        "openapi": "3.0.0",
        "info": { "title": "Fixture", "version": "1.0.0" },
        "paths": {
            "../v1-evil/users": {
                "get": {
                    "operationId": "escape",
                    "responses": { "200": { "description": "ok" } }
                }
            }
        }
    }"#;
    let err = convert_spec("vendor", spec, &["escape".to_string()]).unwrap_err();
    assert_eq!(err.kind(), "config_error");
}

#[test]
fn unallowed_operation_with_unsafe_path_template_is_ignored() {
    let spec = r#"{
        "openapi": "3.0.0",
        "info": { "title": "Fixture", "version": "1.0.0" },
        "paths": {
            "..\\v1-evil": {
                "get": {
                    "operationId": "escape",
                    "responses": { "200": { "description": "ok" } }
                }
            }
        }
    }"#;
    let ops = convert_spec("vendor", spec, &["other".to_string()]).unwrap();
    assert!(ops.is_empty());
}

use soma_provider_core::ProviderCatalog;

use super::{apply_directory_wide_checks, DirectoryNamespace};
use crate::{
    provider_registry::DynamicResourceTemplate,
    providers::filesystem::{ProviderFileInspection, ProviderFileInspectionStatus},
};

fn tool_catalog(provider_name: &str, tool_name: &str) -> ProviderCatalog {
    serde_json::from_str(&format!(
        r#"{{
          "schema_version": 1,
          "provider": {{ "name": "{provider_name}", "kind": "static-rust", "version": "0.1.0" }},
          "tools": [
            {{
              "name": "{tool_name}",
              "description": "probe",
              "input_schema": {{ "type": "object", "properties": {{}}, "additionalProperties": false }}
            }}
          ]
        }}"#
    ))
    .expect("valid catalog fixture")
}

#[test]
fn namespace_register_then_find_conflict_detects_duplicate_provider_name() {
    let mut namespace = DirectoryNamespace::default();
    namespace.register(&tool_catalog("shared", "action_one"), "a.json");

    let conflict = namespace.find_conflict(&tool_catalog("shared", "action_two"));
    assert!(conflict.unwrap().contains("duplicate provider"));
}

#[test]
fn namespace_register_then_find_conflict_detects_duplicate_action() {
    let mut namespace = DirectoryNamespace::default();
    namespace.register(&tool_catalog("provider-a", "shared_action"), "a.json");

    let conflict = namespace.find_conflict(&tool_catalog("provider-b", "shared_action"));
    assert!(conflict.unwrap().contains("duplicate action"));
}

#[test]
fn namespace_find_conflict_is_none_for_disjoint_catalogs() {
    let mut namespace = DirectoryNamespace::default();
    namespace.register(&tool_catalog("provider-a", "action_a"), "a.json");

    assert!(namespace
        .find_conflict(&tool_catalog("provider-b", "action_b"))
        .is_none());
}

fn rest_tool_catalog(
    provider_name: &str,
    tool_name: &str,
    method: &str,
    path: &str,
) -> ProviderCatalog {
    serde_json::from_str(&format!(
        r#"{{
          "schema_version": 1,
          "provider": {{ "name": "{provider_name}", "kind": "static-rust", "version": "0.1.0" }},
          "tools": [
            {{
              "name": "{tool_name}",
              "description": "probe",
              "input_schema": {{ "type": "object", "properties": {{}}, "additionalProperties": false }},
              "rest": {{ "enabled": true, "method": "{method}", "path": "{path}" }}
            }}
          ]
        }}"#
    ))
    .expect("valid catalog fixture")
}

#[test]
fn apply_directory_wide_checks_rejects_a_reserved_infrastructure_route() {
    // /v1/providers has no ACTION_SPECS entry, so nothing but an explicit
    // reservation would ever catch a drop-in provider claiming it — and
    // Axum's literal /v1/providers route always wins over the /v1/{*path}
    // fallback, so the provider's handler would silently never run.
    let mut files = vec![ProviderFileInspection {
        path: "a.json".into(),
        file_name: "a.json".to_owned(),
        status: ProviderFileInspectionStatus::Loaded,
        provider_id: Some("provider-a".to_owned()),
        provider_kind: Some("static-rust".to_owned()),
        actions: vec!["list_providers".to_owned()],
        error: None,
    }];
    let catalogs = vec![Some(rest_tool_catalog(
        "provider-a",
        "list_providers",
        "GET",
        "/v1/providers",
    ))];

    let templates: Vec<Option<DynamicResourceTemplate>> = vec![None; files.len()];
    apply_directory_wide_checks(&mut files, &catalogs, &templates);

    assert_eq!(files[0].status, ProviderFileInspectionStatus::Invalid);
    assert!(files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("infrastructure"));
}

#[test]
fn apply_directory_wide_checks_rejects_a_different_method_on_a_reserved_infrastructure_path() {
    // Axum resolves /v1/providers by path first — a POST to it still hits
    // that route's own MethodRouter (which only handles GET) and gets a
    // 405, never falling through to the dynamic dispatcher. So POST is
    // exactly as shadowed as GET, even though Soma itself only uses GET.
    let mut files = vec![ProviderFileInspection {
        path: "a.json".into(),
        file_name: "a.json".to_owned(),
        status: ProviderFileInspectionStatus::Loaded,
        provider_id: Some("provider-a".to_owned()),
        provider_kind: Some("static-rust".to_owned()),
        actions: vec!["submit_provider".to_owned()],
        error: None,
    }];
    let catalogs = vec![Some(rest_tool_catalog(
        "provider-a",
        "submit_provider",
        "POST",
        "/v1/providers",
    ))];

    let templates: Vec<Option<DynamicResourceTemplate>> = vec![None; files.len()];
    apply_directory_wide_checks(&mut files, &catalogs, &templates);

    assert_eq!(files[0].status, ProviderFileInspectionStatus::Invalid);
    assert!(files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("infrastructure"));
}

#[test]
fn apply_directory_wide_checks_rejects_a_different_method_on_greet() {
    // /v1/greet is a literal route too (Soma registers POST), not routed
    // through the dynamic dispatcher — a provider declaring GET /v1/greet
    // is just as shadowed as one declaring POST /v1/greet.
    let mut files = vec![ProviderFileInspection {
        path: "a.json".into(),
        file_name: "a.json".to_owned(),
        status: ProviderFileInspectionStatus::Loaded,
        provider_id: Some("provider-a".to_owned()),
        provider_kind: Some("static-rust".to_owned()),
        actions: vec!["read_greet".to_owned()],
        error: None,
    }];
    let catalogs = vec![Some(rest_tool_catalog(
        "provider-a",
        "read_greet",
        "GET",
        "/v1/greet",
    ))];

    let templates: Vec<Option<DynamicResourceTemplate>> = vec![None; files.len()];
    apply_directory_wide_checks(&mut files, &catalogs, &templates);

    assert_eq!(files[0].status, ProviderFileInspectionStatus::Invalid);
    assert!(files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("infrastructure"));
}

#[test]
fn apply_directory_wide_checks_rejects_a_path_shadowed_by_the_generic_tools_route() {
    // /v1/tools/{action} is a wildcard matching exactly one segment — any
    // literal /v1/tools/<x> path is shadowed the same way, not just an
    // exact string match against a reserved list.
    let mut files = vec![ProviderFileInspection {
        path: "a.json".into(),
        file_name: "a.json".to_owned(),
        status: ProviderFileInspectionStatus::Loaded,
        provider_id: Some("provider-a".to_owned()),
        provider_kind: Some("static-rust".to_owned()),
        actions: vec!["custom_tool".to_owned()],
        error: None,
    }];
    let catalogs = vec![Some(rest_tool_catalog(
        "provider-a",
        "custom_tool",
        "POST",
        "/v1/tools/custom_tool",
    ))];

    let templates: Vec<Option<DynamicResourceTemplate>> = vec![None; files.len()];
    apply_directory_wide_checks(&mut files, &catalogs, &templates);

    assert_eq!(files[0].status, ProviderFileInspectionStatus::Invalid);
    assert!(files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("/v1/tools/{action}"));
}

#[test]
fn apply_directory_wide_checks_allows_a_tools_subpath_with_extra_segments() {
    // /v1/tools/{action} only matches a single segment; /v1/tools/x/y falls
    // through to the generic /v1/{*path} fallback and is not shadowed.
    let mut files = vec![ProviderFileInspection {
        path: "a.json".into(),
        file_name: "a.json".to_owned(),
        status: ProviderFileInspectionStatus::Loaded,
        provider_id: Some("provider-a".to_owned()),
        provider_kind: Some("static-rust".to_owned()),
        actions: vec!["nested_tool".to_owned()],
        error: None,
    }];
    let catalogs = vec![Some(rest_tool_catalog(
        "provider-a",
        "nested_tool",
        "POST",
        "/v1/tools/nested/action",
    ))];

    let templates: Vec<Option<DynamicResourceTemplate>> = vec![None; files.len()];
    apply_directory_wide_checks(&mut files, &catalogs, &templates);

    assert_eq!(files[0].status, ProviderFileInspectionStatus::Loaded);
}

#[test]
fn apply_directory_wide_checks_handles_an_empty_directory_without_panicking() {
    // No drop-in catalogs at all — the built-in seed alone must not panic or
    // spuriously flag anything (there's nothing to flag).
    let mut files: Vec<ProviderFileInspection> = Vec::new();
    let catalogs: Vec<Option<ProviderCatalog>> = Vec::new();

    let templates: Vec<Option<DynamicResourceTemplate>> = vec![None; files.len()];
    apply_directory_wide_checks(&mut files, &catalogs, &templates);

    assert!(files.is_empty());
}

#[test]
fn apply_directory_wide_checks_leaves_the_first_file_loaded_and_invalidates_the_second() {
    let mut files = vec![
        ProviderFileInspection {
            path: "a.json".into(),
            file_name: "a.json".to_owned(),
            status: ProviderFileInspectionStatus::Loaded,
            provider_id: Some("provider-a".to_owned()),
            provider_kind: Some("static-rust".to_owned()),
            actions: vec!["shared_action".to_owned()],
            error: None,
        },
        ProviderFileInspection {
            path: "b.json".into(),
            file_name: "b.json".to_owned(),
            status: ProviderFileInspectionStatus::Loaded,
            provider_id: Some("provider-b".to_owned()),
            provider_kind: Some("static-rust".to_owned()),
            actions: vec!["shared_action".to_owned()],
            error: None,
        },
    ];
    let catalogs = vec![
        Some(tool_catalog("provider-a", "shared_action")),
        Some(tool_catalog("provider-b", "shared_action")),
    ];

    let templates: Vec<Option<DynamicResourceTemplate>> = vec![None; files.len()];
    apply_directory_wide_checks(&mut files, &catalogs, &templates);

    assert_eq!(files[0].status, ProviderFileInspectionStatus::Loaded);
    assert_eq!(files[1].status, ProviderFileInspectionStatus::Invalid);
    assert!(files[1]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("a.json"));
}

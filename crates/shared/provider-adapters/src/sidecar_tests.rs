use serde_json::json;
use soma_provider_core::{EnvRequirement, ProviderCall, ProviderSurface};

use super::*;

#[test]
fn resolve_sidecar_command_with_env_returns_bare_command_when_path_missing() {
    let resolved = resolve_sidecar_command_with_env("node", None, None);
    assert_eq!(resolved, PathBuf::from("node"));
}

#[test]
fn resolve_sidecar_command_with_env_passes_through_absolute_paths() {
    let absolute = if cfg!(windows) {
        "C:\\tools\\node.exe"
    } else {
        "/usr/bin/node"
    };
    let resolved = resolve_sidecar_command_with_env(absolute, None, None);
    assert_eq!(resolved, PathBuf::from(absolute));
}

#[test]
fn output_exceeded_message_names_the_stream_and_limit() {
    let message = output_exceeded_message("stdout", 1024);
    assert!(message.contains("stdout"));
    assert!(message.contains("1024"));
}

#[test]
fn execution_payload_serializes_the_wire_envelope() {
    let mut call =
        ProviderCall::new("lookup", json!({"query": "status"})).with_surface(ProviderSurface::Cli);
    call.provider = "demo".to_owned();
    call.snapshot_id = "sha256:test".to_owned();

    let bytes = execution_payload(&call).expect("envelope serializes");
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("payload JSON");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["provider"], "demo");
    assert_eq!(payload["action"], "lookup");
    assert_eq!(payload["params"], json!({"query": "status"}));
    assert_eq!(payload["surface"], "cli");
    assert_eq!(payload["snapshot_id"], "sha256:test");
}

#[test]
fn collect_provider_env_applies_the_caller_supplied_prefix() {
    let requirement = EnvRequirement {
        name: "TOKEN".to_owned(),
        description: None,
        required: false,
        sensitive: true,
        server_prefixed: true,
        allow_unprefixed: false,
        default: Some(json!("fallback")),
    };
    let env = collect_provider_env(&[requirement], &[], "demo", "demo-provider", "action")
        .expect("env resolves via default");
    assert_eq!(env, vec![("DEMO_TOKEN".to_owned(), "fallback".to_owned())]);
}

#[test]
fn collect_provider_env_errors_on_missing_required_value() {
    let requirement = EnvRequirement {
        name: "TOKEN".to_owned(),
        description: None,
        required: true,
        sensitive: true,
        server_prefixed: true,
        allow_unprefixed: false,
        default: None,
    };
    let error = collect_provider_env(&[requirement], &[], "demo", "demo-provider", "action")
        .expect_err("missing required env should fail");
    assert_eq!(&*error.code, "missing_provider_env");
}

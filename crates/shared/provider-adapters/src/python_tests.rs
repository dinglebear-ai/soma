use serde_json::json;
use soma_provider_core::{ProviderCall, ProviderSurface};

use super::python_execution_payload;

#[test]
fn default_python_command_matches_platform_launcher() {
    #[cfg(windows)]
    assert_eq!(super::default_python_command(), "python");

    #[cfg(not(windows))]
    assert_eq!(super::default_python_command(), "python3");
}

#[test]
fn python_sidecar_payload_preserves_execution_envelope_fields() {
    let mut call =
        ProviderCall::new("lookup", json!({"query": "status"})).with_surface(ProviderSurface::Cli);
    call.provider = "demo-python".to_owned();
    call.snapshot_id = "sha256:test-snapshot".to_owned();

    let env = vec![("SOMA_DEMO_SECRET".to_owned(), "redacted".to_owned())];
    let bytes = python_execution_payload(std::path::Path::new("/tmp/demo.py"), &call, &env)
        .expect("payload should serialize");
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("payload JSON");

    assert_eq!(payload["mode"], "call");
    assert_eq!(payload["path"], "/tmp/demo.py");
    assert_eq!(payload["env_keys"], json!(["SOMA_DEMO_SECRET"]));
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["provider"], "demo-python");
    assert_eq!(payload["action"], "lookup");
    assert_eq!(payload["params"], json!({"query": "status"}));
    assert_eq!(payload["surface"], "cli");
    assert_eq!(payload["snapshot_id"], "sha256:test-snapshot");
}

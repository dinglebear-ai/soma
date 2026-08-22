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

/// Reproduces the real `ETXTBSY` window: an executable image that is still
/// open for writing cannot be exec'd until that descriptor closes. The retry
/// helper must ride out the window instead of surfacing a spurious
/// "broken provider" error.
#[cfg(unix)]
#[tokio::test]
async fn spawn_retries_an_image_that_is_still_open_for_writing() {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let script = temp.path().join("busy-image.sh");
    let mut writer = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o700)
        .open(&script)
        .expect("create executable");
    writer.write_all(b"#!/bin/sh\nexit 0\n").expect("write");
    writer.flush().expect("flush");

    // Precondition: while the write descriptor is open, a plain spawn fails
    // with exactly the condition the helper is meant to absorb. Without this
    // the test could pass for the wrong reason on a platform that never
    // reports ETXTBSY.
    let immediate = std::process::Command::new(&script).spawn();
    let Err(error) = immediate else {
        // Some filesystems (and non-Linux unices) do not enforce ETXTBSY.
        // The helper is still correct; there is simply nothing to retry here.
        return;
    };
    assert_eq!(error.kind(), std::io::ErrorKind::ExecutableFileBusy);

    // Release the descriptor partway through the retry budget.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(15));
        drop(writer);
    });

    let mut command = Command::new(&script);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = spawn_retrying_busy_image(&mut command)
        .await
        .expect("retry must outlast the transient busy window");
    let status = child.wait().await.expect("wait");
    assert!(status.success());
}

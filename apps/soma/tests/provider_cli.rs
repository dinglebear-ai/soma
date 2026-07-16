use std::{fs, process::Command};

use serde_json::Value;
use tempfile::tempdir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_soma")
}

#[test]
fn providers_list_json_reports_dropped_provider() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path().join("providers");
    fs::create_dir(&providers).expect("create providers dir");
    fs::write(
        providers.join("hello.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "hello", "kind": "static-rust", "version": "0.1.0" },
          "tools": [
            {
              "name": "hello",
              "description": "Hello probe",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false },
              "output_schema": { "type": "object", "properties": {}, "additionalProperties": true }
            }
          ]
        }"#,
    )
    .expect("write provider");

    let output = Command::new(binary())
        .args(["providers", "list", "--dir"])
        .arg(&providers)
        .arg("--json")
        .env("SOMA_API_URL", "")
        .env_remove("SOMA_API_KEY")
        .env_remove("SOMA_MCP_TOKEN")
        .output()
        .expect("run providers list");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(value["summary"]["loaded"], 1);
    assert_eq!(value["files"][0]["provider_id"], "hello");
    assert_eq!(value["files"][0]["actions"][0], "hello");
}

#[test]
fn providers_lint_fails_for_invalid_provider_file() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path().join("providers");
    fs::create_dir(&providers).expect("create providers dir");
    fs::write(providers.join("broken.json"), "{").expect("write invalid provider");

    let output = Command::new(binary())
        .args(["providers", "lint", "--dir"])
        .arg(&providers)
        .env("SOMA_API_URL", "")
        .env_remove("SOMA_API_KEY")
        .env_remove("SOMA_MCP_TOKEN")
        .output()
        .expect("run providers lint");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("invalid"));
}

#[test]
fn providers_status_uses_soma_provider_dir_environment() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path().join("custom-providers");
    fs::create_dir(&providers).expect("create providers dir");

    let output = Command::new(binary())
        .args(["providers", "status"])
        .env("SOMA_PROVIDER_DIR", &providers)
        .env("SOMA_API_URL", "")
        .env_remove("SOMA_API_KEY")
        .env_remove("SOMA_MCP_TOKEN")
        .output()
        .expect("run providers status");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&providers.display().to_string()));
}

#[test]
fn providers_validate_still_dispatches_through_the_live_registry() {
    let output = Command::new(binary())
        .args(["providers", "validate"])
        .env("SOMA_MCP_NO_AUTH", "true")
        .env("SOMA_API_URL", "")
        .env_remove("SOMA_API_KEY")
        .env_remove("SOMA_MCP_TOKEN")
        .env_remove("SOMA_PROVIDER_DIR")
        .output()
        .expect("run providers validate");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(value["ok"], true);
}

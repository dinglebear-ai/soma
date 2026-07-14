use std::fs;

use serde_json::json;
use tempfile::tempdir;

use super::{load_catalog, FileProviderSource, ProviderFileInspectionStatus};

#[test]
fn inspect_reports_loaded_disabled_and_invalid_files_without_executing_handlers() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

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

    fs::write(
        providers.join("disabled.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "disabled", "kind": "static-rust", "enabled": false },
          "tools": []
        }"#,
    )
    .expect("write disabled provider");

    fs::write(providers.join("broken.json"), "{").expect("write invalid provider");
    fs::write(providers.join("notes.txt"), "ignored").expect("write ignored file");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.root, providers);
    assert!(report.exists);
    assert_eq!(report.files.len(), 3);
    assert_eq!(report.providers_loaded, 1);
    assert_eq!(report.providers_disabled, 1);
    assert_eq!(report.providers_invalid, 1);

    let hello = report
        .files
        .iter()
        .find(|file| file.file_name == "hello.json")
        .unwrap();
    assert_eq!(hello.status, ProviderFileInspectionStatus::Loaded);
    assert_eq!(hello.provider_id.as_deref(), Some("hello"));
    assert_eq!(hello.actions, vec!["hello"]);

    let disabled = report
        .files
        .iter()
        .find(|file| file.file_name == "disabled.json")
        .unwrap();
    assert_eq!(disabled.status, ProviderFileInspectionStatus::Disabled);
    assert_eq!(disabled.provider_id.as_deref(), Some("disabled"));

    let broken = report
        .files
        .iter()
        .find(|file| file.file_name == "broken.json")
        .unwrap();
    assert_eq!(broken.status, ProviderFileInspectionStatus::Invalid);
    assert!(broken
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("broken.json"));
}

#[test]
fn inspect_missing_directory_is_a_valid_empty_report() {
    let temp = tempdir().expect("tempdir");
    let missing = temp.path().join("providers");

    let report = FileProviderSource::new(&missing)
        .inspect()
        .expect("inspect missing dir");

    assert_eq!(report.root, missing);
    assert!(!report.exists);
    assert!(report.files.is_empty());
    assert_eq!(report.providers_loaded, 0);
    assert_eq!(report.providers_disabled, 0);
    assert_eq!(report.providers_invalid, 0);
}

#[test]
fn inspect_skips_wasm_sidecar_manifest_as_its_own_entry() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    fs::write(providers.join("edge.wasm"), b"not actually wasm").expect("write wasm");
    fs::write(providers.join("edge.wasm.json"), manifest_bytes("edge"))
        .expect("write sidecar manifest");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].file_name, "edge.wasm");
    assert_eq!(report.files[0].status, ProviderFileInspectionStatus::Loaded);
}

#[test]
fn inspect_skips_python_providers_without_executing_them() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    let side_effect_marker = providers.join("side_effect_ran.txt");

    // Module-level code runs on import, so if `inspect()` ever imports this
    // file to introspect it, the marker file below will exist afterward.
    fs::write(
        providers.join("evil.py"),
        format!(
            "with open({:?}, 'w') as f:\n    f.write('executed')\n\nPROVIDER = {{'name': 'evil', 'kind': 'python'}}\n",
            side_effect_marker.to_str().unwrap()
        ),
    )
    .expect("write python provider");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert!(
        !side_effect_marker.exists(),
        "non-executing inspect() must never import/execute a .py provider"
    );
    assert_eq!(report.providers_skipped, 1);
    assert_eq!(report.providers_loaded, 0);
    assert_eq!(report.providers_invalid, 0);
    assert_eq!(
        report.files[0].status,
        ProviderFileInspectionStatus::Skipped
    );
    assert!(report.files[0].error.is_some());
}

#[test]
fn inspect_marks_manifest_validation_failures_as_invalid() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

    // Deserializes fine, but validate_provider_manifest rejects duplicate
    // tool names — the same check the live registry runs in build_snapshot.
    fs::write(
        providers.join("duplicate-tools.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "dup", "kind": "static-rust", "version": "0.1.0" },
          "tools": [
            {
              "name": "same_name",
              "description": "first",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false },
              "output_schema": { "type": "object", "properties": {}, "additionalProperties": true }
            },
            {
              "name": "same_name",
              "description": "second",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false },
              "output_schema": { "type": "object", "properties": {}, "additionalProperties": true }
            }
          ]
        }"#,
    )
    .expect("write provider with duplicate tool names");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_invalid, 1);
    assert_eq!(report.providers_loaded, 0);
    assert_eq!(
        report.files[0].status,
        ProviderFileInspectionStatus::Invalid
    );
    assert!(report.files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("duplicate"));
}

#[test]
fn inspect_marks_uncompilable_input_schema_as_invalid() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

    // Deserializes fine and passes validate_provider_manifest (which doesn't
    // check schema keyword validity), but `properties` must be an object per
    // JSON Schema — jsonschema::validator_for rejects this the same way
    // provider_registry::build_snapshot() does at real load time.
    fs::write(
        providers.join("bad-schema.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "bad-schema", "kind": "static-rust", "version": "0.1.0" },
          "tools": [
            {
              "name": "broken_tool",
              "description": "has an invalid input_schema",
              "input_schema": { "type": "object", "properties": [], "additionalProperties": false },
              "output_schema": { "type": "object", "properties": {}, "additionalProperties": true }
            }
          ]
        }"#,
    )
    .expect("write provider with invalid input_schema");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_invalid, 1);
    assert_eq!(report.providers_loaded, 0);
    assert_eq!(
        report.files[0].status,
        ProviderFileInspectionStatus::Invalid
    );
    assert!(report.files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("input_schema"));
}

#[test]
fn wasm_sidecar_manifest_is_loaded_as_the_wasm_provider_manifest() {
    let temp = tempdir().expect("tempdir");
    let wasm_path = temp.path().join("edge.wasm");
    let sidecar_path = temp.path().join("edge.wasm.json");
    fs::write(&wasm_path, b"not actually wasm").expect("write wasm placeholder");
    fs::write(&sidecar_path, manifest_bytes("edge")).expect("write sidecar manifest");

    let catalog = load_catalog(&wasm_path).expect("sidecar manifest");

    assert_eq!(catalog.provider.name, "edge");
    assert_eq!(catalog.provider.kind.as_str(), "wasm");
    assert_eq!(catalog.tools[0].name, "edge-run");
}

#[test]
fn wasm_sidecar_manifest_is_not_loaded_as_a_second_provider() {
    let temp = tempdir().expect("tempdir");
    fs::write(temp.path().join("edge.wasm"), b"not actually wasm").expect("write wasm");
    fs::write(temp.path().join("edge.wasm.json"), manifest_bytes("edge"))
        .expect("write sidecar manifest");

    let source = FileProviderSource::new(temp.path());
    let providers = source.load().expect("providers");

    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].catalog().provider.name, "edge");
}

#[test]
fn fingerprint_changes_when_wasm_sidecar_manifest_changes() {
    let temp = tempdir().expect("tempdir");
    fs::write(temp.path().join("edge.wasm"), b"not actually wasm").expect("write wasm");
    let sidecar_path = temp.path().join("edge.wasm.json");
    fs::write(&sidecar_path, manifest_bytes("edge")).expect("write sidecar manifest");
    let source = FileProviderSource::new(temp.path());

    let first = source.fingerprint().expect("first fingerprint");
    fs::write(&sidecar_path, manifest_bytes("edge-next")).expect("update sidecar manifest");
    let second = source.fingerprint().expect("second fingerprint");

    assert_ne!(first, second);
}

#[test]
fn fingerprint_ignores_wasm_binary_bytes_when_sidecar_manifest_exists() {
    let temp = tempdir().expect("tempdir");
    let wasm_path = temp.path().join("edge.wasm");
    fs::write(&wasm_path, b"large placeholder v1").expect("write wasm");
    fs::write(temp.path().join("edge.wasm.json"), manifest_bytes("edge"))
        .expect("write sidecar manifest");
    let source = FileProviderSource::new(temp.path());

    let first = source.fingerprint().expect("first fingerprint");
    fs::write(&wasm_path, b"large placeholder v2 with different bytes").expect("rewrite wasm");
    let second = source.fingerprint().expect("second fingerprint");

    assert_eq!(first, second);
}

#[test]
fn fingerprint_changes_when_python_dependency_changes() {
    let temp = tempdir().expect("tempdir");
    let package = temp.path().join("helpers");
    fs::create_dir(&package).expect("create helper package");
    fs::write(package.join("__init__.py"), "").expect("write package init");
    fs::write(package.join("schema.py"), "ACTION = 'first'\n").expect("write schema");
    fs::write(
        temp.path().join("entry.py"),
        "from helpers.schema import ACTION\nPROVIDER = {'name': 'entry', 'kind': 'python'}\ndef tool():\n    return ACTION\n",
    )
    .expect("write provider entry");
    let source = FileProviderSource::new(temp.path());

    let first = source.fingerprint().expect("first fingerprint");
    fs::write(package.join("schema.py"), "ACTION = 'second'\n").expect("rewrite schema");
    let second = source.fingerprint().expect("second fingerprint");

    assert_ne!(first, second);
}

fn manifest_bytes(name: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema_version": 1,
        "provider": {
            "name": name,
            "kind": "wasm",
            "enabled": true
        },
        "tools": [
            {
                "name": "edge-run",
                "description": "Run an edge provider.",
                "input_schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {}
                }
            }
        ]
    }))
    .expect("manifest json")
}

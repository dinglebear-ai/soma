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
fn inspect_loads_markdown_files_as_prompt_providers() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

    fs::write(
        providers.join("Code Review.md"),
        "# Code Review\n\nReview this change for correctness and missing tests.\n",
    )
    .expect("write markdown prompt");
    fs::write(providers.join("README.md"), "# Prompt Directory\n").expect("write readme");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.files.len(), 1);
    assert_eq!(report.providers_loaded, 1);
    assert_eq!(report.providers_invalid, 0);

    let prompt = report
        .files
        .iter()
        .find(|file| file.file_name == "Code Review.md")
        .unwrap();
    assert_eq!(prompt.status, ProviderFileInspectionStatus::Loaded);
    assert_eq!(prompt.provider_id.as_deref(), Some("code-review"));
    assert_eq!(prompt.provider_kind.as_deref(), Some("static-rust"));
    assert!(prompt.actions.is_empty());
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
fn inspect_marks_invalid_when_rest_path_does_not_match_the_v1_prefix_schema_constraint() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

    // Deserializes fine and passes validate_provider_manifest (which checks
    // structural rules, not schema patterns), but docs/contracts/
    // provider-manifest.schema.json requires rest.path to match
    // ^/v1(/.*)?$ — the HTTP router only mounts custom provider routes
    // under /v1/{*path}, so a path outside that pattern is unreachable at
    // runtime even though the manifest looks fine to every other check.
    fs::write(
        providers.join("bad-route.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "bad-route", "kind": "static-rust", "version": "0.1.0" },
          "tools": [
            {
              "name": "bad_route_tool",
              "description": "rest.path outside /v1",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false },
              "output_schema": { "type": "object", "properties": {}, "additionalProperties": true },
              "rest": { "enabled": true, "method": "POST", "path": "/hello" }
            }
          ]
        }"#,
    )
    .expect("write provider with an unreachable REST route");

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
        .contains("rest/path"));
}

#[test]
fn inspect_accepts_a_rest_overlay_using_path_params_query_params_and_request_body_schema() {
    // Regression guard: RestOverlay (the Rust struct) has always accepted
    // path_params/query_params/request_body_schema and the live registry
    // deserializes them fine, but provider-manifest.schema.json's
    // restOverlay definition didn't list them, so a manifest that
    // legitimately used them passed soma providers validate/runtime
    // loading while failing this lint's schema check — the schema was
    // stale, not the manifest.
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    fs::write(
        providers.join("full-rest.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "full-rest", "kind": "static-rust", "version": "0.1.0" },
          "tools": [
            {
              "name": "full_rest_tool",
              "description": "uses the full rest overlay",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false },
              "rest": {
                "enabled": true,
                "method": "POST",
                "path": "/v1/full-rest",
                "path_params": { "id": { "type": "string" } },
                "query_params": { "limit": { "type": "integer" } },
                "request_body_schema": { "type": "object" }
              }
            }
          ]
        }"#,
    )
    .expect("write provider using the full rest overlay");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 1, "errors: {:?}", report.files);
    assert_eq!(report.providers_invalid, 0);
}

#[test]
fn inspect_does_not_false_positive_on_manifests_that_omit_optional_fields() {
    // Regression guard: an earlier implementation validated the schema
    // against a re-serialized ProviderCatalog, which turns every omitted
    // #[serde(default)] field into an explicit JSON `null` — and the schema
    // rejects `null` where it expects an absent key or a real value. That
    // false-flagged every well-formed, minimal manifest as invalid.
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();
    fs::write(
        providers.join("minimal.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "minimal", "kind": "static-rust" },
          "tools": [
            {
              "name": "minimal_tool",
              "description": "only required fields",
              "input_schema": { "type": "object", "properties": {}, "additionalProperties": false }
            }
          ]
        }"#,
    )
    .expect("write minimal provider");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 1, "errors: {:?}", report.files);
    assert_eq!(report.providers_invalid, 0);
}

fn tool_manifest(provider_name: &str, tool_name: &str, cli_command: Option<&str>) -> String {
    let cli = match cli_command {
        Some(command) => format!(r#", "cli": {{ "enabled": true, "command": "{command}" }}"#),
        None => String::new(),
    };
    format!(
        r#"{{
          "schema_version": 1,
          "provider": {{ "name": "{provider_name}", "kind": "static-rust", "version": "0.1.0" }},
          "tools": [
            {{
              "name": "{tool_name}",
              "description": "probe",
              "input_schema": {{ "type": "object", "properties": {{}}, "additionalProperties": false }},
              "output_schema": {{ "type": "object", "properties": {{}}, "additionalProperties": true }}{cli}
            }}
          ]
        }}"#
    )
}

#[test]
fn inspect_marks_second_file_invalid_on_duplicate_provider_name_across_files() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

    // Two different files, each individually valid, but declaring the same
    // provider name — the live registry's provider_map() rejects this once
    // both are loaded together, even though neither file is wrong in isolation.
    fs::write(
        providers.join("a-first.json"),
        tool_manifest("shared", "action_one", None),
    )
    .expect("write first provider");
    fs::write(
        providers.join("b-second.json"),
        tool_manifest("shared", "action_two", None),
    )
    .expect("write second provider");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 1);
    assert_eq!(report.providers_invalid, 1);

    let first = report
        .files
        .iter()
        .find(|file| file.file_name == "a-first.json")
        .unwrap();
    assert_eq!(first.status, ProviderFileInspectionStatus::Loaded);

    let second = report
        .files
        .iter()
        .find(|file| file.file_name == "b-second.json")
        .unwrap();
    assert_eq!(second.status, ProviderFileInspectionStatus::Invalid);
    assert!(second
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("duplicate provider"));
    assert!(second
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("a-first.json"));
}

#[test]
fn inspect_marks_second_file_invalid_on_duplicate_action_across_files() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

    // Same tool/action name declared by two different providers — build_snapshot()
    // rejects this via its directory-wide action_index, not per-file validation.
    fs::write(
        providers.join("a-first.json"),
        tool_manifest("provider-a", "shared_action", None),
    )
    .expect("write first provider");
    fs::write(
        providers.join("b-second.json"),
        tool_manifest("provider-b", "shared_action", None),
    )
    .expect("write second provider");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 1);
    assert_eq!(report.providers_invalid, 1);

    let second = report
        .files
        .iter()
        .find(|file| file.file_name == "b-second.json")
        .unwrap();
    assert_eq!(second.status, ProviderFileInspectionStatus::Invalid);
    assert!(second
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("duplicate action"));
}

#[test]
fn inspect_marks_second_file_invalid_on_duplicate_cli_command_across_files() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

    // Different provider names and action names, but the same CLI overlay
    // command — build_snapshot()'s cli_index rejects this too.
    fs::write(
        providers.join("a-first.json"),
        tool_manifest("provider-a", "action_one", Some("shared-cmd")),
    )
    .expect("write first provider");
    fs::write(
        providers.join("b-second.json"),
        tool_manifest("provider-b", "action_two", Some("shared-cmd")),
    )
    .expect("write second provider");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 1);
    assert_eq!(report.providers_invalid, 1);

    let second = report
        .files
        .iter()
        .find(|file| file.file_name == "b-second.json")
        .unwrap();
    assert_eq!(second.status, ProviderFileInspectionStatus::Invalid);
    assert!(second
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("duplicate CLI command"));
}

#[test]
fn inspect_marks_invalid_when_provider_name_collides_with_the_builtin_static_rust_provider() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

    // "static-rust" is the built-in provider name every soma binary loads
    // alongside drop-in files (see dynamic_provider_registry_from_dir). A
    // drop-in file reusing it collides at real registry construction even
    // though it's the only file in the directory.
    fs::write(
        providers.join("clashes.json"),
        tool_manifest("static-rust", "some_action", None),
    )
    .expect("write provider colliding with the built-in provider name");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 0);
    assert_eq!(report.providers_invalid, 1);
    assert_eq!(
        report.files[0].status,
        ProviderFileInspectionStatus::Invalid
    );
    assert!(report.files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("duplicate provider"));
    assert!(report.files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("built-in"));
}

#[test]
fn inspect_marks_invalid_when_action_collides_with_a_builtin_action() {
    let temp = tempdir().expect("tempdir");
    let providers = temp.path();

    // "status" is one of ACTION_SPECS' built-in action names. A drop-in
    // provider under a different provider name can still collide on action
    // name alone, same as the live registry's action_index.
    fs::write(
        providers.join("clashes.json"),
        tool_manifest("my-provider", "status", None),
    )
    .expect("write provider colliding with a built-in action name");

    let report = FileProviderSource::new(providers)
        .inspect()
        .expect("inspect providers");

    assert_eq!(report.providers_loaded, 0);
    assert_eq!(report.providers_invalid, 1);
    assert!(report.files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("duplicate action"));
    assert!(report.files[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("built-in"));
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
